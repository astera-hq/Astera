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

/// #1038: Action payload for governance-gated parameter changes across
/// pool, invoice, oracle_registry, and compliance contracts. Each variant
/// corresponds to a governance-gated setter that can only be executed via
/// an approved governance proposal.
#[contracttype]
#[derive(Clone, Debug)]
pub enum GovernanceAction {
    // ── pool ──
    SetPoolYield(u32),
    SetPoolYieldChangePolicy(u64),
    SetPoolFactoringFee(u32),
    SetPoolFeeTier(u32, u32),
    SetPoolTreasury(Address),
    SetPoolMaxUtilization(u32),
    SetPoolMinDeposit(i128),
    SetPoolMaxInvestorConcentration(u32),
    SetPoolLoyaltyTiers(Vec<LoyaltyTier>),
    SetPoolWithdrawalLimits(u32),
    SetPoolMaxWithdrawalQueueAge(u32),
    SetPoolMaxWithdrawalQueueDepth(u32),
    SetPoolOracleContract(Address),
    SetPoolOracleStaleThreshold(u64),
    SetPoolFallbackPrice(Address, i128),
    SetPoolRateBounds(Address, u32, u32),
    SetPoolExchangeRate(Address, i128),
    SetPoolComplianceRegistry(Address),
    SetPoolRequireComplianceCheck(bool),
    SetPoolReferralRegistry(Address),
    SetPoolKycRequired(bool),
    SetPoolCreditScoreContract(Address),
    SetPoolInsuranceContract(Address),
    SetPoolCompoundInterest(bool),
    SetPoolSecondaryMarketContract(Address),
    SetPoolRiskContract(Address),
    SetPoolCollateralConfig(i128),
    SetPoolUpgradeTimelock(u64),
    SetPoolOperationDelay(u64),
    // ── invoice ──
    SetInvoiceGracePeriod(u32),
    SetInvoiceMinDueDateWindow(u64),
    SetInvoiceMaxAmount(i128),
    SetInvoiceMaxSmeOutstanding(i128),
    SetInvoiceExpirationDuration(u64),
    SetInvoiceCompletedTtl(u32),
    SetInvoiceDailyLimit(u32),
    SetInvoiceDisputeWindow(u64),
    SetInvoiceOracle(Address),
    SetInvoiceSecondaryOracle(Option<Address>),
    SetInvoiceOracleRegistry(Address),
    SetInvoiceConsensusRequired(bool),
    SetInvoiceComplianceRegistry(Address),
    SetInvoiceRequireComplianceCheck(bool),
    SetInvoiceRequireRegisteredDebtor(bool),
    SetInvoiceOracleVerifiedFundingOnly(bool),
    SetInvoiceArbitrationContract(Address),
    SetInvoiceDisputeValueThreshold(i128),
    SetInvoiceMetadataImageUri(String),
    // ── oracle_registry ──
    SetOracleRegistryInvoiceContract(Address),
    SetOracleRegistryTreasury(Option<Address>),
    SetOracleRegistryConfig(i128, u32, u32, u64, u64),
    SetOracleRegistryQuorumTiers(Vec<QuorumTier>),
    // ── compliance ──
    SetComplianceRescreeningInterval(u64),
    SetComplianceScreenerTimelock(u64),
}

/// Placeholder types for Vec payloads - these should match the actual types
/// in their respective contracts. For now, using simple representations.
#[contracttype]
#[derive(Clone, Debug)]
pub struct LoyaltyTier {
    pub days_threshold: u32,
    pub bonus_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct FeeTier {
    pub min_amount: i128,
    pub max_amount: i128,
    pub min_credit_score: u32,
    pub fee_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct CollateralConfig {
    pub threshold: i128,
    pub collateral_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct QuorumTier {
    pub min_invoice_amount: i128,
    pub quorum_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Proposal {
    pub id: u64,
    pub proposer: Address,
    pub description: String,
    /// #1038: The target contract this proposal acts upon (pool, invoice, etc.)
    pub target_contract: Address,
    /// #1038: The specific governance action to execute
    pub action: GovernanceAction,
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
    // #1038: governance contract address for self-rotation
    GovernanceAddress,
}

// #1038: Cross-contract client traits for governance-gated parameter changes
#[contractclient(name = "PoolClient")]
pub trait PoolContract {
    fn set_yield_via_governance(env: Env, governance: Address, new_yield_bps: u32);
    fn set_yield_change_policy_via_governance(env: Env, governance: Address, cooldown_secs: u64);
    fn set_factoring_fee_via_governance(env: Env, governance: Address, factoring_fee_bps: u32);
    fn set_treasury_via_governance(env: Env, governance: Address, treasury: Address);
    fn set_max_utilization_via_governance(env: Env, governance: Address, max_bps: u32);
    fn set_oracle_contract_via_governance(env: Env, governance: Address, oracle: Address);
    fn set_kyc_required_via_governance(env: Env, governance: Address, required: bool);
    fn set_compliance_registry_via_governance(env: Env, governance: Address, registry: Address);
    fn set_require_compliance_check_via_governance(env: Env, governance: Address, required: bool);
    fn set_referral_registry_via_governance(env: Env, governance: Address, registry: Address);
    fn set_credit_score_contract_via_governance(env: Env, governance: Address, credit_score_contract: Address);
    fn set_insurance_contract_via_governance(env: Env, governance: Address, insurance_contract: Address);
    fn set_compound_interest_via_governance(env: Env, governance: Address, compound: bool);
    fn set_secondary_market_contract_via_governance(env: Env, governance: Address, secondary_market_contract: Address);
    fn set_risk_contract_via_governance(env: Env, governance: Address, risk_contract: Address);
    fn set_min_deposit_via_governance(env: Env, governance: Address, min_amount: i128);
    fn set_max_investor_concentration_via_governance(env: Env, governance: Address, max_bps: u32);
    fn set_upgrade_timelock_via_governance(env: Env, governance: Address, secs: u64);
    fn set_operation_delay_via_governance(env: Env, governance: Address, secs: u64);
    fn set_withdrawal_limits_via_governance(env: Env, governance: Address, max_bps: u32);
    fn set_max_withdrawal_queue_age_via_governance(env: Env, governance: Address, days: u32);
    fn set_max_withdrawal_queue_depth_via_governance(env: Env, governance: Address, depth: u32);
    fn set_oracle_stale_threshold_via_governance(env: Env, governance: Address, threshold_secs: u64);
    fn set_fee_tier_via_governance(env: Env, governance: Address, tier_id: u32, tier: FeeTier);
    fn set_loyalty_tiers_via_governance(env: Env, governance: Address, tiers: Vec<LoyaltyTier>);
    fn set_fallback_price_via_governance(env: Env, governance: Address, token: Address, price: i128);
    fn set_rate_bounds_via_governance(env: Env, governance: Address, token: Address, min_rate: i128, max_rate: i128);
    fn set_exchange_rate_via_governance(env: Env, governance: Address, token: Address, rate: i128);
    fn set_collateral_config_via_governance(env: Env, governance: Address, config: CollateralConfig);
}

#[contractclient(name = "InvoiceClient")]
pub trait InvoiceContract {
    fn set_grace_period_via_governance(env: Env, governance: Address, days: u32);
    fn set_max_invoice_amount_via_governance(env: Env, governance: Address, max_amount: i128);
    fn set_max_sme_outstanding_via_governance(env: Env, governance: Address, max: i128);
    fn set_expiration_duration_via_governance(env: Env, governance: Address, expiration_duration_secs: u64);
    fn set_completed_invoice_ttl_via_governance(env: Env, governance: Address, ttl_ledgers: u32);
    fn set_daily_invoice_limit_via_governance(env: Env, governance: Address, limit: u32);
    fn set_dispute_window_via_governance(env: Env, governance: Address, window: u64);
    fn set_oracle_via_governance(env: Env, governance: Address, oracle: Address);
    fn set_secondary_oracle_via_governance(env: Env, governance: Address, oracle_secondary: Option<Address>);
    fn set_oracle_registry_via_governance(env: Env, governance: Address, registry: Address);
    fn set_consensus_required_via_governance(env: Env, governance: Address, required: bool);
    fn set_compliance_registry_via_governance(env: Env, governance: Address, registry: Address);
    fn set_require_compliance_check_via_governance(env: Env, governance: Address, required: bool);
    fn set_require_registered_debtor_via_governance(env: Env, governance: Address, required: bool);
    fn set_oracle_verified_funding_only_via_governance(env: Env, governance: Address, required: bool);
    fn set_arbitration_contract_via_governance(env: Env, governance: Address, arbitration: Address);
    fn set_dispute_value_threshold_via_governance(env: Env, governance: Address, threshold: i128);
    fn set_metadata_image_uri_via_governance(env: Env, governance: Address, uri: String);
    fn set_min_due_date_window_via_governance(env: Env, governance: Address, window_secs: u64);
    // Add remaining invoice client methods as needed
}

#[contractclient(name = "OracleRegistryClient")]
pub trait OracleRegistryContract {
    fn set_invoice_contract_via_governance(env: Env, governance: Address, invoice_contract: Address);
    fn set_treasury_via_governance(env: Env, governance: Address, treasury: Option<Address>);
    fn set_registry_config_via_governance(env: Env, governance: Address, min_stake: i128, required_votes: u32, quorum_bps: u32, round_duration_secs: u64, deregister_cooldown_secs: u64);
    fn set_quorum_tiers_via_governance(env: Env, governance: Address, tiers: Vec<QuorumTier>);
}

#[contractclient(name = "ComplianceClient")]
pub trait ComplianceContract {
    fn set_rescreening_interval_via_governance(env: Env, governance: Address, secs: u64);
    fn set_screener_timelock_via_governance(env: Env, governance: Address, secs: u64);
    // Add remaining compliance client methods as needed
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
    // #1038: a `*_via_governance` entrypoint was called but no `governance`
    // contract has been configured via `set_governance` yet.
    GovernanceNotConfigured = 14,
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

/// #1038: Helper function for target contracts to verify the caller is the
/// configured governance contract. This should be called by `*_via_governance`
/// entrypoints in pool, invoice, oracle_registry, and compliance contracts.
fn require_governance(env: &Env, caller: &Address) -> GovernanceResult<()> {
    let configured: Address = env
        .storage()
        .instance()
        .get(&DataKey::GovernanceAddress)
        .ok_or(GovernanceError::GovernanceNotConfigured)?;
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

    /// #931 / #933 / #1038: create a proposal. Caller must hold at least
    /// `min_share_balance` shares (proposal threshold). Category selects the
    /// quorum tier, which is snapshotted onto the proposal. Action specifies
    /// the governance-gated parameter change to execute.
    pub fn create_proposal(
        env: Env,
        proposer: Address,
        description: String,
        target_contract: Address,
        action: GovernanceAction,
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
            action,
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

        // #1038: Execute the governance action by calling the target contract
        Self::execute_governance_action(&env, &proposal.target_contract, &proposal.action)?;

        env.events().publish(
            (EVT, symbol_short!("execute")),
            (
                proposal_id,
                proposal.target_contract.clone(),
            ),
        );
        proposal.status = ProposalStatus::Executed;
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        Ok(())
    }

    /// #1038: Execute a governance action by calling the appropriate contract
    /// with the governance contract's identity for authentication.
    fn execute_governance_action(
        env: &Env,
        target_contract: &Address,
        action: &GovernanceAction,
    ) -> Result<(), GovernanceError> {
        let this_contract = env.current_contract_address();
        
        // Define cross-contract client traits for each target contract
        // These will be implemented as separate client interfaces
        match action {
            // ── pool actions ──
            GovernanceAction::SetPoolYield(new_yield_bps) => {
                PoolClient::new(env, target_contract)
                    .set_yield_via_governance(&this_contract, *new_yield_bps);
            }
            GovernanceAction::SetPoolYieldChangePolicy(cooldown_secs) => {
                PoolClient::new(env, target_contract)
                    .set_yield_change_policy_via_governance(&this_contract, *cooldown_secs);
            }
            GovernanceAction::SetPoolFactoringFee(fee_bps) => {
                PoolClient::new(env, target_contract)
                    .set_factoring_fee_via_governance(&this_contract, *fee_bps);
            }
            GovernanceAction::SetPoolTreasury(treasury) => {
                PoolClient::new(env, target_contract)
                    .set_treasury_via_governance(&this_contract, treasury.clone());
            }
            GovernanceAction::SetPoolMaxUtilization(max_bps) => {
                PoolClient::new(env, target_contract)
                    .set_max_utilization_via_governance(&this_contract, *max_bps);
            }
            GovernanceAction::SetPoolOracleContract(oracle) => {
                PoolClient::new(env, target_contract)
                    .set_oracle_contract_via_governance(&this_contract, oracle.clone());
            }
            GovernanceAction::SetPoolKycRequired(required) => {
                PoolClient::new(env, target_contract)
                    .set_kyc_required_via_governance(&this_contract, *required);
            }
            GovernanceAction::SetPoolComplianceRegistry(registry) => {
                PoolClient::new(env, target_contract)
                    .set_compliance_registry_via_governance(&this_contract, registry.clone());
            }
            GovernanceAction::SetPoolRequireComplianceCheck(required) => {
                PoolClient::new(env, target_contract)
                    .set_require_compliance_check_via_governance(&this_contract, *required);
            }
            GovernanceAction::SetPoolReferralRegistry(registry) => {
                PoolClient::new(env, target_contract)
                    .set_referral_registry_via_governance(&this_contract, registry.clone());
            }
            GovernanceAction::SetPoolCreditScoreContract(credit_score) => {
                PoolClient::new(env, target_contract)
                    .set_credit_score_contract_via_governance(&this_contract, credit_score.clone());
            }
            GovernanceAction::SetPoolInsuranceContract(insurance) => {
                PoolClient::new(env, target_contract)
                    .set_insurance_contract_via_governance(&this_contract, insurance.clone());
            }
            GovernanceAction::SetPoolCompoundInterest(compound) => {
                PoolClient::new(env, target_contract)
                    .set_compound_interest_via_governance(&this_contract, *compound);
            }
            GovernanceAction::SetPoolSecondaryMarketContract(sm) => {
                PoolClient::new(env, target_contract)
                    .set_secondary_market_contract_via_governance(&this_contract, sm.clone());
            }
            GovernanceAction::SetPoolRiskContract(risk) => {
                PoolClient::new(env, target_contract)
                    .set_risk_contract_via_governance(&this_contract, risk.clone());
            }
            GovernanceAction::SetPoolMinDeposit(min_amount) => {
                PoolClient::new(env, target_contract)
                    .set_min_deposit_via_governance(&this_contract, *min_amount);
            }
            GovernanceAction::SetPoolMaxInvestorConcentration(max_bps) => {
                PoolClient::new(env, target_contract)
                    .set_max_investor_concentration_via_governance(&this_contract, *max_bps);
            }
            GovernanceAction::SetPoolUpgradeTimelock(secs) => {
                PoolClient::new(env, target_contract)
                    .set_upgrade_timelock_via_governance(&this_contract, *secs);
            }
            GovernanceAction::SetPoolOperationDelay(secs) => {
                PoolClient::new(env, target_contract)
                    .set_operation_delay_via_governance(&this_contract, *secs);
            }
            GovernanceAction::SetPoolWithdrawalLimits(max_bps) => {
                PoolClient::new(env, target_contract)
                    .set_withdrawal_limits_via_governance(&this_contract, *max_bps);
            }
            GovernanceAction::SetPoolMaxWithdrawalQueueAge(days) => {
                PoolClient::new(env, target_contract)
                    .set_max_withdrawal_queue_age_via_governance(&this_contract, *days);
            }
            GovernanceAction::SetPoolMaxWithdrawalQueueDepth(depth) => {
                PoolClient::new(env, target_contract)
                    .set_max_withdrawal_queue_depth_via_governance(&this_contract, *depth);
            }
            GovernanceAction::SetPoolOracleStaleThreshold(threshold_secs) => {
                PoolClient::new(env, target_contract)
                    .set_oracle_stale_threshold_via_governance(&this_contract, *threshold_secs);
            }
            // Pool actions with complex types (Vec<LoyaltyTier>, etc.) - TODO
            GovernanceAction::SetPoolFeeTier(_, _) => {
                // TODO: Implement
                return Ok(());
            }
            GovernanceAction::SetPoolLoyaltyTiers(_) => {
                // TODO: Implement
                return Ok(());
            }
            GovernanceAction::SetPoolFallbackPrice(_, _) => {
                // TODO: Implement
                return Ok(());
            }
            GovernanceAction::SetPoolRateBounds(_, _, _) => {
                // TODO: Implement
                return Ok(());
            }
            GovernanceAction::SetPoolExchangeRate(_, _) => {
                // TODO: Implement
                return Ok(());
            }
            GovernanceAction::SetPoolCollateralConfig(_) => {
                // TODO: Implement
                return Ok(());
            }
            
            // ── invoice actions ──
            GovernanceAction::SetInvoiceGracePeriod(days) => {
                InvoiceClient::new(env, target_contract)
                    .set_grace_period_via_governance(&this_contract, *days);
            }
            GovernanceAction::SetInvoiceMaxAmount(max_amount) => {
                InvoiceClient::new(env, target_contract)
                    .set_max_invoice_amount_via_governance(&this_contract, *max_amount);
            }
            GovernanceAction::SetInvoiceMaxSmeOutstanding(max) => {
                InvoiceClient::new(env, target_contract)
                    .set_max_sme_outstanding_via_governance(&this_contract, *max);
            }
            GovernanceAction::SetInvoiceExpirationDuration(secs) => {
                InvoiceClient::new(env, target_contract)
                    .set_expiration_duration_via_governance(&this_contract, *secs);
            }
            GovernanceAction::SetInvoiceCompletedTtl(ttl_ledgers) => {
                InvoiceClient::new(env, target_contract)
                    .set_completed_invoice_ttl_via_governance(&this_contract, *ttl_ledgers);
            }
            GovernanceAction::SetInvoiceDailyLimit(limit) => {
                InvoiceClient::new(env, target_contract)
                    .set_daily_invoice_limit_via_governance(&this_contract, *limit);
            }
            GovernanceAction::SetInvoiceDisputeWindow(window) => {
                InvoiceClient::new(env, target_contract)
                    .set_dispute_window_via_governance(&this_contract, *window);
            }
            GovernanceAction::SetInvoiceOracle(oracle) => {
                InvoiceClient::new(env, target_contract)
                    .set_oracle_via_governance(&this_contract, oracle.clone());
            }
            GovernanceAction::SetInvoiceSecondaryOracle(oracle_secondary) => {
                InvoiceClient::new(env, target_contract)
                    .set_secondary_oracle_via_governance(&this_contract, oracle_secondary.clone());
            }
            GovernanceAction::SetInvoiceOracleRegistry(registry) => {
                InvoiceClient::new(env, target_contract)
                    .set_oracle_registry_via_governance(&this_contract, registry.clone());
            }
            GovernanceAction::SetInvoiceConsensusRequired(required) => {
                InvoiceClient::new(env, target_contract)
                    .set_consensus_required_via_governance(&this_contract, *required);
            }
            GovernanceAction::SetInvoiceComplianceRegistry(registry) => {
                InvoiceClient::new(env, target_contract)
                    .set_compliance_registry_via_governance(&this_contract, registry.clone());
            }
            GovernanceAction::SetInvoiceRequireComplianceCheck(required) => {
                InvoiceClient::new(env, target_contract)
                    .set_require_compliance_check_via_governance(&this_contract, *required);
            }
            GovernanceAction::SetInvoiceRequireRegisteredDebtor(required) => {
                InvoiceClient::new(env, target_contract)
                    .set_require_registered_debtor_via_governance(&this_contract, *required);
            }
            GovernanceAction::SetInvoiceOracleVerifiedFundingOnly(required) => {
                InvoiceClient::new(env, target_contract)
                    .set_oracle_verified_funding_only_via_governance(&this_contract, *required);
            }
            GovernanceAction::SetInvoiceArbitrationContract(arbitration) => {
                InvoiceClient::new(env, target_contract)
                    .set_arbitration_contract_via_governance(&this_contract, arbitration.clone());
            }
            GovernanceAction::SetInvoiceDisputeValueThreshold(threshold) => {
                InvoiceClient::new(env, target_contract)
                    .set_dispute_value_threshold_via_governance(&this_contract, *threshold);
            }
            GovernanceAction::SetInvoiceMetadataImageUri(uri) => {
                InvoiceClient::new(env, target_contract)
                    .set_metadata_image_uri_via_governance(&this_contract, uri.clone());
            }
            GovernanceAction::SetInvoiceMinDueDateWindow(window_secs) => {
                InvoiceClient::new(env, target_contract)
                    .set_min_due_date_window_via_governance(&this_contract, *window_secs);
            }
            
            // ── oracle_registry actions ──
            GovernanceAction::SetOracleRegistryInvoiceContract(invoice_contract) => {
                OracleRegistryClient::new(env, target_contract)
                    .set_invoice_contract_via_governance(&this_contract, invoice_contract.clone());
            }
            GovernanceAction::SetOracleRegistryTreasury(treasury) => {
                OracleRegistryClient::new(env, target_contract)
                    .set_treasury_via_governance(&this_contract, treasury.clone());
            }
            GovernanceAction::SetOracleRegistryConfig(min_stake, required_votes, quorum_bps, round_duration_secs, deregister_cooldown_secs) => {
                OracleRegistryClient::new(env, target_contract)
                    .set_registry_config_via_governance(&this_contract, *min_stake, *required_votes, *quorum_bps, *round_duration_secs, *deregister_cooldown_secs);
            }
            GovernanceAction::SetOracleRegistryQuorumTiers(tiers) => {
                OracleRegistryClient::new(env, target_contract)
                    .set_quorum_tiers_via_governance(&this_contract, tiers.clone());
            }
            
            // ── compliance actions ──
            GovernanceAction::SetComplianceRescreeningInterval(secs) => {
                ComplianceClient::new(env, target_contract)
                    .set_rescreening_interval_via_governance(&this_contract, *secs);
            }
            GovernanceAction::SetComplianceScreenerTimelock(secs) => {
                ComplianceClient::new(env, target_contract)
                    .set_screener_timelock_via_governance(&this_contract, *secs);
            }

            // Pool actions with complex types
            GovernanceAction::SetPoolFeeTier(tier_id, tier) => {
                PoolClient::new(env, target_contract)
                    .set_fee_tier_via_governance(&this_contract, *tier_id, tier.clone());
            }
            GovernanceAction::SetPoolLoyaltyTiers(tiers) => {
                PoolClient::new(env, target_contract)
                    .set_loyalty_tiers_via_governance(&this_contract, tiers.clone());
            }
            GovernanceAction::SetPoolFallbackPrice(token, price) => {
                PoolClient::new(env, target_contract)
                    .set_fallback_price_via_governance(&this_contract, token.clone(), *price);
            }
            GovernanceAction::SetPoolRateBounds(token, min_rate, max_rate) => {
                PoolClient::new(env, target_contract)
                    .set_rate_bounds_via_governance(&this_contract, token.clone(), *min_rate, *max_rate);
            }
            GovernanceAction::SetPoolExchangeRate(token, rate) => {
                PoolClient::new(env, target_contract)
                    .set_exchange_rate_via_governance(&this_contract, token.clone(), *rate);
            }
            GovernanceAction::SetPoolCollateralConfig(config) => {
                PoolClient::new(env, target_contract)
                    .set_collateral_config_via_governance(&this_contract, config.clone());
            }
        }

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

    /// #1038: Set the governance contract address. This is called by target
    /// contracts (pool, invoice, oracle_registry, compliance) to bootstrap the
    /// governance relationship. Once set, only the governance contract can call
    /// `*_via_governance` entrypoints. Gated to `config.admin` for security.
    pub fn set_governance_address(
        env: Env,
        caller: Address,
        governance_address: Address,
    ) -> Result<(), GovernanceError> {
        caller.require_auth();
        let config = load_config(&env)?;
        if caller != config.admin {
            return Err(GovernanceError::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&DataKey::GovernanceAddress, &governance_address);
        env.events()
            .publish((EVT, symbol_short!("set_gov")), (caller, governance_address));
        Ok(())
    }

    /// #1038: Get the configured governance contract address.
    pub fn get_governance_address(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::GovernanceAddress)
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
