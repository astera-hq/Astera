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
    pub fn slash_oracle(
        env: Env,
        admin: Address,
        operator: Address,
        bps: u32,
    ) -> Result<(), OracleRegistryError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        if bps == 0 || bps > 10_000 {
            return Err(OracleRegistryError::InvalidBps);
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

        let config = Self::load_config(&env)?;
        if let Some(treasury) = config.treasury {
            let token_client = token::Client::new(&env, &config.stake_token);
            token_client.transfer(&env.current_contract_address(), &treasury, &slash_amount);
        }

        env.events().publish(
            (EVT, symbol_short!("slashed")),
            (operator, bps, slash_amount, admin),
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

        let now = env.ledger().timestamp();
        let round = VerificationRound {
            invoice_id,
            required_votes: config.required_votes,
            total_registered_oracles: active_count,
            votes: Map::new(&env),
            weight_for: 0,
            weight_against: 0,
            total_stake_snapshot: total_stake,
            quorum_bps: config.quorum_bps,
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
        let round_key = DataKey::Round(invoice_id);
        let mut round: VerificationRound = env
            .storage()
            .persistent()
            .get(&round_key)
            .ok_or(OracleRegistryError::RoundNotFound)?;
        if round.status != RoundStatus::Expired {
            return Err(OracleRegistryError::RoundNotExpired);
        }
        round.status = if approved {
            RoundStatus::ConsensusApproved
        } else {
            RoundStatus::ConsensusRejected
        };
        let oracle_hash = round.oracle_hash.clone();
        env.storage().persistent().set(&round_key, &round);
        Self::finalize_on_invoice(&env, invoice_id, approved, &oracle_hash)?;
        env.events().publish(
            (EVT, symbol_short!("fallback")),
            (invoice_id, approved, admin, reason),
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

    fn load_config(env: &Env) -> Result<RegistryConfig, OracleRegistryError> {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(OracleRegistryError::NotInitialized)
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
