#![no_std]

// === AUTHORIZED CALLERS ===
// - Admin: initialize(), set_invoice_contract(), set_config(), pause()/unpause(),
//   admin_resolve_no_quorum() (only once a case has exhausted its committee
//   re-draws without reaching quorum)
// - Invoice contract (the address stored under DataKey::InvoiceContract): open_case()
// - Juror operators: register_juror(), deregister_juror(), commit_vote()/reveal_vote()
//   (own address only)
// - Case parties (claimant/respondent stored on the case): submit_evidence()
// - Anyone: select_jurors(), finalize_case() (permissionless "ticks" gated purely by
//   elapsed deadlines), all read-only view functions
//
// #1043: structured multi-party dispute arbitration. Invoice disputes above a
// value threshold (see `invoice`'s `DisputeValueThreshold` config) are routed
// here instead of being resolved unilaterally by the invoice contract's admin.
// Staked jurors are drawn at random (unweighted — see module docs below) for
// each case, submit a commit-reveal vote once an evidence window closes, and
// on finalization the majority outcome is written back into `invoice` via
// `arbitration_resolve_dispute` (mirrors the `oracle_registry` -> `invoice`
// `consensus_verify` cross-contract pattern this crate is modeled on). A case
// that fails to reach quorum gets one committee re-draw before falling back
// to an admin-resolvable escalation, so a low juror turnout can never brick
// an invoice's dispute permanently.
//
// This is a standalone satellite contract (not folded into `invoice`) to stay
// under Soroban's 204,800-byte wasm deploy limit — `invoice` and `pool` are
// both already close to it (see the `pool`/`secondary_market` split). It has
// no crate dependency on `invoice`; the `DisputeResolution` enum below is a
// structural mirror of `invoice::DisputeResolution` (same variant names),
// matching the `secondary_market` -> `pool` "mirror struct" convention used
// elsewhere in this repo to avoid pulling another contract's compiled object
// code into this one's wasm.
//
// Juror selection uses `env.prng()` (this crate is the first user of Soroban's
// on-chain PRNG anywhere in this repo) purely to pick an UNWEIGHTED committee
// from the active staked pool: a large staker gets exactly one seat's worth of
// selection odds, the same as a juror staked at the minimum, so no single
// staker can buy influence over which cases they sit on. Soroban's PRNG is
// ledger-seeded (public, validator-influenceable) rather than a strong
// randomness beacon — acceptable here because the actual outcome of a case
// isn't decided by the PRNG, only committee membership, and bad-faith voting
// once selected is still deterred economically via the slashing in
// `finalize_case`. Voting within a drawn committee is a plain one-juror-one-
// vote majority (not stake-weighted, unlike `oracle_registry`'s open-round
// voting) since every committee member already cleared the same minimum
// stake bond to be eligible — weighting only matters when *any* staked
// participant can vote in an open round, which isn't the case here.

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, panic_with_error,
    symbol_short, token, Address, Bytes, BytesN, Env, String, Symbol, Vec,
};

/// Structural mirror of `invoice`'s `arbitration_resolve_dispute` entrypoint —
/// see the module-level doc comment for why this crate doesn't depend on the
/// `invoice` crate directly.
#[contractclient(name = "InvoiceContractClient")]
pub trait InvoiceContract {
    fn arbitration_resolve_dispute(
        env: Env,
        arbitration: Address,
        id: u64,
        outcome: DisputeResolution,
    );
}

const LEDGERS_PER_DAY: u32 = 17_280;
const ARBITRATION_TTL: u32 = LEDGERS_PER_DAY * 365;
const DEFAULT_COMMITTEE_SIZE: u32 = 5;
const DEFAULT_QUORUM_FLOOR: u32 = 3;
const DEFAULT_EVIDENCE_WINDOW_SECS: u64 = 3 * 24 * 60 * 60;
const DEFAULT_COMMIT_WINDOW_SECS: u64 = 2 * 24 * 60 * 60;
const DEFAULT_REVEAL_WINDOW_SECS: u64 = 2 * 24 * 60 * 60;
const DEFAULT_DEREGISTER_COOLDOWN_SECS: u64 = 7 * 24 * 60 * 60;
// Minority-on-a-lopsided-vote slash, in bps of stake.
const DEFAULT_SLASH_BPS: u32 = 1_000; // 10%
                                      // Committed-but-never-revealed penalty, in bps of stake — smaller than the
                                      // lopsided-minority slash since non-reveal is presumed carelessness, not
                                      // necessarily bad-faith voting, but must still cost something or a juror
                                      // could commit to every case and simply ghost the ones going against them.
const DEFAULT_NON_REVEAL_SLASH_BPS: u32 = 300; // 3%
                                               // The winning side's share of *revealed* votes must clear this bps for the
                                               // result to count as "reasonable confidence" and trigger minority slashing —
                                               // a bare majority isn't punished, a landslide is.
const DEFAULT_LOPSIDED_CONFIDENCE_BPS: u32 = 6_600; // two-thirds
                                                    // Number of committee re-draws allowed after the first, before a case must
                                                    // be escalated to `admin_resolve_no_quorum`.
const DEFAULT_MAX_RETRIES: u32 = 1;
const MAX_EVIDENCE_ENTRIES: u32 = 20;
const MAX_EVIDENCE_HASH_LEN: u32 = 256;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ArbitrationError {
    AlreadyInitialized = 0,
    NotInitialized = 1,
    Unauthorized = 2,
    ContractPaused = 3,
    InvalidAmount = 4,
    InsufficientStake = 5,
    AlreadyRegistered = 6,
    NotRegistered = 7,
    DeregisterHasPendingCases = 8,
    DeregisterCooldownActive = 9,
    InvalidConfig = 10,
    InvoiceContractNotSet = 11,
    InvoiceCallFailed = 12,
    CaseNotFound = 13,
    /// The case isn't in the phase this action requires (e.g. submitting
    /// evidence on a case that's already past its evidence window, or
    /// committing on a case still in its evidence window).
    InvalidCaseStatus = 14,
    /// The relevant deadline for this action hasn't passed yet.
    WindowNotClosed = 15,
    /// The relevant window for this action has already closed.
    WindowClosed = 16,
    /// `submit_evidence` caller is neither the case's claimant nor respondent.
    InvalidParty = 17,
    NotEnoughActiveJurors = 18,
    NotSelectedJuror = 19,
    AlreadyCommitted = 20,
    NotCommitted = 21,
    AlreadyRevealed = 22,
    /// `sha256(vote || salt)` didn't match the stored commit hash.
    RevealMismatch = 23,
    /// `admin_resolve_no_quorum` called before the case's committee re-draws
    /// were actually exhausted — `select_jurors` should be used instead.
    RetriesNotExhausted = 24,
    CaseNotEscalated = 25,
    /// `select_jurors` called again on a case whose re-draws are already
    /// exhausted — `admin_resolve_no_quorum` should be used instead.
    RetriesExhausted = 26,
    /// `open_case` called with the same address as both claimant and respondent.
    ClaimantCannotBeRespondent = 27,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct JurorInfo {
    pub address: Address,
    pub stake_amount: i128,
    pub stake_token: Address,
    pub is_active: bool,
    pub cases_served: u32,
    pub times_slashed: u32,
    pub non_reveal_strikes: u32,
    pub registered_at: u64,
    /// Set when `deregister_juror` has been called once; the second call
    /// (after `deregister_cooldown_secs` has elapsed) returns the stake and
    /// removes this record entirely. Mirrors `oracle_registry::OracleInfo`.
    pub deregister_requested_at: Option<u64>,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct JurorStats {
    pub address: Address,
    pub stake_amount: i128,
    pub is_active: bool,
    pub cases_served: u32,
    pub times_slashed: u32,
    pub non_reveal_strikes: u32,
    pub registered_at: u64,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct AggregateJurorStats {
    pub total_jurors: u32,
    pub active_jurors: u32,
    pub total_stake: i128,
    pub total_cases_served: u32,
    pub total_slashes: u32,
    pub total_non_reveal_strikes: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PartyRole {
    Claimant,
    Respondent,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct EvidenceEntry {
    pub submitter: Address,
    pub party: PartyRole,
    pub evidence_hash: String,
    pub submitted_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaseStatus {
    EvidenceWindow,
    /// Covers both the commit and reveal sub-phases, distinguished purely by
    /// `commit_deadline`/`reveal_deadline` — no separate on-chain "tick" is
    /// needed between commit and reveal, only the evidence -> commit
    /// transition (`select_jurors`) actually mutates storage.
    CommitReveal,
    Resolved,
    /// Reveal quorum wasn't reached; either retryable via `select_jurors`
    /// again (if `retry_count` hasn't hit the configured max) or terminal,
    /// requiring `admin_resolve_no_quorum`.
    NoQuorumEscalated,
}

/// Structural mirror of `invoice::DisputeResolution` — see module docs.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisputeResolution {
    Pending,
    InFavorOfSME,
    InFavorOfDebtor,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct DisputeCase {
    pub id: u64,
    pub invoice_id: u64,
    pub claimant: Address,
    /// Admin/pool-designated address representing the debtor's side of this
    /// case. `invoice.debtor` is only a `String` today (no wallet, no
    /// `require_auth` capability), so the respondent can't yet be the actual
    /// debtor's own address — this is a documented limitation until Astera
    /// has real debtor wallets, not an oversight.
    pub respondent: Address,
    pub amount: i128,
    pub opened_at: u64,
    pub evidence_deadline: u64,
    pub commit_deadline: u64,
    pub reveal_deadline: u64,
    pub jurors: Vec<Address>,
    pub status: CaseStatus,
    pub resolution: DisputeResolution,
    /// Number of times `select_jurors` has drawn a committee for this case
    /// (1 after the first draw). Both `select_jurors`'s retry gate and
    /// `admin_resolve_no_quorum`'s "retries actually exhausted" gate check
    /// this same counter against `config.max_retries + 1`, so the two paths
    /// can never both think it's their turn.
    pub retry_count: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct JurorVote {
    pub commit_hash: Option<BytesN<32>>,
    pub revealed_vote: Option<bool>,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct ArbitrationConfig {
    pub min_stake: i128,
    pub stake_token: Address,
    pub committee_size: u32,
    pub quorum_floor: u32,
    pub evidence_window_secs: u64,
    pub commit_window_secs: u64,
    pub reveal_window_secs: u64,
    pub deregister_cooldown_secs: u64,
    pub slash_bps: u32,
    pub non_reveal_slash_bps: u32,
    pub lopsided_confidence_bps: u32,
    pub max_retries: u32,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Initialized,
    Paused,
    Config,
    InvoiceContract,
    Juror(Address),
    JurorIds,
    JurorCases(Address),
    CaseCount,
    Case(u64),
    /// Maps an invoice id to its most recently opened case id — lets callers
    /// (the frontend, in particular) go from "this invoice is Disputed" to
    /// "here's the case to look at" without tracking case ids themselves.
    InvoiceCase(u64),
    Evidence(u64),
    Vote(u64, Address),
}

const EVT: Symbol = symbol_short!("ARBTRTN");

fn require_not_paused(env: &Env) {
    if env
        .storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Paused)
        .unwrap_or(false)
    {
        panic_with_error!(env, ArbitrationError::ContractPaused);
    }
}

#[contract]
pub struct ArbitrationContract;

#[contractimpl]
impl ArbitrationContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        invoice_contract: Address,
        stake_token: Address,
        min_stake: i128,
    ) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic_with_error!(&env, ArbitrationError::AlreadyInitialized);
        }
        if min_stake <= 0 {
            panic_with_error!(&env, ArbitrationError::InvalidAmount);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage()
            .instance()
            .set(&DataKey::InvoiceContract, &invoice_contract);
        env.storage().instance().set(
            &DataKey::Config,
            &ArbitrationConfig {
                min_stake,
                stake_token,
                committee_size: DEFAULT_COMMITTEE_SIZE,
                quorum_floor: DEFAULT_QUORUM_FLOOR,
                evidence_window_secs: DEFAULT_EVIDENCE_WINDOW_SECS,
                commit_window_secs: DEFAULT_COMMIT_WINDOW_SECS,
                reveal_window_secs: DEFAULT_REVEAL_WINDOW_SECS,
                deregister_cooldown_secs: DEFAULT_DEREGISTER_COOLDOWN_SECS,
                slash_bps: DEFAULT_SLASH_BPS,
                non_reveal_slash_bps: DEFAULT_NON_REVEAL_SLASH_BPS,
                lopsided_confidence_bps: DEFAULT_LOPSIDED_CONFIDENCE_BPS,
                max_retries: DEFAULT_MAX_RETRIES,
            },
        );
        env.storage()
            .instance()
            .set(&DataKey::JurorIds, &Vec::<Address>::new(&env));
        env.storage().instance().set(&DataKey::CaseCount, &0u64);
        env.storage()
            .instance()
            .extend_ttl(ARBITRATION_TTL, ARBITRATION_TTL);
    }

    pub fn set_invoice_contract(
        env: Env,
        admin: Address,
        invoice_contract: Address,
    ) -> Result<(), ArbitrationError> {
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

    /// Replaces the arbitration parameters wholesale (mirrors
    /// `oracle_registry::set_registry_config`'s validate-then-overwrite
    /// shape) — takes the full `ArbitrationConfig` as one struct argument
    /// rather than one parameter per field: with 12 tunable fields, a
    /// flattened signature would have exceeded Soroban's 10-parameter cap
    /// on contract functions. `stake_token` is preserved from the existing
    /// config regardless of what's passed in `new_config` — swapping the
    /// stake currency out from under already-staked jurors via a config
    /// update would silently orphan their locked balances.
    pub fn set_config(
        env: Env,
        admin: Address,
        new_config: ArbitrationConfig,
    ) -> Result<(), ArbitrationError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        if new_config.min_stake <= 0
            || new_config.committee_size == 0
            || new_config.quorum_floor == 0
            || new_config.quorum_floor > new_config.committee_size
            || new_config.evidence_window_secs == 0
            || new_config.commit_window_secs == 0
            || new_config.reveal_window_secs == 0
            || new_config.slash_bps > 10_000
            || new_config.non_reveal_slash_bps > 10_000
            || new_config.lopsided_confidence_bps == 0
            || new_config.lopsided_confidence_bps > 10_000
        {
            return Err(ArbitrationError::InvalidConfig);
        }
        let mut config = Self::load_config(&env)?;
        let stake_token = config.stake_token.clone();
        config = new_config;
        config.stake_token = stake_token;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events().publish((EVT, symbol_short!("cfg_upd")), admin);
        Ok(())
    }

    pub fn get_config(env: Env) -> Result<ArbitrationConfig, ArbitrationError> {
        Self::load_config(&env)
    }

    pub fn pause(env: Env, admin: Address) -> Result<(), ArbitrationError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        env.storage().instance().set(&DataKey::Paused, &true);
        env.events().publish((EVT, symbol_short!("paused")), admin);
        Ok(())
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), ArbitrationError> {
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

    /// Registers `operator` as a juror, transferring `stake_amount` of the
    /// configured stake token into the contract. Mirrors
    /// `oracle_registry::register_oracle`.
    pub fn register_juror(
        env: Env,
        operator: Address,
        stake_amount: i128,
    ) -> Result<(), ArbitrationError> {
        operator.require_auth();
        require_not_paused(&env);
        let config = Self::load_config(&env)?;
        if stake_amount < config.min_stake {
            return Err(ArbitrationError::InsufficientStake);
        }
        if env
            .storage()
            .persistent()
            .has(&DataKey::Juror(operator.clone()))
        {
            return Err(ArbitrationError::AlreadyRegistered);
        }

        let token_client = token::Client::new(&env, &config.stake_token);
        token_client.transfer(&operator, &env.current_contract_address(), &stake_amount);

        let info = JurorInfo {
            address: operator.clone(),
            stake_amount,
            stake_token: config.stake_token.clone(),
            is_active: true,
            cases_served: 0,
            times_slashed: 0,
            non_reveal_strikes: 0,
            registered_at: env.ledger().timestamp(),
            deregister_requested_at: None,
        };
        let key = DataKey::Juror(operator.clone());
        env.storage().persistent().set(&key, &info);
        env.storage()
            .persistent()
            .extend_ttl(&key, ARBITRATION_TTL, ARBITRATION_TTL);

        let mut ids: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::JurorIds)
            .unwrap_or_else(|| Vec::new(&env));
        if !ids.contains(&operator) {
            ids.push_back(operator.clone());
            env.storage().instance().set(&DataKey::JurorIds, &ids);
        }

        env.events()
            .publish((EVT, symbol_short!("registrd")), (operator, stake_amount));
        Ok(())
    }

    /// Two-phase deregistration, mirrors `oracle_registry::deregister_oracle`.
    /// The first call blocks while the juror has a live (not yet finalized)
    /// commit/reveal assignment on any case; the second, after the cooldown,
    /// refunds the stake and removes the record.
    pub fn deregister_juror(env: Env, operator: Address) -> Result<(), ArbitrationError> {
        operator.require_auth();
        require_not_paused(&env);
        let key = DataKey::Juror(operator.clone());
        let mut info: JurorInfo = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ArbitrationError::NotRegistered)?;
        let config = Self::load_config(&env)?;
        let now = env.ledger().timestamp();

        match info.deregister_requested_at {
            None => {
                if !info.is_active {
                    return Err(ArbitrationError::NotRegistered);
                }
                let case_ids: Vec<u64> = env
                    .storage()
                    .persistent()
                    .get(&DataKey::JurorCases(operator.clone()))
                    .unwrap_or_else(|| Vec::new(&env));
                for case_id in case_ids.iter() {
                    if let Some(case) = env
                        .storage()
                        .persistent()
                        .get::<DataKey, DisputeCase>(&DataKey::Case(case_id))
                    {
                        if case.status == CaseStatus::CommitReveal
                            && case.jurors.contains(&operator)
                        {
                            return Err(ArbitrationError::DeregisterHasPendingCases);
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
                    return Err(ArbitrationError::DeregisterCooldownActive);
                }
                // Checks-effects-interactions: commit the record removal and
                // index update (effects) before the external token transfer
                // (interaction), so this juror can never be double-refunded
                // by re-entering before its own record is gone.
                let stake_amount = info.stake_amount;
                let stake_token = config.stake_token.clone();
                env.storage().persistent().remove(&key);

                let mut ids: Vec<Address> = env
                    .storage()
                    .instance()
                    .get(&DataKey::JurorIds)
                    .unwrap_or_else(|| Vec::new(&env));
                if let Some(idx) = ids.first_index_of(&operator) {
                    ids.remove(idx);
                    env.storage().instance().set(&DataKey::JurorIds, &ids);
                }

                let token_client = token::Client::new(&env, &stake_token);
                token_client.transfer(&env.current_contract_address(), &operator, &stake_amount);

                env.events()
                    .publish((EVT, symbol_short!("dreg_done")), operator);
                Ok(())
            }
        }
    }

    /// Opens a new dispute case. Callable only by the trusted `invoice`
    /// contract (address-pinned, same shape as `invoice::consensus_verify`
    /// pinning `oracle_registry`'s address) — `invoice` calls this from
    /// `raise_dispute` once it decides the dispute's value clears its
    /// configured threshold.
    pub fn open_case(
        env: Env,
        invoice: Address,
        invoice_id: u64,
        claimant: Address,
        respondent: Address,
        amount: i128,
    ) -> Result<u64, ArbitrationError> {
        invoice.require_auth();
        require_not_paused(&env);
        let stored_invoice: Address = env
            .storage()
            .instance()
            .get(&DataKey::InvoiceContract)
            .ok_or(ArbitrationError::InvoiceContractNotSet)?;
        if invoice != stored_invoice {
            return Err(ArbitrationError::Unauthorized);
        }
        if amount <= 0 {
            return Err(ArbitrationError::InvalidAmount);
        }
        if claimant == respondent {
            return Err(ArbitrationError::ClaimantCannotBeRespondent);
        }

        let config = Self::load_config(&env)?;
        let now = env.ledger().timestamp();
        let id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::CaseCount)
            .unwrap_or(0);
        env.storage().instance().set(&DataKey::CaseCount, &(id + 1));

        let case = DisputeCase {
            id,
            invoice_id,
            claimant,
            respondent,
            amount,
            opened_at: now,
            evidence_deadline: now.saturating_add(config.evidence_window_secs),
            commit_deadline: 0,
            reveal_deadline: 0,
            jurors: Vec::new(&env),
            status: CaseStatus::EvidenceWindow,
            resolution: DisputeResolution::Pending,
            retry_count: 0,
        };
        let key = DataKey::Case(id);
        env.storage().persistent().set(&key, &case);
        env.storage()
            .persistent()
            .extend_ttl(&key, ARBITRATION_TTL, ARBITRATION_TTL);
        env.storage()
            .persistent()
            .set(&DataKey::Evidence(id), &Vec::<EvidenceEntry>::new(&env));
        env.storage()
            .persistent()
            .set(&DataKey::InvoiceCase(invoice_id), &id);

        env.events()
            .publish((EVT, symbol_short!("caseopen")), (id, invoice_id, amount));
        Ok(id)
    }

    /// Structured evidence submission — either party may submit multiple
    /// entries during the evidence window, so both sides get a genuine chance
    /// to respond (not just a single opening hash).
    pub fn submit_evidence(
        env: Env,
        case_id: u64,
        submitter: Address,
        evidence_hash: String,
    ) -> Result<(), ArbitrationError> {
        submitter.require_auth();
        require_not_paused(&env);
        let case: DisputeCase = env
            .storage()
            .persistent()
            .get(&DataKey::Case(case_id))
            .ok_or(ArbitrationError::CaseNotFound)?;
        if case.status != CaseStatus::EvidenceWindow {
            return Err(ArbitrationError::InvalidCaseStatus);
        }
        if env.ledger().timestamp() > case.evidence_deadline {
            return Err(ArbitrationError::WindowClosed);
        }
        let party = if submitter == case.claimant {
            PartyRole::Claimant
        } else if submitter == case.respondent {
            PartyRole::Respondent
        } else {
            return Err(ArbitrationError::InvalidParty);
        };
        if evidence_hash.len() > MAX_EVIDENCE_HASH_LEN {
            return Err(ArbitrationError::EvidenceHashTooLong);
        }

        let ev_key = DataKey::Evidence(case_id);
        let mut entries: Vec<EvidenceEntry> = env
            .storage()
            .persistent()
            .get(&ev_key)
            .unwrap_or_else(|| Vec::new(&env));
        if entries.len() >= MAX_EVIDENCE_ENTRIES {
            return Err(ArbitrationError::EvidenceLimitReached);
        }
        entries.push_back(EvidenceEntry {
            submitter: submitter.clone(),
            party,
            evidence_hash,
            submitted_at: env.ledger().timestamp(),
        });
        env.storage().persistent().set(&ev_key, &entries);

        env.events()
            .publish((EVT, symbol_short!("evidence")), (case_id, submitter));
        Ok(())
    }

    /// Permissionless tick: once the evidence window has closed (first draw)
    /// or a prior committee failed to reach quorum and a re-draw is still
    /// available (`retry_count` hasn't hit `config.max_retries + 1`), draws
    /// an unweighted random committee of `config.committee_size` from the
    /// active juror pool and opens the commit/reveal window.
    pub fn select_jurors(env: Env, case_id: u64) -> Result<(), ArbitrationError> {
        require_not_paused(&env);
        let mut case: DisputeCase = env
            .storage()
            .persistent()
            .get(&DataKey::Case(case_id))
            .ok_or(ArbitrationError::CaseNotFound)?;
        let config = Self::load_config(&env)?;
        let max_draws = config.max_retries.saturating_add(1);
        let now = env.ledger().timestamp();

        match case.status {
            CaseStatus::EvidenceWindow => {
                if now <= case.evidence_deadline {
                    return Err(ArbitrationError::WindowNotClosed);
                }
            }
            CaseStatus::NoQuorumEscalated => {
                if case.retry_count >= max_draws {
                    return Err(ArbitrationError::RetriesExhausted);
                }
            }
            _ => return Err(ArbitrationError::InvalidCaseStatus),
        }

        let ids: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::JurorIds)
            .unwrap_or_else(|| Vec::new(&env));
        let mut active: Vec<Address> = Vec::new(&env);
        for id in ids.iter() {
            if let Some(info) = env
                .storage()
                .persistent()
                .get::<DataKey, JurorInfo>(&DataKey::Juror(id.clone()))
            {
                if info.is_active
                    && info.deregister_requested_at.is_none()
                    && info.stake_amount >= config.min_stake
                {
                    active.push_back(id);
                }
            }
        }
        if active.len() < config.committee_size {
            return Err(ArbitrationError::NotEnoughActiveJurors);
        }

        // Unweighted draw: shuffle the active pool and take the first
        // `committee_size` — every active juror has equal selection odds
        // regardless of stake size (see module docs).
        env.prng().shuffle(&mut active);
        let mut committee: Vec<Address> = Vec::new(&env);
        for i in 0..config.committee_size {
            committee.push_back(active.get(i).unwrap());
        }

        case.jurors = committee.clone();
        case.status = CaseStatus::CommitReveal;
        case.commit_deadline = now.saturating_add(config.commit_window_secs);
        case.reveal_deadline = case
            .commit_deadline
            .saturating_add(config.reveal_window_secs);
        case.retry_count += 1;
        env.storage()
            .persistent()
            .set(&DataKey::Case(case_id), &case);

        for juror in committee.iter() {
            env.storage().persistent().set(
                &DataKey::Vote(case_id, juror.clone()),
                &JurorVote {
                    commit_hash: None,
                    revealed_vote: None,
                },
            );
            Self::append_juror_case(&env, juror.clone(), case_id);
        }

        env.events().publish(
            (EVT, symbol_short!("jurorsel")),
            (case_id, config.committee_size, case.retry_count),
        );
        Ok(())
    }

    /// Commits `commit_hash = sha256(vote_byte || salt)` for a case this
    /// juror was selected for. Must happen before `commit_deadline`.
    pub fn commit_vote(
        env: Env,
        case_id: u64,
        juror: Address,
        commit_hash: BytesN<32>,
    ) -> Result<(), ArbitrationError> {
        juror.require_auth();
        require_not_paused(&env);
        let case: DisputeCase = env
            .storage()
            .persistent()
            .get(&DataKey::Case(case_id))
            .ok_or(ArbitrationError::CaseNotFound)?;
        if case.status != CaseStatus::CommitReveal {
            return Err(ArbitrationError::InvalidCaseStatus);
        }
        if env.ledger().timestamp() > case.commit_deadline {
            return Err(ArbitrationError::WindowClosed);
        }
        if !case.jurors.contains(&juror) {
            return Err(ArbitrationError::NotSelectedJuror);
        }
        let vote_key = DataKey::Vote(case_id, juror.clone());
        let mut vote: JurorVote = env
            .storage()
            .persistent()
            .get(&vote_key)
            .ok_or(ArbitrationError::NotSelectedJuror)?;
        if vote.commit_hash.is_some() {
            return Err(ArbitrationError::AlreadyCommitted);
        }
        vote.commit_hash = Some(commit_hash);
        env.storage().persistent().set(&vote_key, &vote);

        env.events()
            .publish((EVT, symbol_short!("commit")), (case_id, juror));
        Ok(())
    }

    /// Reveals a previously committed vote. Only valid strictly after
    /// `commit_deadline` and at/before `reveal_deadline`, and only if
    /// `sha256(vote_choice_byte || salt) == commit_hash` — this is what makes
    /// the pre-reveal vote content structurally undiscoverable from any
    /// on-chain state (only the hash is ever stored beforehand).
    pub fn reveal_vote(
        env: Env,
        case_id: u64,
        juror: Address,
        vote_choice: bool,
        salt: BytesN<32>,
    ) -> Result<(), ArbitrationError> {
        juror.require_auth();
        require_not_paused(&env);
        let case: DisputeCase = env
            .storage()
            .persistent()
            .get(&DataKey::Case(case_id))
            .ok_or(ArbitrationError::CaseNotFound)?;
        if case.status != CaseStatus::CommitReveal {
            return Err(ArbitrationError::InvalidCaseStatus);
        }
        let now = env.ledger().timestamp();
        if now <= case.commit_deadline {
            return Err(ArbitrationError::WindowNotClosed);
        }
        if now > case.reveal_deadline {
            return Err(ArbitrationError::WindowClosed);
        }
        if !case.jurors.contains(&juror) {
            return Err(ArbitrationError::NotSelectedJuror);
        }
        let vote_key = DataKey::Vote(case_id, juror.clone());
        let mut vote: JurorVote = env
            .storage()
            .persistent()
            .get(&vote_key)
            .ok_or(ArbitrationError::NotSelectedJuror)?;
        let commit_hash = vote
            .commit_hash
            .clone()
            .ok_or(ArbitrationError::NotCommitted)?;
        if vote.revealed_vote.is_some() {
            return Err(ArbitrationError::AlreadyRevealed);
        }

        let mut preimage = Bytes::new(&env);
        preimage.push_back(if vote_choice { 1u8 } else { 0u8 });
        preimage.append(&Bytes::from(salt));
        let computed = env.crypto().sha256(&preimage).to_bytes();
        if computed != commit_hash {
            return Err(ArbitrationError::RevealMismatch);
        }

        vote.revealed_vote = Some(vote_choice);
        env.storage().persistent().set(&vote_key, &vote);

        env.events()
            .publish((EVT, symbol_short!("reveal")), (case_id, juror));
        Ok(())
    }

    /// Permissionless tick, callable once `reveal_deadline` has passed.
    /// Tallies revealed votes (`true` = in favor of the debtor, `false` = in
    /// favor of the SME — see `DisputeResolution`), settles juror stakes, and
    /// — on a quorum-reached outcome — calls back into `invoice` to apply the
    /// resolution. On a no-quorum outcome, either flags the case for a
    /// committee re-draw or, if re-draws are exhausted, for admin fallback.
    pub fn finalize_case(env: Env, case_id: u64) -> Result<(), ArbitrationError> {
        require_not_paused(&env);
        let mut case: DisputeCase = env
            .storage()
            .persistent()
            .get(&DataKey::Case(case_id))
            .ok_or(ArbitrationError::CaseNotFound)?;
        if case.status != CaseStatus::CommitReveal {
            return Err(ArbitrationError::InvalidCaseStatus);
        }
        if env.ledger().timestamp() <= case.reveal_deadline {
            return Err(ArbitrationError::WindowNotClosed);
        }
        let config = Self::load_config(&env)?;

        let mut favor_debtor: u32 = 0;
        let mut favor_sme: u32 = 0;
        let mut revealed: Vec<(Address, bool)> = Vec::new(&env);
        let mut non_revealers: Vec<Address> = Vec::new(&env);
        for juror in case.jurors.iter() {
            let vote: JurorVote = env
                .storage()
                .persistent()
                .get(&DataKey::Vote(case_id, juror.clone()))
                .unwrap_or(JurorVote {
                    commit_hash: None,
                    revealed_vote: None,
                });
            match vote.revealed_vote {
                Some(choice) => {
                    revealed.push_back((juror.clone(), choice));
                    if choice {
                        favor_debtor += 1;
                    } else {
                        favor_sme += 1;
                    }
                }
                None => non_revealers.push_back(juror.clone()),
            }
        }
        let reveal_count = revealed.len();

        // Every non-revealer pays a small penalty regardless of whether this
        // round reaches quorum — ghosting a committed vote is never free.
        let mut settlement_pool =
            Self::apply_non_reveal_penalties(&env, &non_revealers, &config, case_id);

        if reveal_count < config.quorum_floor {
            case.status = CaseStatus::NoQuorumEscalated;
            env.storage()
                .persistent()
                .set(&DataKey::Case(case_id), &case);
            env.events().publish(
                (EVT, symbol_short!("noquorum")),
                (case_id, case.retry_count),
            );
            return Ok(());
        }

        let outcome_favor_debtor = favor_debtor > favor_sme;
        let resolution = if outcome_favor_debtor {
            DisputeResolution::InFavorOfDebtor
        } else {
            DisputeResolution::InFavorOfSME
        };

        let winning_count = if outcome_favor_debtor {
            favor_debtor
        } else {
            favor_sme
        };
        let winning_bps = (winning_count * 10_000) / reveal_count;
        let lopsided = winning_bps >= config.lopsided_confidence_bps;

        let mut majority: Vec<Address> = Vec::new(&env);
        for (juror, choice) in revealed.iter() {
            if choice == outcome_favor_debtor {
                majority.push_back(juror.clone());
                Self::bump_cases_served(&env, &juror);
            } else if lopsided {
                settlement_pool += Self::slash_juror(&env, &juror, config.slash_bps, case_id);
            }
        }
        Self::reward_majority(&env, &majority, settlement_pool);

        case.status = CaseStatus::Resolved;
        case.resolution = resolution.clone();
        env.storage()
            .persistent()
            .set(&DataKey::Case(case_id), &case);

        // Best-effort — see `call_invoice_resolve`'s doc comment for why a
        // failed sync must never unwind the settlement above.
        let synced = Self::call_invoice_resolve(&env, case.invoice_id, resolution);

        env.events().publish(
            (EVT, symbol_short!("resolved")),
            (case_id, case.invoice_id, outcome_favor_debtor, synced),
        );
        Ok(())
    }

    /// Admin fallback, only usable once a case's committee re-draws are
    /// actually exhausted (`retry_count >= config.max_retries + 1`) — the
    /// escape hatch that keeps chronically low juror turnout from bricking a
    /// dispute forever, without letting an admin short-circuit a case that
    /// could still legitimately be re-drawn.
    pub fn admin_resolve_no_quorum(
        env: Env,
        admin: Address,
        case_id: u64,
        resolution: DisputeResolution,
    ) -> Result<(), ArbitrationError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        if resolution == DisputeResolution::Pending {
            return Err(ArbitrationError::InvalidConfig);
        }
        let mut case: DisputeCase = env
            .storage()
            .persistent()
            .get(&DataKey::Case(case_id))
            .ok_or(ArbitrationError::CaseNotFound)?;
        if case.status != CaseStatus::NoQuorumEscalated {
            return Err(ArbitrationError::CaseNotEscalated);
        }
        let config = Self::load_config(&env)?;
        let max_draws = config.max_retries.saturating_add(1);
        if case.retry_count < max_draws {
            return Err(ArbitrationError::RetriesNotExhausted);
        }

        case.status = CaseStatus::Resolved;
        case.resolution = resolution.clone();
        env.storage()
            .persistent()
            .set(&DataKey::Case(case_id), &case);

        // Best-effort — see `call_invoice_resolve`'s doc comment.
        let synced = Self::call_invoice_resolve(&env, case.invoice_id, resolution);

        env.events()
            .publish((EVT, symbol_short!("adminres")), (case_id, admin, synced));
        Ok(())
    }

    pub fn get_case(env: Env, case_id: u64) -> Option<DisputeCase> {
        env.storage().persistent().get(&DataKey::Case(case_id))
    }

    /// Looks up the most recently opened case for `invoice_id` — lets a
    /// caller that only knows "this invoice is Disputed" find the case to
    /// display without tracking case ids itself.
    pub fn get_case_by_invoice(env: Env, invoice_id: u64) -> Option<DisputeCase> {
        let case_id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::InvoiceCase(invoice_id))?;
        env.storage().persistent().get(&DataKey::Case(case_id))
    }

    pub fn get_evidence(env: Env, case_id: u64) -> Vec<EvidenceEntry> {
        env.storage()
            .persistent()
            .get(&DataKey::Evidence(case_id))
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_vote(env: Env, case_id: u64, juror: Address) -> Option<JurorVote> {
        env.storage()
            .persistent()
            .get(&DataKey::Vote(case_id, juror))
    }

    pub fn get_juror(env: Env, operator: Address) -> Option<JurorInfo> {
        env.storage().persistent().get(&DataKey::Juror(operator))
    }

    pub fn list_active_jurors(env: Env) -> Vec<Address> {
        let ids: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::JurorIds)
            .unwrap_or_else(|| Vec::new(&env));
        let mut active = Vec::new(&env);
        for id in ids.iter() {
            if let Some(info) = env
                .storage()
                .persistent()
                .get::<DataKey, JurorInfo>(&DataKey::Juror(id.clone()))
            {
                if info.is_active {
                    active.push_back(id);
                }
            }
        }
        active
    }

    pub fn get_juror_cases(env: Env, operator: Address) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::JurorCases(operator))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Lists all cases with the specified status. Useful for finding cases
    /// that need specific actions, e.g., NoQuorumEscalated cases needing
    /// admin_resolve_no_quorum without replaying events off-chain.
    pub fn list_cases_by_status(env: Env, status: CaseStatus) -> Vec<u64> {
        let case_count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::CaseCount)
            .unwrap_or(0);

        let mut matching_cases = Vec::new(&env);

        // Iterate through all case IDs and check their status
        for case_id in 0..case_count {
            if let Some(case) = env
                .storage()
                .persistent()
                .get::<DataKey, DisputeCase>(&DataKey::Case(case_id))
            {
                if case.status == status {
                    matching_cases.push_back(case_id);
                }
            }
        }

        matching_cases
    }

    /// Returns aggregate statistics across all jurors for reputation/leaderboard
    /// display. Provides totals for stake, cases served, slashes, and non-reveal
    /// strikes without requiring individual juror fetches.
    pub fn get_aggregate_juror_stats(env: Env) -> AggregateJurorStats {
        let ids: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::JurorIds)
            .unwrap_or_else(|| Vec::new(&env));

        let mut total_jurors = 0u32;
        let mut active_jurors = 0u32;
        let mut total_stake = 0i128;
        let mut total_cases_served = 0u32;
        let mut total_slashes = 0u32;
        let mut total_non_reveal_strikes = 0u32;

        for id in ids.iter() {
            if let Some(info) = env
                .storage()
                .persistent()
                .get::<DataKey, JurorInfo>(&DataKey::Juror(id.clone()))
            {
                total_jurors += 1;

                if info.is_active {
                    active_jurors += 1;
                }

                total_stake += info.stake_amount;
                total_cases_served += info.cases_served;
                total_slashes += info.times_slashed;
                total_non_reveal_strikes += info.non_reveal_strikes;
            }
        }

        AggregateJurorStats {
            total_jurors,
            active_jurors,
            total_stake,
            total_cases_served,
            total_slashes,
            total_non_reveal_strikes,
        }
    }

    /// Returns individual juror statistics for all jurors, suitable for
    /// leaderboard display. More efficient than fetching each juror individually.
    pub fn get_all_juror_stats(env: Env) -> Vec<JurorStats> {
        let ids: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::JurorIds)
            .unwrap_or_else(|| Vec::new(&env));

        let mut stats = Vec::new(&env);

        for id in ids.iter() {
            if let Some(info) = env
                .storage()
                .persistent()
                .get::<DataKey, JurorInfo>(&DataKey::Juror(id.clone()))
            {
                stats.push_back(JurorStats {
                    address: info.address,
                    stake_amount: info.stake_amount,
                    is_active: info.is_active,
                    cases_served: info.cases_served,
                    times_slashed: info.times_slashed,
                    non_reveal_strikes: info.non_reveal_strikes,
                    registered_at: info.registered_at,
                });
            }
        }

        stats
    }

    fn append_juror_case(env: &Env, juror: Address, case_id: u64) {
        let key = DataKey::JurorCases(juror);
        let mut ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env));
        if !ids.contains(case_id) {
            ids.push_back(case_id);
            env.storage().persistent().set(&key, &ids);
            env.storage()
                .persistent()
                .extend_ttl(&key, ARBITRATION_TTL, ARBITRATION_TTL);
        }
    }

    fn bump_cases_served(env: &Env, juror: &Address) {
        let key = DataKey::Juror(juror.clone());
        if let Some(mut info) = env.storage().persistent().get::<DataKey, JurorInfo>(&key) {
            info.cases_served += 1;
            env.storage().persistent().set(&key, &info);
        }
    }

    /// Slashes `bps` of `juror`'s stake, citing `case_id` as the on-chain
    /// evidence (mirrors `oracle_registry::slash_oracle`'s round-citation
    /// requirement, but automatic/deterministic here rather than an admin's
    /// subjective call). Returns the slashed amount so the caller can fold it
    /// into the settlement pool redistributed to the majority.
    fn slash_juror(env: &Env, juror: &Address, bps: u32, case_id: u64) -> i128 {
        let key = DataKey::Juror(juror.clone());
        let Some(mut info) = env.storage().persistent().get::<DataKey, JurorInfo>(&key) else {
            return 0;
        };
        let amount = (info.stake_amount * bps as i128) / 10_000;
        info.stake_amount -= amount;
        info.times_slashed += 1;
        env.storage().persistent().set(&key, &info);
        env.events().publish(
            (EVT, symbol_short!("slashed")),
            (juror.clone(), bps, amount, case_id),
        );
        amount
    }

    fn apply_non_reveal_penalties(
        env: &Env,
        non_revealers: &Vec<Address>,
        config: &ArbitrationConfig,
        case_id: u64,
    ) -> i128 {
        let mut total = 0i128;
        for juror in non_revealers.iter() {
            let key = DataKey::Juror(juror.clone());
            if let Some(mut info) = env.storage().persistent().get::<DataKey, JurorInfo>(&key) {
                info.non_reveal_strikes += 1;
                env.storage().persistent().set(&key, &info);
            }
            total += Self::slash_juror(env, &juror, config.non_reveal_slash_bps, case_id);
        }
        total
    }

    /// Redistributes `pool` pro-rata across `majority` by each juror's
    /// current stake share of the majority's total stake. Self-funding — the
    /// slashed stake never leaves the contract, it's just reassigned, so no
    /// new escrow/fee path into `invoice`/`pool` is needed.
    fn reward_majority(env: &Env, majority: &Vec<Address>, pool: i128) {
        if pool <= 0 || majority.is_empty() {
            return;
        }
        let mut total_stake: i128 = 0;
        for juror in majority.iter() {
            if let Some(info) = env
                .storage()
                .persistent()
                .get::<DataKey, JurorInfo>(&DataKey::Juror(juror.clone()))
            {
                total_stake += info.stake_amount;
            }
        }
        if total_stake <= 0 {
            return;
        }
        for juror in majority.iter() {
            let key = DataKey::Juror(juror.clone());
            if let Some(mut info) = env.storage().persistent().get::<DataKey, JurorInfo>(&key) {
                let share = (pool * info.stake_amount) / total_stake;
                info.stake_amount += share;
                env.storage().persistent().set(&key, &info);
            }
        }
    }

    /// Best-effort callback into `invoice` — deliberately does NOT propagate
    /// failure up to the caller. `invoice`'s own `resolve_default_dispute`
    /// has an independent deadman's-switch override that can resolve an
    /// above-threshold dispute unilaterally if arbitration stalls; if that
    /// fires *while this case is still in flight*, `invoice` will reject
    /// this callback (`DisputeAlreadyResolved`) once the case does finalize.
    /// If that rejection aborted this whole function (as it did before this
    /// fix, via `?`), `finalize_case`/`admin_resolve_no_quorum` would revert
    /// entirely — including the `case.status = Resolved` write — leaving the
    /// case stuck in `CommitReveal` forever and its jurors permanently
    /// unable to `deregister_juror` (which blocks while a case they're on is
    /// still `CommitReveal`). Settling the case locally regardless of
    /// whether `invoice` accepted the sync keeps arbitration's own
    /// bookkeeping (and juror stake liquidity) from ever depending on
    /// invoice-side state it doesn't control. Returns whether the sync
    /// actually landed, for the emitted event.
    fn call_invoice_resolve(env: &Env, invoice_id: u64, resolution: DisputeResolution) -> bool {
        let Some(invoice_contract) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::InvoiceContract)
        else {
            return false;
        };
        let client = InvoiceContractClient::new(env, &invoice_contract);
        client
            .try_arbitration_resolve_dispute(
                &env.current_contract_address(),
                &invoice_id,
                &resolution,
            )
            .is_ok_and(|inner| inner.is_ok())
    }

    fn require_admin(env: &Env, admin: &Address) -> Result<(), ArbitrationError> {
        let stored: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ArbitrationError::NotInitialized)?;
        if admin != &stored {
            return Err(ArbitrationError::Unauthorized);
        }
        Ok(())
    }

    fn load_config(env: &Env) -> Result<ArbitrationConfig, ArbitrationError> {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(ArbitrationError::NotInitialized)
    }
}
