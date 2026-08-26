//! access_control
//!
//! Role-based multi-signature access control for the invoice/pool/credit_score
//! contracts (#864). Every privileged entrypoint in those contracts today is
//! gated by a single `Admin` address — if that one key is compromised, an
//! attacker has unbounded control. This crate introduces a parallel,
//! additive authorization path: a set of named roles, each with its own
//! `signers: Vec<Address>` and `threshold: u32` (M-of-N), and a
//! propose -> approve -> execute lifecycle for privileged actions.
//!
//! This does NOT replace the legacy single-admin path on any of the three
//! target contracts — that stays fully functional, unchanged, so a
//! deployment can adopt RBAC gradually. See each target contract's new
//! `*_via_ac` entrypoints, which trust calls that carry this
//! contract's own on-chain identity (the same pattern the pool contract
//! already uses to trust the invoice contract for `update_invoice_due_date`:
//! the calling contract's `Address` authenticates itself via
//! `require_auth()`, verified by the Soroban host against the actual
//! calling contract in this invocation).
#![no_std]

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, symbol_short, Address,
    Env, String, Symbol, Vec,
};

const EVT: Symbol = symbol_short!("accctl");

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

const LEDGERS_PER_DAY: u32 = 17_280;
const INSTANCE_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 30;
const INSTANCE_LIFETIME_THRESHOLD: u32 = LEDGERS_PER_DAY * 7;

/// Default window (in seconds) a proposal stays open before it expires
/// unapproved, unless overridden at `initialize()`. 7 days.
const DEFAULT_PROPOSAL_EXPIRY_SECS: u64 = 604_800;

/// Maximum number of signers per role to prevent unbounded iteration.
const MAX_SIGNERS_PER_ROLE: u32 = 32;

// ─── Roles ──────────────────────────────────────────────────────────────────

/// A named role, each with its own independent signer set and threshold.
///
/// `SuperAdmin` is the only role that may add/remove signers or change
/// thresholds for ANY role (including itself) — see `ActionPayload::AddSigner`
/// et al. below. It deliberately has the highest bar: bootstrapped once at
/// `initialize()`, and every subsequent change to it must go through its own
/// multisig proposal, so no single key — not even the deployer — can
/// unilaterally re-seed every role after launch.
#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    SuperAdmin,
    RiskManager,
    TreasuryManager,
    ComplianceOfficer,
    OracleManager,
}

/// M-of-N signer configuration for one role.
#[contracttype]
#[derive(Clone, Debug)]
pub struct MultiSigConfig {
    pub signers: Vec<Address>,
    pub threshold: u32,
}

// ─── Action payloads ────────────────────────────────────────────────────────

/// Every privileged action this system can gate. Each variant mirrors one
/// real entrypoint on `pool`, `invoice`, or `credit_score` (see each
/// contract's new `*_via_ac` methods) — except `AddSigner`,
/// `RemoveSigner`, and `SetThreshold`, which are self-management actions
/// against this contract's own role configuration and may only ever be
/// proposed under `Role::SuperAdmin` (enforced in `propose_action`).
#[contracttype]
#[derive(Clone, Debug)]
pub enum ActionPayload {
    // ── pool ──
    /// pause()/unpause() unified: `true` pauses, `false` unpauses.
    SetPaused(bool),
    SetYield(u32),
    SetTreasury(Address),
    /// (token, amount)
    WithdrawRevenue(Address, i128),
    SetOracleContract(Address),
    SetKycRequired(bool),
    /// (investor, approved)
    SetInvestorKyc(Address, bool),
    SetMaxUtilization(u32),
    // ── invoice ──
    SetOracle(Address),
    /// (debtor_id, debtor_name, max_exposure)
    RegisterDebtor(String, String, i128),
    DeactivateDebtor(String),
    AddKeeper(Address),
    // ── credit_score ──
    SetLateThreshold(i64),
    /// (excellent, very_good, good, fair)
    SetScoreThresholds(u32, u32, u32, u32),
    /// (address, attestor_type discriminant, weight_bps) — the discriminant
    /// is decoded by credit_score's own `AttestorType` mapping so this crate
    /// doesn't need to depend on credit_score's types (0=BusinessRegistry,
    /// 1=CreditBureau, 2=ExternalProtocol, 3=Manual).
    RegisterAttestor(Address, u32, u32),
    // ── oracle_registry ──
    SetOracleRegistryInvoiceContract(Address),
    SetOracleRegistryTreasury(Option<Address>),
    /// (min_stake, required_votes, quorum_bps, round_duration_secs, deregister_cooldown_secs)
    SetOracleRegistryConfig(i128, u32, u32, u64, u64),
    /// pause()/unpause() unified, same convention as `SetPaused` above.
    SetOracleRegistryPaused(bool),
    /// (operator, bps, round_id, evidence)
    SlashOracle(Address, u32, u64, String),
    /// (invoice_id, approved, reason)
    AdminResolveRound(u64, bool, String),
    // ── compliance ──
    SetCompliancePaused(bool),
    RegisterScreener(Address),
    ConfirmScreenerRegistration(Address),
    DeregisterScreener(Address),
    SetRescreeningInterval(u64),
    SetScreenerTimelock(u64),
    // ── governance ──
    /// (quorum_bps, pass_bps)
    UpdateGovernanceConfig(u32, u32),
    /// (category discriminant, quorum_bps) — decoded by governance's own
    /// `ProposalCategory` mapping (0=ParameterChange, 1=Treasury, 2=Critical),
    /// the same "decode by discriminant" convention `RegisterAttestor` above
    /// uses for credit_score's `AttestorType`.
    SetCategoryQuorum(u32, u32),
    // ── referral ──
    SetReferralPaused(bool),
    SetReferralPool(Address),
    SetBorrowRewardBps(u32),
    SetDepositRewardBps(u32),
    // ── admin-key rotation (SuperAdmin only) ──
    //
    // #1042: `set_access_control` on every target contract is itself gated
    // only by that contract's single legacy admin key — a compromised admin
    // key could otherwise repoint or strip the multisig trust anchor on an
    // already-adopted contract, bypassing every other gate this crate adds.
    // Routing rotation itself through a SuperAdmin-threshold proposal closes
    // that hole. These carry the *new* access_control address as payload and
    // are proposed with `target` = the contract whose trust anchor is being
    // rotated (not this contract's own address), so they execute via
    // `execute_cross_contract` like any other cross-contract action, but are
    // restricted to `Role::SuperAdmin` the same way self-management is (see
    // `requires_super_admin`).
    //
    // `pool` deliberately has no rotation variant here: its wasm binary is
    // already within ~300 bytes of Soroban's 200KB per-contract deploy cap
    // on main (see contracts/.wasm-size-baseline.json's `pool` entry) —
    // there's no room left for a new public entrypoint until `pool` is
    // split further (following the precedent already set by
    // `secondary_market` and `auction`). `pool`'s `set_access_control`
    // bootstrap still works via the legacy admin key; only rotation
    // through multisig is unavailable for now.
    SetInvoiceAccessControl(Address),
    SetCreditScoreAccessControl(Address),
    SetOracleRegistryAccessControl(Address),
    SetComplianceAccessControl(Address),
    SetGovernanceAccessControl(Address),
    SetReferralAccessControl(Address),
    // ── self-management (SuperAdmin only) ──
    AddSigner(Role, Address),
    RemoveSigner(Role, Address),
    SetThreshold(Role, u32),
    /// Update the global proposal expiry window (in seconds). Must be > 0.
    /// Gated under `Role::SuperAdmin` — same bar as signer/threshold changes.
    SetProposalExpiry(u64),
}

#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProposalStatus {
    Pending,
    Approved,
    Executed,
    Rejected,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Proposal {
    pub role: Role,
    /// The contract this action targets, or this contract's own address
    /// for a self-management action.
    pub target: Address,
    pub action: ActionPayload,
    pub proposer: Address,
    pub approvals: Vec<Address>,
    pub created_at: u64,
    pub expires_at: u64,
    pub status: ProposalStatus,
}

// ─── Storage ────────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    Initialized,
    RoleConfig(Role),
    Proposal(u64),
    NextProposalId,
    ProposalExpirySecs,
}

// ─── Errors ─────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum AccessControlError {
    AlreadyInitialized = 0,
    NotInitialized = 1,
    NotASigner = 2,
    AlreadyApproved = 3,
    ThresholdNotMet = 4,
    ProposalNotFound = 5,
    ProposalExpired = 6,
    ProposalNotPending = 7,
    ProposalNotApproved = 8,
    InvalidThreshold = 9,
    DuplicateSigner = 10,
    RoleNotConfigured = 11,
    SelfManagementRequiresSuperAdmin = 12,
    InvalidExpiryWindow = 13,
    SignerNotFound = 14,
    NoApprovalToRevoke = 15,
    /// The `action` payload and `target` address are incoherent: self-management
    /// payloads (AddSigner / RemoveSigner / SetThreshold / SetProposalExpiry)
    /// must be proposed with `target == this_contract`, and every cross-contract
    /// payload must be proposed with `target != this_contract`.  A mismatched
    /// proposal would silently no-op on execution, so it is rejected at
    /// creation time instead.
    IncoherentProposal = 16,
    /// The signer set for a role has reached MAX_SIGNERS_PER_ROLE.
    MaxSignersExceeded = 17,
}

type Result_ = Result<(), AccessControlError>;

// ─── Cross-contract interfaces ─────────────────────────────────────────────
//
// Local minimal mirrors of the target contracts' new `*_via_ac`
// entrypoints — the same "define just what you call, decoded by name"
// convention already used by pool/invoice for `PoolClient`/`InvoiceContractClient`/
// `CreditScoreClient` etc., so this crate never depends on pool/invoice/
// credit_score directly.

#[contractclient(name = "PoolClient")]
pub trait PoolContract {
    fn set_paused_via_ac(env: Env, access_control: Address, paused: bool);
    fn set_yield_via_ac(env: Env, access_control: Address, new_yield_bps: u32);
    fn set_treasury_via_ac(env: Env, access_control: Address, treasury: Address);
    fn withdraw_revenue_via_ac(env: Env, access_control: Address, token: Address, amount: i128);
    fn set_oracle_contract_via_ac(env: Env, access_control: Address, oracle: Address);
    fn set_kyc_required_via_ac(env: Env, access_control: Address, required: bool);
    fn set_investor_kyc_via_ac(
        env: Env,
        access_control: Address,
        investor: Address,
        approved: bool,
    );
    fn set_max_utilization_via_ac(env: Env, access_control: Address, max_bps: u32);
    // No set_access_control_via_ac here — pool has no wasm size budget
    // left for a rotation entrypoint; see the comment on `ActionPayload`
    // above.
}

#[contractclient(name = "InvoiceClient")]
pub trait InvoiceContract {
    fn set_paused_via_ac(env: Env, access_control: Address, paused: bool);
    fn set_oracle_via_ac(env: Env, access_control: Address, oracle: Address);
    fn register_debtor_via_ac(
        env: Env,
        access_control: Address,
        debtor_id: String,
        debtor_name: String,
        max_exposure: i128,
    );
    fn deactivate_debtor_via_ac(env: Env, access_control: Address, debtor_id: String);
    fn add_keeper_via_ac(env: Env, access_control: Address, keeper: Address);
    fn set_access_control_via_ac(env: Env, access_control: Address, new_access_control: Address);
}

#[contractclient(name = "CreditScoreClient")]
pub trait CreditScoreContract {
    fn set_paused_via_ac(env: Env, access_control: Address, paused: bool);
    fn set_late_threshold_via_ac(env: Env, access_control: Address, days: i64);
    fn set_score_thresholds_via_ac(
        env: Env,
        access_control: Address,
        excellent: u32,
        very_good: u32,
        good: u32,
        fair: u32,
    );
    fn register_attestor_via_ac(
        env: Env,
        access_control: Address,
        address: Address,
        attestor_type: u32,
        weight_bps: u32,
    );
    fn set_access_control_via_ac(env: Env, access_control: Address, new_access_control: Address);
}

#[contractclient(name = "OracleRegistryClient")]
pub trait OracleRegistryContractTrait {
    fn set_invoice_contract_via_ac(env: Env, access_control: Address, invoice_contract: Address);
    fn set_treasury_via_ac(env: Env, access_control: Address, treasury: Option<Address>);
    #[allow(clippy::too_many_arguments)]
    fn set_registry_config_via_ac(
        env: Env,
        access_control: Address,
        min_stake: i128,
        required_votes: u32,
        quorum_bps: u32,
        round_duration_secs: u64,
        deregister_cooldown_secs: u64,
    );
    fn set_paused_via_ac(env: Env, access_control: Address, paused: bool);
    fn slash_oracle_via_ac(
        env: Env,
        access_control: Address,
        operator: Address,
        bps: u32,
        round_id: u64,
        evidence: String,
    );
    fn admin_resolve_round_via_ac(
        env: Env,
        access_control: Address,
        invoice_id: u64,
        approved: bool,
        reason: String,
    );
    fn set_access_control_via_ac(env: Env, access_control: Address, new_access_control: Address);
}

#[contractclient(name = "ComplianceClient")]
pub trait ComplianceContractTrait {
    fn set_paused_via_ac(env: Env, access_control: Address, paused: bool);
    fn register_screener_via_ac(env: Env, access_control: Address, screener: Address);
    fn confirm_screener_via_ac(env: Env, access_control: Address, screener: Address);
    fn deregister_screener_via_ac(env: Env, access_control: Address, screener: Address);
    fn set_rescreening_interval_via_ac(env: Env, access_control: Address, secs: u64);
    fn set_screener_timelock_via_ac(env: Env, access_control: Address, secs: u64);
    fn set_access_control_via_ac(env: Env, access_control: Address, new_access_control: Address);
}

#[contractclient(name = "GovernanceClient")]
pub trait GovernanceContractTrait {
    fn update_config_via_ac(env: Env, access_control: Address, quorum_bps: u32, pass_bps: u32);
    fn set_category_quorum_via_ac(
        env: Env,
        access_control: Address,
        category: u32,
        quorum_bps: u32,
    );
    fn set_access_control_via_ac(env: Env, access_control: Address, new_access_control: Address);
}

#[contractclient(name = "ReferralClient")]
pub trait ReferralContractTrait {
    fn set_paused_via_ac(env: Env, access_control: Address, paused: bool);
    fn set_pool_via_ac(env: Env, access_control: Address, pool: Address);
    fn set_borrow_reward_bps_via_ac(env: Env, access_control: Address, bps: u32);
    fn set_deposit_reward_bps_via_ac(env: Env, access_control: Address, bps: u32);
    fn set_access_control_via_ac(env: Env, access_control: Address, new_access_control: Address);
}

// ─── Contract ───────────────────────────────────────────────────────────────

#[contract]
pub struct AccessControlContract;

#[contractimpl]
impl AccessControlContract {
    /// One-time setup. Bootstraps only the `SuperAdmin` role — every other
    /// role starts unconfigured (no signers, no threshold) and must be
    /// configured afterwards via `AddSigner`/`SetThreshold` proposals raised
    /// under `Role::SuperAdmin`. This sidesteps the chicken-and-egg problem
    /// of a role-management system needing role management to bootstrap:
    /// `SuperAdmin` is seeded directly here, once, from constructor args.
    pub fn initialize(
        env: Env,
        super_admin_signers: Vec<Address>,
        super_admin_threshold: u32,
        proposal_expiry_secs: u64,
    ) -> Result_ {
        if env.storage().instance().has(&DataKey::Initialized) {
            return Err(AccessControlError::AlreadyInitialized);
        }
        Self::validate_config(&super_admin_signers, super_admin_threshold)?;
        if proposal_expiry_secs == 0 {
            return Err(AccessControlError::InvalidExpiryWindow);
        }

        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage().instance().set(
            &DataKey::RoleConfig(Role::SuperAdmin),
            &MultiSigConfig {
                signers: super_admin_signers,
                threshold: super_admin_threshold,
            },
        );
        env.storage()
            .instance()
            .set(&DataKey::ProposalExpirySecs, &proposal_expiry_secs);
        env.storage()
            .instance()
            .set(&DataKey::NextProposalId, &0u64);
        bump_instance(&env);

        env.events()
            .publish((EVT, symbol_short!("init")), super_admin_threshold);
        Ok(())
    }

    /// Read-only accessor for the current proposal expiry window in seconds.
    pub fn get_proposal_expiry_secs(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::ProposalExpirySecs)
            .unwrap_or(DEFAULT_PROPOSAL_EXPIRY_SECS)
    }

    /// Read-only accessor for a role's current signer set / threshold.
    pub fn get_role_config(env: Env, role: Role) -> Option<MultiSigConfig> {
        env.storage().instance().get(&DataKey::RoleConfig(role))
    }

    pub fn is_signer(env: Env, role: Role, address: Address) -> bool {
        match Self::get_role_config(env, role) {
            Some(cfg) => cfg.signers.contains(&address),
            None => false,
        }
    }

    pub fn get_proposal(env: Env, proposal_id: u64) -> Option<Proposal> {
        env.storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
    }

    /// One past `proposal_id`, i.e. proposals exist for `0..get_next_proposal_id()`.
    /// Lets a frontend page through the full proposal history/queue without
    /// this contract needing its own paginated listing entrypoint.
    pub fn get_next_proposal_id(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::NextProposalId)
            .unwrap_or(0)
    }

    /// Raise a new proposal under `role`. The proposer must already be a
    /// registered signer for that role, and is counted as the first
    /// approval (a proposer that isn't willing to approve their own
    /// proposal shouldn't be raising it).
    pub fn propose_action(
        env: Env,
        role: Role,
        proposer: Address,
        target: Address,
        action: ActionPayload,
    ) -> Result<u64, AccessControlError> {
        proposer.require_auth();
        bump_instance(&env);

        if Self::requires_super_admin(&action) && !matches!(role, Role::SuperAdmin) {
            return Err(AccessControlError::SelfManagementRequiresSuperAdmin);
        }

        // #1135: Validate payload/target coherence before the proposal enters
        // the approval queue.  Self-management payloads mutate *this* contract's
        // own storage, so they must target this contract.  Cross-contract
        // payloads call out to an external contract, so they must not target
        // this contract — executing them against `this_contract` would hit
        // `execute_cross_contract`'s self-management catch-all arm and silently
        // do nothing.  Rejecting here ensures a mismatched proposal never
        // reaches `Executed` status with zero real effect.
        let this_contract = env.current_contract_address();
        let targets_self = target == this_contract;
        if Self::is_self_management(&action) && !targets_self {
            return Err(AccessControlError::IncoherentProposal);
        }
        if !Self::is_self_management(&action) && targets_self {
            return Err(AccessControlError::IncoherentProposal);
        }

        let config: MultiSigConfig = env
            .storage()
            .instance()
            .get(&DataKey::RoleConfig(role))
            .ok_or(AccessControlError::RoleNotConfigured)?;
        if !config.signers.contains(&proposer) {
            return Err(AccessControlError::NotASigner);
        }

        let expiry_secs: u64 = env
            .storage()
            .instance()
            .get(&DataKey::ProposalExpirySecs)
            .unwrap_or(DEFAULT_PROPOSAL_EXPIRY_SECS);
        let now = env.ledger().timestamp();

        let mut approvals = Vec::new(&env);
        approvals.push_back(proposer.clone());
        let status = if config.threshold <= 1 {
            ProposalStatus::Approved
        } else {
            ProposalStatus::Pending
        };

        let proposal_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextProposalId)
            .unwrap_or(0);
        let proposal = Proposal {
            role,
            target: target.clone(),
            action: action.clone(),
            proposer: proposer.clone(),
            approvals,
            created_at: now,
            expires_at: now + expiry_secs,
            status,
        };
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        env.storage()
            .instance()
            .set(&DataKey::NextProposalId, &(proposal_id + 1));

        env.events().publish(
            (EVT, symbol_short!("proposed")),
            (proposal_id, proposer, target),
        );
        Ok(proposal_id)
    }

    /// Approve a pending proposal. Each registered signer for the
    /// proposal's role may approve exactly once; once approvals reach the
    /// role's threshold the proposal becomes `Approved` (executable).
    pub fn approve_action(env: Env, signer: Address, proposal_id: u64) -> Result_ {
        signer.require_auth();
        bump_instance(&env);

        let mut proposal: Proposal = env
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .ok_or(AccessControlError::ProposalNotFound)?;

        if proposal.status != ProposalStatus::Pending && proposal.status != ProposalStatus::Approved
        {
            return Err(AccessControlError::ProposalNotPending);
        }
        if env.ledger().timestamp() > proposal.expires_at {
            return Err(AccessControlError::ProposalExpired);
        }

        let config: MultiSigConfig = env
            .storage()
            .instance()
            .get(&DataKey::RoleConfig(proposal.role))
            .ok_or(AccessControlError::RoleNotConfigured)?;
        if !config.signers.contains(&signer) {
            return Err(AccessControlError::NotASigner);
        }
        if proposal.approvals.contains(&signer) {
            return Err(AccessControlError::AlreadyApproved);
        }

        proposal.approvals.push_back(signer.clone());
        if proposal.approvals.len() >= config.threshold {
            proposal.status = ProposalStatus::Approved;
        }
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        env.events()
            .publish((EVT, symbol_short!("approved")), (proposal_id, signer));
        Ok(())
    }

    /// Remove the caller's own prior approval from a still-pending
    /// proposal — lets a signer correct an approval given in error before
    /// execution.
    pub fn revoke_approval(env: Env, signer: Address, proposal_id: u64) -> Result_ {
        signer.require_auth();
        bump_instance(&env);

        let mut proposal: Proposal = env
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .ok_or(AccessControlError::ProposalNotFound)?;
        // Allowed on Pending or Approved (but not-yet-executed) proposals —
        // a signer can still change their mind right up until execution.
        if proposal.status != ProposalStatus::Pending && proposal.status != ProposalStatus::Approved
        {
            return Err(AccessControlError::ProposalNotPending);
        }
        let idx = proposal.approvals.iter().position(|a| a == signer);
        let idx = match idx {
            Some(i) => i as u32,
            None => return Err(AccessControlError::NoApprovalToRevoke),
        };
        proposal.approvals.remove(idx);

        // Re-derive status from the current approval count: revoking an
        // approval that had pushed the proposal over threshold must drop it
        // back to Pending, never leave a stale Approved with too few
        // approvals on record.
        let config: MultiSigConfig = env
            .storage()
            .instance()
            .get(&DataKey::RoleConfig(proposal.role))
            .ok_or(AccessControlError::RoleNotConfigured)?;
        proposal.status = if proposal.approvals.len() >= config.threshold {
            ProposalStatus::Approved
        } else {
            ProposalStatus::Pending
        };

        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        env.events()
            .publish((EVT, symbol_short!("revoked")), (proposal_id, signer));
        Ok(())
    }

    /// Reject a pending or approved proposal before it executes. Any registered
    /// signer for the proposal's role may reject — correcting a mistaken or
    /// malicious proposal doesn't need the full approval threshold, since
    /// rejecting only ever narrows what can execute, never widens it.
    pub fn reject_action(env: Env, signer: Address, proposal_id: u64) -> Result_ {
        signer.require_auth();
        bump_instance(&env);

        let mut proposal: Proposal = env
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .ok_or(AccessControlError::ProposalNotFound)?;
        if proposal.status != ProposalStatus::Pending {
            return Err(AccessControlError::ProposalNotPending);
        }
        let config: MultiSigConfig = env
            .storage()
            .instance()
            .get(&DataKey::RoleConfig(proposal.role))
            .ok_or(AccessControlError::RoleNotConfigured)?;
        if !config.signers.contains(&signer) {
            return Err(AccessControlError::NotASigner);
        }

        proposal.status = ProposalStatus::Rejected;
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        env.events()
            .publish((EVT, symbol_short!("rejected")), (proposal_id, signer));
        Ok(())
    }

    /// Execute an `Approved` proposal: either mutates this contract's own
    /// role configuration (self-management actions) or makes the
    /// corresponding cross-contract call into `proposal.target`, carrying
    /// this contract's own address so the target can authenticate the call
    /// as coming from a trusted `AccessControl` — never from an arbitrary
    /// caller. Any account may submit the execution once approvals are in
    /// place; nothing about *who* calls `execute_action` grants authority,
    /// only the proposal's already-recorded approvals do.
    pub fn execute_action(env: Env, caller: Address, proposal_id: u64) -> Result_ {
        caller.require_auth();
        bump_instance(&env);

        let mut proposal: Proposal = env
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .ok_or(AccessControlError::ProposalNotFound)?;

        if proposal.status != ProposalStatus::Approved {
            return Err(AccessControlError::ProposalNotApproved);
        }
        if env.ledger().timestamp() > proposal.expires_at {
            return Err(AccessControlError::ProposalExpired);
        }

        // CEI: flip status before the external call. Soroban invocations
        // are atomic, so a panic in the cross-contract call reverts this
        // write too — there's no way to end up "Executed" without the
        // effect actually having happened, or vice versa.
        proposal.status = ProposalStatus::Executed;
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        let this_contract = env.current_contract_address();
        if proposal.target == this_contract {
            Self::execute_self_management(&env, &proposal.action)?;
        } else {
            Self::execute_cross_contract(&env, &this_contract, &proposal.target, &proposal.action);
        }

        env.events()
            .publish((EVT, symbol_short!("executed")), (proposal_id, caller));
        Ok(())
    }

    fn execute_cross_contract(
        env: &Env,
        this_contract: &Address,
        target: &Address,
        action: &ActionPayload,
    ) {
        match action {
            ActionPayload::SetPaused(paused) => {
                // Every target contract exposes the same unified setter
                // name, so try each client in turn is unnecessary — the
                // proposal was raised against a specific `target`, and only
                // one of these three calls will actually match that
                // contract's deployed interface. Soroban resolves this at
                // the call site: whichever client's method the target
                // actually implements succeeds; the others are simply never
                // invoked because `target` only ever hosts one contract.
                PoolClient::new(env, target).set_paused_via_ac(this_contract, paused);
            }
            ActionPayload::SetYield(bps) => {
                PoolClient::new(env, target).set_yield_via_ac(this_contract, bps);
            }
            ActionPayload::SetTreasury(treasury) => {
                PoolClient::new(env, target).set_treasury_via_ac(this_contract, treasury);
            }
            ActionPayload::WithdrawRevenue(token, amount) => {
                PoolClient::new(env, target).withdraw_revenue_via_ac(this_contract, token, amount);
            }
            ActionPayload::SetOracleContract(oracle) => {
                PoolClient::new(env, target).set_oracle_contract_via_ac(this_contract, oracle);
            }
            ActionPayload::SetKycRequired(required) => {
                PoolClient::new(env, target).set_kyc_required_via_ac(this_contract, required);
            }
            ActionPayload::SetInvestorKyc(investor, approved) => {
                PoolClient::new(env, target).set_investor_kyc_via_ac(
                    this_contract,
                    investor,
                    approved,
                );
            }
            ActionPayload::SetMaxUtilization(bps) => {
                PoolClient::new(env, target).set_max_utilization_via_ac(this_contract, bps);
            }
            ActionPayload::SetOracle(oracle) => {
                InvoiceClient::new(env, target).set_oracle_via_ac(this_contract, oracle);
            }
            ActionPayload::RegisterDebtor(debtor_id, debtor_name, max_exposure) => {
                InvoiceClient::new(env, target).register_debtor_via_ac(
                    this_contract,
                    debtor_id,
                    debtor_name,
                    max_exposure,
                );
            }
            ActionPayload::DeactivateDebtor(debtor_id) => {
                InvoiceClient::new(env, target).deactivate_debtor_via_ac(this_contract, debtor_id);
            }
            ActionPayload::AddKeeper(keeper) => {
                InvoiceClient::new(env, target).add_keeper_via_ac(this_contract, keeper);
            }
            ActionPayload::SetLateThreshold(days) => {
                CreditScoreClient::new(env, target).set_late_threshold_via_ac(this_contract, days);
            }
            ActionPayload::SetScoreThresholds(excellent, very_good, good, fair) => {
                CreditScoreClient::new(env, target).set_score_thresholds_via_ac(
                    this_contract,
                    excellent,
                    very_good,
                    good,
                    fair,
                );
            }
            ActionPayload::RegisterAttestor(address, attestor_type, weight_bps) => {
                CreditScoreClient::new(env, target).register_attestor_via_ac(
                    this_contract,
                    address,
                    attestor_type,
                    weight_bps,
                );
            }
            ActionPayload::SetOracleRegistryInvoiceContract(invoice_contract) => {
                OracleRegistryClient::new(env, target)
                    .set_invoice_contract_via_ac(this_contract, invoice_contract);
            }
            ActionPayload::SetOracleRegistryTreasury(treasury) => {
                OracleRegistryClient::new(env, target).set_treasury_via_ac(this_contract, treasury);
            }
            ActionPayload::SetOracleRegistryConfig(
                min_stake,
                required_votes,
                quorum_bps,
                round_duration_secs,
                deregister_cooldown_secs,
            ) => {
                OracleRegistryClient::new(env, target).set_registry_config_via_ac(
                    this_contract,
                    min_stake,
                    required_votes,
                    quorum_bps,
                    round_duration_secs,
                    deregister_cooldown_secs,
                );
            }
            ActionPayload::SetOracleRegistryPaused(paused) => {
                OracleRegistryClient::new(env, target).set_paused_via_ac(this_contract, paused);
            }
            ActionPayload::SlashOracle(operator, bps, round_id, evidence) => {
                OracleRegistryClient::new(env, target).slash_oracle_via_ac(
                    this_contract,
                    operator,
                    bps,
                    round_id,
                    evidence,
                );
            }
            ActionPayload::AdminResolveRound(invoice_id, approved, reason) => {
                OracleRegistryClient::new(env, target).admin_resolve_round_via_ac(
                    this_contract,
                    invoice_id,
                    approved,
                    reason,
                );
            }
            ActionPayload::SetCompliancePaused(paused) => {
                ComplianceClient::new(env, target).set_paused_via_ac(this_contract, paused);
            }
            ActionPayload::RegisterScreener(screener) => {
                ComplianceClient::new(env, target)
                    .register_screener_via_ac(this_contract, screener);
            }
            ActionPayload::ConfirmScreenerRegistration(screener) => {
                ComplianceClient::new(env, target).confirm_screener_via_ac(this_contract, screener);
            }
            ActionPayload::DeregisterScreener(screener) => {
                ComplianceClient::new(env, target)
                    .deregister_screener_via_ac(this_contract, screener);
            }
            ActionPayload::SetRescreeningInterval(secs) => {
                ComplianceClient::new(env, target)
                    .set_rescreening_interval_via_ac(this_contract, secs);
            }
            ActionPayload::SetScreenerTimelock(secs) => {
                ComplianceClient::new(env, target)
                    .set_screener_timelock_via_ac(this_contract, secs);
            }
            ActionPayload::UpdateGovernanceConfig(quorum_bps, pass_bps) => {
                GovernanceClient::new(env, target).update_config_via_ac(
                    this_contract,
                    quorum_bps,
                    pass_bps,
                );
            }
            ActionPayload::SetCategoryQuorum(category, quorum_bps) => {
                GovernanceClient::new(env, target).set_category_quorum_via_ac(
                    this_contract,
                    category,
                    quorum_bps,
                );
            }
            ActionPayload::SetReferralPaused(paused) => {
                ReferralClient::new(env, target).set_paused_via_ac(this_contract, paused);
            }
            ActionPayload::SetReferralPool(pool) => {
                ReferralClient::new(env, target).set_pool_via_ac(this_contract, pool);
            }
            ActionPayload::SetBorrowRewardBps(bps) => {
                ReferralClient::new(env, target).set_borrow_reward_bps_via_ac(this_contract, bps);
            }
            ActionPayload::SetDepositRewardBps(bps) => {
                ReferralClient::new(env, target).set_deposit_reward_bps_via_ac(this_contract, bps);
            }
            ActionPayload::SetInvoiceAccessControl(new_ac) => {
                InvoiceClient::new(env, target).set_access_control_via_ac(this_contract, new_ac);
            }
            ActionPayload::SetCreditScoreAccessControl(new_ac) => {
                CreditScoreClient::new(env, target)
                    .set_access_control_via_ac(this_contract, new_ac);
            }
            ActionPayload::SetOracleRegistryAccessControl(new_ac) => {
                OracleRegistryClient::new(env, target)
                    .set_access_control_via_ac(this_contract, new_ac);
            }
            ActionPayload::SetComplianceAccessControl(new_ac) => {
                ComplianceClient::new(env, target).set_access_control_via_ac(this_contract, new_ac);
            }
            ActionPayload::SetGovernanceAccessControl(new_ac) => {
                GovernanceClient::new(env, target).set_access_control_via_ac(this_contract, new_ac);
            }
            ActionPayload::SetReferralAccessControl(new_ac) => {
                ReferralClient::new(env, target).set_access_control_via_ac(this_contract, new_ac);
            }
            ActionPayload::AddSigner(_, _)
            | ActionPayload::RemoveSigner(_, _)
            | ActionPayload::SetThreshold(_, _)
            | ActionPayload::SetProposalExpiry(_) => {
                // Self-management actions never reach here: execute_action
                // routes them to execute_self_management() based on
                // `target == this_contract`, which is enforced at
                // propose_action time (self-management payloads are only
                // ever proposed with target = this contract's own address
                // by the SDK/frontend; a mismatched target here would just
                // mean the wrong client call is skipped, since none of the
                // Pool/Invoice/CreditScore clients implement these methods).
            }
        }
    }

    fn execute_self_management(env: &Env, action: &ActionPayload) -> Result_ {
        match action {
            ActionPayload::AddSigner(role, address) => {
                let mut config = Self::role_config_or_default(env, *role);
                if config.signers.contains(address) {
                    return Err(AccessControlError::DuplicateSigner);
                }
                // #1141: cap signer set size to prevent unbounded iteration overhead.
                if config.signers.len() >= MAX_SIGNERS_PER_ROLE {
                    return Err(AccessControlError::MaxSignersExceeded);
                }
                config.signers.push_back(address.clone());
                env.storage()
                    .instance()
                    .set(&DataKey::RoleConfig(*role), &config);
                Ok(())
            }
            ActionPayload::RemoveSigner(role, address) => {
                let mut config: MultiSigConfig = env
                    .storage()
                    .instance()
                    .get(&DataKey::RoleConfig(*role))
                    .ok_or(AccessControlError::RoleNotConfigured)?;
                let idx = config
                    .signers
                    .iter()
                    .position(|a| a == *address)
                    .ok_or(AccessControlError::SignerNotFound)?;
                config.signers.remove(idx as u32);
                if config.signers.len() < config.threshold {
                    return Err(AccessControlError::InvalidThreshold);
                }
                env.storage()
                    .instance()
                    .set(&DataKey::RoleConfig(*role), &config);
                Ok(())
            }
            ActionPayload::SetThreshold(role, threshold) => {
                let mut config = Self::role_config_or_default(env, *role);
                Self::validate_config(&config.signers, *threshold)?;
                config.threshold = *threshold;
                env.storage()
                    .instance()
                    .set(&DataKey::RoleConfig(*role), &config);
                Ok(())
            }
            ActionPayload::SetProposalExpiry(new_secs) => {
                if *new_secs == 0 {
                    return Err(AccessControlError::InvalidExpiryWindow);
                }
                env.storage()
                    .instance()
                    .set(&DataKey::ProposalExpirySecs, new_secs);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn role_config_or_default(env: &Env, role: Role) -> MultiSigConfig {
        env.storage()
            .instance()
            .get(&DataKey::RoleConfig(role))
            .unwrap_or(MultiSigConfig {
                signers: Vec::new(env),
                threshold: 0,
            })
    }

    fn is_self_management(action: &ActionPayload) -> bool {
        matches!(
            action,
            ActionPayload::AddSigner(_, _)
                | ActionPayload::RemoveSigner(_, _)
                | ActionPayload::SetThreshold(_, _)
                | ActionPayload::SetProposalExpiry(_)
        )
    }

    /// Self-management actions (this contract's own role config / expiry
    /// window) and access-control-rotation actions (repointing a target
    /// contract's trust anchor) both bypass every other role's threshold
    /// entirely if left ungated — so both are restricted to
    /// `Role::SuperAdmin`, the role with the highest bar to reconfigure.
    fn requires_super_admin(action: &ActionPayload) -> bool {
        Self::is_self_management(action)
            || matches!(
                action,
                ActionPayload::SetInvoiceAccessControl(_)
                    | ActionPayload::SetCreditScoreAccessControl(_)
                    | ActionPayload::SetOracleRegistryAccessControl(_)
                    | ActionPayload::SetComplianceAccessControl(_)
                    | ActionPayload::SetGovernanceAccessControl(_)
                    | ActionPayload::SetReferralAccessControl(_)
            )
    }

    fn validate_config(signers: &Vec<Address>, threshold: u32) -> Result_ {
        if threshold == 0 || threshold > signers.len() {
            return Err(AccessControlError::InvalidThreshold);
        }
        for i in 0..signers.len() {
            let a = signers.get(i).unwrap();
            for j in 0..i {
                if signers.get(j).unwrap() == a {
                    return Err(AccessControlError::DuplicateSigner);
                }
            }
        }
        Ok(())
    }
}
