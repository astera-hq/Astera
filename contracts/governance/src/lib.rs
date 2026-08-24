#![no_std]
#![allow(clippy::too_many_arguments)]

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, symbol_short, Address,
    Env, String, Symbol, Vec,
};

const EVT: Symbol = symbol_short!("gov");
const MIN_VOTING_PERIOD_SECS: u64 = 86_400;
const DEFAULT_VOTING_PERIOD_SECS: u64 = 7 * 86_400;
const DEFAULT_EXECUTION_DELAY_SECS: u64 = 48 * 3_600;
const DEFAULT_QUORUM_BPS: u32 = 1_000;
const DEFAULT_PASS_BPS: u32 = 6_000;
/// #933: default quorum for parameter-change proposals (10%).
const DEFAULT_PARAMETER_QUORUM_BPS: u32 = 1_000;
/// #933: default quorum for treasury proposals (20%).
const DEFAULT_TREASURY_QUORUM_BPS: u32 = 2_000;
/// #933: default quorum for critical proposals e.g. admin changes (50%).
const DEFAULT_CRITICAL_QUORUM_BPS: u32 = 5_000;
/// A passed proposal not executed within this window of passing expires and
/// can no longer be executed, so a stale approval can't be enacted long after
/// the conditions that justified it have changed.
const EXECUTION_EXPIRY_SECS: u64 = 7 * 86_400;

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ProposalStatus {
    Active,
    Passed,
    Rejected,
    Executed,
    Cancelled,
    Expired,
}

/// #933: Proposal severity. Parameter tweaks need less consensus than critical
/// actions (admin changes, upgrades). Quorum is resolved per category.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProposalCategory {
    /// Fee / parameter adjustments — lower quorum bar.
    ParameterChange,
    /// Treasury moves and fund allocation.
    Treasury,
    /// Admin, upgrades, and other high-impact actions — highest quorum.
    Critical,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Proposal {
    pub id: u64,
    pub proposer: Address,
    pub description: String,
    pub target_contract: Address,
    pub function_name: String,
    pub calldata: String,
    pub votes_for: i128,
    pub votes_against: i128,
    pub status: ProposalStatus,
    pub created_at: u64,
    pub voting_ends_at: u64,
    pub execution_delay: u64,
    /// Total share supply snapshotted at proposal creation. Quorum and pass-threshold
    /// calculations always use this value so that post-creation minting cannot
    /// retroactively suppress a proposal that had already reached quorum.
    pub snapshot_supply: i128,
    /// Ledger timestamp at which the proposal transitioned to `Passed`, or 0 if
    /// it has not (yet) passed. Used to enforce `EXECUTION_EXPIRY_SECS`.
    pub passed_at: u64,
    /// #933: category chosen at creation; drives which quorum tier applies.
    pub category: ProposalCategory,
    /// #933: quorum_bps snapshotted at creation so mid-vote admin changes to
    /// category quorums cannot retarget an in-flight proposal.
    pub quorum_bps: u32,
    /// #933: pass_bps snapshotted at creation (same freeze rationale as quorum).
    pub pass_bps: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct GovernanceConfig {
    pub admin: Address,
    pub share_token: Address,
    pub voting_period_secs: u64,
    /// Default / ParameterChange quorum (kept for backward-compatible get_config).
    pub quorum_bps: u32,
    pub pass_bps: u32,
    pub execution_delay_secs: u64,
    /// #931: absolute minimum share balance required to create a proposal.
    /// Must be > 0 so spam from zero-balance addresses is blocked.
    pub min_share_balance: i128,
    /// #933: treasury-category quorum in basis points.
    pub treasury_quorum_bps: u32,
    /// #933: critical-category quorum in basis points.
    pub critical_quorum_bps: u32,
}

#[contracttype]
pub enum DataKey {
    Config,
    Proposal(u64),
    ProposalCount,
    Vote(u64, Address),
    Initialized,
    // #1042: multisig trust anchor. Additive — untouched, this stays unset
    // and every admin-gated entrypoint above works exactly as before.
    AccessControl,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum GovernanceError {
    NotInitialized = 1,
    ProposalNotFound = 2,
    ProposalInactive = 3,
    AlreadyVoted = 4,
    InsufficientShareBalance = 5,
    VotingPeriodActive = 6,
    TimelockActive = 7,
    QuorumNotMet = 8,
    InvalidProposalState = 9,
    Unauthorized = 10,
    ProposalExpired = 11,
    InvalidConfig = 12,
    // #1042: a `*_via_ac` entrypoint was called but no `access_control`
    // contract has been configured via `set_access_control` yet.
    AccessControlNotConfigured = 13,
}

type GovernanceResult<T> = Result<T, GovernanceError>;

#[contractclient(name = "ShareTokenClient")]
pub trait ShareTokenContract {
    fn balance(env: Env, id: Address) -> i128;
    fn total_supply(env: Env) -> i128;
    fn balance_at(env: Env, id: Address, timestamp: u64) -> i128;
}

fn load_config(env: &Env) -> GovernanceResult<GovernanceConfig> {
    env.storage()
        .instance()
        .get(&DataKey::Config)
        .ok_or(GovernanceError::NotInitialized)
}

fn validate_bps(quorum_bps: u32, pass_bps: u32) -> GovernanceResult<()> {
    if quorum_bps == 0 || quorum_bps > 10_000 {
        return Err(GovernanceError::InvalidConfig);
    }
    if pass_bps <= 5_000 || pass_bps > 10_000 {
        return Err(GovernanceError::InvalidConfig);
    }
    Ok(())
}

/// #933: resolve the quorum tier for a category from live config.
fn quorum_for_category(config: &GovernanceConfig, category: &ProposalCategory) -> u32 {
    match category {
        ProposalCategory::ParameterChange => config.quorum_bps,
        ProposalCategory::Treasury => config.treasury_quorum_bps,
        ProposalCategory::Critical => config.critical_quorum_bps,
    }
}

fn require_access_control(env: &Env, caller: &Address) -> GovernanceResult<()> {
    let configured: Address = env
        .storage()
        .instance()
        .get(&DataKey::AccessControl)
        .ok_or(GovernanceError::AccessControlNotConfigured)?;
    if caller != &configured {
        return Err(GovernanceError::Unauthorized);
    }
    Ok(())
}

/// #1042: decodes access_control's discriminant-encoded category (see
/// `ActionPayload::SetCategoryQuorum` in access_control/src/lib.rs) —
/// 0=ParameterChange, 1=Treasury, 2=Critical.
fn category_from_discriminant(discriminant: u32) -> GovernanceResult<ProposalCategory> {
    match discriminant {
        0 => Ok(ProposalCategory::ParameterChange),
        1 => Ok(ProposalCategory::Treasury),
        2 => Ok(ProposalCategory::Critical),
        _ => Err(GovernanceError::InvalidConfig),
    }
}

/// Voting weight is the holder's share balance at the moment the proposal was
/// created (`snapshot_at`), not their balance at vote time — otherwise shares
/// acquired mid-vote (or borrowed just long enough to vote) would inflate
/// voting power beyond what backed the proposal when it was created.
fn proposal_weight(env: &Env, share_token: &Address, voter: &Address, snapshot_at: u64) -> i128 {
    ShareTokenClient::new(env, share_token).balance_at(voter, &snapshot_at)
}

fn finalize_proposal(env: &Env, proposal: &mut Proposal) -> GovernanceResult<()> {
    if proposal.status != ProposalStatus::Active {
        return Ok(());
    }

    let snapshot_supply = proposal.snapshot_supply;
    if snapshot_supply <= 0 {
        proposal.status = ProposalStatus::Rejected;
        return Ok(());
    }

    // #933: use quorum/pass snapshotted onto the proposal at creation.
    let total_votes = proposal.votes_for + proposal.votes_against;
    let quorum = (snapshot_supply * proposal.quorum_bps as i128) / 10_000i128;
    if total_votes < quorum {
        proposal.status = ProposalStatus::Rejected;
        return Err(GovernanceError::QuorumNotMet);
    }

    if proposal.votes_for * 10_000i128 >= total_votes * proposal.pass_bps as i128 {
        proposal.status = ProposalStatus::Passed;
        // Anchored to voting_ends_at (not "now") so a delayed finalization
        // call can't push back the execution-expiry window.
        proposal.passed_at = proposal.voting_ends_at;
        env.events()
            .publish((EVT, symbol_short!("passed")), proposal.id);
    } else {
        proposal.status = ProposalStatus::Rejected;
    }

    Ok(())
}

/// Transitions a `Passed` proposal to `Expired` once `EXECUTION_EXPIRY_SECS`
/// has elapsed since it passed. Returns true if the proposal is (now) expired.
fn mark_expired_if_due(env: &Env, proposal: &mut Proposal) -> bool {
    if proposal.status == ProposalStatus::Passed
        && env.ledger().timestamp() > proposal.passed_at.saturating_add(EXECUTION_EXPIRY_SECS)
    {
        proposal.status = ProposalStatus::Expired;
        true
    } else {
        proposal.status == ProposalStatus::Expired
    }
}

#[contract]
pub struct Governance;

#[contractimpl]
impl Governance {
    pub fn initialize(
        env: Env,
        admin: Address,
        share_token: Address,
        voting_period_secs: u64,
        quorum_bps: u32,
        pass_bps: u32,
        execution_delay_secs: u64,
        min_share_balance: i128,
    ) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!("already initialized");
        }

        if quorum_bps == 0 || quorum_bps > 10_000 {
            panic!("invalid quorum");
        }
        if pass_bps <= 5_000 || pass_bps > 10_000 {
            panic!("invalid threshold");
        }
        if voting_period_secs > 0 && voting_period_secs < MIN_VOTING_PERIOD_SECS {
            panic!("voting period too short");
        }
        // #931: proposal creation requires a positive stake threshold so any
        // address without holdings cannot spam list_proposals.
        if min_share_balance <= 0 {
            panic!("min_share_balance must be positive");
        }

        let config = GovernanceConfig {
            admin: admin.clone(),
            share_token,
            voting_period_secs: if voting_period_secs == 0 {
                DEFAULT_VOTING_PERIOD_SECS
            } else {
                voting_period_secs
            },
            quorum_bps: if quorum_bps == 0 {
                DEFAULT_QUORUM_BPS
            } else {
                quorum_bps
            },
            pass_bps: if pass_bps == 0 {
                DEFAULT_PASS_BPS
            } else {
                pass_bps
            },
            execution_delay_secs: if execution_delay_secs == 0 {
                DEFAULT_EXECUTION_DELAY_SECS
            } else {
                execution_delay_secs
            },
            min_share_balance,
            // #933: tiered defaults (parameter uses quorum_bps above).
            treasury_quorum_bps: DEFAULT_TREASURY_QUORUM_BPS,
            critical_quorum_bps: DEFAULT_CRITICAL_QUORUM_BPS,
        };
        let _ = DEFAULT_PARAMETER_QUORUM_BPS;

        env.storage().instance().set(&DataKey::Config, &config);
        env.storage().instance().set(&DataKey::ProposalCount, &0u64);
        env.storage().instance().set(&DataKey::Initialized, &true);
    }

    /// #931 / #933: create a proposal. Caller must hold at least
    /// `min_share_balance` shares (proposal threshold). Category selects the
    /// quorum tier, which is snapshotted onto the proposal.
    pub fn create_proposal(
        env: Env,
        proposer: Address,
        description: String,
        target_contract: Address,
        function_name: String,
        calldata: String,
        category: ProposalCategory,
    ) -> Result<u64, GovernanceError> {
        proposer.require_auth();
        let config = load_config(&env)?;
        let now = env.ledger().timestamp();
        let balance = proposal_weight(&env, &config.share_token, &proposer, now);
        // #931: absolute proposal threshold — reject zero/under-threshold stake.
        if balance < config.min_share_balance {
            return Err(GovernanceError::InsufficientShareBalance);
        }

        let snapshot_supply = ShareTokenClient::new(&env, &config.share_token).total_supply();
        // #931: quorum-eligibility — proposer must hold a positive stake against
        // a non-zero supply so empty/spam proposals cannot open governance noise.
        if snapshot_supply <= 0 || balance <= 0 {
            return Err(GovernanceError::InsufficientShareBalance);
        }

        let count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::ProposalCount)
            .unwrap_or(0);
        let id = count + 1;
        let quorum_bps = quorum_for_category(&config, &category);
        let proposal = Proposal {
            id,
            proposer: proposer.clone(),
            description,
            target_contract,
            function_name,
            calldata,
            votes_for: 0,
            votes_against: 0,
            status: ProposalStatus::Active,
            created_at: now,
            voting_ends_at: now.saturating_add(config.voting_period_secs),
            execution_delay: config.execution_delay_secs,
            snapshot_supply,
            passed_at: 0,
            category,
            quorum_bps,
            pass_bps: config.pass_bps,
        };

        env.storage()
            .instance()
            .set(&DataKey::Proposal(id), &proposal);
        env.storage().instance().set(&DataKey::ProposalCount, &id);
        env.events()
            .publish((EVT, symbol_short!("create")), (id, proposer));
        Ok(id)
    }

    pub fn vote(
        env: Env,
        proposal_id: u64,
        voter: Address,
        in_favor: bool,
    ) -> Result<(), GovernanceError> {
        voter.require_auth();
        let config = load_config(&env)?;
        let mut proposal: Proposal = env
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .ok_or(GovernanceError::ProposalNotFound)?;
        if proposal.status != ProposalStatus::Active {
            return Err(GovernanceError::ProposalInactive);
        }
        if env
            .storage()
            .persistent()
            .has(&DataKey::Vote(proposal_id, voter.clone()))
        {
            return Err(GovernanceError::AlreadyVoted);
        }
        // #932: voting window is [created_at, voting_ends_at). At equality the
        // period has fully elapsed — no more votes, and finalize may run.
        if env.ledger().timestamp() >= proposal.voting_ends_at {
            let _ = finalize_proposal(&env, &mut proposal);
            env.storage()
                .instance()
                .set(&DataKey::Proposal(proposal_id), &proposal);
            return Err(GovernanceError::VotingPeriodActive);
        }

        let weight = proposal_weight(&env, &config.share_token, &voter, proposal.created_at);
        if weight <= 0 {
            return Err(GovernanceError::InsufficientShareBalance);
        }

        if in_favor {
            proposal.votes_for += weight;
        } else {
            proposal.votes_against += weight;
        }

        env.storage()
            .persistent()
            .set(&DataKey::Vote(proposal_id, voter.clone()), &true);
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        env.events().publish(
            (EVT, symbol_short!("vote")),
            (proposal_id, voter.clone(), in_favor, weight),
        );

        Ok(())
    }

    pub fn execute_proposal(env: Env, proposal_id: u64) -> Result<(), GovernanceError> {
        let config = load_config(&env)?;
        let mut proposal: Proposal = env
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .ok_or(GovernanceError::ProposalNotFound)?;

        if proposal.status == ProposalStatus::Cancelled
            || proposal.status == ProposalStatus::Executed
            || proposal.status == ProposalStatus::Expired
        {
            return Err(GovernanceError::ProposalInactive);
        }
        // #932: voting period must fully elapse before execution. Reject while
        // `now <= voting_ends_at` so fast unanimous support cannot skip the
        // configured window (and so vote/execute never both succeed at equality).
        if env.ledger().timestamp() <= proposal.voting_ends_at {
            return Err(GovernanceError::VotingPeriodActive);
        }
        if env.ledger().timestamp()
            < proposal
                .voting_ends_at
                .saturating_add(config.execution_delay_secs)
        {
            return Err(GovernanceError::TimelockActive);
        }

        let finalization = finalize_proposal(&env, &mut proposal);
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        finalization?;
        if proposal.status != ProposalStatus::Passed {
            return Err(GovernanceError::InvalidProposalState);
        }

        if mark_expired_if_due(&env, &mut proposal) {
            env.storage()
                .instance()
                .set(&DataKey::Proposal(proposal_id), &proposal);
            return Err(GovernanceError::ProposalExpired);
        }

        env.events().publish(
            (EVT, symbol_short!("execute")),
            (
                proposal_id,
                proposal.target_contract.clone(),
                proposal.function_name.clone(),
                proposal.calldata.clone(),
            ),
        );
        proposal.status = ProposalStatus::Executed;
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        Ok(())
    }

    pub fn cancel_proposal(
        env: Env,
        proposal_id: u64,
        caller: Address,
    ) -> Result<(), GovernanceError> {
        caller.require_auth();
        let config = load_config(&env)?;
        let mut proposal: Proposal = env
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .ok_or(GovernanceError::ProposalNotFound)?;

        if caller != proposal.proposer && caller != config.admin {
            return Err(GovernanceError::Unauthorized);
        }
        // #929: also reject Expired and Rejected — only Active/Passed proposals
        // can be cancelled; everything else is already in a terminal state.
        match proposal.status {
            ProposalStatus::Cancelled
            | ProposalStatus::Executed
            | ProposalStatus::Expired
            | ProposalStatus::Rejected => {
                return Err(GovernanceError::InvalidProposalState);
            }
            ProposalStatus::Active | ProposalStatus::Passed => {}
        }

        proposal.status = ProposalStatus::Cancelled;
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        env.events()
            .publish((EVT, symbol_short!("cancel")), (proposal_id, caller));
        Ok(())
    }

    pub fn get_proposal(env: Env, proposal_id: u64) -> Option<Proposal> {
        env.storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
    }

    pub fn list_proposals(env: Env) -> Vec<Proposal> {
        let count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::ProposalCount)
            .unwrap_or(0);
        let mut proposals = Vec::new(&env);
        for id in 1..=count {
            if let Some(mut proposal) = env
                .storage()
                .instance()
                .get::<DataKey, Proposal>(&DataKey::Proposal(id))
            {
                let mut changed = false;
                // #932: align finalize with vote/execute — period fully elapsed
                // at `now >= voting_ends_at`.
                if proposal.status == ProposalStatus::Active
                    && env.ledger().timestamp() >= proposal.voting_ends_at
                {
                    let _ = finalize_proposal(&env, &mut proposal);
                    changed = true;
                }
                if mark_expired_if_due(&env, &mut proposal) {
                    changed = true;
                }
                if changed {
                    env.storage()
                        .instance()
                        .set(&DataKey::Proposal(id), &proposal);
                }
                proposals.push_back(proposal);
            }
        }
        proposals
    }

    pub fn get_config(env: Env) -> Result<GovernanceConfig, GovernanceError> {
        load_config(&env)
    }

    /// #930: Read-only preview of a voter's current voting weight for a given
    /// proposal. Uses the same snapshot-based weight as `vote()` so callers see
    /// exactly how much their vote will count before submitting a transaction.
    /// Returns 0 when the proposal does not exist or the voter held no shares
    /// at the snapshot timestamp.
    pub fn get_voting_power(
        env: Env,
        proposal_id: u64,
        voter: Address,
    ) -> Result<i128, GovernanceError> {
        let config = load_config(&env)?;
        let proposal: Proposal = env
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .ok_or(GovernanceError::ProposalNotFound)?;
        let weight = proposal_weight(&env, &config.share_token, &voter, proposal.created_at);
        Ok(weight)
    }

    /// Returns whether `voter` has already voted on the given proposal.
    /// This is a read-only view of the persistent storage flag that `vote()`
    /// sets; callers do not need to submit a transaction.
    pub fn has_voted(env: Env, proposal_id: u64, voter: Address) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::Vote(proposal_id, voter))
    }

    /// Updates the default (ParameterChange) quorum and pass-threshold.
    /// Gated to `config.admin`. Does not rewrite snapshotted values on
    /// already-created proposals.
    pub fn update_config(
        env: Env,
        caller: Address,
        quorum_bps: u32,
        pass_bps: u32,
    ) -> Result<(), GovernanceError> {
        caller.require_auth();
        let mut config = load_config(&env)?;
        if caller != config.admin {
            return Err(GovernanceError::Unauthorized);
        }
        validate_bps(quorum_bps, pass_bps)?;

        config.quorum_bps = quorum_bps;
        config.pass_bps = pass_bps;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events()
            .publish((EVT, symbol_short!("cfg")), (caller, quorum_bps, pass_bps));
        Ok(())
    }

    /// #933: set per-category quorum (and optional pass threshold via pass_bps).
    /// `pass_bps` applies globally (same supermajority rule for all categories);
    /// category only differentiates quorum. Snapshotted onto new proposals only.
    pub fn set_category_quorum(
        env: Env,
        caller: Address,
        category: ProposalCategory,
        quorum_bps: u32,
    ) -> Result<(), GovernanceError> {
        caller.require_auth();
        let mut config = load_config(&env)?;
        if caller != config.admin {
            return Err(GovernanceError::Unauthorized);
        }
        if quorum_bps == 0 || quorum_bps > 10_000 {
            return Err(GovernanceError::InvalidConfig);
        }

        match category {
            ProposalCategory::ParameterChange => config.quorum_bps = quorum_bps,
            ProposalCategory::Treasury => config.treasury_quorum_bps = quorum_bps,
            ProposalCategory::Critical => config.critical_quorum_bps = quorum_bps,
        }
        env.storage().instance().set(&DataKey::Config, &config);
        env.events().publish(
            (EVT, symbol_short!("cat_q")),
            (caller, category, quorum_bps),
        );
        Ok(())
    }

    /// #933: read the quorum bps configured for a category.
    pub fn get_category_quorum(
        env: Env,
        category: ProposalCategory,
    ) -> Result<u32, GovernanceError> {
        let config = load_config(&env)?;
        Ok(quorum_for_category(&config, &category))
    }

    // #1042: multisig admin path, additive to the legacy single-admin
    // functions above — see access_control/src/lib.rs for the full
    // propose/approve/execute lifecycle. `set_access_control` bootstraps
    // the trust anchor (still gated by `config.admin`); every `*_via_ac`
    // entrypoint below then trusts only calls that carry the configured
    // `access_control` contract's own on-chain identity. Governance's
    // token-weighted proposal/vote/execute flow (`create_proposal`,
    // `vote`, `execute_proposal`) is a separate authorization model and is
    // untouched — only the admin-key-gated config setters below move
    // behind multisig.
    pub fn set_access_control(
        env: Env,
        caller: Address,
        access_control: Address,
    ) -> Result<(), GovernanceError> {
        caller.require_auth();
        let config = load_config(&env)?;
        if caller != config.admin {
            return Err(GovernanceError::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&DataKey::AccessControl, &access_control);
        env.events()
            .publish((EVT, symbol_short!("set_ac")), (caller, access_control));
        Ok(())
    }

    pub fn get_access_control(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::AccessControl)
    }

    /// #1042: rotates the trust anchor itself through the currently
    /// configured `access_control` contract rather than the legacy admin
    /// key.
    pub fn set_access_control_via_ac(
        env: Env,
        access_control: Address,
        new_access_control: Address,
    ) -> Result<(), GovernanceError> {
        access_control.require_auth();
        require_access_control(&env, &access_control)?;
        env.storage()
            .instance()
            .set(&DataKey::AccessControl, &new_access_control);
        env.events().publish(
            (EVT, symbol_short!("ac_rot")),
            (access_control, new_access_control),
        );
        Ok(())
    }

    pub fn update_config_via_ac(
        env: Env,
        access_control: Address,
        quorum_bps: u32,
        pass_bps: u32,
    ) -> Result<(), GovernanceError> {
        access_control.require_auth();
        require_access_control(&env, &access_control)?;
        let mut config = load_config(&env)?;
        validate_bps(quorum_bps, pass_bps)?;

        config.quorum_bps = quorum_bps;
        config.pass_bps = pass_bps;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events().publish(
            (EVT, symbol_short!("ac_cfg")),
            (access_control, quorum_bps, pass_bps),
        );
        Ok(())
    }

    pub fn set_category_quorum_via_ac(
        env: Env,
        access_control: Address,
        category: u32,
        quorum_bps: u32,
    ) -> Result<(), GovernanceError> {
        access_control.require_auth();
        require_access_control(&env, &access_control)?;
        if quorum_bps == 0 || quorum_bps > 10_000 {
            return Err(GovernanceError::InvalidConfig);
        }
        let category = category_from_discriminant(category)?;
        let mut config = load_config(&env)?;

        match category {
            ProposalCategory::ParameterChange => config.quorum_bps = quorum_bps,
            ProposalCategory::Treasury => config.treasury_quorum_bps = quorum_bps,
            ProposalCategory::Critical => config.critical_quorum_bps = quorum_bps,
        }
        env.storage().instance().set(&DataKey::Config, &config);
        env.events().publish(
            (EVT, symbol_short!("ac_cat_q")),
            (access_control, category, quorum_bps),
        );
        Ok(())
    }
}
