#![no_std]

// === AUTHORIZED CALLERS ===
// - Admin: initialize(), set_invoice_contract(), set_treasury(), set_registry_config(),
//   slash_oracle(), pause()/unpause(), admin_resolve_round() (only after a round expires)
// - Oracle operators: register_oracle(), deregister_oracle(), submit_vote() (own address only)
// - Anyone: open_verification_round(), expire_round(), all read-only view functions
//
// #861: N-of-M staked oracle consensus network. Replaces the invoice contract's
// 1-of-2 primary/secondary oracle fallback with stake-weighted voting: a
// `VerificationRound` is opened per invoice, registered oracles vote with
// weight equal to their staked amount, and once weighted approval/rejection
// crosses `quorum_bps` of the round's stake snapshot the registry calls back
// into the invoice contract's `consensus_verify`. If oracle participation is
// too low the round expires and an admin fallback path resolves it so an
// invoice can never be bricked by an unresponsive oracle set.

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, panic_with_error,
    symbol_short, token, Address, Env, Map, String, Symbol, Vec,
};

#[contractclient(name = "InvoiceContractClient")]
pub trait InvoiceContract {
    fn consensus_verify(
        env: Env,
        id: u64,
        registry: Address,
        approved: bool,
        reason: String,
        oracle_hash: String,
    );

    /// #953/#957: returns `(is_awaiting_verification, invoice_amount)` so the
    /// registry can refuse to open a round for an invoice that isn't actually
    /// awaiting verification, and pick a value-appropriate quorum tier.
    fn get_invoice_verification_state(env: Env, id: u64) -> (bool, i128);
}

const LEDGERS_PER_DAY: u32 = 17_280;
const REGISTRY_TTL: u32 = LEDGERS_PER_DAY * 365;
const DEFAULT_REQUIRED_VOTES: u32 = 3;
const DEFAULT_QUORUM_BPS: u32 = 6_600; // two-thirds
const DEFAULT_ROUND_DURATION_SECS: u64 = 3 * 24 * 60 * 60; // 3 days
const DEFAULT_DEREGISTER_COOLDOWN_SECS: u64 = 7 * 24 * 60 * 60; // 7 days

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum OracleRegistryError {
    AlreadyInitialized = 0,
    NotInitialized = 1,
    Unauthorized = 2,
    ContractPaused = 3,
    InvalidAmount = 4,
    InsufficientStake = 5,
    AlreadyRegistered = 6,
    NotRegistered = 7,
    DeregisterHasPendingVotes = 8,
    DeregisterCooldownActive = 9,
    InvalidBps = 10,
    NoActiveOracles = 11,
    RoundAlreadyOpen = 12,
    RoundNotFound = 13,
    RoundNotOpen = 14,
    RoundExpired = 15,
    RoundNotExpired = 16,
    AlreadyVoted = 17,
    InvoiceContractNotSet = 18,
    InvoiceCallFailed = 19,
    InvalidConfig = 20,
    // #953: the invoice referenced by open_verification_round isn't actually
    // awaiting oracle verification.
    InvoiceNotAwaitingVerification = 21,
    // #957: malformed quorum tier list (unsorted thresholds, out-of-range bps).
    InvalidQuorumTiers = 22,
    // #954: slash_oracle's on-chain evidence requirement.
    InvalidEvidence = 23,
    SlashRoundNotFound = 24,
    // #1042: a `*_via_ac` entrypoint was called but no `access_control`
    // contract has been configured via `set_access_control` yet.
    AccessControlNotConfigured = 25,
    // #1038: governance contract not configured
    GovernanceNotConfigured = 26,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OracleInfo {
    pub address: Address,
    pub stake_amount: i128,
    pub stake_token: Address,
    pub is_active: bool,
    pub total_verifications: u32,
    pub total_slashes: u32,
    pub registered_at: u64,
    /// Set when `deregister_oracle` has been called once; the second call
    /// (after `deregister_cooldown_secs` has elapsed) returns the stake and
    /// removes this record entirely.
    pub deregister_requested_at: Option<u64>,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoundStatus {
    Open,
    ConsensusApproved,
    ConsensusRejected,
    Expired,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct VerificationRound {
    pub invoice_id: u64,
    pub required_votes: u32,
    pub total_registered_oracles: u32,
    pub votes: Map<Address, bool>,
    pub weight_for: i128,
    pub weight_against: i128,
    /// Total active stake at the moment the round opened. Quorum is computed
    /// against this snapshot (not the live total) so stake changes elsewhere
    /// in the registry can't shift the bar for a round already in progress.
    pub total_stake_snapshot: i128,
    pub quorum_bps: u32,
    pub status: RoundStatus,
    pub opened_at: u64,
    pub deadline: u64,
    pub oracle_hash: String,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct RegistryConfig {
    pub min_stake: i128,
    pub stake_token: Address,
    pub required_votes: u32,
    pub quorum_bps: u32,
    pub round_duration_secs: u64,
    pub deregister_cooldown_secs: u64,
    pub treasury: Option<Address>,
}

/// #957: one tier of the value-based quorum schedule. Invoices with a
/// principal >= `min_invoice_amount` use `quorum_bps` instead of the
/// registry's flat default — a $500k invoice can require a stricter quorum
/// than a $500 one. Tiers are stored sorted ascending by `min_invoice_amount`;
/// the highest tier whose threshold the invoice amount clears wins.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct QuorumTier {
    pub min_invoice_amount: i128,
    pub quorum_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct OracleReputation {
    pub address: Address,
    pub is_active: bool,
    pub total_verifications: u32,
    pub total_slashes: u32,
    pub registered_at: u64,
    pub rounds_participated: u32,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Initialized,
    Paused,
    Config,
    InvoiceContract,
    Oracle(Address),
    OracleIds,
    Round(u64),
    OpenRounds,
    QuorumTiers,
    OracleRounds(Address),
    // #1042: multisig trust anchor. Additive — untouched, this stays unset
    // and every admin-gated entrypoint above works exactly as before.
    AccessControl,
    // #1038: governance contract address for governance-gated parameter changes
    Governance,
}

const EVT: Symbol = symbol_short!("ORACLE");

fn require_not_paused(env: &Env) {
    if env
        .storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Paused)
        .unwrap_or(false)
    {
        panic_with_error!(env, OracleRegistryError::ContractPaused);
    }
}

#[contract]
pub struct OracleRegistryContract;

#[contractimpl]
impl OracleRegistryContract {
    pub fn initialize(env: Env, admin: Address, stake_token: Address, min_stake: i128) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic_with_error!(&env, OracleRegistryError::AlreadyInitialized);
        }
        if min_stake <= 0 {
            panic_with_error!(&env, OracleRegistryError::InvalidAmount);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage().instance().set(
            &DataKey::Config,
            &RegistryConfig {
                min_stake,
                stake_token,
                required_votes: DEFAULT_REQUIRED_VOTES,
                quorum_bps: DEFAULT_QUORUM_BPS,
                round_duration_secs: DEFAULT_ROUND_DURATION_SECS,
                deregister_cooldown_secs: DEFAULT_DEREGISTER_COOLDOWN_SECS,
                treasury: None,
            },
        );
        env.storage()
            .instance()
            .set(&DataKey::OracleIds, &Vec::<Address>::new(&env));
        env.storage()
            .instance()
            .set(&DataKey::OpenRounds, &Vec::<u64>::new(&env));
        env.storage()
            .instance()
            .extend_ttl(REGISTRY_TTL, REGISTRY_TTL);
    }

    pub fn set_invoice_contract(
        env: Env,
        admin: Address,
        invoice_contract: Address,
    ) -> Result<(), OracleRegistryError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        env.storage()
            .instance()
            .set(&DataKey::InvoiceContract, &invoice_contract);
        env.events()
            .publish((EVT, symbol_short!("inv_set")), invoice_contract);
        Ok(())
    }

    pub fn get_invoice_contract(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::InvoiceContract)
    }

    pub fn set_treasury(
        env: Env,
        admin: Address,
        treasury: Option<Address>,
    ) -> Result<(), OracleRegistryError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        let mut config = Self::load_config(&env)?;
        config.treasury = treasury;
        env.storage().instance().set(&DataKey::Config, &config);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn set_registry_config(
        env: Env,
        admin: Address,
        min_stake: i128,
        required_votes: u32,
        quorum_bps: u32,
        round_duration_secs: u64,
        deregister_cooldown_secs: u64,
    ) -> Result<(), OracleRegistryError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        if min_stake <= 0
            || required_votes == 0
            || quorum_bps == 0
            || quorum_bps > 10_000
            || round_duration_secs == 0
        {
            return Err(OracleRegistryError::InvalidConfig);
        }
        let mut config = Self::load_config(&env)?;
        config.min_stake = min_stake;
        config.required_votes = required_votes;
        config.quorum_bps = quorum_bps;
        config.round_duration_secs = round_duration_secs;
        config.deregister_cooldown_secs = deregister_cooldown_secs;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events().publish((EVT, symbol_short!("cfg_upd")), admin);
        Ok(())
    }

    pub fn get_registry_config(env: Env) -> Result<RegistryConfig, OracleRegistryError> {
        Self::load_config(&env)
    }

    /// #957: replaces the value-based quorum schedule wholesale. `tiers` must
    /// be sorted strictly ascending by `min_invoice_amount` (non-negative,
    /// unique thresholds) with each `quorum_bps` in `1..=10_000` — this keeps
    /// `resolve_quorum_bps`'s single ascending pass correct and lets admins
    /// reason about the schedule without the contract silently reordering it.
    /// An empty vec restores the flat `config.quorum_bps` for every invoice.
    pub fn set_quorum_tiers(
        env: Env,
        admin: Address,
        tiers: Vec<QuorumTier>,
    ) -> Result<(), OracleRegistryError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;

        let mut prev_threshold: Option<i128> = None;
        for tier in tiers.iter() {
            if tier.min_invoice_amount < 0 || tier.quorum_bps == 0 || tier.quorum_bps > 10_000 {
                return Err(OracleRegistryError::InvalidQuorumTiers);
            }
            if let Some(prev) = prev_threshold {
                if tier.min_invoice_amount <= prev {
                    return Err(OracleRegistryError::InvalidQuorumTiers);
                }
            }
            prev_threshold = Some(tier.min_invoice_amount);
        }

        env.storage().instance().set(&DataKey::QuorumTiers, &tiers);
        env.events()
            .publish((EVT, symbol_short!("tiers_upd")), admin);
        Ok(())
    }

    pub fn get_quorum_tiers(env: Env) -> Vec<QuorumTier> {
        env.storage()
            .instance()
            .get(&DataKey::QuorumTiers)
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn pause(env: Env, admin: Address) -> Result<(), OracleRegistryError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        env.storage().instance().set(&DataKey::Paused, &true);
        env.events().publish((EVT, symbol_short!("paused")), admin);
        Ok(())
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), OracleRegistryError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        env.storage().instance().set(&DataKey::Paused, &false);
        env.events()
            .publish((EVT, symbol_short!("unpaused")), admin);
        Ok(())
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Paused)
            .unwrap_or(false)
    }

    /// Registers `operator` as an oracle, transferring `stake_amount` of the
    /// registry's configured stake token into the contract. Rejects a second
    /// registration while a prior entry (active or mid-deregistration-cooldown)
    /// still exists for the same address.
    pub fn register_oracle(
        env: Env,
        operator: Address,
        stake_amount: i128,
    ) -> Result<(), OracleRegistryError> {
        operator.require_auth();
        require_not_paused(&env);
        let config = Self::load_config(&env)?;
        if stake_amount < config.min_stake {
            return Err(OracleRegistryError::InsufficientStake);
        }
        if env
            .storage()
            .persistent()
            .has(&DataKey::Oracle(operator.clone()))
        {
            return Err(OracleRegistryError::AlreadyRegistered);
        }

        let token_client = token::Client::new(&env, &config.stake_token);
        token_client.transfer(&operator, &env.current_contract_address(), &stake_amount);

        let info = OracleInfo {
            address: operator.clone(),
            stake_amount,
            stake_token: config.stake_token.clone(),
            is_active: true,
            total_verifications: 0,
            total_slashes: 0,
            registered_at: env.ledger().timestamp(),
            deregister_requested_at: None,
        };
        let key = DataKey::Oracle(operator.clone());
        env.storage().persistent().set(&key, &info);
        env.storage()
            .persistent()
            .extend_ttl(&key, REGISTRY_TTL, REGISTRY_TTL);

        let mut ids: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::OracleIds)
            .unwrap_or_else(|| Vec::new(&env));
        if !ids.contains(&operator) {
            ids.push_back(operator.clone());
            env.storage().instance().set(&DataKey::OracleIds, &ids);
        }

        env.events()
            .publish((EVT, symbol_short!("registrd")), (operator, stake_amount));
        Ok(())
    }

    /// Two-phase deregistration. The first call (while still active) requests
    /// deregistration and starts the cooldown, but only succeeds if the oracle
    /// has no outstanding vote owed on any currently open round — this prevents
    /// an oracle from voting maliciously and immediately exiting before its
    /// vote can be scrutinized/slashed. The second call, made after
    /// `deregister_cooldown_secs` has elapsed, returns the stake and removes
    /// the oracle record.
    pub fn deregister_oracle(env: Env, operator: Address) -> Result<(), OracleRegistryError> {
        operator.require_auth();
        require_not_paused(&env);
        let key = DataKey::Oracle(operator.clone());
        let mut info: OracleInfo = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(OracleRegistryError::NotRegistered)?;
        let config = Self::load_config(&env)?;
        let now = env.ledger().timestamp();

        match info.deregister_requested_at {
            None => {
                if !info.is_active {
                    return Err(OracleRegistryError::NotRegistered);
                }
                let open_rounds: Vec<u64> = env
                    .storage()
                    .instance()
                    .get(&DataKey::OpenRounds)
                    .unwrap_or_else(|| Vec::new(&env));
                for invoice_id in open_rounds.iter() {
                    if let Some(round) = env
                        .storage()
                        .persistent()
                        .get::<DataKey, VerificationRound>(&DataKey::Round(invoice_id))
                    {
                        if round.status == RoundStatus::Open
                            && !round.votes.contains_key(operator.clone())
                        {
                            return Err(OracleRegistryError::DeregisterHasPendingVotes);
                        }
                    }
                }
                info.is_active = false;
                info.deregister_requested_at = Some(now);
                env.storage().persistent().set(&key, &info);
                env.events()
                    .publish((EVT, symbol_short!("dreg_req")), operator);
                Ok(())
            }
            Some(requested_at) => {
                if now < requested_at.saturating_add(config.deregister_cooldown_secs) {
                    return Err(OracleRegistryError::DeregisterCooldownActive);
                }
                let token_client = token::Client::new(&env, &config.stake_token);
                token_client.transfer(
                    &env.current_contract_address(),
                    &operator,
                    &info.stake_amount,
                );
                env.storage().persistent().remove(&key);

                let mut ids: Vec<Address> = env
                    .storage()
                    .instance()
                    .get(&DataKey::OracleIds)
                    .unwrap_or_else(|| Vec::new(&env));
                if let Some(idx) = ids.first_index_of(&operator) {
                    ids.remove(idx);
                    env.storage().instance().set(&DataKey::OracleIds, &ids);
                }

                env.events()
                    .publish((EVT, symbol_short!("dreg_done")), operator);
                Ok(())
            }
        }
    }

    /// Admin/governance-triggered penalty for a proven-bad verdict, paired
    /// with the invoice contract's dispute-resolution flow. Reduces the
    /// oracle's withdrawable stake by `bps` (out of 10,000) and, if a
    /// treasury address is configured, forwards the slashed amount there;
    /// otherwise it remains in the registry's own balance (unrecoverable by
    /// the oracle, since their tracked `stake_amount` has already been cut).
    #[allow(clippy::too_many_arguments)]
    pub fn slash_oracle(
        env: Env,
        admin: Address,
        operator: Address,
        bps: u32,
        round_id: u64,
        evidence: String,
    ) -> Result<(), OracleRegistryError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        Self::slash_oracle_internal(&env, operator, bps, round_id, evidence, admin)
    }

    fn slash_oracle_internal(
        env: &Env,
        operator: Address,
        bps: u32,
        round_id: u64,
        evidence: String,
        caller: Address,
    ) -> Result<(), OracleRegistryError> {
        if bps == 0 || bps > 10_000 {
            return Err(OracleRegistryError::InvalidBps);
        }
        if evidence.is_empty() {
            return Err(OracleRegistryError::InvalidEvidence);
        }
        // #954: every slash must cite the specific verification round it's
        // punishing behavior from — otherwise a slash is an unaccountable
        // admin fiat with no on-chain trail to audit after the fact.
        if !env.storage().persistent().has(&DataKey::Round(round_id)) {
            return Err(OracleRegistryError::SlashRoundNotFound);
        }
        let key = DataKey::Oracle(operator.clone());
        let mut info: OracleInfo = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(OracleRegistryError::NotRegistered)?;

        let slash_amount = (info.stake_amount * bps as i128) / 10_000;
        info.stake_amount -= slash_amount;
        info.total_slashes += 1;
        env.storage().persistent().set(&key, &info);

        let config = Self::load_config(env)?;
        if let Some(treasury) = config.treasury {
            let token_client = token::Client::new(env, &config.stake_token);
            token_client.transfer(&env.current_contract_address(), &treasury, &slash_amount);
        }

        env.events().publish(
            (EVT, symbol_short!("slashed")),
            (operator, bps, slash_amount, caller, round_id, evidence),
        );
        Ok(())
    }

    /// Withdraws accumulated slashed funds from the registry's balance when no
    /// treasury address is configured. Only callable by the admin; if a treasury
    /// is configured, slashed funds are automatically forwarded there during
    /// slash_oracle and this function cannot be used.
    pub fn withdraw_slashed_funds(
        env: Env,
        admin: Address,
        recipient: Address,
        amount: i128,
    ) -> Result<(), OracleRegistryError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        require_not_paused(&env);

        let config = Self::load_config(&env)?;
        if config.treasury.is_some() {
            return Err(OracleRegistryError::TreasuryAlreadyConfigured);
        }
        if amount <= 0 {
            return Err(OracleRegistryError::InvalidAmount);
        }

        let token_client = token::Client::new(&env, &config.stake_token);
        token_client.transfer(&env.current_contract_address(), &recipient, &amount);

        env.events().publish(
            (EVT, symbol_short!("withdrawn")),
            (admin, recipient, amount),
        );
        Ok(())
    }

    /// Opens a stake-weighted verification round for `invoice_id`. Callable by
    /// anyone once the invoice is in `AwaitingVerification` — the caller
    /// supplies the invoice's verification hash so it can be cross-checked
    /// against votes without the registry needing to read invoice storage
    /// directly (registry and invoice reconcile by hash, not by direct state
    /// coupling).
    pub fn open_verification_round(
        env: Env,
        caller: Address,
        invoice_id: u64,
        oracle_hash: String,
    ) -> Result<(), OracleRegistryError> {
        caller.require_auth();
        require_not_paused(&env);
        let round_key = DataKey::Round(invoice_id);
        if let Some(existing) = env
            .storage()
            .persistent()
            .get::<DataKey, VerificationRound>(&round_key)
        {
            if existing.status == RoundStatus::Open {
                return Err(OracleRegistryError::RoundAlreadyOpen);
            }
        }

        let invoice_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::InvoiceContract)
            .ok_or(OracleRegistryError::InvoiceContractNotSet)?;
        let invoice_client = InvoiceContractClient::new(&env, &invoice_contract);
        let (awaiting_verification, invoice_amount) = invoice_client
            .try_get_invoice_verification_state(&invoice_id)
            .map_err(|_| OracleRegistryError::InvoiceCallFailed)?
            .map_err(|_| OracleRegistryError::InvoiceCallFailed)?;
        if !awaiting_verification {
            return Err(OracleRegistryError::InvoiceNotAwaitingVerification);
        }

        let config = Self::load_config(&env)?;
        let ids: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::OracleIds)
            .unwrap_or_else(|| Vec::new(&env));
        let mut total_stake: i128 = 0;
        let mut active_count: u32 = 0;
        for id in ids.iter() {
            if let Some(info) = env
                .storage()
                .persistent()
                .get::<DataKey, OracleInfo>(&DataKey::Oracle(id.clone()))
            {
                if info.is_active {
                    total_stake += info.stake_amount;
                    active_count += 1;
                }
            }
        }
        if active_count == 0 {
            return Err(OracleRegistryError::NoActiveOracles);
        }

        let quorum_bps = Self::resolve_quorum_bps(&env, config.quorum_bps, invoice_amount);

        let now = env.ledger().timestamp();
        let round = VerificationRound {
            invoice_id,
            required_votes: config.required_votes,
            total_registered_oracles: active_count,
            votes: Map::new(&env),
            weight_for: 0,
            weight_against: 0,
            total_stake_snapshot: total_stake,
            quorum_bps,
            status: RoundStatus::Open,
            opened_at: now,
            deadline: now.saturating_add(config.round_duration_secs),
            oracle_hash,
        };
        env.storage().persistent().set(&round_key, &round);
        env.storage()
            .persistent()
            .extend_ttl(&round_key, REGISTRY_TTL, REGISTRY_TTL);

        let mut open_rounds: Vec<u64> = env
            .storage()
            .instance()
            .get(&DataKey::OpenRounds)
            .unwrap_or_else(|| Vec::new(&env));
        open_rounds.push_back(invoice_id);
        env.storage()
            .instance()
            .set(&DataKey::OpenRounds, &open_rounds);

        env.events().publish(
            (EVT, symbol_short!("rnd_open")),
            (invoice_id, active_count, total_stake),
        );
        Ok(())
    }

    /// Stake-weighted vote submission. Finalizes the round the moment either
    /// side's weight crosses the round's quorum threshold (computed against
    /// the stake snapshot taken when the round opened), calling back into the
    /// invoice contract's `consensus_verify` on finalization.
    pub fn submit_vote(
        env: Env,
        oracle: Address,
        invoice_id: u64,
        approved: bool,
        evidence_hash: String,
    ) -> Result<(), OracleRegistryError> {
        oracle.require_auth();
        require_not_paused(&env);

        let oracle_key = DataKey::Oracle(oracle.clone());
        let mut info: OracleInfo = env
            .storage()
            .persistent()
            .get(&oracle_key)
            .ok_or(OracleRegistryError::NotRegistered)?;
        if !info.is_active {
            return Err(OracleRegistryError::NotRegistered);
        }
        // #950 — an oracle that has initiated deregistration retains is_active=true
        // during the cooldown window but should no longer contribute voting weight;
        // it has committed to leaving and its stake may be returned at any time.
        if info.deregister_requested_at.is_some() {
            return Err(OracleRegistryError::DeregisterCooldownActive);
        }

        let config = Self::load_config(&env)?;
        // #1142: an oracle slashed below min_stake must not participate in voting.
        if info.stake_amount < config.min_stake {
            return Err(OracleRegistryError::InsufficientStake);
        }

        let round_key = DataKey::Round(invoice_id);
        let mut round: VerificationRound = env
            .storage()
            .persistent()
            .get(&round_key)
            .ok_or(OracleRegistryError::RoundNotFound)?;

        if round.status != RoundStatus::Open {
            return Err(OracleRegistryError::RoundNotOpen);
        }

        let now = env.ledger().timestamp();
        if now > round.deadline {
            // Note: a function returning `Err` here discards every storage
            // write made during this same invocation (Soroban rolls back the
            // whole call), so the actual Open -> Expired transition can't
            // happen inline — it's committed separately via `expire_round`
            // (anyone can call it, and it always succeeds with `Ok(())` once
            // the deadline has passed). This branch only rejects the stale
            // vote with a typed error.
            return Err(OracleRegistryError::RoundExpired);
        }

        if round.votes.contains_key(oracle.clone()) {
            return Err(OracleRegistryError::AlreadyVoted);
        }

        round.votes.set(oracle.clone(), approved);
        let weight = info.stake_amount;
        if approved {
            round.weight_for += weight;
        } else {
            round.weight_against += weight;
        }

        info.total_verifications += 1;
        env.storage().persistent().set(&oracle_key, &info);

        Self::append_oracle_round(&env, oracle.clone(), invoice_id);

        env.events().publish(
            (EVT, symbol_short!("voted")),
            (invoice_id, oracle.clone(), approved, weight, evidence_hash),
        );

        // Ceiling division so a non-zero quorum_bps against any non-zero
        // stake snapshot always yields a threshold >= 1 (mirrors the pool
        // contract's fee-rounding convention) — otherwise a tiny stake pool
        // could make the very first vote satisfy both `>= threshold` checks
        // simultaneously via a floored-to-zero threshold.
        let threshold = (round.total_stake_snapshot * round.quorum_bps as i128 + 9_999) / 10_000;

        // N-of-M: stake-weight alone is not enough to finalize. `required_votes`
        // (N) is an independent floor on the number of *distinct* oracles that
        // must participate, so a single high-stake oracle can't unilaterally
        // decide a round just because its stake alone clears quorum_bps.
        let has_min_votes = round.votes.len() >= round.required_votes;

        if has_min_votes && round.weight_for >= threshold {
            round.status = RoundStatus::ConsensusApproved;
            let oracle_hash = round.oracle_hash.clone();
            env.storage().persistent().set(&round_key, &round);
            Self::remove_open_round(&env, invoice_id);
            Self::finalize_on_invoice(&env, invoice_id, true, &oracle_hash)?;
            env.events()
                .publish((EVT, symbol_short!("consensus")), (invoice_id, true));
        } else if has_min_votes && round.weight_against >= threshold {
            round.status = RoundStatus::ConsensusRejected;
            let oracle_hash = round.oracle_hash.clone();
            env.storage().persistent().set(&round_key, &round);
            Self::remove_open_round(&env, invoice_id);
            Self::finalize_on_invoice(&env, invoice_id, false, &oracle_hash)?;
            env.events()
                .publish((EVT, symbol_short!("consensus")), (invoice_id, false));
        } else {
            env.storage().persistent().set(&round_key, &round);
        }

        Ok(())
    }

    /// Anyone may call this once a round's deadline has passed without
    /// reaching quorum, moving it to `Expired` so `admin_resolve_round` (or a
    /// fresh `open_verification_round`) can take over. Also invoked lazily
    /// from `submit_vote` when a stale vote arrives after the deadline.
    pub fn expire_round(env: Env, invoice_id: u64) -> Result<(), OracleRegistryError> {
        let round_key = DataKey::Round(invoice_id);
        let mut round: VerificationRound = env
            .storage()
            .persistent()
            .get(&round_key)
            .ok_or(OracleRegistryError::RoundNotFound)?;
        if round.status != RoundStatus::Open {
            return Err(OracleRegistryError::RoundNotOpen);
        }
        if env.ledger().timestamp() <= round.deadline {
            return Err(OracleRegistryError::RoundNotExpired);
        }
        round.status = RoundStatus::Expired;
        env.storage().persistent().set(&round_key, &round);
        Self::remove_open_round(&env, invoice_id);
        env.events()
            .publish((EVT, symbol_short!("rnd_exp")), invoice_id);
        Ok(())
    }

    /// Admin fallback for a round that expired without reaching quorum — the
    /// escape hatch that keeps low oracle participation from permanently
    /// bricking an invoice. Only callable once a round has actually expired.
    pub fn admin_resolve_round(
        env: Env,
        admin: Address,
        invoice_id: u64,
        approved: bool,
        reason: String,
    ) -> Result<(), OracleRegistryError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        Self::admin_resolve_round_internal(&env, invoice_id, approved, reason, admin)
    }

    fn admin_resolve_round_internal(
        env: &Env,
        invoice_id: u64,
        approved: bool,
        reason: String,
        caller: Address,
    ) -> Result<(), OracleRegistryError> {
        let round_key = DataKey::Round(invoice_id);
        let mut round: VerificationRound = env
            .storage()
            .persistent()
            .get(&round_key)
            .ok_or(OracleRegistryError::RoundNotFound)?;
        // #956: defer to the exact same condition `expire_round` uses (deadline
        // passed) rather than trusting the stored `status` field alone — an
        // admin must never be able to short-circuit oracle consensus on a
        // round still within its normal voting window. Checking both the
        // status *and* re-deriving the deadline condition means this can't
        // silently regress if a future code path ever sets `Expired` without
        // going through `expire_round`.
        if round.status != RoundStatus::Expired || env.ledger().timestamp() <= round.deadline {
            return Err(OracleRegistryError::RoundNotExpired);
        }
        round.status = if approved {
            RoundStatus::ConsensusApproved
        } else {
            RoundStatus::ConsensusRejected
        };
        let oracle_hash = round.oracle_hash.clone();
        env.storage().persistent().set(&round_key, &round);
        Self::finalize_on_invoice(env, invoice_id, approved, &oracle_hash)?;
        env.events().publish(
            (EVT, symbol_short!("fallback")),
            (invoice_id, approved, caller, reason),
        );
        Ok(())
    }

    pub fn get_oracle_info(env: Env, operator: Address) -> Option<OracleInfo> {
        env.storage().persistent().get(&DataKey::Oracle(operator))
    }

    pub fn list_active_oracles(env: Env) -> Vec<Address> {
        let ids: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::OracleIds)
            .unwrap_or_else(|| Vec::new(&env));
        let mut active = Vec::new(&env);
        for id in ids.iter() {
            if let Some(info) = env
                .storage()
                .persistent()
                .get::<DataKey, OracleInfo>(&DataKey::Oracle(id.clone()))
            {
                if info.is_active {
                    active.push_back(id);
                }
            }
        }
        active
    }

    pub fn get_verification_round(env: Env, invoice_id: u64) -> Option<VerificationRound> {
        env.storage().persistent().get(&DataKey::Round(invoice_id))
    }

    pub fn get_round_votes(env: Env, invoice_id: u64) -> Vec<(Address, bool)> {
        let round: Option<VerificationRound> =
            env.storage().persistent().get(&DataKey::Round(invoice_id));
        let mut out = Vec::new(&env);
        if let Some(r) = round {
            for (addr, approved) in r.votes.iter() {
                out.push_back((addr, approved));
            }
        }
        out
    }

    pub fn get_oracle_round_history(env: Env, oracle: Address) -> Vec<(u64, bool, String)> {
        let key = DataKey::OracleRounds(oracle.clone());
        let ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));
        let mut out = Vec::new(&env);
        for i in 0..ids.len() {
            let invoice_id = ids.get(i).unwrap();
            let round: Option<VerificationRound> =
                env.storage().persistent().get(&DataKey::Round(invoice_id));
            if let Some(r) = round {
                if let Some(approved) = r.votes.get(oracle.clone()) {
                    out.push_back((invoice_id, approved, r.oracle_hash.clone()));
                }
            }
        }
        out
    }

    pub fn get_oracle_reputation(env: Env, operator: Address) -> Option<OracleReputation> {
        let info: OracleInfo = env
            .storage()
            .persistent()
            .get(&DataKey::Oracle(operator.clone()))?;
        let key = DataKey::OracleRounds(operator.clone());
        let ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));
        Some(OracleReputation {
            address: operator,
            is_active: info.is_active,
            total_verifications: info.total_verifications,
            total_slashes: info.total_slashes,
            registered_at: info.registered_at,
            rounds_participated: ids.len(),
        })
    }

    fn append_oracle_round(env: &Env, oracle: Address, invoice_id: u64) {
        let key = DataKey::OracleRounds(oracle.clone());
        let mut ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env));
        if !ids.contains(invoice_id) {
            ids.push_back(invoice_id);
            env.storage().persistent().set(&key, &ids);
            env.storage()
                .persistent()
                .extend_ttl(&key, REGISTRY_TTL, REGISTRY_TTL);
        }
    }

    // #1042: multisig admin path, additive to the legacy single-admin
    // functions above — see access_control/src/lib.rs for the full
    // propose/approve/execute lifecycle. `set_access_control` bootstraps
    // the trust anchor (still gated by the legacy admin key, same as
    // pool/invoice/credit_score); every `*_via_ac` entrypoint below then
    // trusts only calls that carry the configured `access_control`
    // contract's own on-chain identity.

    pub fn set_access_control(
        env: Env,
        admin: Address,
        access_control: Address,
    ) -> Result<(), OracleRegistryError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        env.storage()
            .instance()
            .set(&DataKey::AccessControl, &access_control);
        env.events()
            .publish((EVT, symbol_short!("set_ac")), (admin, access_control));
        Ok(())
    }

    pub fn get_access_control(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::AccessControl)
    }

     // #1038: Bootstrap the governance contract address. Admin-gated one-time setup.
    pub fn set_governance_address(
        env: Env,
        admin: Address,
        governance: Address,
    ) -> Result<(), OracleRegistryError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        env.storage()
            .instance()
            .set(&DataKey::Governance, &governance);
        env.events()
            .publish((EVT, symbol_short!("set_gov")), (admin, governance));
        Ok(())
    }

    // #1038: Get the configured governance contract address.
    pub fn get_governance_address(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Governance)
    }

    /// #1042: rotates the trust anchor itself through the currently
    /// configured `access_control` contract rather than the legacy admin
    /// key, so a compromised admin key alone can no longer repoint or strip
    /// multisig gating once this registry has adopted it.
    pub fn set_access_control_via_ac(
        env: Env,
        access_control: Address,
        new_access_control: Address,
    ) -> Result<(), OracleRegistryError> {
        access_control.require_auth();
        Self::require_access_control(&env, &access_control)?;
        env.storage()
            .instance()
            .set(&DataKey::AccessControl, &new_access_control);
        env.events().publish(
            (EVT, symbol_short!("ac_rot")),
            (access_control, new_access_control),
        );
        Ok(())
    }

    pub fn set_invoice_contract_via_ac(
        env: Env,
        access_control: Address,
        invoice_contract: Address,
    ) -> Result<(), OracleRegistryError> {
        access_control.require_auth();
        Self::require_access_control(&env, &access_control)?;
        env.storage()
            .instance()
            .set(&DataKey::InvoiceContract, &invoice_contract);
        env.events()
            .publish((EVT, symbol_short!("inv_set")), invoice_contract);
        Ok(())
    }

    pub fn set_treasury_via_ac(
        env: Env,
        access_control: Address,
        treasury: Option<Address>,
    ) -> Result<(), OracleRegistryError> {
        access_control.require_auth();
        Self::require_access_control(&env, &access_control)?;
        let mut config = Self::load_config(&env)?;
        config.treasury = treasury;
        env.storage().instance().set(&DataKey::Config, &config);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn set_registry_config_via_ac(
        env: Env,
        access_control: Address,
        min_stake: i128,
        required_votes: u32,
        quorum_bps: u32,
        round_duration_secs: u64,
        deregister_cooldown_secs: u64,
    ) -> Result<(), OracleRegistryError> {
        access_control.require_auth();
        Self::require_access_control(&env, &access_control)?;
        if min_stake <= 0
            || required_votes == 0
            || quorum_bps == 0
            || quorum_bps > 10_000
            || round_duration_secs == 0
        {
            return Err(OracleRegistryError::InvalidConfig);
        }
        let mut config = Self::load_config(&env)?;
        config.min_stake = min_stake;
        config.required_votes = required_votes;
        config.quorum_bps = quorum_bps;
        config.round_duration_secs = round_duration_secs;
        config.deregister_cooldown_secs = deregister_cooldown_secs;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events()
            .publish((EVT, symbol_short!("cfg_upd")), access_control);
        Ok(())
    }

    pub fn set_paused_via_ac(
        env: Env,
        access_control: Address,
        paused: bool,
    ) -> Result<(), OracleRegistryError> {
        access_control.require_auth();
        Self::require_access_control(&env, &access_control)?;
        env.storage().instance().set(&DataKey::Paused, &paused);
        env.events()
            .publish((EVT, symbol_short!("ac_pause")), (access_control, paused));
        Ok(())
    }

    pub fn slash_oracle_via_ac(
        env: Env,
        access_control: Address,
        operator: Address,
        bps: u32,
        round_id: u64,
        evidence: String,
    ) -> Result<(), OracleRegistryError> {
        access_control.require_auth();
        Self::require_access_control(&env, &access_control)?;
        Self::slash_oracle_internal(&env, operator, bps, round_id, evidence, access_control)
    }

    pub fn admin_resolve_round_via_ac(
        env: Env,
        access_control: Address,
        invoice_id: u64,
        approved: bool,
        reason: String,
    ) -> Result<(), OracleRegistryError> {
        access_control.require_auth();
        Self::require_access_control(&env, &access_control)?;
        Self::admin_resolve_round_internal(&env, invoice_id, approved, reason, access_control)
    }

    // ---- #1038: Governance-gated parameter changes ----

    // #1038: Set invoice contract via governance proposal.
    pub fn set_invoice_contract_via_governance(
        env: Env,
        governance: Address,
        invoice_contract: Address,
    ) -> Result<(), OracleRegistryError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        env.storage()
            .instance()
            .set(&DataKey::InvoiceContract, &invoice_contract);
        env.events()
            .publish((EVT, symbol_short!("gov_inv")), (governance, invoice_contract));
        Ok(())
    }

    // #1038: Set treasury via governance proposal.
    pub fn set_treasury_via_governance(
        env: Env,
        governance: Address,
        treasury: Option<Address>,
    ) -> Result<(), OracleRegistryError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        let mut config = Self::load_config(&env)?;
        config.treasury = treasury;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events()
            .publish((EVT, symbol_short!("gov_treas")), (governance, treasury));
        Ok(())
    }

    // #1038: Set registry config via governance proposal.
    #[allow(clippy::too_many_arguments)]
    pub fn set_registry_config_via_governance(
        env: Env,
        governance: Address,
        min_stake: i128,
        required_votes: u32,
        quorum_bps: u32,
        round_duration_secs: u64,
        deregister_cooldown_secs: u64,
    ) -> Result<(), OracleRegistryError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        if min_stake <= 0
            || required_votes == 0
            || quorum_bps == 0
            || quorum_bps > 10_000
            || round_duration_secs == 0
        {
            return Err(OracleRegistryError::InvalidConfig);
        }
        let mut config = Self::load_config(&env)?;
        config.min_stake = min_stake;
        config.required_votes = required_votes;
        config.quorum_bps = quorum_bps;
        config.round_duration_secs = round_duration_secs;
        config.deregister_cooldown_secs = deregister_cooldown_secs;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events()
            .publish((EVT, symbol_short!("gov_cfg")), governance);
        Ok(())
    }

    // #1038: Set quorum tiers via governance proposal.
    pub fn set_quorum_tiers_via_governance(
        env: Env,
        governance: Address,
        tiers: Vec<QuorumTier>,
    ) -> Result<(), OracleRegistryError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        if tiers.is_empty() {
            return Err(OracleRegistryError::InvalidQuorumTiers);
        }
        let mut prev_threshold: Option<i128> = None;
        for i in 0..tiers.len() {
            let tier = tiers.get(i).unwrap();
            if tier.quorum_bps == 0 || tier.quorum_bps > 10_000 {
                return Err(OracleRegistryError::InvalidQuorumTiers);
            }
            if let Some(prev) = prev_threshold {
                if tier.min_invoice_amount <= prev {
                    return Err(OracleRegistryError::InvalidQuorumTiers);
                }
            }
            prev_threshold = Some(tier.min_invoice_amount);
        }
        env.storage().instance().set(&DataKey::QuorumTiers, &tiers);
        env.events()
            .publish((EVT, symbol_short!("gov_quorum")), (governance, tiers.len()));
        Ok(())
    }

    fn require_admin(env: &Env, admin: &Address) -> Result<(), OracleRegistryError> {
        let stored: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(OracleRegistryError::NotInitialized)?;
        if admin != &stored {
            return Err(OracleRegistryError::Unauthorized);
        }
        Ok(())
    }

    fn require_access_control(env: &Env, caller: &Address) -> Result<(), OracleRegistryError> {
        let configured: Address = env
            .storage()
            .instance()
            .get(&DataKey::AccessControl)
            .ok_or(OracleRegistryError::AccessControlNotConfigured)?;
        if caller != &configured {
            return Err(OracleRegistryError::Unauthorized);
        }
        Ok(())
    }

    // #1038: Helper function to verify the caller is the configured governance contract
    fn require_governance(env: &Env, caller: &Address) -> Result<(), OracleRegistryError> {
        let configured: Address = env
            .storage()
            .instance()
            .get(&DataKey::Governance)
            .ok_or(OracleRegistryError::GovernanceNotConfigured)?;
        if caller != &configured {
            return Err(OracleRegistryError::Unauthorized);
        }
        Ok(())
    }

    fn load_config(env: &Env) -> Result<RegistryConfig, OracleRegistryError> {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(OracleRegistryError::NotInitialized)
    }

    /// #957: picks the quorum_bps for `invoice_amount` from the configured
    /// tier schedule. Tiers are stored sorted ascending, so the highest
    /// threshold the amount clears wins; falls back to `default_bps` (the
    /// registry's flat `config.quorum_bps`) if no tiers are configured or the
    /// amount is below every tier's threshold.
    fn resolve_quorum_bps(env: &Env, default_bps: u32, invoice_amount: i128) -> u32 {
        let tiers: Vec<QuorumTier> = env
            .storage()
            .instance()
            .get(&DataKey::QuorumTiers)
            .unwrap_or_else(|| Vec::new(env));
        let mut resolved = default_bps;
        for tier in tiers.iter() {
            if invoice_amount >= tier.min_invoice_amount {
                resolved = tier.quorum_bps;
            } else {
                break;
            }
        }
        resolved
    }

    fn remove_open_round(env: &Env, invoice_id: u64) {
        let mut open_rounds: Vec<u64> = env
            .storage()
            .instance()
            .get(&DataKey::OpenRounds)
            .unwrap_or_else(|| Vec::new(env));
        if let Some(idx) = open_rounds.first_index_of(invoice_id) {
            open_rounds.remove(idx);
            env.storage()
                .instance()
                .set(&DataKey::OpenRounds, &open_rounds);
        }
    }

    fn finalize_on_invoice(
        env: &Env,
        invoice_id: u64,
        approved: bool,
        oracle_hash: &String,
    ) -> Result<(), OracleRegistryError> {
        let invoice_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::InvoiceContract)
            .ok_or(OracleRegistryError::InvoiceContractNotSet)?;
        let reason = if approved {
            String::from_str(env, "consensus approved")
        } else {
            String::from_str(env, "consensus rejected")
        };
        let client = InvoiceContractClient::new(env, &invoice_contract);
        client
            .try_consensus_verify(
                &invoice_id,
                &env.current_contract_address(),
                &approved,
                &reason,
                oracle_hash,
            )
            .map_err(|_| OracleRegistryError::InvoiceCallFailed)?
            .map_err(|_| OracleRegistryError::InvoiceCallFailed)?;
        Ok(())
    }
}
