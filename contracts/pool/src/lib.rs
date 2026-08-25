// === AUTHORIZED CALLERS ===
// - Admin: pause(), unpause(), set_yield(), add_token(), admin-only setters
// - Pool contract: N/A (this is the pool contract)
// - Invoice contract: may call pool for state reads
// - Anyone: public view functions

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, token,
    Address, BytesN, Env, IntoVal, Map, Symbol, Vec,
};

use soroban_sdk::contractclient;

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PoolContractVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

fn parse_pool_version() -> PoolContractVersion {
    let v = env!("CARGO_PKG_VERSION");
    let mut parts = v.splitn(3, '.');
    let major = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts
        .next()
        .and_then(|s| s.split('-').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    PoolContractVersion {
        major,
        minor,
        patch,
    }
}

#[contracterror(export = false)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PoolError {
    AlreadyInitialized = 0,
    NotInitialized = 1,
    TokenNotAccepted = 2,
    TokenAlreadyAccepted = 3,
    TokenNotWhitelisted = 4,
    InvoiceNotFound = 5,
    AlreadyFullyRepaid = 6,
    Overpayment = 7,
    InvalidAmount = 8,
    Unauthorized = 9,
    StorageCorrupted = 10,
    ShareTokenNotConfigured = 11,
    InvalidFeeTier = 24,
    FeeTierNotFound = 25,
    ContractPaused = 12,
    CollateralNotFound = 13,
    CollateralAlreadySettled = 14,
    // #235
    DepositBelowMinimum = 15,
    // #236
    InsufficientRevenue = 16,
    TreasuryNotConfigured = 17,
    // #244
    WithdrawalExceedsLimit = 18,
    WithdrawalCooldownActive = 19,
    // #247
    InsufficientCoFundShare = 20,
    InsufficientLiquidity = 26,
    // #217: withdrawal queue errors
    WithdrawalRequestNotFound = 21,
    AlreadyQueuedForWithdrawal = 22,
    InvalidRequestId = 23,
    // #222: token removal safety checks
    TokenHasActiveBalances = 27,
    TokenHasDeployedCapital = 28,
    TokenHasPendingWithdrawals = 29,
    // #233
    ConcentrationLimitExceeded = 30,
    // #275: utilization guardrails
    UtilizationLimitExceeded = 33,
    AmountOverflow = 34,
    BatchTooLarge = 35,
    // #227 / #222
    YieldProposalNotFound = 31,
    YieldChangeNotReady = 32,
    // #367: unsupported token decimal precision
    UnsupportedTokenDecimals = 36,
    // #384: distinct zero and negative amount errors
    ZeroAmount = 37,
    NegativeAmount = 38,
    // fee-on-transfer token mismatch
    TransferMismatch = 39,
    // #109: investor not KYC-approved
    KycNotApproved = 40,
    // collateral threshold/ratio validation
    InvalidThreshold = 41,
    // #386: seize_collateral requires invoice to be in Defaulted status
    NotDefaulted = 42,
    // #385: pool address stored in invoice contract does not match this pool
    InvoicePoolMismatch = 43,
    // #333: share token must exist and be initialized before token registration
    InvalidShareToken = 44,
    // #335: duplicate invoice IDs in batch funding
    DuplicateInvoiceId = 45,
    InvalidCollateralThreshold = 46,
    InvalidCollateralBps = 47,
    // #337: tri-state KYC errors
    KycNotRequested = 48,
    KycRejected = 49,
    // #338: upgrade timelock errors
    UpgradeTimelockNotExpired = 50,
    InvalidUpgradeTimelock = 51,
    // #340: invalid WASM hash (e.g. all-zero)
    InvalidWasmHash = 52,
    // #532: pool token balance insufficient to cover withdrawal
    InsufficientPoolFunds = 53,
    // #565: admin key rotation timelock errors
    AdminChangePending = 54,
    AdminChangeTimelockNotExpired = 55,
    NoAdminChangeProposed = 56,
    // #655: estimate_repayment rejects an as_of_timestamp earlier than now
    TimestampInPast = 57,
    // #531: funding rejected because the invoice due date has already passed
    InvoiceExpired = 58,
    // #742: critical admin operations must go through the two-step proposal flow
    OperationRequiresProposal = 59,
    ProposalNotFound = 60,
    ProposalNotReady = 61,
    ProposalAlreadyExecuted = 62,
    ProposalAlreadyCancelled = 63,
    InvalidOperationDelay = 64,
    // #567: token removal blocked while a funded invoice still references this token
    TokenHasActiveCofundingCommitments = 65,
    // #860: multi-investor co-funding rounds
    CoFundingRoundNotFound = 66,
    CoFundingRoundAlreadyExists = 67,
    CoFundingRoundNotOpen = 68,
    CoFundingRoundAlreadyFinalized = 69,
    CoFundingDeadlinePassed = 70,
    CoFundingDeadlineNotPassed = 71,
    CoFundingBelowMinimum = 72,
    CoFundingInvestorCapExceeded = 73,
    CoFundingNoCommitment = 74,
    CoFundingRoundNotFilled = 75,
    CoFundingTooManyParticipants = 76,
    InvalidCoFundingParams = 77,
    // #865: global withdrawal-queue depth cap reached for this token
    WithdrawalQueueFull = 78,
    // #863: utilization-driven rate model errors
    InvalidRateModelConfig = 79,
    RateModelNotConfigured = 80,
    RateModelProposalNotFound = 81,
    RateModelChangeNotReady = 82,
    // #867: on-chain compliance / sanctions screening gate
    ComplianceNotCleared = 83,
    ComplianceCheckFailed = 84,
    // #777: Reflector oracle collateral price feed — neither the oracle
    // nor the admin fallback has a usable price for the requested token.
    OraclePriceUnavailable = 85,
    // #773: loyalty tier list rejected by set_loyalty_tiers (empty, out of
    // ascending order, or a bonus_bps above MAX_LOYALTY_BONUS_BPS)
    InvalidLoyaltyTiers = 86,
    // #992: deposit's optional min_rate guard rejected the current rate
    RateBelowMinimum = 87,
    // #864: role-based multisig access-control
    AccessControlNotConfigured = 88,
    AccessControlAlreadyConfigured = 89,
    // #1025: secondary market errors
    ListingNotFound = 90,
    ListingNotOpen = 91,
    ListingNotSeller = 92,
    InsufficientListingBalance = 93,
    TooManyListings = 94,
    ListingPriceMismatch = 95,
    // #789: invoice contract declined the Funded -> Cancelled transition
    InvoiceNotCancelled = 96,
    // #1038: governance contract not configured
    GovernanceNotConfigured = 97,
}

type PoolResult<T> = Result<T, PoolError>;

const DEFAULT_YIELD_BPS: u32 = 800;
const DEFAULT_FACTORING_FEE_BPS: u32 = 0;
const BPS_DENOM: u32 = 10_000;
const SECS_PER_YEAR: u64 = 31_536_000;
// #367: Stellar-native tokens use 7 decimal places (stroops)
const EXPECTED_DECIMALS: u32 = 7;
const SECS_PER_DAY: u64 = 86_400;
// #773: loyalty bonus tiers — sanity ceiling on any single tier's bonus
// (20%) so a misconfigured admin call can't promise an unpayable APY.
const MAX_LOYALTY_BONUS_BPS: u32 = 2_000;
// #275: default max utilization — disabled (10_000 bps = 100%).
// Many flows legitimately deploy 100% of available liquidity.
const DEFAULT_MAX_UTILIZATION_BPS: u32 = 10_000;
// #275: warning threshold — 80% (8_000 bps)
const DEFAULT_UTILIZATION_WARNING_BPS: u32 = 8_000;
const DEFAULT_COLLATERAL_THRESHOLD: i128 = 100_000_000_000; // 10,000 USDC
const DEFAULT_COLLATERAL_BPS: u32 = 2_000;
const DEFAULT_YIELD_CHANGE_COOLDOWN_SECS: u64 = 86_400; // 24 hours
const DEFAULT_MAX_YIELD_CHANGE_BPS: u32 = 200; // +/- 200 bps per adjustment
                                               // #227: yield timelock — 48 hours default delay for two-step yield change
const DEFAULT_YIELD_TIMELOCK_SECS: u64 = 172_800; // 48 hours
                                                  // #235: minimum deposit — 0 = disabled
const DEFAULT_MIN_DEPOSIT_AMOUNT: i128 = 0;
// #233: max single-investor concentration — 2_000 bps = 20% (0 = disabled, 10_000 = 100%)
const DEFAULT_MAX_SINGLE_INVESTOR_BPS: u32 = 2_000;
// #244: withdrawal rate limiting — 10_000 bps (100%) and 0s = disabled by default
const DEFAULT_MAX_SINGLE_WITHDRAWAL_BPS: u32 = 10_000;
const DEFAULT_WITHDRAWAL_COOLDOWN_SECS: u64 = 0;
const DEFAULT_MAX_WITHDRAWAL_QUEUE_AGE_DAYS: u32 = 30;
// #860: co-funding rounds — cap on distinct investors per round so
// finalization/repayment distribution (which iterates participants) stays
// bounded, matching the existing MAX_BATCH_SIZE convention used elsewhere.
const MAX_CO_FUNDING_PARTICIPANTS: u32 = 20;
// #865: global cap on outstanding withdrawal-queue entries per token (0 = unlimited).
// Bounds the O(n) queue scan/rewrite cost in request_withdrawal/cancel_withdrawal_request/
// process_withdrawal_queue. Each investor can already only hold one queued request at a
// time (request_withdrawal rejects a second with AlreadyQueuedForWithdrawal), so this is
// effectively a cap on the number of distinct investors queued at once.
const DEFAULT_MAX_WITHDRAWAL_QUEUE_DEPTH: u32 = 500;
// #865: fixed-size ring buffer capacity for the trailing deposit-inflow rate used by
// estimate_withdrawal_wait/get_liquidity_forecast, mirroring the credit_score contract's
// PaymentHistory rolling-window pattern.
const MAX_INFLOW_HISTORY: u32 = 50;
// #863: fixed-size ring buffer capacity for the per-token rate history
// ((timestamp, utilization_bps, rate_bps) samples) charted by the frontend.
// 720 samples ≈ 30 days at hourly granularity; oldest entry is evicted on
// overflow so storage never grows unbounded.
const MAX_RATE_HISTORY: u32 = 720;
// #863: absolute protocol ceiling for any rate the curve can produce or an
// admin can configure — matches the existing 5_000 bps cap in set_yield /
// propose_yield_change so the dynamic model can never price beyond what the
// manual override already allows.
const MAX_RATE_BPS_CAP: u32 = 5_000;

const LEDGERS_PER_DAY: u32 = 17_280;
const ACTIVE_INVOICE_TTL: u32 = LEDGERS_PER_DAY * 365;
// 5 years — aligned with the invoice contract's DEFAULT_COMPLETED_INVOICE_TTL so a
// pool FundedInvoice record does not expire long before its invoice record (#636).
const COMPLETED_INVOICE_TTL: u32 = LEDGERS_PER_DAY * 365 * 5;
// Collateral records are kept for 90 days after settlement (repayment or
// seizure) so auditors and the SME have time to verify the record exists,
// covering the post-settlement audit/dispute window.
const SETTLEMENT_COLLATERAL_TTL: u32 = LEDGERS_PER_DAY * 90;
const INSTANCE_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 30;
const INSTANCE_LIFETIME_THRESHOLD: u32 = LEDGERS_PER_DAY * 7;
const UPGRADE_TIMELOCK_SECS: u64 = 86400; // 24 hours — default
const MIN_UPGRADE_TIMELOCK_SECS: u64 = 3_600; // 1 hour minimum (#338)
const ADMIN_CHANGE_TIMELOCK_SECS: u64 = 172_800; // 48 hours — #565
                                                 // #742: two-step confirmation delay for critical admin operations.
                                                 // Default 24 hours, configurable down to MIN_OPERATION_DELAY_SECS.
const OPERATION_DELAY_SECS: u64 = 86_400; // 24 hours default
const MIN_OPERATION_DELAY_SECS: u64 = 3_600; // 1 hour minimum
const CURRENT_MIGRATION_VERSION: u32 = 1;

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct WithdrawalRequest {
    pub investor: Address,
    pub token: Address,
    pub shares: i128,
    pub requested_at: u64,
    pub request_id: u64,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct InflowEvent {
    pub amount: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RateModelConfig {
    pub base_rate_bps: u32,
    pub optimal_utilization_bps: u32,
    pub slope1_bps: u32,
    pub slope2_bps: u32,
    pub max_rate_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RateSnapshot {
    pub timestamp: u64,
    pub utilization_bps: u32,
    pub rate_bps: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct RateModelProposal {
    pub config: RateModelConfig,
    pub proposed_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct PoolConfig {
    pub invoice_contract: Address,
    pub admin: Address,
    pub yield_bps: u32,
    pub factoring_fee_bps: u32,
    pub compound_interest: bool,
    pub last_yield_change_at: u64,
    pub yield_change_cooldown_secs: u64,
    pub max_yield_change_bps: u32,
    // #227: yield timelock — two-step yield change
    pub proposed_yield_bps: u32,
    pub yield_proposal_at: u64,
    pub yield_timelock_secs: u64,
    // #235: minimum deposit per transaction (0 = disabled)
    pub min_deposit_amount: i128,
    // #233: maximum single-investor concentration (2_000 = 20%, 10_000 = 100% = disabled)
    pub max_single_investor_bps: u32,
    // #244: withdrawal rate limiting (10_000 bps = disabled; 0 secs = disabled)
    pub max_single_withdrawal_bps: u32,
    pub withdrawal_cooldown_secs: u64,
    // #275: pool utilization guardrails (bps)
    pub max_utilization_bps: u32,
    pub utilization_warning_bps: u32,
    pub max_withdrawal_queue_age_days: u32,
    // #865: global cap on outstanding withdrawal-queue entries per token (0 = unlimited).
    pub max_withdrawal_queue_depth: u32,
}

#[contracttype]
#[derive(Clone, Default)]
pub struct PoolTokenTotals {
    pub pool_value: i128,
    pub total_deployed: i128,
    pub total_paid_out: i128,
    pub total_fee_revenue: i128,
    pub reward_per_share: i128,
    // #236: protocol fee revenue available for treasury withdrawal (separate from investor pool)
    pub protocol_revenue: i128,
}

#[contracttype]
#[derive(Clone, PartialEq, Debug)]
pub struct FeeTier {
    pub min_amount: i128,
    pub max_amount: i128,
    pub min_credit_score: u32,
    pub fee_bps: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct CreditScoreData {
    pub sme: Address,
    pub score: u32,
    pub total_invoices: u32,
    pub paid_on_time: u32,
    pub paid_late: u32,
    pub defaulted: u32,
    pub total_volume: i128,
    pub average_payment_days: i64,
    pub last_updated: u64,
    pub score_version: u32,
}

const REWARD_PRECISION: i128 = 1_000_000_000_000;
const MAX_BATCH_SIZE: u32 = 20;

// #367: Token configuration including decimal precision
#[contracttype]
#[derive(Clone)]
pub struct TokenConfig {
    pub token: Address,
    pub share_token: Address,
    pub decimals: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct ExchangeRateBounds {
    pub min_bps: u32,
    pub max_bps: u32,
}

#[contracttype]
#[derive(Clone, Default)]
pub struct InvestorPosition {
    pub deposited: i128,
    pub available: i128,
    pub deployed: i128,
    pub earned: i128,
    pub deposit_count: u32,
    // #773: ledger timestamp this investor's current continuous position
    // started (0 = no active position). Reset to 0 when `available` returns
    // to zero so a later re-deposit restarts the loyalty timer; used to
    // compute the tenure-based loyalty bonus tier in `get_deposit_info`.
    pub loyalty_start_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct LoyaltyTier {
    pub min_days: u32,
    pub bonus_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DepositInfo {
    pub deposited_at: u64,
    pub days_active: u64,
    pub tier: u32,
    pub bonus_bps: u32,
    pub base_apy_bps: u32,
    pub effective_apy_bps: u32,
    pub next_tier_days: Option<u32>,
}

#[contracttype]
#[derive(Clone)]
pub struct FundedInvoice {
    pub invoice_id: u64,
    pub sme: Address,
    pub token: Address,
    pub principal: i128,
    pub funded_at: u64,
    pub factoring_fee: i128,
    pub due_date: u64,
    pub repaid_amount: i128,
    pub co_funding_round_id: Option<u64>,
    pub locked_yield_bps: u32,
}

// #860: multi-investor co-funding rounds — every co-funder ranks pari passu
// and owns a proportional bps slice of one invoice's principal, interest,
// and collateral claim. This is distinct from (and a prerequisite for) any
// future tranching/waterfall work.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum CoFundingStatus {
    Open,
    Filled,
    Cancelled,
    Expired,
}

#[contracttype]
#[derive(Clone)]
pub struct CoFundingRound {
    pub invoice_id: u64,
    pub token: Address,
    pub sme: Address,
    pub due_date: u64,
    pub target_principal: i128,
    pub committed_principal: i128,
    pub funding_deadline: u64,
    pub status: CoFundingStatus,
    pub min_commitment: i128,
    pub max_investor_bps: u32,
    pub participants: Vec<Address>,
}

#[contracttype]
#[derive(Clone)]
pub struct FundingRequest {
    pub invoice_id: u64,
    pub principal: i128,
    pub sme: Address,
    pub due_date: u64,
    pub token: Address,
}

// Bundles `market_settle_listing`'s params (matches `FundingRequest`'s
// existing role of keeping multi-field contract entrypoints under clippy's
// too-many-arguments threshold).
#[contracttype]
#[derive(Clone)]
pub struct ListingSettlement {
    pub buyer: Address,
    pub seller: Address,
    pub invoice_id: u64,
    pub is_co_funding: bool,
    pub amount_or_bps: u64,
    pub price: i128,
}

#[contracttype]
#[derive(Clone)]
pub struct OpenCoFundingRequest {
    pub invoice_id: u64,
    pub token: Address,
    pub target_principal: i128,
    pub sme: Address,
    pub due_date: u64,
    pub funding_deadline: u64,
    pub min_commitment: i128,
    pub max_investor_bps: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct RepaymentRequest {
    pub invoice_id: u64,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Default)]
pub struct PoolStorageStats {
    pub total_funded_invoices: u64,
    pub active_funded_invoices: u64,
    pub cleaned_invoices: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct CollateralConfig {
    pub threshold: i128,
    pub collateral_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum AdminOperation {
    RemoveToken(Address),
    SetCollateralConfig(i128, u32),
    SeizeCollateral(u64),
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    pub operation: AdminOperation,
    pub execute_after: u64,
    pub proposed_at: u64,
    pub proposer: Address,
    pub executed: bool,
    pub cancelled: bool,
}

#[contracttype]
#[derive(Clone, PartialEq, Debug)]
pub enum KycStatus {
    NotRequested,
    Approved,
    Rejected,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ComplianceGateConfig {
    pub registry: Address,
    pub required: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct CollateralDeposit {
    pub invoice_id: u64,
    pub depositor: Address,
    pub token: Address,
    pub amount: i128,
    pub settled: bool,
    pub posted_at: u64,
    pub released_at: u64,
    pub seized_at: u64,
    // #764: the CollateralConfig in effect when this deposit was made,
    // snapshotted so a later admin change to the pool's collateral ratio
    // can't retroactively make an already-posted deposit "insufficient" at
    // funding time. fund_invoice_request validates the deposit against
    // *this* snapshot, not whatever CollateralConfig is live when funding
    // is attempted.
    pub collateral_bps_at_deposit: u32,
    pub threshold_at_deposit: i128,
}

#[contracttype]
pub enum DataKey {
    Config,
    ShareToken(Address),
    FundedInvoice(u64),
    AcceptedTokens,
    TokenTotals(Address),
    Initialized,
    StorageStats,
    Paused,
    ProposedWasmHash,
    UpgradeScheduledAt,
    // #111: exchange rate for each accepted token (bps of USD, e.g. 10000 = 1:1 USD)
    ExchangeRate(Address),
    ExchangeRateBounds(Address),
    // #367: token configuration including decimal precision
    TokenConfig(Address),
    // #109: KYC / investor whitelist
    KycRequired,
    InvestorKyc(Address),
    // Collateral: threshold config and per-invoice deposits
    CollateralConfig,
    CollateralDeposit(u64),

    ReentrancyGuard,
    InvestorRewardSnapshot(Address, Address),
    // #244: last withdrawal timestamp per (investor, token)
    LastWithdrawalTime(Address, Address),
    // #236: treasury address for protocol revenue withdrawals
    Treasury,
    CreditScoreContract,
    FeeTier(u32),
    FeeTierIds,
    // #247: co-fund share ownership per (invoice_id, investor): stores bps (0-10_000)
    CoFundShare(u64, Address),
    // #233: per-investor deposited amount per token for concentration limit
    InvestorPosition(Address, Address),
    ContractVersion,
    MigrationVersion,
    WithdrawalQueue(Address),
    WithdrawalQueueCounter(Address),
    WithdrawalRequest(Address, u64), // (investor, request_id)
    UpgradeTimelockSecs,
    // #565: admin key rotation timelock
    PendingAdmin,
    AdminChangeScheduledAt,
    // #742: two-step confirmation for critical admin operations
    Proposal(u64),
    NextProposalId,
    OperationDelaySecs,
    // #860: multi-investor co-funding rounds, keyed by invoice_id (one round
    // per invoice). CoFundingRoundIds is an append-only registry so
    // `list_co_funding_rounds` doesn't need to scan the whole invoice ID space.
    CoFundingRound(u64),
    CoFundingRoundIds,
    CoFundCommitted(u64, Address),
    // #865: per-token index of every invoice_id ever funded, so liquidity forecasting and
    // wait estimation can enumerate open invoices without assuming caller-supplied
    // invoice_ids are a dense sequential range (they are not — see fund_invoice_request).
    TokenInvoiceIds(Address),
    // #865: deposit-inflow ring buffer (token -> current length / start index), mirroring
    // the credit_score contract's PaymentHistory pattern. Instance storage: small, hot.
    InflowHistoryLen(Address),
    InflowHistoryStart(Address),
    // #865: individual ring-buffer slot: (token, slot_index) -> InflowEvent.
    InflowRecord(Address, u32),
    // #863: per-token kinked interest-rate model parameters.
    RateModel(Address),
    // #863: pending timelocked rate-model change per token.
    PendingRateModel(Address),
    // #863: timestamp of the last executed rate-model change per token —
    // enforces the same cooldown pattern as yield changes.
    RateModelChangedAt(Address),
    // #863: rate-history ring buffer bookkeeping (token -> (length, start)),
    // mirroring the InflowHistory pattern above. Combined into one (u32, u32)
    // tuple (rather than two separate keys) to stay within the #[contracttype]
    // union's 50-case cap alongside #866's InsuranceContract addition.
    RateHistoryBounds(Address),
    // #863: individual rate-history ring-buffer slot: (token, slot_index) -> RateSnapshot.
    RateRecord(Address, u32),
    // #866: optional default-insurance reserve integration
    InsuranceContract,
}

const EVT: Symbol = symbol_short!("pool");
// #799: referral registry contract address, if configured.
const REFERRAL_CFG: Symbol = symbol_short!("ref_cfg");
// #867: stored under a Symbol key (not DataKey) — pool DataKey is already at
// the Soroban 50-variant ceiling after #863.
const COMPLIANCE_CFG: Symbol = symbol_short!("cmp_cfg");
// #777: Reflector Oracle config, same Symbol-key workaround as #867 above.
const REFLECTOR_ORACLE: Symbol = symbol_short!("rflector");
// #773: admin-configurable loyalty tier list. Stored under a Symbol key
// rather than a DataKey variant — DataKey is already at Soroban's 50-variant
// ceiling (see #867/#777 above).
const LOYALTY_TIERS: Symbol = symbol_short!("loy_tier");
// Pre-existing build breakage found while working this branch: these two
// keys are read/written by `get_insurance_contract`/`set_insurance_contract`
// and `record_referral_activity`/the referral registry setter, but
// `DataKey::InsuranceContract` and a `REFERRAL_CFG` constant were both
// missing on `main` (the enum-variant version would also have breached the
// 50-variant ceiling above). Restored as Symbol keys, matching the same
// workaround already used for compliance/Reflector config.
const INSURANCE_CFG: Symbol = symbol_short!("ins_cfg");
// #864: role-based multisig access-control contract, if configured. Symbol
// key, not a DataKey variant — DataKey is already at Soroban's 50-variant
// ceiling (see #867/#777/#866/#799/#869 above).
const ACCESS_CONTROL: Symbol = symbol_short!("ac_addr");
// #1038: governance contract address for governance-gated parameter changes
const GOVERNANCE: Symbol = symbol_short!("gov_addr");
const ORACLE_STALE_SECS: Symbol = symbol_short!("o_stale");
// #777: per-token admin fallback price, stored as a single Map so it
// doesn't need its own DataKey variant either.
const ORACLE_FALLBACK_PX: Symbol = symbol_short!("fb_price");
// #777: default max age (seconds) a Reflector price may have before it's
// treated as stale and the admin fallback price is used instead.
const DEFAULT_ORACLE_STALE_SECS: u64 = 3600;
// Trusted secondary-market satellite contract allowed to call
// market_settle_listing. Symbol key because DataKey is at Soroban's
// 50-variant ceiling.
const SECONDARY_MARKET_CONTRACT: Symbol = symbol_short!("mkt_ctrt");
// #1036: trusted collateral-risk-response satellite (contracts/auction) —
// the only caller risk_liquidate_collateral accepts. Symbol key,
// not a DataKey variant — DataKey is at Soroban's 50-variant ceiling. The
// live-ratio computation, danger-threshold config, and at-risk/liquidation
// decision logic all live in the satellite (originally implemented directly
// in pool, but that pushed pool's wasm ~68KB over Soroban's 200KB deploy
// limit); pool keeps only the storage mutation, gated to this trusted caller,
// mirroring market_settle_listing's pattern for secondary_market above.
const RISK_CONTRACT: Symbol = symbol_short!("risk_ctr");

#[contractclient(name = "CreditScoreClient")]
pub trait CreditScoreContract {
    fn get_credit_score(env: Env, sme: Address) -> CreditScoreData;
    fn record_funding(env: Env, caller: Address, invoice_id: u64, sme: Address, amount: i128);
}

#[contractclient(name = "InvoiceContractClient")]
pub trait InvoiceContract {
    fn get_authorized_pool(env: Env) -> Address;
    fn is_invoice_defaulted(env: Env, id: u64) -> bool;
    fn record_funding(env: Env, id: u64, amount: i128, pool: Address);
    fn mark_cancelled(env: Env, id: u64, pool: Address);
}

#[contractclient(name = "ComplianceClient")]
pub trait ComplianceContract {
    fn is_cleared(env: Env, address: Address) -> bool;
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ReflectorAsset {
    Stellar(Address),
    Other(Symbol),
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ReflectorPriceData {
    pub price: i128,
    pub timestamp: u64,
}

#[contractclient(name = "ReflectorClient")]
pub trait ReflectorContract {
    fn lastprice(env: Env, asset: ReflectorAsset) -> Option<ReflectorPriceData>;
}

// Pre-existing build breakage found while working this branch: everything
// below this comment through `ReferralContract` was referenced (fund_invoice_request's
// insurance-purchase call, record_referral_activity) but never defined on
// `main`, so the pool contract didn't compile at HEAD. Restored here — same
// "local minimal mirror, decoded by field name" convention as
// `CreditScoreData`/`ReflectorPriceData` above — so the crate builds. Not
// part of any of the four assigned issues.

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CoverageRecord {
    pub premium_paid: i128,
}

#[contractclient(name = "InsuranceClient")]
pub trait InsuranceContract {
    fn purchase_coverage(
        env: Env,
        payer: Address,
        invoice_id: u64,
        principal: i128,
        sme: Address,
        due_date: u64,
        token: Address,
    ) -> CoverageRecord;
}

#[contractclient(name = "ReferralClient")]
pub trait ReferralContract {
    fn record_activity(
        env: Env,
        caller: Address,
        referee: Address,
        kind: Symbol,
        fee_amount: i128,
        token: Address,
    ) -> i128;
}

// Cache for config to reduce storage reads
fn get_config_cached(env: &Env) -> PoolResult<PoolConfig> {
    env.storage()
        .instance()
        .get(&DataKey::Config)
        .ok_or(PoolError::NotInitialized)
}

// Optimized bump that only extends if needed
fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

fn require_not_paused(env: &Env) {
    if env
        .storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Paused)
        .unwrap_or(false)
    {
        panic_with_error!(env, PoolError::ContractPaused);
    }
}

// #109 / #337: enforce KYC when required — tri-state status.
fn require_kyc_approved(env: &Env, investor: &Address) -> PoolResult<()> {
    let kyc_required: bool = env
        .storage()
        .instance()
        .get(&DataKey::KycRequired)
        .unwrap_or(false);
    if kyc_required {
        let status: KycStatus = env
            .storage()
            .persistent()
            .get(&DataKey::InvestorKyc(investor.clone()))
            .unwrap_or(KycStatus::NotRequested);
        match status {
            KycStatus::Approved => {}
            KycStatus::NotRequested => return Err(PoolError::KycNotRequested),
            KycStatus::Rejected => return Err(PoolError::KycRejected),
        }
    }
    Ok(())
}

fn set_funded_invoice_ttl(env: &Env, invoice_id: u64, is_completed: bool) {
    let ttl = if is_completed {
        COMPLETED_INVOICE_TTL
    } else {
        ACTIVE_INVOICE_TTL
    };
    let key = DataKey::FundedInvoice(invoice_id);
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(&key, ttl, ttl);
    }
}

fn fixed_point_pow(mut base: u128, mut exp: u64, precision: u128) -> u128 {
    let mut result = precision;
    while exp > 0 {
        if exp & 1 == 1 {
            result = result * base / precision;
        }
        base = base * base / precision;
        exp >>= 1;
    }
    result
}

fn div_round_half_up(numerator: u128, denominator: u128) -> PoolResult<u128> {
    if denominator == 0 {
        return Err(PoolError::AmountOverflow);
    }

    numerator
        .checked_add(denominator / 2)
        .ok_or(PoolError::AmountOverflow)
        .map(|rounded| rounded / denominator)
}

fn calculate_interest(
    principal: u128,
    yield_bps: u32,
    elapsed_secs: u64,
    is_compound: bool,
) -> PoolResult<u128> {
    let denominator = BPS_DENOM as u128 * SECS_PER_YEAR as u128;
    if !is_compound {
        let numerator = principal
            .checked_mul(yield_bps as u128)
            .and_then(|value| value.checked_mul(elapsed_secs as u128))
            .ok_or(PoolError::AmountOverflow)?;
        return div_round_half_up(numerator, denominator);
    }
    let elapsed_days = elapsed_secs / SECS_PER_DAY;
    let mut amount_numerator = principal
        .checked_mul(denominator)
        .ok_or(PoolError::AmountOverflow)?;
    if elapsed_days > 0 {
        let daily_rate_num = yield_bps as u128 * SECS_PER_DAY as u128;
        let num = denominator
            .checked_add(daily_rate_num)
            .ok_or(PoolError::AmountOverflow)?;
        let growth_factor = fixed_point_pow(num, elapsed_days, denominator);
        amount_numerator = principal
            .checked_mul(growth_factor)
            .ok_or(PoolError::AmountOverflow)?;
    }
    let remaining_secs = elapsed_secs % SECS_PER_DAY;
    if remaining_secs > 0 {
        let remaining_rate_num = (yield_bps as u128)
            .checked_mul(remaining_secs as u128)
            .ok_or(PoolError::AmountOverflow)?;
        let accrued_numerator = amount_numerator
            .checked_mul(remaining_rate_num)
            .and_then(|value| value.checked_div(denominator))
            .ok_or(PoolError::AmountOverflow)?;
        amount_numerator = amount_numerator
            .checked_add(accrued_numerator)
            .ok_or(PoolError::AmountOverflow)?;
    }
    let principal_numerator = principal
        .checked_mul(denominator)
        .ok_or(PoolError::AmountOverflow)?;
    let interest_numerator = amount_numerator
        .checked_sub(principal_numerator)
        .ok_or(PoolError::AmountOverflow)?;
    div_round_half_up(interest_numerator, denominator)
}

pub fn compute_current_rate(utilization_bps: u32, config: &RateModelConfig) -> u32 {
    let util = utilization_bps.min(BPS_DENOM) as u128;
    let optimal = config.optimal_utilization_bps as u128;
    let base = config.base_rate_bps as u128;
    let ceiling = config.max_rate_bps as u128;

    let mul_div = |a: u128, b: u128, denom: u128| -> u128 {
        a.checked_mul(b)
            .and_then(|v| v.checked_div(denom))
            .unwrap_or(u128::MAX)
    };

    // Below (and at) the kink: base plus the pro-rata share of slope1.
    // `checked_div(0)` on a zero kink yields None -> saturates to the ceiling.
    let mut rate = base.saturating_add(mul_div(
        util.min(optimal),
        config.slope1_bps as u128,
        optimal,
    ));

    // Above the kink: add the pro-rata share of slope2 over the remaining span.
    // `util > optimal` implies span > 0 here, but checked_div keeps it total.
    if util > optimal {
        let span = (BPS_DENOM as u128).saturating_sub(optimal);
        rate = rate.saturating_add(mul_div(util - optimal, config.slope2_bps as u128, span));
    }

    rate.min(ceiling).min(u32::MAX as u128) as u32
}

fn validate_rate_model_config(config: &RateModelConfig) -> PoolResult<()> {
    // Kink must be inside (0%, 100%] — 0 would make the slope1 pro-ration
    // meaningless (and the below-kink division undefined).
    if config.optimal_utilization_bps == 0 || config.optimal_utilization_bps > BPS_DENOM {
        return Err(PoolError::InvalidRateModelConfig);
    }
    if config.max_rate_bps == 0 || config.max_rate_bps > MAX_RATE_BPS_CAP {
        return Err(PoolError::InvalidRateModelConfig);
    }
    if config.base_rate_bps > config.max_rate_bps {
        return Err(PoolError::InvalidRateModelConfig);
    }
    if config.slope1_bps > MAX_RATE_BPS_CAP || config.slope2_bps > MAX_RATE_BPS_CAP {
        return Err(PoolError::InvalidRateModelConfig);
    }
    Ok(())
}

fn utilization_bps(tt: &PoolTokenTotals) -> u32 {
    if tt.pool_value <= 0 {
        return 0;
    }
    ((tt.total_deployed as u128 * 10_000u128) / tt.pool_value as u128) as u32
}

fn current_rate_for_token(env: &Env, config: &PoolConfig, token: &Address) -> u32 {
    let tt: PoolTokenTotals = env
        .storage()
        .instance()
        .get(&DataKey::TokenTotals(token.clone()))
        .unwrap_or_default();
    match env
        .storage()
        .instance()
        .get::<DataKey, RateModelConfig>(&DataKey::RateModel(token.clone()))
    {
        Some(model) => compute_current_rate(utilization_bps(&tt), &model),
        None => config.yield_bps,
    }
}

fn record_rate_snapshot(env: &Env, token: &Address) {
    let model: Option<RateModelConfig> = env
        .storage()
        .instance()
        .get(&DataKey::RateModel(token.clone()));
    let Some(model) = model else { return };
    let tt: PoolTokenTotals = env
        .storage()
        .instance()
        .get(&DataKey::TokenTotals(token.clone()))
        .unwrap_or_default();
    let util = utilization_bps(&tt);
    let rate = compute_current_rate(util, &model);

    let bounds_key = DataKey::RateHistoryBounds(token.clone());
    let (len, start): (u32, u32) = env.storage().instance().get(&bounds_key).unwrap_or((0, 0));

    // Collapse consecutive duplicates: read the newest slot if any.
    if len > 0 {
        let newest_idx = (start + len - 1) % MAX_RATE_HISTORY;
        if let Some(last) = env
            .storage()
            .persistent()
            .get::<DataKey, RateSnapshot>(&DataKey::RateRecord(token.clone(), newest_idx))
        {
            if last.utilization_bps == util && last.rate_bps == rate {
                return;
            }
        }
    }

    let snapshot = RateSnapshot {
        timestamp: env.ledger().timestamp(),
        utilization_bps: util,
        rate_bps: rate,
    };
    if len < MAX_RATE_HISTORY {
        env.storage()
            .persistent()
            .set(&DataKey::RateRecord(token.clone(), len), &snapshot);
        env.storage().instance().set(&bounds_key, &(len + 1, start));
    } else {
        env.storage()
            .persistent()
            .set(&DataKey::RateRecord(token.clone(), start), &snapshot);
        let new_start = (start + 1) % MAX_RATE_HISTORY;
        env.storage().instance().set(&bounds_key, &(len, new_start));
    }

    // Indexed off-chain into a time-series table for the rate-history API.
    env.events().publish(
        (EVT, symbol_short!("rate_snap")),
        (token.clone(), snapshot.timestamp, util, rate),
    );
}

fn u128_to_i128(value: u128) -> PoolResult<i128> {
    if value > i128::MAX as u128 {
        return Err(PoolError::AmountOverflow);
    }
    Ok(value as i128)
}

fn update_investor_available(
    env: &Env,
    investor: &Address,
    token: &Address,
    delta: i128,
) -> PoolResult<()> {
    let pos_key = DataKey::InvestorPosition(investor.clone(), token.clone());
    let mut position: InvestorPosition =
        env.storage().persistent().get(&pos_key).unwrap_or_default();

    if delta >= 0 {
        position.available = position
            .available
            .checked_add(delta)
            .ok_or(PoolError::AmountOverflow)?;
    } else {
        position.available = position.available.saturating_sub(-delta);
        // #773: fully exited — reset the loyalty timer so a later re-deposit
        // starts a fresh tenure instead of inheriting the old start time.
        if position.available == 0 {
            position.loyalty_start_at = 0;
        }
    }

    env.storage().persistent().set(&pos_key, &position);
    Ok(())
}

fn default_loyalty_tiers(env: &Env) -> Vec<LoyaltyTier> {
    let mut tiers = Vec::new(env);
    tiers.push_back(LoyaltyTier {
        min_days: 0,
        bonus_bps: 0,
    });
    tiers.push_back(LoyaltyTier {
        min_days: 31,
        bonus_bps: 50,
    });
    tiers.push_back(LoyaltyTier {
        min_days: 91,
        bonus_bps: 150,
    });
    tiers.push_back(LoyaltyTier {
        min_days: 366,
        bonus_bps: 300,
    });
    tiers
}

fn get_loyalty_tiers_cached(env: &Env) -> Vec<LoyaltyTier> {
    env.storage()
        .instance()
        .get(&LOYALTY_TIERS)
        .unwrap_or_else(|| default_loyalty_tiers(env))
}

fn loyalty_days_active(env: &Env, loyalty_start_at: u64) -> u64 {
    if loyalty_start_at == 0 {
        return 0;
    }
    env.ledger().timestamp().saturating_sub(loyalty_start_at) / SECS_PER_DAY
}

fn resolve_loyalty_tier(tiers: &Vec<LoyaltyTier>, days_active: u64) -> (u32, u32, Option<u32>) {
    let mut tier_index: u32 = 0;
    let mut bonus_bps: u32 = 0;
    let mut next_tier_days: Option<u32> = None;
    for i in 0..tiers.len() {
        let tier = tiers.get(i).unwrap();
        if days_active >= tier.min_days as u64 {
            tier_index = i + 1;
            bonus_bps = tier.bonus_bps;
        } else if next_tier_days.is_none() {
            next_tier_days = Some(tier.min_days);
        }
    }
    (tier_index, bonus_bps, next_tier_days)
}

fn calculate_factoring_fee(principal: i128, factoring_fee_bps: u32) -> PoolResult<i128> {
    let numerator = (principal as u128)
        .checked_mul(factoring_fee_bps as u128)
        .ok_or(PoolError::AmountOverflow)?;
    // Ceiling division: round up so that any non-zero fee rate on any
    // non-zero principal yields at least 1 stroop.  Without this, small
    // invoices where principal × fee_bps < BPS_DENOM always truncate to 0.
    let fee = numerator.div_ceil(BPS_DENOM as u128);
    u128_to_i128(fee)
}

fn calculate_total_due(
    record: &FundedInvoice,
    config: &PoolConfig,
    now: u64,
) -> PoolResult<(u128, i128)> {
    let accrual_end = if now > record.due_date {
        record.due_date
    } else {
        now
    };
    let elapsed_secs = accrual_end
        .checked_sub(record.funded_at)
        .ok_or(PoolError::AmountOverflow)?;
    // #863: interest accrues at the rate locked in when the invoice was
    // funded (curve output at funding-time utilization, or the static
    // yield_bps fallback) — never the current live rate.
    let total_interest = calculate_interest(
        record.principal as u128,
        record.locked_yield_bps,
        elapsed_secs,
        config.compound_interest,
    )?;
    let total_interest_i128 = u128_to_i128(total_interest)?;
    let total_due = record
        .principal
        .checked_add(total_interest_i128)
        .and_then(|value| value.checked_add(record.factoring_fee))
        .ok_or(PoolError::AmountOverflow)?;
    Ok((total_interest, total_due))
}

fn release_collateral(env: &Env, invoice_id: u64, released_by: &Address, settled_at: u64) {
    if let Some(mut col) = env
        .storage()
        .persistent()
        .get::<DataKey, CollateralDeposit>(&DataKey::CollateralDeposit(invoice_id))
    {
        if !col.settled {
            let col_token_client = token::Client::new(env, &col.token);
            col_token_client.transfer(&env.current_contract_address(), &col.depositor, &col.amount);
            col.settled = true;
            col.released_at = settled_at;
            env.storage()
                .persistent()
                .set(&DataKey::CollateralDeposit(invoice_id), &col);
            // Extend TTL so the settled record survives 90 days post-release
            // for audit purposes, regardless of when the invoice was originally funded.
            env.storage().persistent().extend_ttl(
                &DataKey::CollateralDeposit(invoice_id),
                SETTLEMENT_COLLATERAL_TTL,
                SETTLEMENT_COLLATERAL_TTL,
            );
            env.events().publish(
                (EVT, symbol_short!("col_ret")),
                (
                    invoice_id,
                    col.depositor,
                    col.amount,
                    released_by.clone(),
                    settled_at,
                ),
            );
        }
    }
}

fn get_token_config(env: &Env, token: &Address) -> PoolResult<TokenConfig> {
    env.storage()
        .instance()
        .get(&DataKey::TokenConfig(token.clone()))
        .ok_or(PoolError::StorageCorrupted)
}

fn normalize_to_stroops(amount: i128, token_decimals: u32) -> i128 {
    if token_decimals >= EXPECTED_DECIMALS {
        // If token has more decimals than 7, divide down
        amount / (10i128.pow(token_decimals - EXPECTED_DECIMALS))
    } else {
        // If token has fewer decimals than 7, multiply up
        amount * (10i128.pow(EXPECTED_DECIMALS - token_decimals))
    }
}

fn denormalize_from_stroops(amount: i128, token_decimals: u32) -> i128 {
    if token_decimals >= EXPECTED_DECIMALS {
        // If token has more decimals than 7, multiply up
        amount * (10i128.pow(token_decimals - EXPECTED_DECIMALS))
    } else {
        // If token has fewer decimals than 7, divide down
        amount / (10i128.pow(EXPECTED_DECIMALS - token_decimals))
    }
}

fn get_credit_score_contract(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::CreditScoreContract)
}

fn get_insurance_contract(env: &Env) -> Option<Address> {
    env.storage().instance().get(&INSURANCE_CFG)
}

fn fee_tier_matches(tier: &FeeTier, principal: i128, score: u32) -> bool {
    principal >= tier.min_amount && principal <= tier.max_amount && score >= tier.min_credit_score
}

fn resolve_factoring_fee(
    env: &Env,
    config: &PoolConfig,
    principal: i128,
    sme: Address,
    token: &Address,
) -> PoolResult<i128> {
    let mut fee_bps = config.factoring_fee_bps;

    if let Some(cs_contract) = get_credit_score_contract(env) {
        let credit_client = CreditScoreClient::new(env, &cs_contract);
        let credit_data = credit_client.get_credit_score(&sme);
        let tier_ids: Vec<u32> = env
            .storage()
            .instance()
            .get(&DataKey::FeeTierIds)
            .unwrap_or(Vec::new(env));

        for i in 0..tier_ids.len() {
            let tier_id = tier_ids.get(i).expect("storage corrupted");
            if let Some(tier) = env.storage().instance().get(&DataKey::FeeTier(tier_id)) {
                if fee_tier_matches(&tier, principal, credit_data.score) {
                    fee_bps = tier.fee_bps;
                    break;
                }
            }
        }
    }

    // #367: Normalize principal to stroops for fee calculation
    let token_config = get_token_config(env, token)?;
    let normalized_principal = normalize_to_stroops(principal, token_config.decimals);
    let normalized_fee = calculate_factoring_fee(normalized_principal, fee_bps)?;
    // Denormalize fee back to token units
    let fee = denormalize_from_stroops(normalized_fee, token_config.decimals);
    Ok(fee)
}

fn required_collateral(principal: i128, config: &CollateralConfig) -> i128 {
    if principal < config.threshold {
        return 0;
    }
    ((principal as u128 * config.collateral_bps as u128) / BPS_DENOM as u128) as i128
}

fn available_liquidity(tt: &PoolTokenTotals) -> PoolResult<i128> {
    tt.pool_value
        .checked_sub(tt.total_deployed)
        .ok_or(PoolError::AmountOverflow)
}

fn record_inflow_event(env: &Env, token: &Address, amount: i128) {
    let len_key = DataKey::InflowHistoryLen(token.clone());
    let start_key = DataKey::InflowHistoryStart(token.clone());
    let len: u32 = env.storage().instance().get(&len_key).unwrap_or(0);
    let start: u32 = env.storage().instance().get(&start_key).unwrap_or(0);
    let event = InflowEvent {
        amount,
        timestamp: env.ledger().timestamp(),
    };

    if len < MAX_INFLOW_HISTORY {
        env.storage()
            .persistent()
            .set(&DataKey::InflowRecord(token.clone(), len), &event);
        env.storage().instance().set(&len_key, &(len + 1));
    } else {
        env.storage()
            .persistent()
            .set(&DataKey::InflowRecord(token.clone(), start), &event);
        let new_start = (start + 1) % MAX_INFLOW_HISTORY;
        env.storage().instance().set(&start_key, &new_start);
    }
}

fn trailing_inflow_rate_per_sec(env: &Env, token: &Address) -> i128 {
    let len: u32 = env
        .storage()
        .instance()
        .get(&DataKey::InflowHistoryLen(token.clone()))
        .unwrap_or(0);
    if len == 0 {
        return 0;
    }
    let start: u32 = env
        .storage()
        .instance()
        .get(&DataKey::InflowHistoryStart(token.clone()))
        .unwrap_or(0);

    let mut total: i128 = 0;
    let mut oldest_ts: u64 = u64::MAX;
    let now = env.ledger().timestamp();
    for offset in 0..len {
        let idx = (start + offset) % MAX_INFLOW_HISTORY;
        if let Some(record) = env
            .storage()
            .persistent()
            .get::<DataKey, InflowEvent>(&DataKey::InflowRecord(token.clone(), idx))
        {
            total = total.saturating_add(record.amount);
            if record.timestamp < oldest_ts {
                oldest_ts = record.timestamp;
            }
        }
    }
    // Elapsed window; floor at 1 day so a burst of deposits in the same ledger close
    // doesn't produce a division by (near) zero and an absurdly high rate.
    let elapsed = now.saturating_sub(oldest_ts).max(SECS_PER_DAY);
    total / elapsed as i128
}

fn record_funded_invoice_id(env: &Env, token: &Address, invoice_id: u64) {
    let key = DataKey::TokenInvoiceIds(token.clone());
    let mut ids: Vec<u64> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(Vec::new(env));
    ids.push_back(invoice_id);
    env.storage().persistent().set(&key, &ids);
}

fn calculate_reward_delta(total_interest: i128, total_shares: i128) -> PoolResult<i128> {
    total_interest
        .checked_mul(REWARD_PRECISION)
        .and_then(|value| value.checked_div(total_shares))
        .ok_or(PoolError::AmountOverflow)
}

fn credit_investor_value(
    env: &Env,
    token: &Address,
    investor: &Address,
    amount: i128,
    tt: &mut PoolTokenTotals,
) -> PoolResult<()> {
    if amount <= 0 {
        return Ok(());
    }
    let share_token_key = DataKey::ShareToken(token.clone());
    let investor_pos_key = DataKey::InvestorPosition(investor.clone(), token.clone());

    let share_token: Address = env
        .storage()
        .instance()
        .get(&share_token_key)
        .ok_or(PoolError::ShareTokenNotConfigured)?;

    let rate_bps: u32 = env
        .storage()
        .instance()
        .get(&DataKey::ExchangeRate(token.clone()))
        .unwrap_or(10_000u32);
    let usdc_equiv = amount
        .checked_mul(rate_bps as i128)
        .ok_or(PoolError::AmountOverflow)?
        .checked_div(10_000i128)
        .ok_or(PoolError::AmountOverflow)?;

    let total_shares: i128 = env.invoke_contract(
        &share_token,
        &Symbol::new(env, "total_supply"),
        Vec::new(env),
    );
    let shares_to_mint = if total_shares == 0 || tt.pool_value == 0 {
        usdc_equiv
    } else {
        usdc_equiv
            .checked_mul(total_shares)
            .ok_or(PoolError::AmountOverflow)?
            .checked_div(tt.pool_value)
            .ok_or(PoolError::AmountOverflow)?
    };

    tt.pool_value = tt
        .pool_value
        .checked_add(usdc_equiv)
        .ok_or(PoolError::AmountOverflow)?;

    let mut mint_args = Vec::new(env);
    mint_args.push_back(investor.clone().into_val(env));
    mint_args.push_back(shares_to_mint.into_val(env));
    let _: () = env.invoke_contract(&share_token, &Symbol::new(env, "mint"), mint_args);

    let mut position: InvestorPosition = env
        .storage()
        .persistent()
        .get(&investor_pos_key)
        .unwrap_or_default();
    position.deposited = position
        .deposited
        .checked_add(usdc_equiv)
        .ok_or(PoolError::AmountOverflow)?;
    position.available = position
        .available
        .checked_add(shares_to_mint)
        .ok_or(PoolError::AmountOverflow)?;
    env.storage().persistent().set(&investor_pos_key, &position);

    Ok(())
}

fn refund_co_funding_investor(
    env: &Env,
    round: &CoFundingRound,
    investor: &Address,
    tt: &mut PoolTokenTotals,
) -> PoolResult<i128> {
    // #860: refund from the EXACT cumulative committed amount, not from bps
    // (bps truncates to 1/10_000 of target_principal — reconstructing the
    // amount from it would silently under-refund an investor by up to ~1bp
    // of the round every time, since bps can't represent an arbitrary
    // fraction exactly).
    let committed_key = DataKey::CoFundCommitted(round.invoice_id, investor.clone());
    let committed_amount: i128 = env.storage().persistent().get(&committed_key).unwrap_or(0);
    if committed_amount == 0 {
        return Ok(0);
    }

    credit_investor_value(env, &round.token, investor, committed_amount, tt)?;
    env.storage()
        .persistent()
        .remove(&DataKey::CoFundShare(round.invoice_id, investor.clone()));
    env.storage().persistent().remove(&committed_key);
    Ok(committed_amount)
}

fn remove_participant(env: &Env, round: &mut CoFundingRound, investor: &Address) {
    let mut updated = Vec::new(env);
    for i in 0..round.participants.len() {
        if let Some(addr) = round.participants.get(i) {
            if &addr != investor {
                updated.push_back(addr);
            }
        }
    }
    round.participants = updated;
}

fn distribute_pari_passu_repayment(
    env: &Env,
    round_id: u64,
    actual_payment: i128,
    tt: &mut PoolTokenTotals,
    stats: &mut PoolStorageStats,
    fully_repaid: bool,
) -> PoolResult<()> {
    let round: CoFundingRound = env
        .storage()
        .persistent()
        .get(&DataKey::CoFundingRound(round_id))
        .ok_or(PoolError::CoFundingRoundNotFound)?;
    let participants = round.participants.clone();
    let mut distributed: i128 = 0;
    for i in 0..participants.len() {
        let holder = participants.get(i).ok_or(PoolError::StorageCorrupted)?;
        let bps: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::CoFundShare(round_id, holder.clone()))
            .unwrap_or(0);
        if bps == 0 {
            continue;
        }
        let holder_amount = actual_payment
            .checked_mul(bps as i128)
            .ok_or(PoolError::AmountOverflow)?
            .checked_div(BPS_DENOM as i128)
            .ok_or(PoolError::AmountOverflow)?;
        if holder_amount > 0 {
            credit_investor_value(env, &round.token, &holder, holder_amount, tt)?;
            distributed = distributed
                .checked_add(holder_amount)
                .ok_or(PoolError::AmountOverflow)?;
        }
    }
    let dust = actual_payment
        .checked_sub(distributed)
        .ok_or(PoolError::AmountOverflow)?;
    if dust > 0 {
        tt.protocol_revenue = tt
            .protocol_revenue
            .checked_add(dust)
            .ok_or(PoolError::AmountOverflow)?;
        tt.pool_value = tt
            .pool_value
            .checked_add(dust)
            .ok_or(PoolError::AmountOverflow)?;
    }
    tt.total_paid_out = tt
        .total_paid_out
        .checked_add(actual_payment)
        .ok_or(PoolError::AmountOverflow)?;
    if fully_repaid {
        stats.active_funded_invoices = stats.active_funded_invoices.saturating_sub(1);
    }
    Ok(())
}

fn fund_invoice_request(
    env: &Env,
    config: &PoolConfig,
    accepted_tokens: &Vec<Address>,
    stats: &mut PoolStorageStats,
    request: &FundingRequest,
) -> PoolResult<()> {
    if request.principal <= 0 {
        return Err(PoolError::InvalidAmount);
    }

    // Fail fast on liquidity before scanning token allowlist.
    // This keeps unsuccessful requests cheap when the pool cannot fund them.
    // Ensure sufficient liquidity (cash = NAV - deployed).
    let token_totals_key = DataKey::TokenTotals(request.token.clone());
    let mut tt: PoolTokenTotals = env
        .storage()
        .instance()
        .get(&token_totals_key)
        .unwrap_or_default();
    let available_liquidity = available_liquidity(&tt)?;
    if available_liquidity < request.principal {
        return Err(PoolError::InsufficientLiquidity);
    }

    // Verify the token is accepted.
    let mut token_ok = false;
    for i in 0..accepted_tokens.len() {
        let accepted = accepted_tokens.get(i).ok_or(PoolError::StorageCorrupted)?;
        if accepted == request.token {
            token_ok = true;
            break;
        }
    }
    if !token_ok {
        return Err(PoolError::TokenNotAccepted);
    }

    let now = env.ledger().timestamp();

    // #531: reject funding if the invoice due date has already passed
    if now >= request.due_date {
        return Err(PoolError::InvoiceExpired);
    }

    // Load existing FundedInvoice if this is a partial funding, or create new
    let _is_first_funding = !env
        .storage()
        .persistent()
        .has(&DataKey::FundedInvoice(request.invoice_id));

    let prev_deployed = tt.total_deployed;
    tt.total_deployed = tt
        .total_deployed
        .checked_add(request.principal)
        .ok_or(PoolError::AmountOverflow)?;
    // #863: lock in the rate live at funding time. With a configured rate
    // model this prices the invoice off post-deployment utilization (the
    // liquidity this funding consumes is reflected in its own rate);
    // otherwise it falls back to the static `config.yield_bps`.
    let locked_yield_bps = {
        let model: Option<RateModelConfig> = env
            .storage()
            .instance()
            .get(&DataKey::RateModel(request.token.clone()));
        match model {
            Some(m) => compute_current_rate(utilization_bps(&tt), &m),
            None => config.yield_bps,
        }
    };
    // #869: resolve factoring fee at funding time; auction contract may
    // override this with a discount via fund_invoice_with_discount.
    let factoring_fee = resolve_factoring_fee(
        env,
        config,
        request.principal,
        request.sme.clone(),
        &request.token,
    )?;
    let funded_key = DataKey::FundedInvoice(request.invoice_id);
    let is_partial = env.storage().persistent().has(&funded_key);
    if is_partial {
        // #934: accumulate principal for partial fundings.
        let mut existing: FundedInvoice = env
            .storage()
            .persistent()
            .get(&funded_key)
            .expect("just checked");
        existing.principal = existing
            .principal
            .checked_add(request.principal)
            .ok_or(PoolError::AmountOverflow)?;
        env.storage().persistent().set(&funded_key, &existing);
    } else {
        let funded = FundedInvoice {
            invoice_id: request.invoice_id,
            sme: request.sme.clone(),
            token: request.token.clone(),
            principal: request.principal,
            funded_at: now,
            factoring_fee,
            due_date: request.due_date,
            repaid_amount: 0i128,
            co_funding_round_id: None,
            locked_yield_bps,
        };
        env.storage().persistent().set(&funded_key, &funded);
        // #865: index this invoice_id only on first funding.
        record_funded_invoice_id(env, &request.token, request.invoice_id);
    }
    set_funded_invoice_ttl(env, request.invoice_id, false);

    env.storage().instance().set(&token_totals_key, &tt);

    // Update invoice contract's funded_amount via cross-contract call
    let invoice_client = InvoiceContractClient::new(env, &config.invoice_contract);
    let _ = invoice_client.try_record_funding(
        &request.invoice_id,
        &request.principal,
        &env.current_contract_address(),
    );

    // #275: check utilization after deployment
    if tt.pool_value > 0 {
        let config = get_config_cached(env)?;
        let utilization = ((tt.total_deployed as u128 * 10_000u128) / tt.pool_value as u128) as u32;
        if utilization > config.max_utilization_bps {
            // Revert the deployment
            tt.total_deployed = tt
                .total_deployed
                .checked_sub(request.principal)
                .ok_or(PoolError::AmountOverflow)?;
            env.storage().instance().set(&token_totals_key, &tt);
            return Err(PoolError::UtilizationLimitExceeded);
        }
        // #653: emit a utilization warning only when this funding *crosses* the
        // warning threshold — i.e. utilization was below the threshold before
        // this deployment and is at or above it now. Funding only ever raises
        // utilization, so this edge-triggered check means the warning is not
        // re-emitted on subsequent funding calls while already above the
        // threshold; it fires again only after utilization drops below (via a
        // repayment or fresh deposit) and crosses once more. Off-chain monitors
        // get one alert per crossing instead of one per funding call.
        let prev_utilization =
            ((prev_deployed as u128 * 10_000u128) / tt.pool_value as u128) as u32;
        if utilization >= config.utilization_warning_bps
            && prev_utilization < config.utilization_warning_bps
        {
            env.events().publish(
                (EVT, symbol_short!("util_warn")),
                (request.token.clone(), utilization),
            );
        }
    }

    if !is_partial {
        stats.total_funded_invoices = stats
            .total_funded_invoices
            .checked_add(1)
            .ok_or(PoolError::AmountOverflow)?;
        stats.active_funded_invoices = stats
            .active_funded_invoices
            .checked_add(1)
            .ok_or(PoolError::AmountOverflow)?;
    }
    env.storage().instance().set(&DataKey::StorageStats, stats);

    // #934: record cumulative funded_amount on the invoice contract.
    let invoice_client = InvoiceContractClient::new(env, &config.invoice_contract);
    let _ = invoice_client.try_record_funding(
        &request.invoice_id,
        &request.principal,
        &env.current_contract_address(),
    );

    // Transfer principal to SME LAST - interaction
    // NAV is unchanged because the funded invoice becomes an asset.
    let token_client = token::Client::new(env, &request.token);
    token_client.transfer(
        &env.current_contract_address(),
        &request.sme,
        &request.principal,
    );

    env.events().publish(
        (EVT, symbol_short!("funded")),
        (
            request.invoice_id,
            request.sme.clone(),
            request.principal,
            request.token.clone(),
            env.ledger().timestamp(),
        ),
    );

    // #863: utilization moved with this funding — record a rate sample.
    record_rate_snapshot(env, &request.token);

    // #534: notify the credit score contract that this borrower secured funding.
    // Non-fatal — a cross-contract failure must not revert a successful funding.
    if let Some(cs_contract) = get_credit_score_contract(env) {
        let cs_client = CreditScoreClient::new(env, &cs_contract);
        let _ = cs_client.try_record_funding(
            &env.current_contract_address(),
            &request.invoice_id,
            &request.sme,
            &request.principal,
        );
    }

    // #866: optionally purchase default-insurance coverage for this invoice.
    // Non-fatal — a temporary insurance-contract outage must never block
    // funding, mirroring the credit_score integration above. The pool itself
    // is the payer (auto-authorized since it's the direct caller): the
    // insurance contract pulls the premium directly out of the pool's real
    // token balance via its own `transfer` call. That's real value leaving
    // the pool regardless of internal bookkeeping, so pool_value (investor
    // NAV) must be written down by the same amount or later withdrawals
    // would overdraw the pool's actual balance — exactly the class of bug
    // fixed in execute_seize_collateral above. protocol_revenue is drawn
    // down first (best-effort, floored at zero) since that's the intended
    // funding source; any remainder still comes out of pool_value.
    if let Some(insurance_contract) = get_insurance_contract(env) {
        let insurance_client = InsuranceClient::new(env, &insurance_contract);
        if let Ok(Ok(coverage)) = insurance_client.try_purchase_coverage(
            &env.current_contract_address(),
            &request.invoice_id,
            &request.principal,
            &request.sme,
            &request.due_date,
            &request.token,
        ) {
            let mut tt: PoolTokenTotals = env
                .storage()
                .instance()
                .get(&token_totals_key)
                .unwrap_or_default();
            tt.protocol_revenue = tt.protocol_revenue.saturating_sub(coverage.premium_paid);
            tt.pool_value = tt.pool_value.saturating_sub(coverage.premium_paid);
            env.storage().instance().set(&token_totals_key, &tt);
        }
    }

    Ok(())
}

macro_rules! non_reentrant {
    ($env:expr, $body:block) => {{
        Self::non_reentrant_start($env);
        let result = { $body };
        Self::non_reentrant_end($env);
        result
    }};
}

#[contract]
pub struct FundingPool;

#[contractimpl]
impl FundingPool {
    pub fn initialize(
        env: Env,
        admin: Address,
        initial_token: Address,
        initial_share_token: Address,
        invoice_contract: Address,
    ) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic_with_error!(&env, PoolError::AlreadyInitialized);
        }

        let config = PoolConfig {
            invoice_contract,
            admin: admin.clone(),
            yield_bps: DEFAULT_YIELD_BPS,
            factoring_fee_bps: DEFAULT_FACTORING_FEE_BPS,
            compound_interest: false,
            last_yield_change_at: env.ledger().timestamp(),
            yield_change_cooldown_secs: DEFAULT_YIELD_CHANGE_COOLDOWN_SECS,
            max_yield_change_bps: DEFAULT_MAX_YIELD_CHANGE_BPS,
            // #227: yield timelock defaults
            proposed_yield_bps: 0,
            yield_proposal_at: 0,
            yield_timelock_secs: DEFAULT_YIELD_TIMELOCK_SECS,
            // #235: minimum deposit per transaction (0 = disabled)
            min_deposit_amount: DEFAULT_MIN_DEPOSIT_AMOUNT,
            // #233: maximum single-investor concentration (2000 = 20%)
            max_single_investor_bps: DEFAULT_MAX_SINGLE_INVESTOR_BPS,
            // #244: withdrawal rate limiting (10_000 bps = disabled; 0 secs = disabled)
            max_single_withdrawal_bps: DEFAULT_MAX_SINGLE_WITHDRAWAL_BPS,
            withdrawal_cooldown_secs: DEFAULT_WITHDRAWAL_COOLDOWN_SECS,
            // #275: utilization guardrails
            max_utilization_bps: DEFAULT_MAX_UTILIZATION_BPS,
            utilization_warning_bps: DEFAULT_UTILIZATION_WARNING_BPS,
            max_withdrawal_queue_age_days: DEFAULT_MAX_WITHDRAWAL_QUEUE_AGE_DAYS,
            max_withdrawal_queue_depth: DEFAULT_MAX_WITHDRAWAL_QUEUE_DEPTH,
        };

        let mut tokens: Vec<Address> = Vec::new(&env);
        tokens.push_back(initial_token.clone());

        let token_client = token::Client::new(&env, &initial_token);
        let token_decimals = token_client.decimals();
        if token_decimals != EXPECTED_DECIMALS {
            panic!("unsupported token decimals");
        }

        env.storage().instance().set(&DataKey::Config, &config);
        env.storage()
            .instance()
            .set(&DataKey::AcceptedTokens, &tokens);
        env.storage().instance().set(
            &DataKey::TokenTotals(initial_token.clone()),
            &PoolTokenTotals::default(),
        );
        env.storage().instance().set(
            &DataKey::ShareToken(initial_token.clone()),
            &initial_share_token,
        );
        env.storage().instance().set(
            &DataKey::TokenConfig(initial_token.clone()),
            &TokenConfig {
                token: initial_token.clone(),
                share_token: initial_share_token.clone(),
                decimals: token_decimals,
            },
        );
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage()
            .instance()
            .set(&DataKey::StorageStats, &PoolStorageStats::default());
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage().instance().set(
            &DataKey::CollateralConfig,
            &CollateralConfig {
                threshold: DEFAULT_COLLATERAL_THRESHOLD,
                collateral_bps: DEFAULT_COLLATERAL_BPS,
            },
        );
        // Store compile-time version (#237)
        env.storage()
            .instance()
            .set(&DataKey::ContractVersion, &parse_pool_version());
        env.storage()
            .instance()
            .set(&DataKey::MigrationVersion, &0u32);
        bump_instance(&env);
    }

    pub fn version(env: Env) -> PoolContractVersion {
        env.storage()
            .instance()
            .get(&DataKey::ContractVersion)
            .unwrap_or_else(parse_pool_version)
    }

    pub fn migration_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::MigrationVersion)
            .unwrap_or(0)
    }

    pub fn run_migration(env: Env, admin: Address) -> Result<(), PoolError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        let current: u32 = env
            .storage()
            .instance()
            .get(&DataKey::MigrationVersion)
            .unwrap_or(0);
        if current >= CURRENT_MIGRATION_VERSION {
            return Ok(());
        }
        // Future migration arms (current -> current + 1) transform storage here.
        env.storage()
            .instance()
            .set(&DataKey::MigrationVersion, &CURRENT_MIGRATION_VERSION);
        Ok(())
    }

    pub fn pause(env: Env, admin: Address) -> Result<(), PoolError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        // #779: pause policy — new deposits, withdrawals, and funding are
        // blocked while paused. Admin emergency controls (set_yield,
        // set_investor_kyc, unpause) and repay_invoice() remain available:
        // borrowers must always be able to exit their debt, even during an
        // emergency freeze.
        env.storage().instance().set(&DataKey::Paused, &true);
        bump_instance(&env);
        env.events().publish(
            (EVT, symbol_short!("paused")),
            (admin, env.ledger().timestamp()),
        );
        Ok(())
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), PoolError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        env.storage().instance().set(&DataKey::Paused, &false);
        bump_instance(&env);
        env.events().publish(
            (EVT, symbol_short!("unpaused")),
            (admin, env.ledger().timestamp()),
        );
        Ok(())
    }

    pub fn is_paused(env: Env) -> bool {
        bump_instance(&env);
        env.storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn add_token(
        env: Env,
        admin: Address,
        token: Address,
        share_token: Address,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;

        // #367: Fetch and validate token decimals
        let token_client = token::Client::new(&env, &token);
        let token_decimals = token_client.decimals();
        if token_decimals != EXPECTED_DECIMALS {
            return Err(PoolError::UnsupportedTokenDecimals);
        }
        let share_client = token::Client::new(&env, &share_token);
        if share_client.try_decimals().is_err() {
            return Err(PoolError::InvalidShareToken);
        }

        let mut tokens: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AcceptedTokens)
            .ok_or(PoolError::NotInitialized)?;

        for i in 0..tokens.len() {
            if tokens.get(i).ok_or(PoolError::StorageCorrupted)? == token {
                return Err(PoolError::TokenAlreadyAccepted);
            }
        }

        bump_instance(&env);

        tokens.push_back(token.clone());
        env.storage()
            .instance()
            .set(&DataKey::AcceptedTokens, &tokens);
        env.events()
            .publish((EVT, symbol_short!("add_token")), (admin, token.clone()));

        if !env
            .storage()
            .instance()
            .has(&DataKey::TokenTotals(token.clone()))
        {
            env.storage().instance().set(
                &DataKey::TokenTotals(token.clone()),
                &PoolTokenTotals::default(),
            );
            env.storage()
                .instance()
                .set(&DataKey::ShareToken(token.clone()), &share_token);

            // #367: Store token configuration with decimals
            let config = TokenConfig {
                token: token.clone(),
                share_token,
                decimals: token_decimals,
            };
            env.storage()
                .instance()
                .set(&DataKey::TokenConfig(token), &config);
        }
        Ok(())
    }

    pub fn remove_token(env: Env, admin: Address, _token: Address) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        Err(PoolError::OperationRequiresProposal)
    }

    fn execute_remove_token(env: &Env, admin: &Address, token: &Address) -> PoolResult<()> {
        let tokens: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AcceptedTokens)
            .ok_or(PoolError::NotInitialized)?;

        let mut new_tokens: Vec<Address> = Vec::new(env);
        let mut found = false;
        for i in 0..tokens.len() {
            let t = tokens.get(i).ok_or(PoolError::StorageCorrupted)?;
            if &t == token {
                found = true;
            } else {
                new_tokens.push_back(t);
            }
        }
        if !found {
            return Err(PoolError::TokenNotWhitelisted);
        }

        // #222: Safety checks before token removal
        let tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&DataKey::TokenTotals(token.clone()))
            .unwrap_or_default();

        // Check 1: No deployed capital (active funded invoices)
        if tt.total_deployed > 0 {
            return Err(PoolError::TokenHasDeployedCapital);
        }

        // Check 2: No pending withdrawal requests (withdrawal queue)
        let queue_key = DataKey::WithdrawalQueue(token.clone());
        let queue: Vec<WithdrawalRequest> = env
            .storage()
            .persistent()
            .get(&queue_key)
            .unwrap_or(Vec::new(env));
        if !queue.is_empty() {
            return Err(PoolError::TokenHasPendingWithdrawals);
        }

        // Check 3: No active balances (share token supply is zero)
        let share_token: Address = env
            .storage()
            .instance()
            .get(&DataKey::ShareToken(token.clone()))
            .ok_or(PoolError::ShareTokenNotConfigured)?;

        let total_shares: i128 = env.invoke_contract(
            &share_token,
            &Symbol::new(env, "total_supply"),
            Vec::new(env),
        );

        if total_shares > 0 {
            return Err(PoolError::TokenHasActiveBalances);
        }

        // Check 4: No active co-funding commitments in this token (#567)
        // Scan all FundedInvoice records to find any in co-funding phase with commitments in this token
        let stats: PoolStorageStats = env
            .storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default();

        for invoice_idx in 0..stats.total_funded_invoices {
            if let Some(funded_invoice) = env
                .storage()
                .persistent()
                .get::<DataKey, FundedInvoice>(&DataKey::FundedInvoice(invoice_idx))
            {
                // Check if this funded invoice uses the target token
                if &funded_invoice.token == token {
                    // Found an active funded invoice using this token
                    // This represents a co-funding commitment that must be settled before removal
                    return Err(PoolError::TokenHasActiveCofundingCommitments);
                }
            }
        }

        env.storage()
            .instance()
            .set(&DataKey::AcceptedTokens, &new_tokens);
        env.events().publish(
            (EVT, symbol_short!("rm_token")),
            (admin.clone(), token.clone()),
        );
        Ok(())
    }

    pub fn deposit(
        env: Env,
        investor: Address,
        token: Address,
        amount: i128,
        min_rate: Option<u32>,
    ) -> Result<(), PoolError> {
        investor.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        // #384: Distinguish zero and negative amounts
        if amount == 0 {
            return Err(PoolError::ZeroAmount);
        }
        if amount < 0 {
            return Err(PoolError::NegativeAmount);
        }
        Self::assert_accepted_token(&env, &token)?;

        Self::non_reentrant_start(&env);

        // #235: enforce minimum deposit amount
        let config = get_config_cached(&env)?;
        if config.min_deposit_amount > 0 && amount < config.min_deposit_amount {
            return Err(PoolError::DepositBelowMinimum);
        }

        // #992: optional slippage guard. `get_current_rate` can shift between
        // the caller simulating this call and actually submitting it (other
        // deposits/withdrawals move utilization in between) — a caller that
        // passes `min_rate` gets a revert instead of silently locking in a
        // worse rate than they simulated for.
        if let Some(min_rate) = min_rate {
            let current_rate = current_rate_for_token(&env, &config, &token);
            if current_rate < min_rate {
                return Err(PoolError::RateBelowMinimum);
            }
        }

        require_kyc_approved(&env, &investor)?;

        // #867: opt-in compliance / sanctions screening gate (fatal when enabled)
        Self::require_compliance_cleared(&env, &investor)?;

        // Batch read: get both token totals and share token in one go
        let token_totals_key = DataKey::TokenTotals(token.clone());
        let share_token_key = DataKey::ShareToken(token.clone());
        let investor_pos_key = DataKey::InvestorPosition(investor.clone(), token.clone());

        let mut tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&token_totals_key)
            .unwrap_or_default();

        let mut investor_position: InvestorPosition = env
            .storage()
            .persistent()
            .get(&investor_pos_key)
            .unwrap_or_default();

        // Normalise deposit amount to USDC equivalent using stored exchange rate
        let rate_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ExchangeRate(token.clone()))
            .unwrap_or(10_000u32);
        let usdc_received = (amount * rate_bps as i128) / 10_000i128;

        // #233: enforce maximum single-investor concentration limit
        let config = get_config_cached(&env)?;
        if config.max_single_investor_bps < 10_000 {
            let new_investor_total = investor_position.deposited + usdc_received;
            let new_pool_total = tt.pool_value + usdc_received;
            if new_pool_total > 0 {
                let investor_share_bps =
                    ((new_investor_total as u128 * 10_000u128) / new_pool_total as u128) as u32;
                if investor_share_bps > config.max_single_investor_bps {
                    env.events().publish(
                        (EVT, symbol_short!("conc_excd")),
                        (
                            investor.clone(),
                            investor_share_bps,
                            config.max_single_investor_bps,
                        ),
                    );
                    return Err(PoolError::ConcentrationLimitExceeded);
                }
            }
        }

        let share_token: Address = env
            .storage()
            .instance()
            .get(&share_token_key)
            .ok_or(PoolError::ShareTokenNotConfigured)?;

        // Calculate shares using USDC-equivalent deposit value
        let total_shares: i128 = env.invoke_contract(
            &share_token,
            &Symbol::new(&env, "total_supply"),
            Vec::new(&env),
        );

        let shares_to_mint = if total_shares == 0 || tt.pool_value == 0 {
            usdc_received
        } else {
            (usdc_received * total_shares) / tt.pool_value
        };

        // Pool value is maintained in USDC terms for consistent multi-token accounting
        tt.pool_value += usdc_received;

        // Batch write: update token totals
        env.storage().instance().set(&token_totals_key, &tt);

        // Mint shares (single external call)
        let mut mint_args = Vec::new(&env);
        mint_args.push_back(investor.clone().into_val(&env));
        mint_args.push_back(shares_to_mint.into_val(&env));
        let _: () = env.invoke_contract(&share_token, &Symbol::new(&env, "mint"), mint_args);

        // #773: starting (or restarting, after a full withdrawal) a position —
        // begin the loyalty tenure clock now. Topping up an already-open
        // position does not push the timer back.
        if investor_position.available == 0 {
            investor_position.loyalty_start_at = env.ledger().timestamp();
        }

        // #233: update investor position — track in USDC terms to match pool_value
        investor_position.deposited += usdc_received;
        investor_position.available = investor_position
            .available
            .checked_add(shares_to_mint)
            .ok_or(PoolError::AmountOverflow)?;
        investor_position.deposit_count += 1;
        env.storage()
            .persistent()
            .set(&investor_pos_key, &investor_position);

        // Transfer deposited stablecoin LAST - interaction.
        let token_client = token::Client::new(&env, &token);
        let balance_before = token_client.balance(&env.current_contract_address());
        token_client.transfer(&investor, &env.current_contract_address(), &amount);

        // Verify exact amount received (handles fee-on-transfer tokens).
        let balance_after = token_client.balance(&env.current_contract_address());
        let received = balance_after.wrapping_sub(balance_before);
        if received != amount {
            return Err(PoolError::TransferMismatch);
        }

        // #865: record this deposit as an inflow event for the trailing-rate forecast,
        // and opportunistically drain any queued withdrawals now that fresh liquidity
        // has arrived — don't wait solely for the next repayment (mirrors the same
        // best-effort, non-blocking pattern used in repay_invoice_request).
        record_inflow_event(&env, &token, usdc_received);
        // #799: activates the referral on this investor's first deposit.
        // The pool has no separate yield fee to share yet, so fee_amount is
        // 0 here — record_referral_activity is a no-op reward-wise but
        // still records the qualifying activity / referral count.
        Self::record_referral_activity(&env, &investor, symbol_short!("deposit"), 0, &token);
        let post_deposit_liquidity = available_liquidity(&tt).unwrap_or(0);
        if let Err(e) = Self::process_withdrawal_queue(&env, token.clone(), post_deposit_liquidity)
        {
            let _ = e;
            env.logs().add("Failed to process withdrawal queue", &[]);
        }

        Self::non_reentrant_end(&env);

        env.events().publish(
            (EVT, symbol_short!("deposit")),
            (
                investor,
                token,
                received,
                shares_to_mint,
                env.ledger().timestamp(),
            ),
        );
        Ok(())
    }

    pub fn withdraw(
        env: Env,
        investor: Address,
        token: Address,
        shares: i128,
    ) -> Result<(), PoolError> {
        investor.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        if shares <= 0 {
            return Err(PoolError::InvalidAmount);
        }
        Self::assert_accepted_token(&env, &token)?;

        // #867: opt-in compliance gate on fund outflow
        Self::require_compliance_cleared(&env, &investor)?;

        Self::non_reentrant_start(&env); // <- ADD GUARD START

        // #244: withdrawal rate limiting
        let config = get_config_cached(&env)?;
        let now = env.ledger().timestamp();
        let is_admin = config.admin == investor;
        if !is_admin && config.withdrawal_cooldown_secs > 0 {
            let last: u64 = env
                .storage()
                .persistent()
                .get(&DataKey::LastWithdrawalTime(
                    investor.clone(),
                    token.clone(),
                ))
                .unwrap_or(0);
            if now < last.saturating_add(config.withdrawal_cooldown_secs) {
                return Err(PoolError::WithdrawalCooldownActive);
            }
        }

        let share_token_key = DataKey::ShareToken(token.clone());
        let token_totals_key = DataKey::TokenTotals(token.clone());
        let share_token: Address = env
            .storage()
            .instance()
            .get(&share_token_key)
            .ok_or(PoolError::ShareTokenNotConfigured)?;
        let mut tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&token_totals_key)
            .unwrap_or_default();

        let mut bal_args = Vec::new(&env);
        bal_args.push_back(investor.clone().into_val(&env));
        let share_balance: i128 =
            env.invoke_contract(&share_token, &Symbol::new(&env, "balance"), bal_args);
        if share_balance < shares {
            return Err(PoolError::InvalidAmount);
        }

        let total_shares: i128 = env.invoke_contract(
            &share_token,
            &Symbol::new(&env, "total_supply"),
            Vec::new(&env),
        );

        // pool_value is USDC-denominated; compute USDC then convert to token amount
        let usdc_amount = shares
            .checked_mul(tt.pool_value)
            .ok_or(PoolError::AmountOverflow)?
            .checked_div(total_shares)
            .ok_or(PoolError::AmountOverflow)?;
        let rate_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ExchangeRate(token.clone()))
            .unwrap_or(10_000u32);
        let amount = usdc_amount
            .checked_mul(10_000i128)
            .ok_or(PoolError::AmountOverflow)?
            .checked_div(rate_bps as i128)
            .ok_or(PoolError::AmountOverflow)?;

        // #782: reject before any state change (share burn / pool_value update
        // below) when the pool doesn't have enough undeployed liquidity to
        // cover this withdrawal — otherwise the accounting step could
        // succeed while the token transfer later fails, burning the
        // investor's shares without paying them out.
        let available_liquidity = tt.pool_value - tt.total_deployed;
        if available_liquidity < usdc_amount {
            return Err(PoolError::InsufficientLiquidity);
        }

        // #244: single-withdrawal cap (skip for admin) — compare in USDC terms
        if !is_admin && config.max_single_withdrawal_bps < BPS_DENOM {
            let max_single =
                (tt.pool_value * config.max_single_withdrawal_bps as i128) / BPS_DENOM as i128;
            if usdc_amount > max_single {
                return Err(PoolError::WithdrawalExceedsLimit);
            }
        }

        // Burn shares FIRST - effects
        let mut burn_args = Vec::new(&env);
        burn_args.push_back(investor.clone().into_val(&env));
        burn_args.push_back(shares.into_val(&env));
        let _: () = env.invoke_contract(&share_token, &Symbol::new(&env, "burn"), burn_args);
        update_investor_available(&env, &investor, &token, -shares)?;

        // Update USDC-denominated pool value SECOND - effects
        tt.pool_value -= usdc_amount;
        env.storage().instance().set(&token_totals_key, &tt);

        // #244: record withdrawal timestamp
        if config.withdrawal_cooldown_secs > 0 {
            env.storage().persistent().set(
                &DataKey::LastWithdrawalTime(investor.clone(), token.clone()),
                &now,
            );
        }

        // #532: verify pool has sufficient token balance before transfer
        let token_client = token::Client::new(&env, &token);
        let pool_balance = token_client.balance(&env.current_contract_address());
        if amount > pool_balance {
            return Err(PoolError::InsufficientPoolFunds);
        }
        token_client.transfer(&env.current_contract_address(), &investor, &amount);

        Self::non_reentrant_end(&env); // <- ADD GUARD END

        env.events().publish(
            (EVT, symbol_short!("withdraw")),
            (investor, token, amount, shares, now),
        );
        Ok(())
    }

    pub fn request_withdrawal(
        env: Env,
        investor: Address,
        token: Address,
        shares: i128,
    ) -> Result<u64, PoolError> {
        investor.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        if shares <= 0 {
            return Err(PoolError::ZeroAmount);
        }
        Self::assert_accepted_token(&env, &token)?;

        // #867: opt-in compliance gate on withdrawal requests
        Self::require_compliance_cleared(&env, &investor)?;

        // Check if investor already has a pending request for this token
        let queue_key = DataKey::WithdrawalQueue(token.clone());
        let mut queue: Vec<WithdrawalRequest> = env
            .storage()
            .persistent()
            .get(&queue_key)
            .unwrap_or(Vec::new(&env));

        for request in queue.iter() {
            if request.investor == investor {
                return Err(PoolError::AlreadyQueuedForWithdrawal);
            }
        }

        let investor_pos_key = DataKey::InvestorPosition(investor.clone(), token.clone());
        let position: InvestorPosition = env
            .storage()
            .persistent()
            .get(&investor_pos_key)
            .unwrap_or_default();
        if shares > position.available {
            return Err(PoolError::WithdrawalExceedsLimit);
        }

        Self::non_reentrant_start(&env);

        let share_token_key = DataKey::ShareToken(token.clone());
        let token_totals_key = DataKey::TokenTotals(token.clone());
        let share_token: Address = env
            .storage()
            .instance()
            .get(&share_token_key)
            .ok_or(PoolError::ShareTokenNotConfigured)?;
        let tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&token_totals_key)
            .unwrap_or_default();

        let mut bal_args = Vec::new(&env);
        bal_args.push_back(investor.clone().into_val(&env));
        let share_balance: i128 =
            env.invoke_contract(&share_token, &Symbol::new(&env, "balance"), bal_args);
        if share_balance < shares {
            Self::non_reentrant_end(&env);
            return Err(PoolError::InvalidAmount);
        }

        let total_shares: i128 = env.invoke_contract(
            &share_token,
            &Symbol::new(&env, "total_supply"),
            Vec::new(&env),
        );
        if total_shares <= 0 {
            Self::non_reentrant_end(&env);
            return Err(PoolError::InvalidAmount);
        }
        let amount = Self::withdrawal_amount(shares, tt.pool_value, total_shares)?;
        let available_liquidity = available_liquidity(&tt)?;

        let now = env.ledger().timestamp();

        // Return `0` when processed immediately (no queued request created).
        let mut request_id: u64 = 0;
        if available_liquidity >= amount {
            // Sufficient liquidity - process immediately
            Self::process_immediate_withdrawal(
                &env,
                investor,
                token,
                shares,
                amount,
                tt,
                share_token,
            )?;
        } else {
            // #865: bound the number of distinct investors that can sit in the queue at
            // once (each investor already can only hold one entry — see the
            // AlreadyQueuedForWithdrawal check above — so this caps total queue depth).
            let config = get_config_cached(&env)?;
            if config.max_withdrawal_queue_depth > 0
                && queue.len() >= config.max_withdrawal_queue_depth
            {
                Self::non_reentrant_end(&env);
                return Err(PoolError::WithdrawalQueueFull);
            }

            // Insufficient liquidity - queue the request
            update_investor_available(&env, &investor, &token, -shares)?;
            request_id = Self::generate_request_id(&env, &token);
            let request = WithdrawalRequest {
                investor: investor.clone(),
                token: token.clone(),
                shares,
                requested_at: now,
                request_id,
            };

            queue.push_back(request.clone());
            env.storage().persistent().set(&queue_key, &queue);

            // Store individual request for lookup
            let request_key = DataKey::WithdrawalRequest(investor.clone(), request_id);
            env.storage().persistent().set(&request_key, &request);

            // #865: include `token` so the indexer can reconstruct per-token queue
            // state without a live contract call — see settle_queued_withdrawal /
            // process_withdrawal_queue's wd_full/wd_part events below, which do the
            // same for consistency.
            env.events().publish(
                (EVT, symbol_short!("wd_queue")),
                (investor, token, shares, request_id),
            );
        }

        Self::non_reentrant_end(&env);
        Ok(request_id)
    }

    pub fn cancel_withdrawal_request(
        env: Env,
        investor: Address,
        token: Address,
    ) -> Result<(), PoolError> {
        investor.require_auth();
        bump_instance(&env);

        non_reentrant!(&env, {
            let queue_key = DataKey::WithdrawalQueue(token.clone());
            let queue: Vec<WithdrawalRequest> = env
                .storage()
                .persistent()
                .get(&queue_key)
                .unwrap_or(Vec::new(&env));

            let mut new_queue = Vec::new(&env);
            let mut request_id = 0u64;
            let mut request_shares = 0i128;
            for req in queue.iter() {
                if req.investor == investor {
                    request_id = req.request_id;
                    request_shares = req.shares;
                } else {
                    new_queue.push_back(req);
                }
            }
            if request_id == 0 {
                return Err(PoolError::WithdrawalRequestNotFound);
            }
            env.storage().persistent().set(&queue_key, &new_queue);

            let request_key = DataKey::WithdrawalRequest(investor.clone(), request_id);
            env.storage().persistent().remove(&request_key);
            update_investor_available(&env, &investor, &token, request_shares)?;

            env.events().publish(
                (EVT, symbol_short!("wd_cncl")),
                (investor, token, request_id),
            );
            Ok(())
        })
    }

    pub fn get_withdrawal_queue(env: Env, token: Address) -> Vec<WithdrawalRequest> {
        bump_instance(&env);
        let queue_key = DataKey::WithdrawalQueue(token);
        env.storage()
            .persistent()
            .get(&queue_key)
            .unwrap_or(Vec::new(&env))
    }

    pub fn get_trailing_inflow_rate(env: Env, token: Address) -> i128 {
        trailing_inflow_rate_per_sec(&env, &token)
    }

    fn process_immediate_withdrawal(
        env: &Env,
        investor: Address,
        token: Address,
        shares: i128,
        amount: i128,
        mut tt: PoolTokenTotals,
        share_token: Address,
    ) -> Result<(), PoolError> {
        Self::burn_withdrawal_shares(env, &share_token, investor.clone(), shares);
        update_investor_available(env, &investor, &token, -shares)?;

        tt.pool_value -= amount;
        let token_totals_key = DataKey::TokenTotals(token.clone());
        env.storage().instance().set(&token_totals_key, &tt);

        let token_client = token::Client::new(env, &token);
        token_client.transfer(&env.current_contract_address(), &investor, &amount);

        // #865: `request_id` is 0 here (settled immediately, never queued) — matches
        // the same "0 = not queued" convention `request_withdrawal` returns to the
        // caller, so the indexer can uniformly read index 4 across every wd_full
        // event and know 0 means "not a queue removal."
        env.events().publish(
            (EVT, symbol_short!("wd_full")),
            (investor, token, amount, shares, 0u64),
        );
        Ok(())
    }

    pub fn get_open_invoices_for_token(env: Env, token: Address) -> Vec<FundedInvoice> {
        let ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::TokenInvoiceIds(token.clone()))
            .unwrap_or(Vec::new(&env));
        let mut open = Vec::new(&env);
        for invoice_id in ids.iter() {
            if let Some(record) = env
                .storage()
                .persistent()
                .get::<DataKey, FundedInvoice>(&DataKey::FundedInvoice(invoice_id))
            {
                if record.token == token && record.repaid_amount < record.principal {
                    open.push_back(record);
                }
            }
        }
        open
    }

    fn generate_request_id(env: &Env, token: &Address) -> u64 {
        let counter_key = DataKey::WithdrawalQueueCounter(token.clone());
        let current_count: u64 = env.storage().persistent().get(&counter_key).unwrap_or(0);
        let new_id = current_count + 1;
        env.storage().persistent().set(&counter_key, &new_id);
        new_id
    }

    fn withdrawal_amount(shares: i128, pool_value: i128, total_shares: i128) -> PoolResult<i128> {
        if total_shares <= 0 {
            return Err(PoolError::InvalidAmount);
        }
        shares
            .checked_mul(pool_value)
            .and_then(|value| value.checked_div(total_shares))
            .ok_or(PoolError::AmountOverflow)
    }

    fn burn_withdrawal_shares(env: &Env, share_token: &Address, investor: Address, shares: i128) {
        let mut burn_args = Vec::new(env);
        burn_args.push_back(investor.into_val(env));
        burn_args.push_back(shares.into_val(env));
        let _: () = env.invoke_contract(share_token, &Symbol::new(env, "burn"), burn_args);
    }

    fn settle_queued_withdrawal(
        env: &Env,
        token: &Address,
        share_token: &Address,
        request: &WithdrawalRequest,
        shares_to_burn: i128,
        amount: i128,
    ) -> PoolResult<()> {
        Self::burn_withdrawal_shares(env, share_token, request.investor.clone(), shares_to_burn);

        let token_client = token::Client::new(env, token);
        token_client.transfer(&env.current_contract_address(), &request.investor, &amount);

        env.events().publish(
            (EVT, symbol_short!("wd_part")),
            (
                request.investor.clone(),
                token.clone(),
                amount,
                shares_to_burn,
                request.request_id,
            ),
        );
        Ok(())
    }

    fn process_withdrawal_queue(
        env: &Env,
        token: Address,
        available_amount: i128,
    ) -> Result<(), PoolError> {
        let queue_key = DataKey::WithdrawalQueue(token.clone());
        let queue: Vec<WithdrawalRequest> = env
            .storage()
            .persistent()
            .get(&queue_key)
            .unwrap_or(Vec::new(env));

        if queue.is_empty() {
            return Ok(());
        }

        let mut tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&DataKey::TokenTotals(token.clone()))
            .unwrap_or_default();
        let liquid = available_liquidity(&tt)?;
        let mut remaining_amount = if available_amount < liquid {
            available_amount
        } else {
            liquid
        };
        if remaining_amount <= 0 {
            return Ok(());
        }

        let share_token_key = DataKey::ShareToken(token.clone());
        let share_token: Address = env
            .storage()
            .instance()
            .get(&share_token_key)
            .ok_or(PoolError::ShareTokenNotConfigured)?;
        let total_shares: i128 = env.invoke_contract(
            &share_token,
            &Symbol::new(env, "total_supply"),
            Vec::new(env),
        );
        if total_shares <= 0 {
            return Ok(());
        }

        let config = get_config_cached(env)?;
        let max_age_secs = (config.max_withdrawal_queue_age_days as u64) * SECS_PER_DAY;
        let now = env.ledger().timestamp();
        let valuation_pool_value = tt.pool_value;
        let mut due_by_age = Vec::new(env);
        let mut regular = Vec::new(env);
        let mut total_due_shares: i128 = 0;

        for request in queue.iter() {
            let aged = max_age_secs > 0 && now >= request.requested_at.saturating_add(max_age_secs);
            if aged {
                total_due_shares = total_due_shares
                    .checked_add(request.shares)
                    .ok_or(PoolError::AmountOverflow)?;
                due_by_age.push_back(request);
            } else {
                regular.push_back(request);
            }
        }

        if total_due_shares == 0 {
            due_by_age = regular;
            regular = Vec::new(env);
            for request in due_by_age.iter() {
                total_due_shares = total_due_shares
                    .checked_add(request.shares)
                    .ok_or(PoolError::AmountOverflow)?;
            }
        }

        let distributable_amount = remaining_amount;
        let mut remaining_queue = Vec::new(env);
        for request in due_by_age.iter() {
            let available_for_request = distributable_amount
                .checked_mul(request.shares)
                .and_then(|value| value.checked_div(total_due_shares))
                .ok_or(PoolError::AmountOverflow)?;
            let request_amount =
                Self::withdrawal_amount(request.shares, valuation_pool_value, total_shares)?;
            let payout = if available_for_request > request_amount {
                request_amount
            } else {
                available_for_request
            };

            if payout <= 0 {
                remaining_queue.push_back(request);
                continue;
            }

            let shares_to_burn = if payout == request_amount {
                request.shares
            } else {
                payout
                    .checked_mul(request.shares)
                    .and_then(|value| value.checked_div(request_amount))
                    .ok_or(PoolError::AmountOverflow)?
            };

            Self::settle_queued_withdrawal(
                env,
                &token,
                &share_token,
                &request,
                shares_to_burn,
                payout,
            )?;
            tt.pool_value = tt
                .pool_value
                .checked_sub(payout)
                .ok_or(PoolError::AmountOverflow)?;
            remaining_amount = remaining_amount
                .checked_sub(payout)
                .ok_or(PoolError::AmountOverflow)?;

            if shares_to_burn == request.shares {
                let request_key =
                    DataKey::WithdrawalRequest(request.investor.clone(), request.request_id);
                env.storage().persistent().remove(&request_key);
                env.events().publish(
                    (EVT, symbol_short!("wd_full")),
                    (
                        request.investor,
                        request.token.clone(),
                        payout,
                        request.shares,
                        request.request_id,
                    ),
                );
            } else {
                let remaining_request = WithdrawalRequest {
                    investor: request.investor.clone(),
                    token: request.token.clone(),
                    shares: request
                        .shares
                        .checked_sub(shares_to_burn)
                        .ok_or(PoolError::AmountOverflow)?,
                    requested_at: request.requested_at,
                    request_id: request.request_id,
                };
                let request_key = DataKey::WithdrawalRequest(
                    remaining_request.investor.clone(),
                    remaining_request.request_id,
                );
                env.storage()
                    .persistent()
                    .set(&request_key, &remaining_request);
                remaining_queue.push_back(remaining_request);
            }
        }

        for request in regular.iter() {
            remaining_queue.push_back(request);
        }

        env.storage().persistent().set(&queue_key, &remaining_queue);

        let token_totals_key = DataKey::TokenTotals(token);
        env.storage().instance().set(&token_totals_key, &tt);

        Ok(())
    }

    pub fn claim_yield(env: Env, investor: Address, token: Address) -> Result<(), PoolError> {
        investor.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);

        non_reentrant!(&env, {
            let token_totals_key = DataKey::TokenTotals(token.clone());
            let mut tt: PoolTokenTotals = env
                .storage()
                .instance()
                .get(&token_totals_key)
                .unwrap_or_default();

            let snapshot_key = DataKey::InvestorRewardSnapshot(investor.clone(), token.clone());
            let last_rps: i128 = env.storage().persistent().get(&snapshot_key).unwrap_or(0);

            let share_token: Address = env
                .storage()
                .instance()
                .get(&DataKey::ShareToken(token.clone()))
                .ok_or(PoolError::ShareTokenNotConfigured)?;

            let investor_shares: i128 =
                env.invoke_contract(&share_token, &Symbol::new(&env, "balance"), {
                    let mut args = Vec::new(&env);
                    args.push_back(investor.clone().into_val(&env));
                    args
                });

            let claimable = if investor_shares > 0 && tt.reward_per_share > last_rps {
                ((tt.reward_per_share - last_rps) * investor_shares) / REWARD_PRECISION
            } else {
                0
            };

            // #773: loyalty bonus — scales the yield just claimed by
            // (bonus_bps / base yield_bps), i.e. a long-term depositor's
            // realized yield is boosted in the same proportion their
            // effective APY is above the pool's base rate. Funded out of
            // protocol_revenue (the same fee pool #784's treasury draws
            // from) and capped by whatever is actually available there, so
            // this can never draw down other investors' principal.
            let mut bonus: i128 = 0;
            if claimable > 0 {
                let position: InvestorPosition = env
                    .storage()
                    .persistent()
                    .get(&DataKey::InvestorPosition(investor.clone(), token.clone()))
                    .unwrap_or_default();
                let config = get_config_cached(&env)?;
                if config.yield_bps > 0 {
                    let tiers = get_loyalty_tiers_cached(&env);
                    let days_active = loyalty_days_active(&env, position.loyalty_start_at);
                    let (_, bonus_bps, _) = resolve_loyalty_tier(&tiers, days_active);
                    if bonus_bps > 0 {
                        let raw_bonus = claimable
                            .checked_mul(bonus_bps as i128)
                            .ok_or(PoolError::AmountOverflow)?
                            .checked_div(config.yield_bps as i128)
                            .ok_or(PoolError::AmountOverflow)?;
                        bonus = raw_bonus.min(tt.protocol_revenue).max(0);
                    }
                }
            }
            let total_claim = claimable
                .checked_add(bonus)
                .ok_or(PoolError::AmountOverflow)?;

            if total_claim > 0 {
                let token_client = token::Client::new(&env, &token);
                // Issue #336 Fix: Use try_transfer to detect failures
                // Only update snapshot if transfer succeeds
                match token_client.try_transfer(
                    &env.current_contract_address(),
                    &investor,
                    &total_claim,
                ) {
                    Ok(_) => {
                        // Transfer succeeded - update snapshot
                        env.storage()
                            .persistent()
                            .set(&snapshot_key, &tt.reward_per_share);
                        if bonus > 0 {
                            tt.protocol_revenue = tt
                                .protocol_revenue
                                .checked_sub(bonus)
                                .ok_or(PoolError::AmountOverflow)?;
                            env.storage().instance().set(&token_totals_key, &tt);
                        }
                    }
                    Err(_) => {
                        // Transfer failed - do NOT update snapshot
                        // Investor can retry claim later
                        return Err(PoolError::TransferMismatch);
                    }
                }
            } else {
                // No yield to claim - safe to update snapshot
                env.storage()
                    .persistent()
                    .set(&snapshot_key, &tt.reward_per_share);
            }

            env.events().publish(
                (EVT, symbol_short!("yld_claim")),
                (investor, token, claimable, bonus),
            );
            Ok(())
        })
    }

    pub fn fund_invoice(
        env: Env,
        admin: Address,
        invoice_id: u64,
        principal: i128,
        sme: Address,
        due_date: u64,
        token: Address,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;

        Self::non_reentrant_start(&env);

        let config = get_config_cached(&env)?;

        // #385: verify the invoice contract still has this pool as its authorized pool.
        // Guards against funding invoices that belong to a stale or swapped-out pool config.
        let invoice_client = InvoiceContractClient::new(&env, &config.invoice_contract);
        let this_contract = env.current_contract_address();
        match invoice_client.try_get_authorized_pool() {
            Ok(Ok(ref auth_pool)) if auth_pool == &this_contract => {}
            _ => return Err(PoolError::InvoicePoolMismatch),
        }
        let accepted_tokens: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AcceptedTokens)
            .ok_or(PoolError::NotInitialized)?;

        // Collateral check: high-value invoices must have collateral deposited first.
        let deposit: Option<CollateralDeposit> = env
            .storage()
            .persistent()
            .get(&DataKey::CollateralDeposit(invoice_id));

        // #764: if collateral was already deposited, validate against the
        // CollateralConfig snapshotted at deposit time, not whatever is live
        // now — a later admin change to collateral_bps must not retroactively
        // invalidate a deposit the SME already posted in good faith. An
        // invoice with no deposit at all has no prior commitment to honor,
        // so it falls back to whatever config is live right now.
        let req_collateral = match &deposit {
            Some(d) => required_collateral(
                principal,
                &CollateralConfig {
                    threshold: d.threshold_at_deposit,
                    collateral_bps: d.collateral_bps_at_deposit,
                },
            ),
            None => {
                let collateral_cfg: CollateralConfig = env
                    .storage()
                    .instance()
                    .get(&DataKey::CollateralConfig)
                    .unwrap_or(CollateralConfig {
                        threshold: DEFAULT_COLLATERAL_THRESHOLD,
                        collateral_bps: DEFAULT_COLLATERAL_BPS,
                    });
                required_collateral(principal, &collateral_cfg)
            }
        };
        if req_collateral > 0 {
            match deposit {
                None => return Err(PoolError::CollateralNotFound),
                Some(d) => {
                    if d.settled {
                        return Err(PoolError::CollateralAlreadySettled);
                    }
                    if d.token == token {
                        // Same-asset collateral: compare raw amounts directly, no
                        // oracle dependency for the common case (preserves existing
                        // behavior for pools that never configure a price oracle).
                        if d.amount < req_collateral {
                            return Err(PoolError::InvalidAmount);
                        }
                    } else {
                        // #1036: cross-asset collateral — convert both sides to a
                        // common unit via the same oracle price feed the
                        // risk-response satellite (contracts/auction) reads for its
                        // own ongoing monitoring (Self::get_asset_price, bounded by
                        // the same staleness window either way — computed inline
                        // here rather than via a cross-contract call, since it's a
                        // pure read pool already has and doesn't need the
                        // satellite's config/liquidation state). collateral_value <
                        // required_value means the posted collateral's current market
                        // value doesn't cover the required amount — equivalent to (and
                        // cheaper than) computing a bps ratio and comparing it to
                        // BPS_DENOM, since floor(a/b) < c iff a < b*c for b > 0.
                        let collateral_price = Self::get_asset_price(env.clone(), d.token.clone())?;
                        let funding_price = Self::get_asset_price(env.clone(), token.clone())?;
                        let collateral_value = d
                            .amount
                            .checked_mul(collateral_price)
                            .ok_or(PoolError::AmountOverflow)?;
                        let required_value = req_collateral
                            .checked_mul(funding_price)
                            .ok_or(PoolError::AmountOverflow)?;
                        if required_value > 0 && collateral_value < required_value {
                            return Err(PoolError::InvalidAmount);
                        }
                    }
                }
            }
        }

        // #867: gate invoice funding on SME compliance when enabled
        Self::require_compliance_cleared(&env, &sme)?;

        let mut stats: PoolStorageStats = env
            .storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default();
        let request = FundingRequest {
            invoice_id,
            principal,
            sme,
            due_date,
            token,
        };
        fund_invoice_request(&env, &config, &accepted_tokens, &mut stats, &request)?;
        env.storage().instance().set(&DataKey::StorageStats, &stats);

        Self::non_reentrant_end(&env);
        Ok(())
    }

    pub fn fund_multiple_invoices(
        env: Env,
        admin: Address,
        requests: Vec<FundingRequest>,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;

        if requests.is_empty() {
            return Err(PoolError::InvalidAmount);
        }
        if requests.len() > MAX_BATCH_SIZE {
            return Err(PoolError::BatchTooLarge);
        }

        bump_instance(&env);
        Self::non_reentrant_start(&env);

        let config = get_config_cached(&env)?;
        let accepted_tokens: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AcceptedTokens)
            .ok_or(PoolError::NotInitialized)?;
        let mut stats: PoolStorageStats = env
            .storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default();

        for i in 0..requests.len() {
            let request = requests.get(i).ok_or(PoolError::StorageCorrupted)?;
            fund_invoice_request(&env, &config, &accepted_tokens, &mut stats, &request)?;
        }

        env.storage().instance().set(&DataKey::StorageStats, &stats);
        Self::non_reentrant_end(&env);
        Ok(())
    }

    pub fn fund_invoices_batch(
        env: Env,
        admin: Address,
        requests: Vec<FundingRequest>,
    ) -> Result<(), PoolError> {
        Self::fund_multiple_invoices(env, admin, requests)
    }

    pub fn repay_invoices_batch(
        env: Env,
        payer: Address,
        repayments: Vec<RepaymentRequest>,
    ) -> Result<(), PoolError> {
        payer.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        if repayments.is_empty() {
            return Err(PoolError::InvalidAmount);
        }
        if repayments.len() > MAX_BATCH_SIZE {
            return Err(PoolError::BatchTooLarge);
        }

        for i in 0..repayments.len() {
            let request = repayments.get(i).ok_or(PoolError::StorageCorrupted)?;
            Self::repay_invoice_request(&env, request.invoice_id, payer.clone(), request.amount)?;
        }
        Ok(())
    }

    fn repay_invoice_request(
        env: &Env,
        invoice_id: u64,
        payer: Address,
        amount: i128,
    ) -> Result<(), PoolError> {
        if amount <= 0 {
            return Err(PoolError::InvalidAmount);
        }

        let config: PoolConfig = get_config_cached(env)?;
        let funded_invoice_key = DataKey::FundedInvoice(invoice_id);
        let mut record: FundedInvoice = env
            .storage()
            .persistent()
            .get(&funded_invoice_key)
            .ok_or(PoolError::InvoiceNotFound)?;

        // #536: only the invoice's borrower (SME) may repay their own invoice
        if payer != record.sme {
            return Err(PoolError::Unauthorized);
        }

        let now = env.ledger().timestamp();
        let (total_interest, total_due) = calculate_total_due(&record, &config, now)?;
        let total_interest_i128 = u128_to_i128(total_interest)?;

        if record.repaid_amount >= total_due {
            return Err(PoolError::AlreadyFullyRepaid);
        }
        // A final repayment may be larger than the outstanding balance. Cap
        // it so only the amount actually owed is transferred from the payer.
        let remaining_amount = total_due
            .checked_sub(record.repaid_amount)
            .ok_or(PoolError::AmountOverflow)?;
        let actual_payment = if amount > remaining_amount {
            remaining_amount
        } else {
            amount
        };
        let new_repaid_amount = record
            .repaid_amount
            .checked_add(actual_payment)
            .ok_or(PoolError::AmountOverflow)?;

        Self::non_reentrant_start(env);
        let guarded_result = (|| -> Result<(bool, Address, i128, i128, i128), PoolError> {
            // Update state FIRST - effects
            record.repaid_amount = new_repaid_amount;
            let fully_repaid = record.repaid_amount >= total_due;

            let token_totals_key = DataKey::TokenTotals(record.token.clone());
            let mut tt: PoolTokenTotals = env
                .storage()
                .instance()
                .get(&token_totals_key)
                .unwrap_or_default();

            let mut stats: PoolStorageStats = env
                .storage()
                .instance()
                .get(&DataKey::StorageStats)
                .unwrap_or_default();

            if let Some(round_id) = record.co_funding_round_id {
                distribute_pari_passu_repayment(
                    env,
                    round_id,
                    actual_payment,
                    &mut tt,
                    &mut stats,
                    fully_repaid,
                )?;
            } else if fully_repaid {
                tt.total_deployed = tt
                    .total_deployed
                    .checked_sub(record.principal)
                    .ok_or(PoolError::AmountOverflow)?;
                tt.pool_value = tt
                    .pool_value
                    .checked_add(total_interest_i128)
                    .ok_or(PoolError::AmountOverflow)?;
                tt.total_fee_revenue = tt
                    .total_fee_revenue
                    .checked_add(record.factoring_fee)
                    .ok_or(PoolError::AmountOverflow)?;
                tt.protocol_revenue = tt
                    .protocol_revenue
                    .checked_add(record.factoring_fee)
                    .ok_or(PoolError::AmountOverflow)?;
                tt.total_paid_out = tt
                    .total_paid_out
                    .checked_add(total_due)
                    .ok_or(PoolError::AmountOverflow)?;
                stats.active_funded_invoices = stats.active_funded_invoices.saturating_sub(1);

                // Distribute interest proportionally to share holders via reward_per_share accumulator.
                let share_token: Address = env
                    .storage()
                    .instance()
                    .get(&DataKey::ShareToken(record.token.clone()))
                    .ok_or(PoolError::ShareTokenNotConfigured)?;
                let total_shares: i128 = env.invoke_contract(
                    &share_token,
                    &Symbol::new(env, "total_supply"),
                    Vec::new(env),
                );
                if total_shares > 0 {
                    let reward_delta = calculate_reward_delta(total_interest_i128, total_shares)?;
                    tt.reward_per_share = tt
                        .reward_per_share
                        .checked_add(reward_delta)
                        .ok_or(PoolError::AmountOverflow)?;
                }
            }

            // Write all state BEFORE external call
            env.storage().persistent().set(&funded_invoice_key, &record);
            if fully_repaid {
                set_funded_invoice_ttl(env, invoice_id, true);
            }
            env.storage().instance().set(&token_totals_key, &tt);
            env.storage().instance().set(&DataKey::StorageStats, &stats);

            // Transfer LAST - interaction
            let token_client = token::Client::new(env, &record.token);
            token_client.transfer(&payer, &env.current_contract_address(), &actual_payment);

            // Handle collateral release after main transfer
            if fully_repaid {
                release_collateral(env, invoice_id, &payer, now);
            }

            let available_amount = if fully_repaid {
                available_liquidity(&tt)?
            } else {
                0
            };

            Ok((
                fully_repaid,
                record.token.clone(),
                record.principal,
                record.repaid_amount,
                available_amount,
            ))
        })();
        Self::non_reentrant_end(env);
        let (fully_repaid, token, _principal, repaid_amount, available_amount) = guarded_result?;

        // #863: a repayment changes utilization (deployed capital and/or pool
        // value) — record a rate sample for the history chart.
        record_rate_snapshot(env, &token);

        if fully_repaid {
            // #217: Process withdrawal queue after repayment
            if let Err(e) = Self::process_withdrawal_queue(env, token.clone(), available_amount) {
                // Log error but don't fail the repayment
                // `format!` is unavailable in `no_std`; keep a lightweight log.
                let _ = e;
                env.logs().add("Failed to process withdrawal queue", &[]);
            }

            // #799: pay the referrer (if any) their cut of the
            // factoring fee just realized above.
            Self::record_referral_activity(
                env,
                &record.sme,
                symbol_short!("borrow"),
                record.factoring_fee,
                &token,
            );

            env.events().publish(
                (EVT, symbol_short!("repaid")),
                (
                    invoice_id,
                    payer.clone(),
                    record.principal,
                    total_interest_i128,
                    now,
                ),
            );
        } else {
            env.events().publish(
                (EVT, symbol_short!("part_pay")),
                (invoice_id, actual_payment, repaid_amount, now),
            );
        }
        Ok(())
    }

    pub fn repay_invoice(
        env: Env,
        invoice_id: u64,
        payer: Address,
        amount: i128,
    ) -> Result<(), PoolError> {
        payer.require_auth();
        bump_instance(&env);
        // #779: repayment is explicitly exempt from the pause guard — see
        // the policy note on `pause()`.
        Self::repay_invoice_request(&env, invoice_id, payer, amount)
    }

    // ---- Collateral management ----

    pub fn set_collateral_config(
        env: Env,
        admin: Address,
        _threshold: i128,
        _collateral_bps: u32,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        Err(PoolError::OperationRequiresProposal)
    }

    fn validate_collateral_config(threshold: i128, collateral_bps: u32) -> PoolResult<()> {
        if threshold < 0 {
            return Err(PoolError::InvalidCollateralThreshold);
        }
        if collateral_bps > BPS_DENOM {
            return Err(PoolError::InvalidCollateralBps);
        }
        if collateral_bps == 0 && threshold > 0 {
            return Err(PoolError::InvalidCollateralBps);
        }
        Ok(())
    }

    fn execute_set_collateral_config(
        env: &Env,
        admin: &Address,
        threshold: i128,
        collateral_bps: u32,
    ) -> PoolResult<()> {
        Self::validate_collateral_config(threshold, collateral_bps)?;
        let cfg = CollateralConfig {
            threshold,
            collateral_bps,
        };
        env.storage()
            .instance()
            .set(&DataKey::CollateralConfig, &cfg);
        env.events().publish(
            (EVT, symbol_short!("col_cfg")),
            (admin.clone(), threshold, collateral_bps),
        );
        Ok(())
    }

    pub fn get_collateral_config(env: Env) -> CollateralConfig {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::CollateralConfig)
            .unwrap_or(CollateralConfig {
                threshold: DEFAULT_COLLATERAL_THRESHOLD,
                collateral_bps: DEFAULT_COLLATERAL_BPS,
            })
    }

    pub fn required_collateral_for(env: Env, principal: i128) -> i128 {
        bump_instance(&env);
        let cfg: CollateralConfig = env
            .storage()
            .instance()
            .get(&DataKey::CollateralConfig)
            .unwrap_or(CollateralConfig {
                threshold: DEFAULT_COLLATERAL_THRESHOLD,
                collateral_bps: DEFAULT_COLLATERAL_BPS,
            });
        required_collateral(principal, &cfg)
    }

    // #1036: registers the trusted collateral-risk-response satellite
    // (contracts/auction) — the only caller risk_liquidate_collateral
    // accepts. Direct admin setter (not the propose_operation timelock) —
    // same tier of trust as set_oracle_contract.
    pub fn set_risk_contract(
        env: Env,
        admin: Address,
        risk_contract: Address,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;
        env.storage().instance().set(&RISK_CONTRACT, &risk_contract);
        env.events()
            .publish((EVT, symbol_short!("set_risk")), (admin, risk_contract));
        Ok(())
    }

    fn require_risk_contract(env: &Env, caller: &Address) -> PoolResult<()> {
        let trusted: Address = env
            .storage()
            .instance()
            .get(&RISK_CONTRACT)
            .ok_or(PoolError::NotInitialized)?;
        if caller != &trusted {
            return Err(PoolError::Unauthorized);
        }
        Ok(())
    }

    // Trusted entrypoint: the risk-response satellite (contracts/auction) has
    // already verified this liquidation is warranted — performs the seizure,
    // crediting the asset into that token's own pool liquidity (mirrors
    // execute_seize_collateral's default-seizure credit path). Pool trusts
    // the caller's decision entirely (same relationship market_settle_listing
    // has with secondary_market) — the at-risk flag itself is purely
    // informational and lives in the satellite's own storage, not here.
    pub fn risk_liquidate_collateral(
        env: Env,
        caller: Address,
        invoice_id: u64,
    ) -> Result<(), PoolError> {
        caller.require_auth();
        Self::require_risk_contract(&env, &caller)?;
        bump_instance(&env);
        Self::require_not_paused(&env);

        non_reentrant!(&env, {
            let key = DataKey::CollateralDeposit(invoice_id);
            let mut deposit: CollateralDeposit = env
                .storage()
                .persistent()
                .get(&key)
                .ok_or(PoolError::CollateralNotFound)?;
            if deposit.settled {
                return Err(PoolError::CollateralAlreadySettled);
            }

            let token_totals_key = DataKey::TokenTotals(deposit.token.clone());
            let mut tt: PoolTokenTotals = env
                .storage()
                .instance()
                .get(&token_totals_key)
                .unwrap_or_default();
            tt.pool_value = tt
                .pool_value
                .checked_add(deposit.amount)
                .ok_or(PoolError::AmountOverflow)?;
            env.storage().instance().set(&token_totals_key, &tt);

            deposit.settled = true;
            deposit.seized_at = env.ledger().timestamp();
            env.storage().persistent().set(&key, &deposit);
            env.storage().persistent().extend_ttl(
                &key,
                SETTLEMENT_COLLATERAL_TTL,
                SETTLEMENT_COLLATERAL_TTL,
            );
            // #1036: matches execute_seize_collateral's own seizure event
            // (col_seiz_default/liq_seized), which also doesn't carry token —
            // this seizure record mirrors that established shape.
            env.events().publish(
                (EVT, symbol_short!("col_liq")),
                (invoice_id, deposit.depositor, deposit.amount),
            );
            Ok(())
        })
    }

    pub fn deposit_collateral(
        env: Env,
        invoice_id: u64,
        depositor: Address,
        token: Address,
        amount: i128,
    ) -> Result<(), PoolError> {
        depositor.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::assert_accepted_token(&env, &token)?;

        if amount <= 0 {
            return Err(PoolError::InvalidAmount);
        }

        non_reentrant!(&env, {
            // Prevent depositing collateral for an already-funded invoice.
            if env
                .storage()
                .persistent()
                .has(&DataKey::FundedInvoice(invoice_id))
            {
                return Err(PoolError::StorageCorrupted);
            }

            // Prevent double-deposit.
            if env
                .storage()
                .persistent()
                .has(&DataKey::CollateralDeposit(invoice_id))
            {
                return Err(PoolError::StorageCorrupted);
            }

            // Transfer collateral from depositor to pool.
            let token_client = token::Client::new(&env, &token);
            token_client.transfer(&depositor, &env.current_contract_address(), &amount);

            // #764: snapshot the collateral config in effect right now, so a
            // later admin change to collateral_bps can't retroactively make
            // this deposit "insufficient" when fund_invoice_request checks it.
            let collateral_cfg: CollateralConfig = env
                .storage()
                .instance()
                .get(&DataKey::CollateralConfig)
                .unwrap_or(CollateralConfig {
                    threshold: DEFAULT_COLLATERAL_THRESHOLD,
                    collateral_bps: DEFAULT_COLLATERAL_BPS,
                });

            let record = CollateralDeposit {
                invoice_id,
                depositor: depositor.clone(),
                token: token.clone(),
                amount,
                settled: false,
                posted_at: env.ledger().timestamp(),
                released_at: 0,
                seized_at: 0,
                collateral_bps_at_deposit: collateral_cfg.collateral_bps,
                threshold_at_deposit: collateral_cfg.threshold,
            };
            env.storage()
                .persistent()
                .set(&DataKey::CollateralDeposit(invoice_id), &record);
            // Use active invoice TTL — collateral lives as long as the invoice.
            env.storage().persistent().extend_ttl(
                &DataKey::CollateralDeposit(invoice_id),
                ACTIVE_INVOICE_TTL,
                ACTIVE_INVOICE_TTL,
            );

            // #1036: dropped the duplicate trailing `depositor` this tuple used
            // to carry twice (once at index 1, once again at the end) — a
            // leftover redundancy from before this branch's history, not a
            // deliberate two-actor encoding; index 1 already carries it.
            // Also drops the trailing timestamp col_dep used to carry —
            // already recorded as posted_at on the persisted record, and
            // col_topup (the closest sibling event) never carried one either.
            env.events().publish(
                (EVT, symbol_short!("col_dep")),
                (invoice_id, depositor, token, amount),
            );
            Ok(())
        })
    }

    pub fn top_up_collateral(
        env: Env,
        invoice_id: u64,
        depositor: Address,
        token: Address,
        amount: i128,
    ) -> Result<(), PoolError> {
        depositor.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::assert_accepted_token(&env, &token)?;

        if amount <= 0 {
            return Err(PoolError::InvalidAmount);
        }

        non_reentrant!(&env, {
            let key = DataKey::CollateralDeposit(invoice_id);
            let mut record: CollateralDeposit = env
                .storage()
                .persistent()
                .get(&key)
                .ok_or(PoolError::CollateralNotFound)?;

            if record.settled || record.depositor != depositor || record.token != token {
                return Err(PoolError::StorageCorrupted);
            }

            let token_client = token::Client::new(&env, &token);
            token_client.transfer(&depositor, &env.current_contract_address(), &amount);
            record.amount = record
                .amount
                .checked_add(amount)
                .ok_or(PoolError::AmountOverflow)?;

            env.storage().persistent().set(&key, &record);
            env.storage()
                .persistent()
                .extend_ttl(&key, ACTIVE_INVOICE_TTL, ACTIVE_INVOICE_TTL);
            env.events().publish(
                (EVT, symbol_short!("col_topup")),
                (invoice_id, depositor, token, amount, record.amount),
            );
            Ok(())
        })
    }

    pub fn get_collateral_deposit(env: Env, invoice_id: u64) -> Option<CollateralDeposit> {
        bump_instance(&env);
        env.storage()
            .persistent()
            .get(&DataKey::CollateralDeposit(invoice_id))
    }

    pub fn seize_collateral(env: Env, admin: Address, invoice_id: u64) -> Result<(), PoolError> {
        let _ = invoice_id;
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        Err(PoolError::OperationRequiresProposal)
    }

    fn execute_seize_collateral(env: &Env, admin: &Address, invoice_id: u64) -> PoolResult<()> {
        Self::seize_collateral_core(env, admin, invoice_id, "col_seiz_default")
    }

    // #1037: permissionless liquidation trigger. Unlike seize_collateral (which
    // requires an admin-proposed governance action), anyone may call this once an
    // invoice is objectively Defaulted on the invoice contract — no admin gating on
    // the trigger itself. Idempotent: a second call on an already-seized invoice
    // returns CollateralAlreadySettled rather than double-seizing.
    //
    // This only performs the seizure step. Insurance draw-down (insurance.file_claim)
    // and auction clearing remain separate permissionless follow-up calls — Soroban
    // disallows the pool->insurance->pool re-entrancy a single atomic call would
    // require (see the note in seize_collateral_core), so a keeper chains these
    // three permissionless calls rather than the pool orchestrating them itself.
    //
    // (#1036: switched from a /// doc comment to a regular one — #[contractimpl]
    // embeds pub fn doc comments into the on-chain contractspecv0 section, and
    // this one alone measurably affected pool's deploy-size budget.)
    pub fn liquidate_invoice(env: Env, caller: Address, invoice_id: u64) -> Result<(), PoolError> {
        caller.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        env.events().publish(
            (EVT, Symbol::new(&env, "liq_started")),
            (invoice_id, caller.clone(), env.ledger().timestamp()),
        );
        Self::seize_collateral_core(&env, &caller, invoice_id, "liq_seized")
    }

    fn seize_collateral_core(
        env: &Env,
        triggered_by: &Address,
        invoice_id: u64,
        event_name: &str,
    ) -> PoolResult<()> {
        non_reentrant!(env, {
            let record: FundedInvoice = env
                .storage()
                .persistent()
                .get(&DataKey::FundedInvoice(invoice_id))
                .ok_or(PoolError::InvoiceNotFound)?;

            // Calculate total due to check if fully repaid
            let config: PoolConfig = env
                .storage()
                .instance()
                .get(&DataKey::Config)
                .ok_or(PoolError::NotInitialized)?;
            let now = env.ledger().timestamp();
            let (_total_interest, total_due) = calculate_total_due(&record, &config, now)?;

            if record.repaid_amount >= total_due {
                return Err(PoolError::AlreadyFullyRepaid);
            }

            // #386: require the invoice to be explicitly in Defaulted status before seizing.
            // Prevents premature seizure on invoices that are merely overdue but not yet defaulted.
            let invoice_client = InvoiceContractClient::new(env, &config.invoice_contract);
            let is_defaulted = match invoice_client.try_is_invoice_defaulted(&invoice_id) {
                Ok(Ok(v)) => v,
                _ => false,
            };
            if !is_defaulted {
                return Err(PoolError::NotDefaulted);
            }

            let mut col: CollateralDeposit = env
                .storage()
                .persistent()
                .get(&DataKey::CollateralDeposit(invoice_id))
                .ok_or(PoolError::CollateralNotFound)?;

            if col.settled {
                return Err(PoolError::CollateralAlreadySettled);
            }

            // Credit the seized collateral into the pool's token totals so investors benefit.
            let token_totals_key = DataKey::TokenTotals(col.token.clone());
            let mut tt: PoolTokenTotals = env
                .storage()
                .instance()
                .get(&token_totals_key)
                .unwrap_or_default();

            // The defaulted invoice's principal is a receivable that pool_value already
            // counts as an asset (via total_deployed) — it must be written off here, offset
            // by whatever collateral was actually recovered, or pool_value silently overstates
            // investor claims by the unrecovered shortfall. total_deployed drops by the full
            // principal regardless, since the invoice is no longer outstanding either way.
            tt.pool_value = tt
                .pool_value
                .checked_sub(record.principal)
                .and_then(|v| v.checked_add(col.amount))
                .ok_or(PoolError::AmountOverflow)?;
            tt.total_deployed -= record.principal;
            env.storage().instance().set(&token_totals_key, &tt);

            col.settled = true;
            col.seized_at = now;
            env.storage()
                .persistent()
                .set(&DataKey::CollateralDeposit(invoice_id), &col);
            // Use SETTLEMENT_COLLATERAL_TTL (90 days) so the seizure record
            // remains queryable for the full post-default audit window.
            env.storage().persistent().extend_ttl(
                &DataKey::CollateralDeposit(invoice_id),
                SETTLEMENT_COLLATERAL_TTL,
                SETTLEMENT_COLLATERAL_TTL,
            );

            // #386/#1037: emit status-triggered seizure event (invoice was Defaulted).
            // event_name distinguishes the governance-proposed path ("col_seiz_default")
            // from the permissionless liquidation path ("liq_seized") for indexer/frontend
            // pipeline-stage tracking.
            env.events().publish(
                (EVT, Symbol::new(env, event_name)),
                (
                    invoice_id,
                    col.depositor.clone(),
                    col.amount,
                    triggered_by.clone(),
                    now,
                ),
            );

            // #866: the default-insurance reserve's file_claim is deliberately
            // *not* called from here — insurance.file_claim calls back into
            // this pool contract (get_funded_invoice / get_collateral_deposit
            // / receive_insurance_payout) to independently re-derive the
            // shortfall, and Soroban disallows that A→B→A re-entrancy while
            // pool is still on the call stack executing this very function.
            // Instead file_claim is permissionless: anyone (a keeper, the SME,
            // the frontend) files it directly against the insurance contract
            // as a follow-up call once collateral has been seized.

            Ok(())
        })
    }

    pub fn set_yield(env: Env, admin: Address, new_yield_bps: u32) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        let mut config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(PoolError::NotInitialized)?;
        Self::require_admin(&env, &admin)?;
        if new_yield_bps > 5_000 {
            return Err(PoolError::InvalidAmount);
        }
        let now = env.ledger().timestamp();
        if now
            < config
                .last_yield_change_at
                .saturating_add(config.yield_change_cooldown_secs)
        {
            return Err(PoolError::InvalidAmount);
        }
        let current = config.yield_bps;
        let delta = new_yield_bps.abs_diff(current);
        if delta > config.max_yield_change_bps {
            return Err(PoolError::InvalidAmount);
        }
        config.yield_bps = new_yield_bps;
        config.last_yield_change_at = now;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events().publish(
            (EVT, symbol_short!("yield_chg")),
            (admin, current, new_yield_bps),
        );
        Ok(())
    }

    // #227: Two-step yield change with timelock

    pub fn propose_yield_change(
        env: Env,
        admin: Address,
        new_yield_bps: u32,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        // Admin emergency controls remain available while paused.
        let mut config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(PoolError::NotInitialized)?;
        Self::require_admin(&env, &admin)?;

        // Enforce cooldown between successful yield changes (#227/#244 tests rely on this).
        let now = env.ledger().timestamp();
        if now
            < config
                .last_yield_change_at
                .saturating_add(config.yield_change_cooldown_secs)
        {
            return Err(PoolError::InvalidAmount);
        }
        if new_yield_bps > 5_000 {
            return Err(PoolError::InvalidAmount);
        }

        let current = config.yield_bps;
        let delta = new_yield_bps.abs_diff(current);
        if delta > config.max_yield_change_bps {
            return Err(PoolError::InvalidAmount);
        }

        config.proposed_yield_bps = new_yield_bps;
        config.yield_proposal_at = now;
        env.storage().instance().set(&DataKey::Config, &config);

        let effective_at = now + config.yield_timelock_secs;
        env.events().publish(
            (EVT, symbol_short!("y_prop")),
            (admin, current, new_yield_bps, effective_at),
        );
        Ok(())
    }

    pub fn execute_yield_change(env: Env) -> Result<(), PoolError> {
        bump_instance(&env);
        // Yield execution is safe while paused (admin emergency control).
        let mut config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(PoolError::NotInitialized)?;
        if config.proposed_yield_bps == 0 || config.yield_proposal_at == 0 {
            return Err(PoolError::YieldProposalNotFound);
        }

        let now = env.ledger().timestamp();
        let effective_at = config.yield_proposal_at + config.yield_timelock_secs;
        if now < effective_at {
            return Err(PoolError::YieldChangeNotReady);
        }

        let old_bps = config.yield_bps;
        config.yield_bps = config.proposed_yield_bps;
        config.last_yield_change_at = now;
        config.proposed_yield_bps = 0;
        config.yield_proposal_at = 0;
        env.storage().instance().set(&DataKey::Config, &config);

        env.events().publish(
            (EVT, symbol_short!("yield_chg")),
            (old_bps, config.yield_bps),
        );
        Ok(())
    }

    pub fn cancel_yield_proposal(env: Env, admin: Address) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        let mut config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(PoolError::NotInitialized)?;
        config.proposed_yield_bps = 0;
        config.yield_proposal_at = 0;
        env.storage().instance().set(&DataKey::Config, &config);

        env.events().publish((EVT, symbol_short!("y_cncl")), admin);
        Ok(())
    }

    pub fn set_yield_change_policy(
        env: Env,
        admin: Address,
        cooldown_secs: u64,
        max_change_bps: u32,
        timelock_secs: u64,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        // Admin emergency controls remain available while paused.
        let mut config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(PoolError::NotInitialized)?;
        Self::require_admin(&env, &admin)?;
        if cooldown_secs == 0 {
            return Err(PoolError::InvalidAmount);
        }
        if max_change_bps == 0 {
            return Err(PoolError::InvalidAmount);
        }
        if timelock_secs < 3600 {
            return Err(PoolError::InvalidAmount); // minimum 1 hour
        }
        config.yield_change_cooldown_secs = cooldown_secs;
        config.max_yield_change_bps = max_change_bps;
        config.yield_timelock_secs = timelock_secs;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events().publish(
            (EVT, symbol_short!("set_y_pol")),
            (admin, cooldown_secs, max_change_bps, timelock_secs),
        );
        Ok(())
    }

    // #863: utilization-driven rate model governance — the curve's *parameters*
    // move through the same cooldown + 48h-timelock pattern as yield changes;
    // the realized rate itself moves automatically with utilization.

    pub fn propose_rate_model_change(
        env: Env,
        admin: Address,
        token: Address,
        new_config: RateModelConfig,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        let config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(PoolError::NotInitialized)?;
        Self::require_admin(&env, &admin)?;
        validate_rate_model_config(&new_config)?;

        // Same cooldown pattern as propose_yield_change, tracked per token.
        let now = env.ledger().timestamp();
        let last_change: u64 = env
            .storage()
            .instance()
            .get(&DataKey::RateModelChangedAt(token.clone()))
            .unwrap_or(0);
        if now < last_change.saturating_add(config.yield_change_cooldown_secs) {
            return Err(PoolError::InvalidAmount);
        }

        let proposal = RateModelProposal {
            config: new_config,
            proposed_at: now,
        };
        let effective_at = now + config.yield_timelock_secs;
        env.storage()
            .instance()
            .set(&DataKey::PendingRateModel(token.clone()), &proposal);
        env.events().publish(
            (EVT, symbol_short!("rm_prop")),
            (admin, token, effective_at),
        );
        Ok(())
    }

    pub fn execute_rate_model_change(env: Env, token: Address) -> Result<(), PoolError> {
        bump_instance(&env);
        let config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(PoolError::NotInitialized)?;
        let proposal: RateModelProposal = env
            .storage()
            .instance()
            .get(&DataKey::PendingRateModel(token.clone()))
            .ok_or(PoolError::RateModelProposalNotFound)?;

        let now = env.ledger().timestamp();
        let effective_at = proposal
            .proposed_at
            .saturating_add(config.yield_timelock_secs);
        if now < effective_at {
            return Err(PoolError::RateModelChangeNotReady);
        }

        env.storage()
            .instance()
            .set(&DataKey::RateModel(token.clone()), &proposal.config);
        env.storage()
            .instance()
            .remove(&DataKey::PendingRateModel(token.clone()));
        env.storage()
            .instance()
            .set(&DataKey::RateModelChangedAt(token.clone()), &now);

        // Record a sample so history reflects the new curve immediately.
        record_rate_snapshot(&env, &token);
        env.events()
            .publish((EVT, symbol_short!("rm_exec")), (token, now));
        Ok(())
    }

    pub fn cancel_rate_model_change(
        env: Env,
        admin: Address,
        token: Address,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        env.storage()
            .instance()
            .remove(&DataKey::PendingRateModel(token.clone()));
        env.events()
            .publish((EVT, symbol_short!("rm_cncl")), (admin, token));
        Ok(())
    }

    pub fn get_current_rate(env: Env, token: Address) -> Result<u32, PoolError> {
        let model: RateModelConfig = env
            .storage()
            .instance()
            .get(&DataKey::RateModel(token.clone()))
            .ok_or(PoolError::RateModelNotConfigured)?;
        let tt = Self::get_token_totals(env, token);
        Ok(compute_current_rate(utilization_bps(&tt), &model))
    }

    pub fn get_rate_model_config(env: Env, token: Address) -> Option<RateModelConfig> {
        env.storage().instance().get(&DataKey::RateModel(token))
    }

    pub fn get_rate_history(env: Env, token: Address, limit: u32) -> Vec<RateSnapshot> {
        let mut out = Vec::new(&env);
        let (len, start): (u32, u32) = env
            .storage()
            .instance()
            .get(&DataKey::RateHistoryBounds(token.clone()))
            .unwrap_or((0, 0));
        if len == 0 || limit == 0 {
            return out;
        }
        let take = len.min(limit);
        // Skip the oldest `len - take` samples so the most recent `take` remain.
        let skip = len - take;
        for offset in skip..len {
            let idx = (start + offset) % MAX_RATE_HISTORY;
            if let Some(snapshot) = env
                .storage()
                .persistent()
                .get::<DataKey, RateSnapshot>(&DataKey::RateRecord(token.clone(), idx))
            {
                out.push_back(snapshot);
            }
        }
        out
    }

    pub fn preview_rate_at_utilization(
        env: Env,
        token: Address,
        hypothetical_util_bps: u32,
    ) -> Result<u32, PoolError> {
        let model: RateModelConfig = env
            .storage()
            .instance()
            .get(&DataKey::RateModel(token))
            .ok_or(PoolError::RateModelNotConfigured)?;
        Ok(compute_current_rate(hypothetical_util_bps, &model))
    }

    pub fn set_factoring_fee(
        env: Env,
        admin: Address,
        factoring_fee_bps: u32,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        let mut config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(PoolError::NotInitialized)?;
        Self::require_admin(&env, &admin)?;
        if factoring_fee_bps > BPS_DENOM {
            return Err(PoolError::InvalidAmount);
        }
        config.factoring_fee_bps = factoring_fee_bps;
        env.storage().instance().set(&DataKey::Config, &config);
        Ok(())
    }

    pub fn set_fee_tier(
        env: Env,
        admin: Address,
        tier_id: u32,
        tier: FeeTier,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;
        if tier.min_amount < 0 || tier.max_amount < tier.min_amount || tier.fee_bps > BPS_DENOM {
            return Err(PoolError::InvalidFeeTier);
        }

        let mut tier_ids: Vec<u32> = env
            .storage()
            .instance()
            .get(&DataKey::FeeTierIds)
            .unwrap_or(Vec::new(&env));
        let mut found = false;
        for i in 0..tier_ids.len() {
            let existing_id = tier_ids.get(i).expect("storage corrupted");
            if existing_id == tier_id {
                found = true;
                break;
            }
        }
        if !found {
            tier_ids.push_back(tier_id);
            env.storage()
                .instance()
                .set(&DataKey::FeeTierIds, &tier_ids);
        }

        env.storage()
            .instance()
            .set(&DataKey::FeeTier(tier_id), &tier);
        Ok(())
    }

    pub fn remove_fee_tier(env: Env, admin: Address, tier_id: u32) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;

        let tier_ids: Vec<u32> = env
            .storage()
            .instance()
            .get(&DataKey::FeeTierIds)
            .unwrap_or(Vec::new(&env));
        let mut new_ids: Vec<u32> = Vec::new(&env);
        let mut removed = false;
        for i in 0..tier_ids.len() {
            let existing_id = tier_ids.get(i).expect("storage corrupted");
            if existing_id == tier_id {
                removed = true;
                continue;
            }
            new_ids.push_back(existing_id);
        }
        if !removed {
            return Err(PoolError::FeeTierNotFound);
        }
        env.storage().instance().set(&DataKey::FeeTierIds, &new_ids);
        env.storage().instance().remove(&DataKey::FeeTier(tier_id));
        Ok(())
    }

    pub fn get_fee_tier(env: Env, tier_id: u32) -> Option<FeeTier> {
        bump_instance(&env);
        env.storage().instance().get(&DataKey::FeeTier(tier_id))
    }

    pub fn list_fee_tiers(env: Env) -> Vec<(u32, FeeTier)> {
        bump_instance(&env);
        let mut result: Vec<(u32, FeeTier)> = Vec::new(&env);
        let tier_ids: Vec<u32> = env
            .storage()
            .instance()
            .get(&DataKey::FeeTierIds)
            .unwrap_or(Vec::new(&env));
        for i in 0..tier_ids.len() {
            let tier_id = tier_ids.get(i).expect("storage corrupted");
            if let Some(tier) = env.storage().instance().get(&DataKey::FeeTier(tier_id)) {
                result.push_back((tier_id, tier));
            }
        }
        result
    }

    pub fn set_credit_score_contract(
        env: Env,
        admin: Address,
        credit_score_contract: Address,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;
        env.storage()
            .instance()
            .set(&DataKey::CreditScoreContract, &credit_score_contract);
        Ok(())
    }

    pub fn get_credit_score_contract(env: Env) -> Option<Address> {
        bump_instance(&env);
        env.storage().instance().get(&DataKey::CreditScoreContract)
    }

    pub fn set_insurance_contract(
        env: Env,
        admin: Address,
        insurance_contract: Address,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;
        env.storage()
            .instance()
            .set(&INSURANCE_CFG, &insurance_contract);
        Ok(())
    }

    pub fn get_insurance_contract(env: Env) -> Option<Address> {
        bump_instance(&env);
        env.storage().instance().get(&INSURANCE_CFG)
    }

    pub fn receive_insurance_payout(
        env: Env,
        insurance: Address,
        token: Address,
        invoice_id: u64,
        amount: i128,
    ) -> Result<(), PoolError> {
        insurance.require_auth();
        bump_instance(&env);
        let configured: Address = env
            .storage()
            .instance()
            .get(&INSURANCE_CFG)
            .ok_or(PoolError::Unauthorized)?;
        if insurance != configured {
            return Err(PoolError::Unauthorized);
        }
        if amount <= 0 {
            return Err(PoolError::InvalidAmount);
        }
        let token_totals_key = DataKey::TokenTotals(token.clone());
        let mut tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&token_totals_key)
            .unwrap_or_default();
        tt.pool_value = tt
            .pool_value
            .checked_add(amount)
            .ok_or(PoolError::AmountOverflow)?;
        env.storage().instance().set(&token_totals_key, &tt);
        env.events()
            .publish((EVT, symbol_short!("ins_pay")), (invoice_id, token, amount));
        Ok(())
    }

    pub fn set_compound_interest(
        env: Env,
        admin: Address,
        compound: bool,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;
        let mut config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(PoolError::NotInitialized)?;
        config.compound_interest = compound;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events()
            .publish((EVT, symbol_short!("set_comp")), (admin, compound));
        Ok(())
    }

    // ---- #235: minimum deposit ----

    pub fn set_min_deposit(env: Env, admin: Address, min_amount: i128) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        if min_amount < 0 {
            return Err(PoolError::InvalidAmount);
        }
        let mut config = get_config_cached(&env)?;
        config.min_deposit_amount = min_amount;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events()
            .publish((EVT, symbol_short!("set_min_d")), (admin, min_amount));
        Ok(())
    }

    pub fn get_min_deposit(env: Env) -> i128 {
        env.storage()
            .instance()
            .get::<DataKey, PoolConfig>(&DataKey::Config)
            .map(|c| c.min_deposit_amount)
            .unwrap_or(0)
    }

    // ---- #233: maximum single-investor concentration limit ----

    pub fn set_max_investor_concentration(
        env: Env,
        admin: Address,
        max_bps: u32,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        if max_bps > BPS_DENOM {
            return Err(PoolError::InvalidAmount);
        }
        let mut config = get_config_cached(&env)?;
        config.max_single_investor_bps = max_bps;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events()
            .publish((EVT, symbol_short!("set_conc")), (admin, max_bps));
        Ok(())
    }

    pub fn get_investor_concentration(
        env: Env,
        investor: Address,
        token: Address,
    ) -> Result<u32, PoolError> {
        let tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&DataKey::TokenTotals(token.clone()))
            .unwrap_or_default();
        if tt.pool_value <= 0 {
            return Ok(0);
        }
        let pos_key = DataKey::InvestorPosition(investor.clone(), token);
        let position: InvestorPosition =
            env.storage().persistent().get(&pos_key).unwrap_or_default();
        let share_bps = ((position.deposited as u128 * 10_000u128) / tt.pool_value as u128) as u32;
        Ok(share_bps)
    }

    // ---- #236: protocol revenue & treasury ----

    pub fn set_treasury(env: Env, admin: Address, treasury: Address) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        env.storage().instance().set(&DataKey::Treasury, &treasury);
        env.events()
            .publish((EVT, symbol_short!("set_treas")), (admin, treasury));
        Ok(())
    }

    pub fn get_treasury(env: Env) -> Result<Address, PoolError> {
        env.storage()
            .instance()
            .get(&DataKey::Treasury)
            .ok_or(PoolError::TreasuryNotConfigured)
    }

    pub fn get_protocol_revenue(env: Env, token: Address) -> i128 {
        let tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&DataKey::TokenTotals(token))
            .unwrap_or_default();
        tt.protocol_revenue
    }

    pub fn withdraw_revenue(
        env: Env,
        admin: Address,
        token: Address,
        amount: i128,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;
        if amount <= 0 {
            return Err(PoolError::InvalidAmount);
        }
        non_reentrant!(&env, {
            let treasury: Address = env
                .storage()
                .instance()
                .get(&DataKey::Treasury)
                .ok_or(PoolError::TreasuryNotConfigured)?;
            let token_totals_key = DataKey::TokenTotals(token.clone());
            let mut tt: PoolTokenTotals = env
                .storage()
                .instance()
                .get(&token_totals_key)
                .unwrap_or_default();
            if amount > tt.protocol_revenue {
                return Err(PoolError::InsufficientRevenue);
            }
            tt.protocol_revenue -= amount;
            env.storage().instance().set(&token_totals_key, &tt);
            let token_client = token::Client::new(&env, &token);
            token_client.transfer(&env.current_contract_address(), &treasury, &amount);
            env.events()
                .publish((EVT, symbol_short!("rev_wdraw")), (token, amount, treasury));
            Ok(())
        })
    }

    // ---- #864: role-based multisig access control (additive) ----
    //
    // Every entrypoint below is a parallel authorization path alongside the
    // legacy single-admin one above — it does not replace or disable any
    // existing admin-gated function. A deployment that never calls
    // `set_access_control` behaves exactly as before.

    pub fn set_access_control(
        env: Env,
        admin: Address,
        access_control: Address,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        env.storage()
            .instance()
            .set(&ACCESS_CONTROL, &access_control);
        env.events()
            .publish((EVT, symbol_short!("set_ac")), (admin, access_control));
        Ok(())
    }

    // #1042: no set_access_control_via_ac (rotation) entrypoint here —
    // pool's wasm binary is already within ~300 bytes of Soroban's 200KB
    // per-contract deploy cap on main (see
    // contracts/.wasm-size-baseline.json), so there's no room for a new
    // public entrypoint until pool is split further (as secondary_market
    // and auction already were). The legacy-admin-gated bootstrap above
    // still works; only rotation through multisig is unavailable for now.

    pub fn get_access_control(env: Env) -> Option<Address> {
        env.storage().instance().get(&ACCESS_CONTROL)
    }

    // #1038: Bootstrap the governance contract address. Admin-gated one-time setup.
    pub fn set_governance_address(
        env: Env,
        admin: Address,
        governance: Address,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        env.storage()
            .instance()
            .set(&GOVERNANCE, &governance);
        env.events()
            .publish((EVT, symbol_short!("set_gov")), (admin, governance));
        Ok(())
    }

    // #1038: Get the configured governance contract address.
    pub fn get_governance_address(env: Env) -> Option<Address> {
        env.storage().instance().get(&GOVERNANCE)
    }

    pub fn set_paused_via_ac(
        env: Env,
        access_control: Address,
        paused: bool,
    ) -> Result<(), PoolError> {
        access_control.require_auth();
        Self::require_access_control(&env, &access_control)?;
        bump_instance(&env);
        env.storage().instance().set(&DataKey::Paused, &paused);
        env.events().publish(
            (EVT, symbol_short!("ac_pause")),
            (access_control, paused, env.ledger().timestamp()),
        );
        Ok(())
    }

    pub fn set_yield_via_ac(
        env: Env,
        access_control: Address,
        new_yield_bps: u32,
    ) -> Result<(), PoolError> {
        access_control.require_auth();
        Self::require_access_control(&env, &access_control)?;
        bump_instance(&env);
        Self::require_not_paused(&env);
        let mut config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(PoolError::NotInitialized)?;
        if new_yield_bps > 5_000 {
            return Err(PoolError::InvalidAmount);
        }
        let now = env.ledger().timestamp();
        if now
            < config
                .last_yield_change_at
                .saturating_add(config.yield_change_cooldown_secs)
        {
            return Err(PoolError::InvalidAmount);
        }
        let current = config.yield_bps;
        let delta = new_yield_bps.abs_diff(current);
        if delta > config.max_yield_change_bps {
            return Err(PoolError::InvalidAmount);
        }
        config.yield_bps = new_yield_bps;
        config.last_yield_change_at = now;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events().publish(
            (EVT, symbol_short!("ac_yield")),
            (access_control, current, new_yield_bps),
        );
        Ok(())
    }

    pub fn set_treasury_via_ac(
        env: Env,
        access_control: Address,
        treasury: Address,
    ) -> Result<(), PoolError> {
        access_control.require_auth();
        Self::require_access_control(&env, &access_control)?;
        bump_instance(&env);
        env.storage().instance().set(&DataKey::Treasury, &treasury);
        env.events()
            .publish((EVT, symbol_short!("ac_treas")), (access_control, treasury));
        Ok(())
    }

    pub fn withdraw_revenue_via_ac(
        env: Env,
        access_control: Address,
        token: Address,
        amount: i128,
    ) -> Result<(), PoolError> {
        access_control.require_auth();
        Self::require_access_control(&env, &access_control)?;
        bump_instance(&env);
        Self::require_not_paused(&env);
        if amount <= 0 {
            return Err(PoolError::InvalidAmount);
        }
        non_reentrant!(&env, {
            let treasury: Address = env
                .storage()
                .instance()
                .get(&DataKey::Treasury)
                .ok_or(PoolError::TreasuryNotConfigured)?;
            let token_totals_key = DataKey::TokenTotals(token.clone());
            let mut tt: PoolTokenTotals = env
                .storage()
                .instance()
                .get(&token_totals_key)
                .unwrap_or_default();
            if amount > tt.protocol_revenue {
                return Err(PoolError::InsufficientRevenue);
            }
            tt.protocol_revenue -= amount;
            env.storage().instance().set(&token_totals_key, &tt);
            let token_client = token::Client::new(&env, &token);
            token_client.transfer(&env.current_contract_address(), &treasury, &amount);
            env.events()
                .publish((EVT, symbol_short!("ac_rev")), (token, amount, treasury));
            Ok(())
        })
    }

    pub fn set_oracle_contract_via_ac(
        env: Env,
        access_control: Address,
        oracle: Address,
    ) -> Result<(), PoolError> {
        access_control.require_auth();
        Self::require_access_control(&env, &access_control)?;
        bump_instance(&env);
        Self::require_not_paused(&env);
        env.storage().instance().set(&REFLECTOR_ORACLE, &oracle);
        env.events()
            .publish((EVT, symbol_short!("ac_orcl")), (access_control, oracle));
        Ok(())
    }

    pub fn set_kyc_required_via_ac(
        env: Env,
        access_control: Address,
        required: bool,
    ) -> Result<(), PoolError> {
        access_control.require_auth();
        Self::require_access_control(&env, &access_control)?;
        bump_instance(&env);
        env.storage()
            .instance()
            .set(&DataKey::KycRequired, &required);
        env.events().publish(
            (EVT, symbol_short!("ac_kycreq")),
            (access_control, required),
        );
        Ok(())
    }

    pub fn set_investor_kyc_via_ac(
        env: Env,
        access_control: Address,
        investor: Address,
        approved: bool,
    ) -> Result<(), PoolError> {
        access_control.require_auth();
        Self::require_access_control(&env, &access_control)?;
        bump_instance(&env);

        let config = get_config_cached(&env)?;
        if investor == config.admin
            || investor == env.current_contract_address()
            || investor == config.invoice_contract
        {
            return Err(PoolError::Unauthorized);
        }

        let status = if approved {
            KycStatus::Approved
        } else {
            KycStatus::Rejected
        };
        env.storage()
            .persistent()
            .set(&DataKey::InvestorKyc(investor.clone()), &status);
        env.events().publish(
            (EVT, symbol_short!("ac_kycset")),
            (access_control, investor, approved),
        );
        Ok(())
    }

    pub fn set_max_utilization_via_ac(
        env: Env,
        access_control: Address,
        max_bps: u32,
    ) -> Result<(), PoolError> {
        access_control.require_auth();
        Self::require_access_control(&env, &access_control)?;
        bump_instance(&env);
        Self::require_not_paused(&env);
        if max_bps > 10_000 {
            return Err(PoolError::InvalidAmount);
        }
        let mut config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(PoolError::NotInitialized)?;
        config.max_utilization_bps = max_bps;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events()
            .publish((EVT, symbol_short!("ac_util")), max_bps);
        Ok(())
    }

    // ---- #1038: Governance-gated parameter changes ----

    // #1038: Set yield via governance proposal. Bypasses yield change cooldown
    // since governance already provides timelock protection.
    pub fn set_yield_via_governance(
        env: Env,
        governance: Address,
        new_yield_bps: u32,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        Self::require_not_paused(&env);
        let mut config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(PoolError::NotInitialized)?;
        if new_yield_bps > 5_000 {
            return Err(PoolError::InvalidAmount);
        }
        let current = config.yield_bps;
        config.yield_bps = new_yield_bps;
        config.last_yield_change_at = env.ledger().timestamp();
        env.storage().instance().set(&DataKey::Config, &config);
        env.events().publish(
            (EVT, symbol_short!("gov_yield")),
            (governance, current, new_yield_bps),
        );
        Ok(())
    }

    // #1038: Set treasury via governance proposal.
    pub fn set_treasury_via_governance(
        env: Env,
        governance: Address,
        treasury: Address,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        env.storage().instance().set(&DataKey::Treasury, &treasury);
        env.events()
            .publish((EVT, symbol_short!("gov_treas")), (governance, treasury));
        Ok(())
    }

    // #1038: Set max utilization via governance proposal.
    pub fn set_max_utilization_via_governance(
        env: Env,
        governance: Address,
        max_bps: u32,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        Self::require_not_paused(&env);
        if max_bps > 10_000 {
            return Err(PoolError::InvalidAmount);
        }
        let mut config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(PoolError::NotInitialized)?;
        config.max_utilization_bps = max_bps;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events()
            .publish((EVT, symbol_short!("gov_util")), (governance, max_bps));
        Ok(())
    }

    // #1038: Set oracle contract via governance proposal.
    pub fn set_oracle_contract_via_governance(
        env: Env,
        governance: Address,
        oracle: Address,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        Self::require_not_paused(&env);
        env.storage().instance().set(&REFLECTOR_ORACLE, &oracle);
        env.events()
            .publish((EVT, symbol_short!("gov_orcl")), (governance, oracle));
        Ok(())
    }

    // #1038: Set KYC required via governance proposal.
    pub fn set_kyc_required_via_governance(
        env: Env,
        governance: Address,
        required: bool,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        env.storage()
            .instance()
            .set(&DataKey::KycRequired, &required);
        env.events().publish(
            (EVT, symbol_short!("gov_kycreq")),
            (governance, required),
        );
        Ok(())
    }

    // #1038: Set compliance registry via governance proposal.
    pub fn set_compliance_registry_via_governance(
        env: Env,
        governance: Address,
        registry: Address,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        let mut gate: ComplianceGateConfig = env
            .storage()
            .instance()
            .get(&COMPLIANCE_CFG)
            .unwrap_or(ComplianceGateConfig {
                registry: registry.clone(),
                required: false,
            });
        gate.registry = registry;
        env.storage().instance().set(&COMPLIANCE_CFG, &gate);
        env.events().publish(
            (EVT, symbol_short!("gov_comp")),
            (governance, registry),
        );
        Ok(())
    }

    // #1038: Set require compliance check via governance proposal.
    pub fn set_require_compliance_check_via_governance(
        env: Env,
        governance: Address,
        required: bool,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        let mut gate: ComplianceGateConfig = env
            .storage()
            .instance()
            .get(&COMPLIANCE_CFG)
            .unwrap_or(ComplianceGateConfig {
                registry: Address::generate(&env),
                required: false,
            });
        gate.required = required;
        env.storage().instance().set(&COMPLIANCE_CFG, &gate);
        env.events().publish(
            (EVT, symbol_short!("gov_comp_req")),
            (governance, required),
        );
        Ok(())
    }

    // #1038: Set referral registry via governance proposal.
    pub fn set_referral_registry_via_governance(
        env: Env,
        governance: Address,
        registry: Address,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        env.storage().instance().set(&REFERRAL_CFG, &registry);
        env.events().publish(
            (EVT, symbol_short!("gov_ref")),
            (governance, registry),
        );
        Ok(())
    }

    // #1038: Set credit score contract via governance proposal.
    pub fn set_credit_score_contract_via_governance(
        env: Env,
        governance: Address,
        credit_score_contract: Address,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        env.storage()
            .instance()
            .set(&DataKey::CreditScoreContract, &credit_score_contract);
        env.events().publish(
            (EVT, symbol_short!("gov_cs")),
            (governance, credit_score_contract),
        );
        Ok(())
    }

    // #1038: Set insurance contract via governance proposal.
    pub fn set_insurance_contract_via_governance(
        env: Env,
        governance: Address,
        insurance_contract: Address,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        env.storage()
            .instance()
            .set(&INSURANCE_CFG, &insurance_contract);
        env.events().publish(
            (EVT, symbol_short!("gov_ins")),
            (governance, insurance_contract),
        );
        Ok(())
    }

    // #1038: Set compound interest via governance proposal.
    pub fn set_compound_interest_via_governance(
        env: Env,
        governance: Address,
        compound: bool,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        let mut config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(PoolError::NotInitialized)?;
        config.compound_interest = compound;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events().publish(
            (EVT, symbol_short!("gov_comp_int")),
            (governance, compound),
        );
        Ok(())
    }

    // #1038: Set secondary market contract via governance proposal.
    pub fn set_secondary_market_contract_via_governance(
        env: Env,
        governance: Address,
        secondary_market_contract: Address,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        env.storage()
            .instance()
            .set(&SECONDARY_MARKET, &secondary_market_contract);
        env.events().publish(
            (EVT, symbol_short!("gov_sm")),
            (governance, secondary_market_contract),
        );
        Ok(())
    }

    // #1038: Set risk contract via governance proposal.
    pub fn set_risk_contract_via_governance(
        env: Env,
        governance: Address,
        risk_contract: Address,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        env.storage()
            .instance()
            .set(&RISK_CONTRACT, &risk_contract);
        env.events().publish(
            (EVT, symbol_short!("gov_risk")),
            (governance, risk_contract),
        );
        Ok(())
    }

    // #1038: Set min deposit via governance proposal.
    pub fn set_min_deposit_via_governance(
        env: Env,
        governance: Address,
        min_amount: i128,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        Self::require_admin(&env, &governance)?;
        if min_amount <= 0 {
            return Err(PoolError::InvalidAmount);
        }
        env.storage()
            .instance()
            .set(&DataKey::MinDeposit, &min_amount);
        env.events()
            .publish((EVT, symbol_short!("gov_min_dep")), (governance, min_amount));
        Ok(())
    }

    // #1038: Set max investor concentration via governance proposal.
    pub fn set_max_investor_concentration_via_governance(
        env: Env,
        governance: Address,
        max_bps: u32,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        if max_bps > 10_000 {
            return Err(PoolError::InvalidAmount);
        }
        env.storage()
            .instance()
            .set(&DataKey::MaxInvestorConcentration, &max_bps);
        env.events().publish(
            (EVT, symbol_short!("gov_max_conc")),
            (governance, max_bps),
        );
        Ok(())
    }

    // #1038: Set upgrade timelock via governance proposal.
    pub fn set_upgrade_timelock_via_governance(
        env: Env,
        governance: Address,
        secs: u64,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        Self::require_admin(&env, &governance)?;
        if secs < 86_400 {
            return Err(PoolError::InvalidUpgradeTimelock);
        }
        env.storage()
            .instance()
            .set(&DataKey::UpgradeTimelock, &secs);
        env.events()
            .publish((EVT, symbol_short!("gov_up_tl")), (governance, secs));
        Ok(())
    }

    // #1038: Set operation delay via governance proposal.
    pub fn set_operation_delay_via_governance(
        env: Env,
        governance: Address,
        secs: u64,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        Self::require_admin(&env, &governance)?;
        if secs < 3_600 {
            return Err(PoolError::InvalidOperationDelay);
        }
        env.storage()
            .instance()
            .set(&DataKey::OperationDelay, &secs);
        env.events()
            .publish((EVT, symbol_short!("gov_op_del")), (governance, secs));
        Ok(())
    }

    // #1038: Set yield change policy via governance proposal.
    pub fn set_yield_change_policy_via_governance(
        env: Env,
        governance: Address,
        cooldown_secs: u64,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        let mut config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(PoolError::NotInitialized)?;
        config.yield_change_cooldown_secs = cooldown_secs;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events().publish(
            (EVT, symbol_short!("gov_yield_pol")),
            (governance, cooldown_secs),
        );
        Ok(())
    }

    // #1038: Set factoring fee via governance proposal.
    pub fn set_factoring_fee_via_governance(
        env: Env,
        governance: Address,
        factoring_fee_bps: u32,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        if factoring_fee_bps > 5_000 {
            return Err(PoolError::InvalidAmount);
        }
        env.storage()
            .instance()
            .set(&DataKey::FactoringFeeBps, &factoring_fee_bps);
        env.events().publish(
            (EVT, symbol_short!("gov_fact")),
            (governance, factoring_fee_bps),
        );
        Ok(())
    }

    // #1038: Set withdrawal limits via governance proposal.
    pub fn set_withdrawal_limits_via_governance(
        env: Env,
        governance: Address,
        max_bps: u32,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        if max_bps > 10_000 {
            return Err(PoolError::InvalidAmount);
        }
        env.storage()
            .instance()
            .set(&DataKey::WithdrawalLimitBps, &max_bps);
        env.events().publish(
            (EVT, symbol_short!("gov_wd_lim")),
            (governance, max_bps),
        );
        Ok(())
    }

    // #1038: Set max withdrawal queue age via governance proposal.
    pub fn set_max_withdrawal_queue_age_via_governance(
        env: Env,
        governance: Address,
        days: u32,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        env.storage()
            .instance()
            .set(&DataKey::MaxWithdrawalQueueAge, &days);
        env.events().publish(
            (EVT, symbol_short!("gov_wd_age")),
            (governance, days),
        );
        Ok(())
    }

    // #1038: Set max withdrawal queue depth via governance proposal.
    pub fn set_max_withdrawal_queue_depth_via_governance(
        env: Env,
        governance: Address,
        depth: u32,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        env.storage()
            .instance()
            .set(&DataKey::MaxWithdrawalQueueDepth, &depth);
        env.events().publish(
            (EVT, symbol_short!("gov_wd_dep")),
            (governance, depth),
        );
        Ok(())
    }

    // #1038: Set oracle stale threshold via governance proposal.
    pub fn set_oracle_stale_threshold_via_governance(
        env: Env,
        governance: Address,
        threshold_secs: u64,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        Self::require_not_paused(&env);
        if threshold_secs == 0 || threshold_secs > MAX_ORACLE_STALE_SECS {
            return Err(PoolError::InvalidAmount);
        }
        let current = env
            .storage()
            .instance()
            .get(&ORACLE_STALE_SECS)
            .unwrap_or(DEFAULT_ORACLE_STALE_SECS);
        env.storage()
            .instance()
            .set(&ORACLE_STALE_SECS, &threshold_secs);
        env.events().publish(
            (EVT, symbol_short!("gov_orc_stale")),
            (governance, current, threshold_secs),
        );
        Ok(())
    }

    // #1038: Set fee tier via governance proposal.
    pub fn set_fee_tier_via_governance(
        env: Env,
        governance: Address,
        tier_id: u32,
        tier: FeeTier,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        Self::require_not_paused(&env);
        if tier.min_amount < 0 || tier.max_amount < tier.min_amount || tier.fee_bps > BPS_DENOM {
            return Err(PoolError::InvalidFeeTier);
        }

        let mut tier_ids: Vec<u32> = env
            .storage()
            .instance()
            .get(&DataKey::FeeTierIds)
            .unwrap_or(Vec::new(&env));
        let mut found = false;
        for i in 0..tier_ids.len() {
            let existing_id = tier_ids.get(i).expect("storage corrupted");
            if existing_id == tier_id {
                found = true;
                break;
            }
        }
        if !found {
            tier_ids.push_back(tier_id);
            env.storage()
                .instance()
                .set(&DataKey::FeeTierIds, &tier_ids);
        }

        env.storage()
            .instance()
            .set(&DataKey::FeeTier(tier_id), &tier);
        env.events()
            .publish((EVT, symbol_short!("gov_fee")), (governance, tier_id));
        Ok(())
    }

    // #1038: Set loyalty tiers via governance proposal.
    pub fn set_loyalty_tiers_via_governance(
        env: Env,
        governance: Address,
        tiers: Vec<LoyaltyTier>,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        if tiers.is_empty() {
            return Err(PoolError::InvalidLoyaltyTiers);
        }
        let mut prev_min_days: Option<u32> = None;
        for i in 0..tiers.len() {
            let tier = tiers.get(i).unwrap();
            if tier.bonus_bps > MAX_LOYALTY_BONUS_BPS {
                return Err(PoolError::InvalidLoyaltyTiers);
            }
            if let Some(prev) = prev_min_days {
                if tier.min_days <= prev {
                    return Err(PoolError::InvalidLoyaltyTiers);
                }
            }
            prev_min_days = Some(tier.min_days);
        }
        env.storage().instance().set(&LOYALTY_TIERS, &tiers);
        env.events()
            .publish((EVT, symbol_short!("gov_loy")), (governance, tiers.len()));
        Ok(())
    }

    // #1038: Set fallback price via governance proposal.
    pub fn set_fallback_price_via_governance(
        env: Env,
        governance: Address,
        token: Address,
        price: i128,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        Self::require_not_paused(&env);
        if price <= 0 {
            return Err(PoolError::InvalidAmount);
        }
        env.storage()
            .instance()
            .set(&DataKey::FallbackPrice(token), &price);
        env.events()
            .publish((EVT, symbol_short!("gov_fallback")), (governance, token, price));
        Ok(())
    }

    // #1038: Set rate bounds via governance proposal.
    pub fn set_rate_bounds_via_governance(
        env: Env,
        governance: Address,
        token: Address,
        min_rate: i128,
        max_rate: i128,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        Self::require_not_paused(&env);
        if min_rate < 0 || max_rate <= min_rate {
            return Err(PoolError::InvalidAmount);
        }
        env.storage()
            .instance()
            .set(&DataKey::RateBounds(token), &(min_rate, max_rate));
        env.events()
            .publish((EVT, symbol_short!("gov_rate")), (governance, token, min_rate, max_rate));
        Ok(())
    }

    // #1038: Set exchange rate via governance proposal.
    pub fn set_exchange_rate_via_governance(
        env: Env,
        governance: Address,
        token: Address,
        rate: i128,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        Self::require_not_paused(&env);
        if rate <= 0 {
            return Err(PoolError::InvalidAmount);
        }
        env.storage()
            .instance()
            .set(&DataKey::ExchangeRate(token), &rate);
        env.events()
            .publish((EVT, symbol_short!("gov_exch")), (governance, token, rate));
        Ok(())
    }

    // #1038: Set collateral config via governance proposal.
    pub fn set_collateral_config_via_governance(
        env: Env,
        governance: Address,
        config: CollateralConfig,
    ) -> Result<(), PoolError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        bump_instance(&env);
        Self::require_not_paused(&env);
        if config.threshold <= 0 || config.collateral_bps > 10_000 {
            return Err(PoolError::InvalidAmount);
        }
        env.storage()
            .instance()
            .set(&DataKey::CollateralConfig, &config);
        env.events()
            .publish((EVT, symbol_short!("gov_coll")), (governance, config.threshold, config.collateral_bps));
        Ok(())
    }

    // ---- #773: loyalty bonus APY for long-term depositors ----

    pub fn set_loyalty_tiers(
        env: Env,
        admin: Address,
        tiers: Vec<LoyaltyTier>,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        if tiers.is_empty() {
            return Err(PoolError::InvalidLoyaltyTiers);
        }
        let mut prev_min_days: Option<u32> = None;
        for i in 0..tiers.len() {
            let tier = tiers.get(i).unwrap();
            if tier.bonus_bps > MAX_LOYALTY_BONUS_BPS {
                return Err(PoolError::InvalidLoyaltyTiers);
            }
            if let Some(prev) = prev_min_days {
                if tier.min_days <= prev {
                    return Err(PoolError::InvalidLoyaltyTiers);
                }
            }
            prev_min_days = Some(tier.min_days);
        }
        env.storage().instance().set(&LOYALTY_TIERS, &tiers);
        env.events().publish((EVT, symbol_short!("loy_set")), admin);
        Ok(())
    }

    pub fn get_loyalty_tiers(env: Env) -> Vec<LoyaltyTier> {
        get_loyalty_tiers_cached(&env)
    }

    pub fn get_deposit_info(
        env: Env,
        investor: Address,
        token: Address,
    ) -> Result<DepositInfo, PoolError> {
        bump_instance(&env);
        let position: InvestorPosition = env
            .storage()
            .persistent()
            .get(&DataKey::InvestorPosition(investor, token))
            .unwrap_or_default();
        let tiers = get_loyalty_tiers_cached(&env);
        let days_active = loyalty_days_active(&env, position.loyalty_start_at);
        let (tier, bonus_bps, next_tier_days) = resolve_loyalty_tier(&tiers, days_active);
        let config = get_config_cached(&env)?;
        Ok(DepositInfo {
            deposited_at: position.loyalty_start_at,
            days_active,
            tier,
            bonus_bps,
            base_apy_bps: config.yield_bps,
            effective_apy_bps: config.yield_bps.saturating_add(bonus_bps),
            next_tier_days,
        })
    }

    // ---- #244: withdrawal rate limiting ----

    pub fn set_withdrawal_limits(
        env: Env,
        admin: Address,
        max_bps: u32,
        cooldown_secs: u64,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        if max_bps > BPS_DENOM {
            return Err(PoolError::InvalidAmount);
        }
        let mut config = get_config_cached(&env)?;
        config.max_single_withdrawal_bps = max_bps;
        config.withdrawal_cooldown_secs = cooldown_secs;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events().publish(
            (EVT, symbol_short!("set_wdlim")),
            (admin, max_bps, cooldown_secs),
        );
        Ok(())
    }

    pub fn set_max_withdrawal_queue_age(
        env: Env,
        admin: Address,
        days: u32,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        let mut config = get_config_cached(&env)?;
        config.max_withdrawal_queue_age_days = days;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events()
            .publish((EVT, symbol_short!("set_wdage")), (admin, days));
        Ok(())
    }

    pub fn set_max_withdrawal_queue_depth(
        env: Env,
        admin: Address,
        depth: u32,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        let mut config = get_config_cached(&env)?;
        config.max_withdrawal_queue_depth = depth;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events()
            .publish((EVT, symbol_short!("set_wdcap")), (admin, depth));
        Ok(())
    }

    pub fn drain_withdrawal_queue(
        env: Env,
        caller: Address,
        token: Address,
    ) -> Result<(), PoolError> {
        caller.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::assert_accepted_token(&env, &token)?;

        non_reentrant!(&env, {
            let tt: PoolTokenTotals = env
                .storage()
                .instance()
                .get(&DataKey::TokenTotals(token.clone()))
                .unwrap_or_default();
            let liquid = available_liquidity(&tt)?;
            Self::process_withdrawal_queue(&env, token, liquid)
        })
    }

    // ---- #860: multi-investor co-funding rounds ----

    pub fn open_co_funding(
        env: Env,
        admin: Address,
        request: OpenCoFundingRequest,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;
        Self::assert_accepted_token(&env, &request.token)?;

        if request.target_principal <= 0 {
            return Err(PoolError::InvalidAmount);
        }
        if request.min_commitment < 0 || request.min_commitment > request.target_principal {
            return Err(PoolError::InvalidCoFundingParams);
        }
        if request.max_investor_bps > BPS_DENOM {
            return Err(PoolError::InvalidCoFundingParams);
        }
        let now = env.ledger().timestamp();
        if request.funding_deadline <= now {
            return Err(PoolError::InvalidCoFundingParams);
        }
        if request.due_date <= now {
            return Err(PoolError::InvoiceExpired);
        }

        let round_key = DataKey::CoFundingRound(request.invoice_id);
        if env.storage().persistent().has(&round_key) {
            return Err(PoolError::CoFundingRoundAlreadyExists);
        }
        if env
            .storage()
            .persistent()
            .has(&DataKey::FundedInvoice(request.invoice_id))
        {
            return Err(PoolError::StorageCorrupted);
        }

        let round = CoFundingRound {
            invoice_id: request.invoice_id,
            token: request.token.clone(),
            sme: request.sme,
            due_date: request.due_date,
            target_principal: request.target_principal,
            committed_principal: 0,
            funding_deadline: request.funding_deadline,
            status: CoFundingStatus::Open,
            min_commitment: request.min_commitment,
            max_investor_bps: request.max_investor_bps,
            participants: Vec::new(&env),
        };
        env.storage().persistent().set(&round_key, &round);
        env.storage().persistent().extend_ttl(
            &round_key,
            INSTANCE_LIFETIME_THRESHOLD,
            ACTIVE_INVOICE_TTL,
        );

        let mut ids: Vec<u64> = env
            .storage()
            .instance()
            .get(&DataKey::CoFundingRoundIds)
            .unwrap_or(Vec::new(&env));
        ids.push_back(request.invoice_id);
        env.storage()
            .instance()
            .set(&DataKey::CoFundingRoundIds, &ids);

        env.events().publish(
            (EVT, symbol_short!("cf_open")),
            (
                request.invoice_id,
                request.token,
                request.target_principal,
                request.funding_deadline,
            ),
        );
        Ok(())
    }

    pub fn commit_to_invoice(
        env: Env,
        investor: Address,
        invoice_id: u64,
        amount: i128,
    ) -> Result<(), PoolError> {
        investor.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);

        if amount <= 0 {
            return Err(PoolError::InvalidAmount);
        }

        non_reentrant!(&env, {
            let round_key = DataKey::CoFundingRound(invoice_id);
            let mut round: CoFundingRound = env
                .storage()
                .persistent()
                .get(&round_key)
                .ok_or(PoolError::CoFundingRoundNotFound)?;

            if round.status != CoFundingStatus::Open {
                return Err(PoolError::CoFundingRoundNotOpen);
            }
            let now = env.ledger().timestamp();
            if now >= round.funding_deadline {
                return Err(PoolError::CoFundingDeadlinePassed);
            }
            let remaining = round
                .target_principal
                .checked_sub(round.committed_principal)
                .ok_or(PoolError::AmountOverflow)?;
            if remaining <= 0 {
                return Err(PoolError::CoFundingRoundNotOpen);
            }
            let commit_amount = if amount > remaining {
                remaining
            } else {
                amount
            };

            let token = round.token.clone();
            let share_token_key = DataKey::ShareToken(token.clone());
            let token_totals_key = DataKey::TokenTotals(token.clone());
            let share_token: Address = env
                .storage()
                .instance()
                .get(&share_token_key)
                .ok_or(PoolError::ShareTokenNotConfigured)?;
            let mut tt: PoolTokenTotals = env
                .storage()
                .instance()
                .get(&token_totals_key)
                .unwrap_or_default();

            let rate_bps: u32 = env
                .storage()
                .instance()
                .get(&DataKey::ExchangeRate(token.clone()))
                .unwrap_or(10_000u32);
            let usdc_equiv = commit_amount
                .checked_mul(rate_bps as i128)
                .ok_or(PoolError::AmountOverflow)?
                .checked_div(10_000i128)
                .ok_or(PoolError::AmountOverflow)?;

            let total_shares: i128 = env.invoke_contract(
                &share_token,
                &Symbol::new(&env, "total_supply"),
                Vec::new(&env),
            );
            if total_shares == 0 || tt.pool_value == 0 {
                return Err(PoolError::InsufficientLiquidity);
            }
            let shares_to_burn = usdc_equiv
                .checked_mul(total_shares)
                .ok_or(PoolError::AmountOverflow)?
                .checked_div(tt.pool_value)
                .ok_or(PoolError::AmountOverflow)?;
            if shares_to_burn <= 0 {
                return Err(PoolError::InvalidAmount);
            }

            let investor_pos_key = DataKey::InvestorPosition(investor.clone(), token.clone());
            let mut position: InvestorPosition = env
                .storage()
                .persistent()
                .get(&investor_pos_key)
                .unwrap_or_default();
            if position.available < shares_to_burn {
                return Err(PoolError::InvalidAmount);
            }

            let cofund_key = DataKey::CoFundShare(invoice_id, investor.clone());
            let existing_bps: u32 = env.storage().persistent().get(&cofund_key).unwrap_or(0);
            // #860: track cumulative committed amount exactly (not derived
            // from bps, which truncates to 1/10_000 of target_principal) so
            // a later refund can return 100% of what was actually put in.
            let committed_key = DataKey::CoFundCommitted(invoice_id, investor.clone());
            let existing_committed: i128 =
                env.storage().persistent().get(&committed_key).unwrap_or(0);
            let new_investor_committed = existing_committed
                .checked_add(commit_amount)
                .ok_or(PoolError::AmountOverflow)?;
            if round.max_investor_bps > 0 {
                let cap_amount = round
                    .target_principal
                    .checked_mul(round.max_investor_bps as i128)
                    .ok_or(PoolError::AmountOverflow)?
                    .checked_div(BPS_DENOM as i128)
                    .ok_or(PoolError::AmountOverflow)?;
                if new_investor_committed > cap_amount {
                    return Err(PoolError::CoFundingInvestorCapExceeded);
                }
            }

            // Burn shares — the token amount stays in the contract's own
            // balance (never transferred), earmarked for this round.
            let mut burn_args = Vec::new(&env);
            burn_args.push_back(investor.clone().into_val(&env));
            burn_args.push_back(shares_to_burn.into_val(&env));
            let _: () = env.invoke_contract(&share_token, &Symbol::new(&env, "burn"), burn_args);

            position.available = position
                .available
                .checked_sub(shares_to_burn)
                .ok_or(PoolError::AmountOverflow)?;
            env.storage().persistent().set(&investor_pos_key, &position);

            tt.pool_value = tt
                .pool_value
                .checked_sub(usdc_equiv)
                .ok_or(PoolError::AmountOverflow)?;
            env.storage().instance().set(&token_totals_key, &tt);

            // Recompute bps from scratch off the cumulative committed amount
            // (rather than incrementally adding bps) to avoid rounding drift
            // across multiple partial commits from the same investor.
            let new_bps = (new_investor_committed as u64 * BPS_DENOM as u64
                / round.target_principal as u64) as u32;
            if existing_bps == 0 {
                if round.participants.len() >= MAX_CO_FUNDING_PARTICIPANTS {
                    return Err(PoolError::CoFundingTooManyParticipants);
                }
                round.participants.push_back(investor.clone());
            }
            env.storage().persistent().set(&cofund_key, &new_bps);
            env.storage()
                .persistent()
                .set(&committed_key, &new_investor_committed);

            round.committed_principal = round
                .committed_principal
                .checked_add(commit_amount)
                .ok_or(PoolError::AmountOverflow)?;
            let committed_principal = round.committed_principal;
            env.storage().persistent().set(&round_key, &round);

            env.events().publish(
                (EVT, symbol_short!("cf_commit")),
                (invoice_id, investor, commit_amount, committed_principal),
            );
            Ok(())
        })
    }

    pub fn withdraw_co_funding_commitment(
        env: Env,
        investor: Address,
        invoice_id: u64,
    ) -> Result<(), PoolError> {
        investor.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);

        non_reentrant!(&env, {
            let round_key = DataKey::CoFundingRound(invoice_id);
            let mut round: CoFundingRound = env
                .storage()
                .persistent()
                .get(&round_key)
                .ok_or(PoolError::CoFundingRoundNotFound)?;
            if round.status != CoFundingStatus::Open {
                return Err(PoolError::CoFundingRoundNotOpen);
            }

            let token_totals_key = DataKey::TokenTotals(round.token.clone());
            let mut tt: PoolTokenTotals = env
                .storage()
                .instance()
                .get(&token_totals_key)
                .unwrap_or_default();
            let refunded = refund_co_funding_investor(&env, &round, &investor, &mut tt)?;
            if refunded == 0 {
                return Err(PoolError::CoFundingNoCommitment);
            }
            env.storage().instance().set(&token_totals_key, &tt);

            round.committed_principal = round.committed_principal.saturating_sub(refunded);
            remove_participant(&env, &mut round, &investor);
            env.storage().persistent().set(&round_key, &round);

            env.events().publish(
                (EVT, symbol_short!("cf_wthdw")),
                (invoice_id, investor, refunded),
            );
            Ok(())
        })
    }

    pub fn cancel_co_funding_round(
        env: Env,
        admin: Address,
        invoice_id: u64,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;

        let round_key = DataKey::CoFundingRound(invoice_id);
        let mut round: CoFundingRound = env
            .storage()
            .persistent()
            .get(&round_key)
            .ok_or(PoolError::CoFundingRoundNotFound)?;
        if round.status != CoFundingStatus::Open {
            return Err(PoolError::CoFundingRoundNotOpen);
        }
        if round.committed_principal != 0 {
            return Err(PoolError::InvalidCoFundingParams);
        }
        round.status = CoFundingStatus::Cancelled;
        env.storage().persistent().set(&round_key, &round);
        env.events()
            .publish((EVT, symbol_short!("cf_cncl")), invoice_id);
        Ok(())
    }

    pub fn cancel_funded_invoice(
        env: Env,
        admin: Address,
        invoice_id: u64,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;

        let config: PoolConfig = get_config_cached(&env)?;

        // Load the funded invoice record
        let funded_key = DataKey::FundedInvoice(invoice_id);
        let record: FundedInvoice = env
            .storage()
            .persistent()
            .get(&funded_key)
            .ok_or(PoolError::InvoiceNotFound)?;

        let principal = record.principal;
        let token = record.token.clone();
        let is_co_funded = record.co_funding_round_id.is_some();

        // Notify the invoice contract: transition Funded → Cancelled
        let invoice_client = InvoiceContractClient::new(&env, &config.invoice_contract);
        match invoice_client.try_mark_cancelled(&invoice_id, &env.current_contract_address()) {
            Ok(Ok(())) => {}
            _ => return Err(PoolError::InvoiceNotCancelled),
        }

        // Decrement deployed capital (co-funded invoices bypass total_deployed)
        if !is_co_funded {
            let token_totals_key = DataKey::TokenTotals(token.clone());
            let mut tt: PoolTokenTotals = env
                .storage()
                .instance()
                .get(&token_totals_key)
                .unwrap_or_default();
            tt.total_deployed = tt
                .total_deployed
                .checked_sub(principal)
                .ok_or(PoolError::AmountOverflow)?;
            env.storage().instance().set(&token_totals_key, &tt);

            // Record rate snapshot since utilization changed
            record_rate_snapshot(&env, &token);
        }

        // Remove the FundedInvoice record
        env.storage().persistent().remove(&funded_key);

        // Update stats
        let mut stats: PoolStorageStats = env
            .storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default();
        stats.active_funded_invoices = stats.active_funded_invoices.saturating_sub(1);
        env.storage().instance().set(&DataKey::StorageStats, &stats);

        // Emit cancellation event
        env.events().publish(
            (EVT, symbol_short!("inv_cncl")),
            (
                invoice_id,
                admin,
                principal,
                token,
                env.ledger().timestamp(),
            ),
        );

        Ok(())
    }

    pub fn finalize_co_funding(
        env: Env,
        caller: Address,
        invoice_id: u64,
    ) -> Result<(), PoolError> {
        caller.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);

        non_reentrant!(&env, {
            let round_key = DataKey::CoFundingRound(invoice_id);
            let mut round: CoFundingRound = env
                .storage()
                .persistent()
                .get(&round_key)
                .ok_or(PoolError::CoFundingRoundNotFound)?;
            if round.status != CoFundingStatus::Open {
                return Err(PoolError::CoFundingRoundAlreadyFinalized);
            }

            let now = env.ledger().timestamp();
            let deadline_passed = now >= round.funding_deadline;
            let target_met = round.committed_principal >= round.target_principal;

            if !target_met && !deadline_passed {
                return Err(PoolError::CoFundingRoundNotOpen);
            }

            // NOTE: both arms below end with `Ok(())`/`Err(...)` as a tail
            // expression rather than an explicit `return` — an explicit
            // `return` inside a `non_reentrant!` block exits the whole
            // function directly, skipping the macro's `non_reentrant_end`
            // cleanup and leaving the guard permanently stuck (a real bug
            // caught by this crate's own co-funding integration test: a
            // stuck guard makes every subsequent call, including unrelated
            // ones like `withdraw`, panic with "reentrant call").
            if !target_met && round.committed_principal < round.min_commitment {
                // Deadline passed, under minimum — refund everyone, no SME payout.
                let token_totals_key = DataKey::TokenTotals(round.token.clone());
                let mut tt: PoolTokenTotals = env
                    .storage()
                    .instance()
                    .get(&token_totals_key)
                    .unwrap_or_default();
                let participants = round.participants.clone();
                let mut total_refunded: i128 = 0;
                for i in 0..participants.len() {
                    let investor = participants.get(i).ok_or(PoolError::StorageCorrupted)?;
                    total_refunded += refund_co_funding_investor(&env, &round, &investor, &mut tt)?;
                }
                env.storage().instance().set(&token_totals_key, &tt);
                round.committed_principal = 0;
                round.participants = Vec::new(&env);
                round.status = CoFundingStatus::Expired;
                env.storage().persistent().set(&round_key, &round);

                env.events()
                    .publish((EVT, symbol_short!("cf_exp")), (invoice_id, total_refunded));
                Ok(())
            } else {
                // Success: fund the SME. Deliberately does NOT touch
                // tt.total_deployed/pool_value the way fund_invoice_request
                // does for admin-driven lump-sum funding — this capital
                // already left pool_value (and was never counted in
                // total_deployed) when each investor's commitment burned
                // their shares in commit_to_invoice.
                if env
                    .storage()
                    .persistent()
                    .has(&DataKey::FundedInvoice(invoice_id))
                {
                    return Err(PoolError::StorageCorrupted);
                }

                let mut stats: PoolStorageStats = env
                    .storage()
                    .instance()
                    .get(&DataKey::StorageStats)
                    .unwrap_or_default();
                // #863: co-funded invoices lock the live rate at finalization
                // too, so their repayment interest is computed from the same
                // single source of truth as pool-funded invoices.
                let config = get_config_cached(&env)?;
                let funded = FundedInvoice {
                    invoice_id,
                    sme: round.sme.clone(),
                    token: round.token.clone(),
                    principal: round.committed_principal,
                    funded_at: now,
                    factoring_fee: 0,
                    due_date: round.due_date,
                    repaid_amount: 0,
                    co_funding_round_id: Some(invoice_id),
                    locked_yield_bps: current_rate_for_token(&env, &config, &round.token),
                };
                env.storage()
                    .persistent()
                    .set(&DataKey::FundedInvoice(invoice_id), &funded);
                set_funded_invoice_ttl(&env, invoice_id, false);

                stats.total_funded_invoices = stats
                    .total_funded_invoices
                    .checked_add(1)
                    .ok_or(PoolError::AmountOverflow)?;
                stats.active_funded_invoices = stats
                    .active_funded_invoices
                    .checked_add(1)
                    .ok_or(PoolError::AmountOverflow)?;
                env.storage().instance().set(&DataKey::StorageStats, &stats);

                round.status = CoFundingStatus::Filled;
                env.storage().persistent().set(&round_key, &round);

                let token_client = token::Client::new(&env, &round.token);
                token_client.transfer(
                    &env.current_contract_address(),
                    &round.sme,
                    &round.committed_principal,
                );

                env.events().publish(
                    (EVT, symbol_short!("funded")),
                    (
                        invoice_id,
                        round.sme.clone(),
                        round.committed_principal,
                        round.token.clone(),
                        now,
                    ),
                );
                env.events().publish(
                    (EVT, symbol_short!("cf_fin")),
                    (invoice_id, round.committed_principal),
                );

                if let Some(cs_contract) = get_credit_score_contract(&env) {
                    let cs_client = CreditScoreClient::new(&env, &cs_contract);
                    let _ = cs_client.try_record_funding(
                        &env.current_contract_address(),
                        &invoice_id,
                        &round.sme,
                        &round.committed_principal,
                    );
                }

                Ok(())
            }
        })
    }

    pub fn list_co_funding_rounds(env: Env) -> Vec<u64> {
        env.storage()
            .instance()
            .get(&DataKey::CoFundingRoundIds)
            .unwrap_or(Vec::new(&env))
    }

    pub fn get_co_funding_round(env: Env, invoice_id: u64) -> Option<CoFundingRound> {
        env.storage()
            .persistent()
            .get(&DataKey::CoFundingRound(invoice_id))
    }

    pub fn get_investor_co_fund_positions(env: Env, investor: Address) -> Vec<(u64, u32)> {
        let ids: Vec<u64> = env
            .storage()
            .instance()
            .get(&DataKey::CoFundingRoundIds)
            .unwrap_or(Vec::new(&env));
        let mut out = Vec::new(&env);
        for i in 0..ids.len() {
            let Some(invoice_id) = ids.get(i) else {
                continue;
            };
            let bps: u32 = env
                .storage()
                .persistent()
                .get(&DataKey::CoFundShare(invoice_id, investor.clone()))
                .unwrap_or(0);
            if bps > 0 {
                out.push_back((invoice_id, bps));
            }
        }
        out
    }

    pub fn get_co_fund_share(env: Env, invoice_id: u64, investor: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::CoFundShare(invoice_id, investor))
            .unwrap_or(0)
    }

    pub fn transfer_co_fund_share(
        env: Env,
        from: Address,
        invoice_id: u64,
        token: Address,
        to: Address,
        bps: u32,
    ) -> Result<(), PoolError> {
        from.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::assert_accepted_token(&env, &token)?;

        if bps == 0 {
            return Err(PoolError::ZeroAmount);
        }
        if bps > BPS_DENOM {
            return Err(PoolError::InvalidAmount);
        }

        non_reentrant!(&env, {
            // Invoice must exist and not be fully repaid
            let record: FundedInvoice = env
                .storage()
                .persistent()
                .get(&DataKey::FundedInvoice(invoice_id))
                .ok_or(PoolError::InvoiceNotFound)?;
            if record.repaid_amount >= record.principal.saturating_add(record.factoring_fee) {
                return Err(PoolError::AlreadyFullyRepaid);
            }

            // #860: co-fund shares are only tradeable once the round they
            // came from is Filled — no trading unfunded/still-committing
            // positions.
            if let Some(round_id) = record.co_funding_round_id {
                let round: CoFundingRound = env
                    .storage()
                    .persistent()
                    .get(&DataKey::CoFundingRound(round_id))
                    .ok_or(PoolError::CoFundingRoundNotFound)?;
                if round.status != CoFundingStatus::Filled {
                    return Err(PoolError::CoFundingRoundNotFilled);
                }
            }

            require_kyc_approved(&env, &to)?;

            let from_key = DataKey::CoFundShare(invoice_id, from.clone());
            let to_key = DataKey::CoFundShare(invoice_id, to.clone());

            let from_share: u32 = env.storage().persistent().get(&from_key).unwrap_or(0);

            // Calculate share amount to transfer
            let transfer_amount = (from_share as u64 * bps as u64 / BPS_DENOM as u64) as u32;
            if transfer_amount == 0 || transfer_amount > from_share {
                return Err(PoolError::InsufficientCoFundShare);
            }

            let to_share: u32 = env.storage().persistent().get(&to_key).unwrap_or(0);

            let new_from_share = from_share - transfer_amount;
            let new_to_share = to_share.saturating_add(transfer_amount);

            if new_from_share == 0 {
                env.storage().persistent().remove(&from_key);
            } else {
                env.storage().persistent().set(&from_key, &new_from_share);
            }
            env.storage().persistent().set(&to_key, &new_to_share);

            // #860: keep the round's participant list in sync so repayment
            // distribution (which iterates `round.participants`) still finds
            // and pays out the new holder, and so a fully-divested `from`
            // doesn't keep occupying a MAX_CO_FUNDING_PARTICIPANTS slot.
            if let Some(round_id) = record.co_funding_round_id {
                if let Some(mut round) = env
                    .storage()
                    .persistent()
                    .get::<DataKey, CoFundingRound>(&DataKey::CoFundingRound(round_id))
                {
                    if new_from_share == 0 {
                        remove_participant(&env, &mut round, &from);
                    }
                    if to_share == 0 {
                        if round.participants.len() >= MAX_CO_FUNDING_PARTICIPANTS {
                            return Err(PoolError::CoFundingTooManyParticipants);
                        }
                        round.participants.push_back(to.clone());
                    }
                    env.storage()
                        .persistent()
                        .set(&DataKey::CoFundingRound(round_id), &round);
                }
            }

            env.events().publish(
                (EVT, symbol_short!("shr_xfer")),
                (invoice_id, from, to, bps, transfer_amount),
            );
            Ok(())
        })
    }

    pub fn get_config(env: Env) -> Result<PoolConfig, PoolError> {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(PoolError::NotInitialized)
    }
    pub fn accepted_tokens(env: Env) -> Result<Vec<Address>, PoolError> {
        env.storage()
            .instance()
            .get(&DataKey::AcceptedTokens)
            .ok_or(PoolError::NotInitialized)
    }
    pub fn get_token_totals(env: Env, token: Address) -> PoolTokenTotals {
        env.storage()
            .instance()
            .get(&DataKey::TokenTotals(token))
            .unwrap_or_default()
    }

    pub fn get_pool_balance(env: Env, token: Address) -> i128 {
        let token_client = token::Client::new(&env, &token);
        token_client.balance(&env.current_contract_address())
    }

    // Public so the withdrawal-wait/liquidity-forecast analytics satellite
    // can look up the share token to read total supply for a token.
    pub fn get_share_token(env: Env, token: Address) -> Option<Address> {
        env.storage().instance().get(&DataKey::ShareToken(token))
    }

    pub fn set_oracle_contract(env: Env, admin: Address, oracle: Address) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;
        env.storage().instance().set(&REFLECTOR_ORACLE, &oracle);
        env.events()
            .publish((EVT, symbol_short!("set_orcl")), (admin, oracle));
        Ok(())
    }

    pub fn get_oracle_contract(env: Env) -> Option<Address> {
        env.storage().instance().get(&REFLECTOR_ORACLE)
    }

    pub fn set_oracle_stale_threshold(
        env: Env,
        admin: Address,
        threshold_secs: u64,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;
        if threshold_secs == 0 {
            return Err(PoolError::InvalidThreshold);
        }
        env.storage()
            .instance()
            .set(&ORACLE_STALE_SECS, &threshold_secs);
        env.events()
            .publish((EVT, symbol_short!("orcl_stl")), (admin, threshold_secs));
        Ok(())
    }

    pub fn get_oracle_stale_threshold(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&ORACLE_STALE_SECS)
            .unwrap_or(DEFAULT_ORACLE_STALE_SECS)
    }

    pub fn set_fallback_price(
        env: Env,
        admin: Address,
        token: Address,
        price: i128,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;
        if price <= 0 {
            return Err(PoolError::InvalidAmount);
        }
        // #1036: the fallback price now carries the timestamp it was set at, so
        // get_asset_price can bound its own staleness the same way a live Reflector
        // reading is bounded — an admin backstop price set months ago must not be
        // usable to justify a liquidation today.
        let mut prices: Map<Address, (i128, u64)> = env
            .storage()
            .instance()
            .get(&ORACLE_FALLBACK_PX)
            .unwrap_or_else(|| Map::new(&env));
        prices.set(token.clone(), (price, env.ledger().timestamp()));
        env.storage().instance().set(&ORACLE_FALLBACK_PX, &prices);
        env.events()
            .publish((EVT, symbol_short!("fb_price")), (admin, token, price));
        Ok(())
    }

    pub fn get_fallback_price(env: Env, token: Address) -> Option<i128> {
        let prices: Map<Address, (i128, u64)> = env
            .storage()
            .instance()
            .get(&ORACLE_FALLBACK_PX)
            .unwrap_or_else(|| Map::new(&env));
        prices.get(token).map(|(price, _set_at)| price)
    }

    pub fn get_asset_price(env: Env, token: Address) -> Result<i128, PoolError> {
        if let Some(oracle) = Self::get_oracle_contract(env.clone()) {
            let reflector = ReflectorClient::new(&env, &oracle);
            if let Some(price_data) = reflector.lastprice(&ReflectorAsset::Stellar(token.clone())) {
                let now = env.ledger().timestamp();
                let age = now.saturating_sub(price_data.timestamp);
                if age <= Self::get_oracle_stale_threshold(env.clone()) {
                    return Ok(price_data.price);
                }
            }
        }
        // #1036: bound the fallback price's own staleness by the same threshold —
        // a fallback set outside the staleness window is refused, not just an
        // absent one, so a stale admin backstop price can't be used to justify
        // a liquidation either (this is what the risk-response satellite in
        // contracts/auction relies on via its own get_asset_price reads here).
        let now = env.ledger().timestamp();
        let stale_secs = Self::get_oracle_stale_threshold(env.clone());
        let prices: Map<Address, (i128, u64)> = env
            .storage()
            .instance()
            .get(&ORACLE_FALLBACK_PX)
            .unwrap_or_else(|| Map::new(&env));
        match prices.get(token) {
            Some((price, set_at)) if now.saturating_sub(set_at) <= stale_secs => Ok(price),
            _ => Err(PoolError::OraclePriceUnavailable),
        }
    }

    pub fn get_utilization(env: Env, token: Address) -> u32 {
        let tt = Self::get_token_totals(env, token);
        if tt.pool_value <= 0 {
            return 0;
        }
        ((tt.total_deployed as u128 * 10_000u128) / tt.pool_value as u128) as u32
    }

    pub fn set_max_utilization(env: Env, admin: Address, max_bps: u32) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;
        if max_bps > 10_000 {
            return Err(PoolError::InvalidAmount);
        }
        let mut config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(PoolError::NotInitialized)?;
        config.max_utilization_bps = max_bps;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events()
            .publish((EVT, symbol_short!("max_util")), max_bps);
        Ok(())
    }
    pub fn get_funded_invoice(env: Env, invoice_id: u64) -> Option<FundedInvoice> {
        env.storage()
            .persistent()
            .get(&DataKey::FundedInvoice(invoice_id))
    }

    pub fn get_funded_invoices_batch(
        env: Env,
        ids: Vec<u64>,
    ) -> Result<Vec<Option<FundedInvoice>>, PoolError> {
        if ids.len() > MAX_BATCH_SIZE {
            return Err(PoolError::BatchTooLarge);
        }

        let mut invoices = Vec::new(&env);
        for i in 0..ids.len() {
            let invoice_id = ids.get(i).ok_or(PoolError::StorageCorrupted)?;
            invoices.push_back(
                env.storage()
                    .persistent()
                    .get(&DataKey::FundedInvoice(invoice_id)),
            );
        }
        Ok(invoices)
    }

    pub fn available_liquidity(env: Env, token: Address) -> i128 {
        let tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&DataKey::TokenTotals(token))
            .unwrap_or_default();
        tt.pool_value - tt.total_deployed
    }
    pub fn get_storage_stats(env: Env) -> PoolStorageStats {
        env.storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default()
    }

    pub fn cleanup_funded_invoice(
        env: Env,
        admin: Address,
        invoice_id: u64,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;

        non_reentrant!(&env, {
            let record: FundedInvoice = env
                .storage()
                .persistent()
                .get(&DataKey::FundedInvoice(invoice_id))
                .ok_or(PoolError::InvoiceNotFound)?;

            // Calculate total due to check if fully repaid
            let config: PoolConfig = env
                .storage()
                .instance()
                .get(&DataKey::Config)
                .ok_or(PoolError::NotInitialized)?;
            let now = env.ledger().timestamp();
            let (_total_interest, total_due) = calculate_total_due(&record, &config, now)?;

            if record.repaid_amount < total_due {
                return Err(PoolError::InvalidAmount);
            }
            env.storage()
                .persistent()
                .remove(&DataKey::FundedInvoice(invoice_id));

            let mut stats: PoolStorageStats = env
                .storage()
                .instance()
                .get(&DataKey::StorageStats)
                .unwrap_or_default();
            stats.cleaned_invoices += 1;
            env.storage().instance().set(&DataKey::StorageStats, &stats);
            env.events()
                .publish((EVT, symbol_short!("cleanup")), invoice_id);
            Ok(())
        })
    }

    pub fn estimate_repayment(
        env: Env,
        invoice_id: u64,
        as_of_timestamp: Option<u64>,
    ) -> Result<i128, PoolError> {
        bump_instance(&env);
        let config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(PoolError::NotInitialized)?;
        let record: FundedInvoice = env
            .storage()
            .persistent()
            .get(&DataKey::FundedInvoice(invoice_id))
            .ok_or(PoolError::InvoiceNotFound)?;
        if record.funded_at == 0 {
            return Ok(record.principal);
        }

        // #655: allow callers to simulate a future repayment; reject past timestamps
        // so this view never reports less interest than is currently owed.
        let now = env.ledger().timestamp();
        let as_of = match as_of_timestamp {
            Some(ts) => {
                if ts < now {
                    return Err(PoolError::TimestampInPast);
                }
                ts
            }
            None => now,
        };
        let (_interest, total_due) = calculate_total_due(&record, &config, as_of)?;
        // Return remaining amount due (total - already repaid)
        let remaining = total_due - record.repaid_amount;
        if remaining < 0 {
            Ok(0)
        } else {
            Ok(remaining)
        }
    }

    pub fn is_invoice_repaid(env: Env, invoice_id: u64) -> Result<bool, PoolError> {
        bump_instance(&env);
        let config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(PoolError::NotInitialized)?;
        let record: FundedInvoice = env
            .storage()
            .persistent()
            .get(&DataKey::FundedInvoice(invoice_id))
            .ok_or(PoolError::InvoiceNotFound)?;
        let (_interest, total_due) =
            calculate_total_due(&record, &config, env.ledger().timestamp())?;
        Ok(record.repaid_amount >= total_due)
    }

    pub fn update_invoice_due_date(
        env: Env,
        invoice_contract: Address,
        invoice_id: u64,
        new_due_date: u64,
    ) {
        invoice_contract.require_auth();
        bump_instance(&env);

        Self::non_reentrant_start(&env);

        let config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .expect("not initialized");
        if invoice_contract != config.invoice_contract {
            Self::non_reentrant_end(&env);
            panic_with_error!(&env, PoolError::Unauthorized);
        }

        let mut record: FundedInvoice = env
            .storage()
            .persistent()
            .get(&DataKey::FundedInvoice(invoice_id))
            .unwrap_or_else(|| {
                Self::non_reentrant_end(&env);
                panic_with_error!(&env, PoolError::InvoiceNotFound)
            });
        if new_due_date <= record.due_date {
            Self::non_reentrant_end(&env);
            panic_with_error!(&env, PoolError::InvalidAmount);
        }

        let old_due_date = record.due_date;
        record.due_date = new_due_date;
        env.storage()
            .persistent()
            .set(&DataKey::FundedInvoice(invoice_id), &record);
        set_funded_invoice_ttl(&env, invoice_id, false);
        Self::non_reentrant_end(&env);
        env.events().publish(
            (EVT, symbol_short!("due_ext")),
            (
                invoice_id,
                old_due_date,
                new_due_date,
                env.ledger().timestamp(),
            ),
        );
    }

    fn require_admin(env: &Env, admin: &Address) -> PoolResult<()> {
        let config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(PoolError::NotInitialized)?;
        if admin != &config.admin {
            return Err(PoolError::Unauthorized);
        }
        Ok(())
    }

    fn require_access_control(env: &Env, caller: &Address) -> PoolResult<()> {
        let configured: Address = env
            .storage()
            .instance()
            .get(&ACCESS_CONTROL)
            .ok_or(PoolError::AccessControlNotConfigured)?;
        if caller != &configured {
            return Err(PoolError::Unauthorized);
        }
        Ok(())
    }

    // #1038: Helper function to verify the caller is the configured governance contract
    fn require_governance(env: &Env, caller: &Address) -> PoolResult<()> {
        let configured: Address = env
            .storage()
            .instance()
            .get(&GOVERNANCE)
            .ok_or(PoolError::GovernanceNotConfigured)?;
        if caller != &configured {
            return Err(PoolError::Unauthorized);
        }
        Ok(())
    }

    fn require_compliance_cleared(env: &Env, address: &Address) -> PoolResult<()> {
        let gate: Option<ComplianceGateConfig> = env.storage().instance().get(&COMPLIANCE_CFG);
        let Some(gate) = gate else {
            return Ok(());
        };
        if !gate.required {
            return Ok(());
        }
        let client = ComplianceClient::new(env, &gate.registry);
        match client.try_is_cleared(address) {
            Ok(Ok(true)) => Ok(()),
            Ok(Ok(false)) => Err(PoolError::ComplianceNotCleared),
            _ => Err(PoolError::ComplianceCheckFailed),
        }
    }

    fn record_referral_activity(
        env: &Env,
        referee: &Address,
        kind: Symbol,
        fee_amount: i128,
        token: &Address,
    ) {
        if fee_amount < 0 {
            return;
        }
        let registry: Option<Address> = env.storage().instance().get(&REFERRAL_CFG);
        let Some(registry) = registry else {
            return;
        };
        let client = ReferralClient::new(env, &registry);
        let reward = match client.try_record_activity(
            &env.current_contract_address(),
            referee,
            &kind,
            &fee_amount,
            token,
        ) {
            Ok(Ok(amount)) if amount > 0 => amount,
            _ => return,
        };

        let token_totals_key = DataKey::TokenTotals(token.clone());
        let mut tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&token_totals_key)
            .unwrap_or_default();
        tt.protocol_revenue = tt.protocol_revenue.saturating_sub(reward);
        env.storage().instance().set(&token_totals_key, &tt);

        let token_client = token::Client::new(env, token);
        token_client.transfer(&env.current_contract_address(), &registry, &reward);
        env.events()
            .publish((EVT, symbol_short!("ref_pay")), (referee.clone(), reward));
    }

    fn require_not_paused(env: &Env) {
        require_not_paused(env);
    }

    fn assert_accepted_token(env: &Env, token: &Address) -> PoolResult<()> {
        let tokens: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AcceptedTokens)
            .ok_or(PoolError::NotInitialized)?;
        for i in 0..tokens.len() {
            if tokens.get(i).ok_or(PoolError::StorageCorrupted)? == *token {
                return Ok(());
            }
        }
        Err(PoolError::TokenNotAccepted)
    }

    // ---- #111: Exchange rate methods ----

    pub fn set_rate_bounds(
        env: Env,
        admin: Address,
        token: Address,
        min_bps: u32,
        max_bps: u32,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        Self::assert_accepted_token(&env, &token)?;
        if min_bps == 0 || max_bps == 0 {
            return Err(PoolError::InvalidAmount);
        }
        if min_bps > max_bps {
            return Err(PoolError::InvalidAmount);
        }

        env.storage().instance().set(
            &DataKey::ExchangeRateBounds(token.clone()),
            &ExchangeRateBounds { min_bps, max_bps },
        );
        env.events().publish(
            (EVT, symbol_short!("bounds")),
            (admin, token, min_bps, max_bps),
        );
        Ok(())
    }

    pub fn set_exchange_rate(
        env: Env,
        admin: Address,
        token: Address,
        rate_bps: u32,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        Self::assert_accepted_token(&env, &token)?;
        if rate_bps == 0 {
            return Err(PoolError::InvalidAmount);
        }
        let bounds = env
            .storage()
            .instance()
            .get::<DataKey, ExchangeRateBounds>(&DataKey::ExchangeRateBounds(token.clone()));
        if let Some(bounds) = bounds {
            if rate_bps < bounds.min_bps || rate_bps > bounds.max_bps {
                return Err(PoolError::InvalidAmount);
            }
        }
        env.storage()
            .instance()
            .set(&DataKey::ExchangeRate(token.clone()), &rate_bps);
        env.events()
            .publish((EVT, symbol_short!("set_rate")), (admin, token, rate_bps));
        Ok(())
    }

    pub fn get_exchange_rate(env: Env, token: Address) -> u32 {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::ExchangeRate(token))
            .unwrap_or(10_000u32)
    }

    pub fn get_rate_bounds(env: Env, token: Address) -> ExchangeRateBounds {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::ExchangeRateBounds(token))
            .unwrap_or(ExchangeRateBounds {
                min_bps: 10_000u32,
                max_bps: 10_000u32,
            })
    }

    // ---- #867: Compliance registry integration ----

    pub fn set_compliance_registry(
        env: Env,
        admin: Address,
        registry: Address,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        let mut gate: ComplianceGateConfig = env
            .storage()
            .instance()
            .get(&COMPLIANCE_CFG)
            .unwrap_or(ComplianceGateConfig {
                registry: registry.clone(),
                required: false,
            });
        gate.registry = registry.clone();
        env.storage().instance().set(&COMPLIANCE_CFG, &gate);
        env.events()
            .publish((EVT, symbol_short!("cmp_set")), (admin, registry));
        Ok(())
    }

    pub fn get_compliance_registry(env: Env) -> Option<Address> {
        bump_instance(&env);
        env.storage()
            .instance()
            .get::<Symbol, ComplianceGateConfig>(&COMPLIANCE_CFG)
            .map(|g| g.registry)
    }

    pub fn set_require_compliance_check(
        env: Env,
        admin: Address,
        required: bool,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        let mut gate: ComplianceGateConfig = env
            .storage()
            .instance()
            .get(&COMPLIANCE_CFG)
            .ok_or(PoolError::ComplianceCheckFailed)?;
        gate.required = required;
        env.storage().instance().set(&COMPLIANCE_CFG, &gate);
        env.events()
            .publish((EVT, symbol_short!("cmp_req")), (admin, required));
        Ok(())
    }

    pub fn require_compliance_check(env: Env) -> bool {
        bump_instance(&env);
        env.storage()
            .instance()
            .get::<Symbol, ComplianceGateConfig>(&COMPLIANCE_CFG)
            .map(|g| g.required)
            .unwrap_or(false)
    }

    // ---- #799: Referral program integration ----

    pub fn set_referral_registry(
        env: Env,
        admin: Address,
        registry: Address,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        env.storage().instance().set(&REFERRAL_CFG, &registry);
        env.events()
            .publish((EVT, symbol_short!("ref_set")), (admin, registry));
        Ok(())
    }

    pub fn get_referral_registry(env: Env) -> Option<Address> {
        bump_instance(&env);
        env.storage().instance().get(&REFERRAL_CFG)
    }

    // ---- #109: Investor KYC / whitelist methods ----

    pub fn set_kyc_required(env: Env, admin: Address, required: bool) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        env.storage()
            .instance()
            .set(&DataKey::KycRequired, &required);
        env.events()
            .publish((EVT, symbol_short!("kyc_req")), (admin, required));
        Ok(())
    }

    pub fn kyc_required(env: Env) -> bool {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::KycRequired)
            .unwrap_or(false)
    }

    pub fn set_investor_kyc(
        env: Env,
        admin: Address,
        investor: Address,
        approved: bool,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;

        // Issue #345: Validate investor address
        let config = get_config_cached(&env)?;
        if investor == config.admin
            || investor == env.current_contract_address()
            || investor == config.invoice_contract
        {
            return Err(PoolError::Unauthorized);
        }

        let status = if approved {
            KycStatus::Approved
        } else {
            KycStatus::Rejected
        };
        env.storage()
            .persistent()
            .set(&DataKey::InvestorKyc(investor.clone()), &status);
        env.events()
            .publish((EVT, symbol_short!("kyc_set")), (admin, investor, approved));
        Ok(())
    }

    pub fn approve_investor_kyc(
        env: Env,
        admin: Address,
        investor: Address,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;

        let config = get_config_cached(&env)?;
        if investor == config.admin
            || investor == env.current_contract_address()
            || investor == config.invoice_contract
        {
            return Err(PoolError::Unauthorized);
        }

        env.storage().persistent().set(
            &DataKey::InvestorKyc(investor.clone()),
            &KycStatus::Approved,
        );
        env.events()
            .publish((EVT, symbol_short!("kyc_appr")), (admin, investor));
        Ok(())
    }

    pub fn reject_investor_kyc(
        env: Env,
        admin: Address,
        investor: Address,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;

        let config = get_config_cached(&env)?;
        if investor == config.admin
            || investor == env.current_contract_address()
            || investor == config.invoice_contract
        {
            return Err(PoolError::Unauthorized);
        }

        env.storage().persistent().set(
            &DataKey::InvestorKyc(investor.clone()),
            &KycStatus::Rejected,
        );
        env.events()
            .publish((EVT, symbol_short!("kyc_rej")), (admin, investor));
        Ok(())
    }

    pub fn get_investor_kyc_status(env: Env, investor: Address) -> KycStatus {
        bump_instance(&env);
        env.storage()
            .persistent()
            .get(&DataKey::InvestorKyc(investor))
            .unwrap_or(KycStatus::NotRequested)
    }

    pub fn get_investor_kyc(env: Env, investor: Address) -> bool {
        bump_instance(&env);
        matches!(
            env.storage()
                .persistent()
                .get::<DataKey, KycStatus>(&DataKey::InvestorKyc(investor))
                .unwrap_or(KycStatus::NotRequested),
            KycStatus::Approved
        )
    }

    pub fn set_upgrade_timelock(env: Env, admin: Address, secs: u64) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        if secs < MIN_UPGRADE_TIMELOCK_SECS {
            return Err(PoolError::InvalidUpgradeTimelock);
        }
        let old_secs: u64 = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeTimelockSecs)
            .unwrap_or(UPGRADE_TIMELOCK_SECS);
        env.storage()
            .instance()
            .set(&DataKey::UpgradeTimelockSecs, &secs);
        env.events().publish(
            (EVT, Symbol::new(&env, "timelock_updated")),
            (admin, old_secs, secs),
        );
        Ok(())
    }

    pub fn get_upgrade_timelock(env: Env) -> u64 {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::UpgradeTimelockSecs)
            .unwrap_or(UPGRADE_TIMELOCK_SECS)
    }

    pub fn propose_upgrade(
        env: Env,
        admin: Address,
        wasm_hash: BytesN<32>,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        // #340: reject all-zero hash
        if wasm_hash == BytesN::from_array(&env, &[0u8; 32]) {
            return Err(PoolError::InvalidWasmHash);
        }
        env.storage()
            .instance()
            .set(&DataKey::ProposedWasmHash, &wasm_hash);
        env.storage()
            .instance()
            .set(&DataKey::UpgradeScheduledAt, &env.ledger().timestamp());
        let timelock: u64 = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeTimelockSecs)
            .unwrap_or(UPGRADE_TIMELOCK_SECS);
        env.events().publish(
            (EVT, symbol_short!("upg_prop")),
            (admin, env.ledger().timestamp() + timelock),
        );
        Ok(())
    }

    pub fn execute_upgrade(env: Env, admin: Address) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        let scheduled_at: u64 = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeScheduledAt)
            .ok_or(PoolError::NotInitialized)?;
        let timelock: u64 = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeTimelockSecs)
            .unwrap_or(UPGRADE_TIMELOCK_SECS);
        let now = env.ledger().timestamp();
        if now < scheduled_at + timelock {
            return Err(PoolError::UpgradeTimelockNotExpired);
        }
        let wasm_hash: BytesN<32> = env
            .storage()
            .instance()
            .get(&DataKey::ProposedWasmHash)
            .ok_or(PoolError::NotInitialized)?;
        env.deployer().update_current_contract_wasm(wasm_hash);
        env.events()
            .publish((EVT, symbol_short!("upgraded")), (admin, now));
        Ok(())
    }

    pub fn propose_admin_change(
        env: Env,
        admin: Address,
        new_admin: Address,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        Self::require_not_paused(&env);
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        env.storage()
            .instance()
            .set(&DataKey::PendingAdmin, &new_admin);
        env.storage()
            .instance()
            .set(&DataKey::AdminChangeScheduledAt, &env.ledger().timestamp());
        env.events().publish(
            (EVT, Symbol::new(&env, "admin_chg_proposed")),
            (
                admin,
                new_admin,
                env.ledger().timestamp() + ADMIN_CHANGE_TIMELOCK_SECS,
            ),
        );
        Ok(())
    }

    pub fn finalize_admin_change(env: Env, admin: Address) -> Result<(), PoolError> {
        admin.require_auth();
        Self::require_not_paused(&env);
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        let maybe_scheduled: Option<u64> = env
            .storage()
            .instance()
            .get(&DataKey::AdminChangeScheduledAt);
        let scheduled_at = match maybe_scheduled {
            Some(v) => v,
            None => return Err(PoolError::NoAdminChangeProposed),
        };
        let now = env.ledger().timestamp();
        if now < scheduled_at + ADMIN_CHANGE_TIMELOCK_SECS {
            return Err(PoolError::AdminChangeTimelockNotExpired);
        }
        let maybe_new_admin: Option<Address> = env.storage().instance().get(&DataKey::PendingAdmin);
        let new_admin = match maybe_new_admin {
            Some(v) => v,
            None => return Err(PoolError::NoAdminChangeProposed),
        };
        let mut config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(PoolError::NotInitialized)?;
        config.admin = new_admin.clone();
        env.storage().instance().set(&DataKey::Config, &config);
        env.storage().instance().remove(&DataKey::PendingAdmin);
        env.storage()
            .instance()
            .remove(&DataKey::AdminChangeScheduledAt);
        env.events().publish(
            (EVT, Symbol::new(&env, "admin_changed")),
            (admin, new_admin, now),
        );
        Ok(())
    }

    // ---- #742: two-step confirmation for critical admin operations ----

    pub fn get_operation_delay(env: Env) -> u64 {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::OperationDelaySecs)
            .unwrap_or(OPERATION_DELAY_SECS)
    }

    pub fn set_operation_delay(env: Env, admin: Address, secs: u64) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        if secs < MIN_OPERATION_DELAY_SECS {
            return Err(PoolError::InvalidOperationDelay);
        }
        let old: u64 = env
            .storage()
            .instance()
            .get(&DataKey::OperationDelaySecs)
            .unwrap_or(OPERATION_DELAY_SECS);
        env.storage()
            .instance()
            .set(&DataKey::OperationDelaySecs, &secs);
        env.events()
            .publish((EVT, symbol_short!("op_delay")), (admin, old, secs));
        Ok(())
    }

    pub fn propose_operation(
        env: Env,
        admin: Address,
        operation: AdminOperation,
    ) -> Result<u64, PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;

        // Validate operation parameters up-front so bad proposals fail fast.
        match &operation {
            AdminOperation::SetCollateralConfig(threshold, collateral_bps) => {
                Self::validate_collateral_config(*threshold, *collateral_bps)?
            }
            AdminOperation::RemoveToken(_) | AdminOperation::SeizeCollateral(_) => {}
        }

        let now = env.ledger().timestamp();
        let delay = env
            .storage()
            .instance()
            .get(&DataKey::OperationDelaySecs)
            .unwrap_or(OPERATION_DELAY_SECS);
        let execute_after = now.checked_add(delay).ok_or(PoolError::AmountOverflow)?;

        let proposal_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextProposalId)
            .unwrap_or(1);
        env.storage()
            .instance()
            .set(&DataKey::NextProposalId, &(proposal_id + 1));

        let proposal = Proposal {
            operation,
            execute_after,
            proposed_at: now,
            proposer: admin.clone(),
            executed: false,
            cancelled: false,
        };
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        env.events().publish(
            (EVT, symbol_short!("op_prop")),
            (admin, proposal_id, execute_after),
        );
        Ok(proposal_id)
    }

    pub fn execute_operation(env: Env, admin: Address, proposal_id: u64) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;

        let mut proposal: Proposal = env
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .ok_or(PoolError::ProposalNotFound)?;
        if proposal.executed {
            return Err(PoolError::ProposalAlreadyExecuted);
        }
        if proposal.cancelled {
            return Err(PoolError::ProposalAlreadyCancelled);
        }
        let now = env.ledger().timestamp();
        if now < proposal.execute_after {
            return Err(PoolError::ProposalNotReady);
        }

        // Mark as executed before dispatch so a re-entrant call cannot replay it.
        proposal.executed = true;
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        let op = proposal.operation.clone();
        match op {
            AdminOperation::RemoveToken(token) => Self::execute_remove_token(&env, &admin, &token)?,
            AdminOperation::SetCollateralConfig(threshold, collateral_bps) => {
                Self::execute_set_collateral_config(&env, &admin, threshold, collateral_bps)?
            }
            AdminOperation::SeizeCollateral(invoice_id) => {
                Self::execute_seize_collateral(&env, &admin, invoice_id)?
            }
        }

        env.events()
            .publish((EVT, symbol_short!("op_exec")), (admin, proposal_id, now));
        Ok(())
    }

    pub fn cancel_admin_change(env: Env, admin: Address) -> Result<(), PoolError> {
        admin.require_auth();
        Self::require_not_paused(&env);
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        if !env.storage().instance().has(&DataKey::PendingAdmin) {
            return Err(PoolError::NoAdminChangeProposed);
        }
        env.storage().instance().remove(&DataKey::PendingAdmin);
        env.storage()
            .instance()
            .remove(&DataKey::AdminChangeScheduledAt);
        env.events()
            .publish((EVT, Symbol::new(&env, "admin_chg_cancelled")), admin);
        Ok(())
    }

    pub fn cancel_operation(env: Env, admin: Address, proposal_id: u64) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;

        let mut proposal: Proposal = env
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .ok_or(PoolError::ProposalNotFound)?;
        if proposal.executed {
            return Err(PoolError::ProposalAlreadyExecuted);
        }
        if proposal.cancelled {
            return Err(PoolError::ProposalAlreadyCancelled);
        }
        proposal.cancelled = true;
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        env.events().publish(
            (EVT, symbol_short!("op_cncl")),
            (admin, proposal_id, env.ledger().timestamp()),
        );
        Ok(())
    }

    pub fn get_proposal(env: Env, proposal_id: u64) -> Option<Proposal> {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
    }

    // ---- Internal utility methods ----

    fn non_reentrant_start(env: &Env) {
        let key = DataKey::ReentrancyGuard;
        if env
            .storage()
            .instance()
            .get::<DataKey, bool>(&key)
            .unwrap_or(false)
        {
            panic!("reentrant call");
        }
        env.storage().instance().set(&key, &true);
    }

    fn non_reentrant_end(env: &Env) {
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyGuard, &false);
    }

    // ── #1025 / secondary-market extraction: listing bookkeeping now lives in
    // a satellite `secondary_market` contract (kept under the 200KB WASM
    // deploy limit). Pool retains only the trusted settlement entrypoint that
    // actually moves value, plus the read access the satellite needs.

    pub fn set_secondary_market_contract(
        env: Env,
        admin: Address,
        secondary_market_contract: Address,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;
        env.storage()
            .instance()
            .set(&SECONDARY_MARKET_CONTRACT, &secondary_market_contract);
        Ok(())
    }

    pub fn get_secondary_market_contract(env: Env) -> Option<Address> {
        bump_instance(&env);
        env.storage().instance().get(&SECONDARY_MARKET_CONTRACT)
    }

    pub fn get_investor_position(env: Env, investor: Address, token: Address) -> (i128, i128) {
        bump_instance(&env);
        let position: InvestorPosition = env
            .storage()
            .persistent()
            .get(&DataKey::InvestorPosition(investor, token))
            .unwrap_or_default();
        (position.available, position.deployed)
    }

    pub fn market_settle_listing(
        env: Env,
        market: Address,
        settlement: ListingSettlement,
    ) -> Result<(), PoolError> {
        let ListingSettlement {
            buyer,
            seller,
            invoice_id,
            is_co_funding,
            amount_or_bps,
            price,
        } = settlement;
        market.require_auth();
        let authorized_market: Address = env
            .storage()
            .instance()
            .get(&SECONDARY_MARKET_CONTRACT)
            .ok_or(PoolError::NotInitialized)?;
        if market != authorized_market {
            return Err(PoolError::Unauthorized);
        }
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_compliance_cleared(&env, &buyer)?;
        // #1035: the order-book matching engine can settle a fill between
        // two arbitrary resting orders, not just a buyer accepting a
        // seller's own listing, so the seller's compliance/KYC status is
        // re-checked here too rather than assumed still valid from
        // whenever they originally entered their position.
        Self::require_compliance_cleared(&env, &seller)?;

        if seller == buyer {
            return Err(PoolError::Unauthorized);
        }

        non_reentrant!(&env, {
            require_kyc_approved(&env, &buyer)?;
            require_kyc_approved(&env, &seller)?;

            let record: FundedInvoice = env
                .storage()
                .persistent()
                .get(&DataKey::FundedInvoice(invoice_id))
                .ok_or(PoolError::InvoiceNotFound)?;
            let token = record.token.clone();
            let token_totals_key = DataKey::TokenTotals(token.clone());
            let tt: PoolTokenTotals = env
                .storage()
                .instance()
                .get(&token_totals_key)
                .unwrap_or_default();

            // Buyer must have enough available balance to pay the price.
            let buyer_pos_key = DataKey::InvestorPosition(buyer.clone(), token.clone());
            let mut buyer_pos: InvestorPosition = env
                .storage()
                .persistent()
                .get(&buyer_pos_key)
                .unwrap_or_default();
            if buyer_pos.available < price {
                return Err(PoolError::InvalidAmount);
            }

            // Concentration cap: buying deployed capital increases the buyer's
            // effective share of the pool. Check against max_single_investor_bps.
            let config = get_config_cached(&env)?;
            if config.max_single_investor_bps < BPS_DENOM {
                let buyer_new_deployed = buyer_pos
                    .deployed
                    .checked_add(price)
                    .ok_or(PoolError::AmountOverflow)?;
                let pool_value = tt.pool_value.max(1);
                let share_bps =
                    ((buyer_new_deployed as u128 * BPS_DENOM as u128) / pool_value as u128) as u32;
                if share_bps > config.max_single_investor_bps {
                    return Err(PoolError::ConcentrationLimitExceeded);
                }
            }

            // Transfer the claim to the buyer.
            if is_co_funding {
                let round_id = record.co_funding_round_id.ok_or(PoolError::InvalidAmount)?;

                let from_key = DataKey::CoFundShare(round_id, seller.clone());
                let to_key = DataKey::CoFundShare(round_id, buyer.clone());

                let from_bps: u32 = env.storage().persistent().get(&from_key).unwrap_or(0);
                let transfer_bps = amount_or_bps as u32;
                if transfer_bps > from_bps {
                    return Err(PoolError::InsufficientListingBalance);
                }
                let to_bps: u32 = env.storage().persistent().get(&to_key).unwrap_or(0);

                let new_from = from_bps - transfer_bps;
                let new_to = to_bps
                    .checked_add(transfer_bps)
                    .ok_or(PoolError::AmountOverflow)?;

                if new_from == 0 {
                    env.storage().persistent().remove(&from_key);
                } else {
                    env.storage().persistent().set(&from_key, &new_from);
                }
                env.storage().persistent().set(&to_key, &new_to);

                // Keep round participant list in sync.
                if let Some(mut round) = env
                    .storage()
                    .persistent()
                    .get::<DataKey, CoFundingRound>(&DataKey::CoFundingRound(round_id))
                {
                    if new_from == 0 {
                        remove_participant(&env, &mut round, &seller);
                    }
                    if to_bps == 0 {
                        if round.participants.len() >= MAX_CO_FUNDING_PARTICIPANTS {
                            return Err(PoolError::CoFundingTooManyParticipants);
                        }
                        round.participants.push_back(buyer.clone());
                    }
                    env.storage()
                        .persistent()
                        .set(&DataKey::CoFundingRound(round_id), &round);
                }
            }

            let seller_pos_key = DataKey::InvestorPosition(seller.clone(), token.clone());
            let mut seller_pos: InvestorPosition = env
                .storage()
                .persistent()
                .get(&seller_pos_key)
                .unwrap_or_default();

            if !is_co_funding {
                // Move deployed principal slice from seller to buyer.
                let transfer_amount = amount_or_bps as i128;
                if seller_pos.deployed < transfer_amount {
                    return Err(PoolError::InsufficientListingBalance);
                }
                seller_pos.deployed = seller_pos
                    .deployed
                    .checked_sub(transfer_amount)
                    .ok_or(PoolError::AmountOverflow)?;
                buyer_pos.deployed = buyer_pos
                    .deployed
                    .checked_add(transfer_amount)
                    .ok_or(PoolError::AmountOverflow)?;
            }

            // Debit buyer's available balance and credit seller.
            buyer_pos.available = buyer_pos
                .available
                .checked_sub(price)
                .ok_or(PoolError::AmountOverflow)?;
            env.storage().persistent().set(&buyer_pos_key, &buyer_pos);

            seller_pos.available = seller_pos
                .available
                .checked_add(price)
                .ok_or(PoolError::AmountOverflow)?;
            env.storage().persistent().set(&seller_pos_key, &seller_pos);

            env.events().publish(
                (EVT, symbol_short!("mkt_stl")),
                (invoice_id, seller.clone(), buyer.clone(), price),
            );
            Ok(())
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        BytesN, Env, IntoVal,
    };

    mod dummy_share_contract {
        use super::*;

        #[contract]
        pub struct DummyShare;
        #[contractimpl]
        impl DummyShare {
            pub fn decimals(_env: Env) -> u32 {
                EXPECTED_DECIMALS
            }
            pub fn total_supply(env: Env) -> i128 {
                env.storage()
                    .instance()
                    .get(&symbol_short!("tot"))
                    .unwrap_or(0)
            }
            pub fn balance(env: Env, id: Address) -> i128 {
                env.storage().persistent().get(&id).unwrap_or(0)
            }
            pub fn mint(env: Env, to: Address, amount: i128) {
                let t = Self::total_supply(env.clone());
                let b = Self::balance(env.clone(), to.clone());
                env.storage()
                    .instance()
                    .set(&symbol_short!("tot"), &(t + amount));
                env.storage().persistent().set(&to, &(b + amount));
            }
            pub fn burn(env: Env, from: Address, amount: i128) {
                let t = Self::total_supply(env.clone());
                let b = Self::balance(env.clone(), from.clone());
                env.storage()
                    .instance()
                    .set(&symbol_short!("tot"), &(t - amount));
                env.storage().persistent().set(&from, &(b - amount));
            }
        }
    }
    pub use dummy_share_contract::DummyShare;

    mod failing_mint_share_contract {
        use super::*;

        #[contract]
        pub struct FailingMintShare;
        #[contractimpl]
        impl FailingMintShare {
            pub fn total_supply(_env: Env) -> i128 {
                0
            }
            pub fn balance(_env: Env, _id: Address) -> i128 {
                0
            }
            pub fn mint(_env: Env, _to: Address, _amount: i128) {
                panic!("mint failed");
            }
            pub fn burn(_env: Env, _from: Address, _amount: i128) {}
        }
    }
    pub use failing_mint_share_contract::FailingMintShare;

    mod panicking_outgoing_token_contract {
        use super::*;

        #[contract]
        pub struct PanickingOutgoingToken;
        #[contractimpl]
        impl PanickingOutgoingToken {
            pub fn decimals(_env: Env) -> u32 {
                EXPECTED_DECIMALS
            }
            pub fn set_pool(env: Env, pool: Address) {
                env.storage().instance().set(&symbol_short!("pool"), &pool);
            }
            pub fn set_balance(env: Env, to: Address, amount: i128) {
                env.storage().persistent().set(&to, &amount);
            }
            pub fn balance(env: Env, id: Address) -> i128 {
                env.storage().persistent().get(&id).unwrap_or(0)
            }
            pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
                let pool: Address = env
                    .storage()
                    .instance()
                    .get(&symbol_short!("pool"))
                    .expect("pool not set");
                if from == pool {
                    panic!("outgoing transfer failed");
                }
                let from_balance = Self::balance(env.clone(), from.clone());
                let to_balance = Self::balance(env.clone(), to.clone());
                if from_balance < amount {
                    panic!("insufficient balance");
                }
                env.storage()
                    .persistent()
                    .set(&from, &(from_balance - amount));
                env.storage().persistent().set(&to, &(to_balance + amount));
            }
        }
    }
    pub use panicking_outgoing_token_contract::{
        PanickingOutgoingToken, PanickingOutgoingTokenClient,
    };

    mod dummy_credit_score_contract {
        use super::*;

        #[contract]
        pub struct DummyCreditScoreContract;
        #[contractimpl]
        impl DummyCreditScoreContract {
            pub fn get_credit_score(env: Env, sme: Address) -> CreditScoreData {
                CreditScoreData {
                    sme,
                    score: 750,
                    total_invoices: 5,
                    paid_on_time: 5,
                    paid_late: 0,
                    defaulted: 0,
                    total_volume: 1_000_000_000,
                    average_payment_days: 1,
                    last_updated: env.ledger().timestamp(),
                    score_version: 1,
                }
            }
        }
    }
    pub use dummy_credit_score_contract::DummyCreditScoreContract;

    mod dummy_token_6_decimals_contract {
        use super::*;

        // #367: Test token with 6 decimals (non-standard)
        #[contract]
        pub struct DummyToken6Decimals;
        #[contractimpl]
        impl DummyToken6Decimals {
            pub fn decimals(_env: Env) -> u32 {
                6
            }
        }
    }
    pub use dummy_token_6_decimals_contract::DummyToken6Decimals;

    mod dummy_invoice_contract {
        use super::*;

        #[contract]
        pub struct DummyInvoice;
        #[contractimpl]
        impl DummyInvoice {
            pub fn get_authorized_pool(env: Env) -> Address {
                env.storage()
                    .instance()
                    .get(&symbol_short!("pool"))
                    .expect("not initialized")
            }
            pub fn set_pool(env: Env, pool: Address) {
                env.storage().instance().set(&symbol_short!("pool"), &pool);
            }
            pub fn is_invoice_defaulted(env: Env, id: u64) -> bool {
                let stored: Option<bool> =
                    env.storage().persistent().get(&DataKey::FundedInvoice(id));
                stored.unwrap_or(false)
            }
            pub fn set_invoice_defaulted(env: Env, id: u64, defaulted: bool) {
                if defaulted {
                    env.storage()
                        .persistent()
                        .set(&DataKey::FundedInvoice(id), &true);
                } else {
                    env.storage()
                        .persistent()
                        .set(&DataKey::FundedInvoice(id), &false);
                }
            }
            pub fn add_funding(env: Env, id: u64, amount: i128, pool: Address) -> i128 {
                pool.require_auth();
                let funded_so_far: i128 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::FundedInvoice(id))
                    .unwrap_or(0);
                let new_funded = funded_so_far.checked_add(amount).expect("overflow");
                env.storage()
                    .persistent()
                    .set(&DataKey::FundedInvoice(id), &new_funded);
                new_funded
            }
            pub fn get_funded_amount(env: Env, id: u64) -> i128 {
                env.storage()
                    .persistent()
                    .get(&DataKey::FundedInvoice(id))
                    .unwrap_or(0)
            }
        }
    }
    pub use dummy_invoice_contract::{DummyInvoice, DummyInvoiceClient};

    mod mock_reflector_contract {
        use super::*;

        #[contract]
        pub struct MockReflector;
        #[contractimpl]
        impl MockReflector {
            pub fn set_price(env: Env, asset: ReflectorAsset, price: i128, timestamp: u64) {
                let ReflectorAsset::Stellar(token) = asset else {
                    panic!("mock only supports Stellar assets");
                };
                env.storage()
                    .persistent()
                    .set(&token, &ReflectorPriceData { price, timestamp });
            }
            pub fn lastprice(env: Env, asset: ReflectorAsset) -> Option<ReflectorPriceData> {
                let ReflectorAsset::Stellar(token) = asset else {
                    return None;
                };
                env.storage().persistent().get(&token)
            }
        }
    }
    pub use mock_reflector_contract::{MockReflector, MockReflectorClient};

    fn setup(env: &Env) -> (FundingPoolClient<'_>, Address, Address, Address) {
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let contract_id = env.register(FundingPool, ());
        let client = FundingPoolClient::new(env, &contract_id);
        let admin = Address::generate(env);
        let token_admin = Address::generate(env);
        let usdc_id = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let invoice_contract = env.register(DummyInvoice, ());
        DummyInvoiceClient::new(env, &invoice_contract).set_pool(&contract_id);

        let share_token = env.register(DummyShare, ());
        client.initialize(&admin, &usdc_id, &share_token, &invoice_contract);
        // Most unit tests assume a single investor can fully fund the pool.
        // Disable the concentration limit in this test harness.
        client.set_max_investor_concentration(&admin, &10_000u32);
        (client, admin, usdc_id, share_token)
    }

    fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
        soroban_sdk::token::StellarAssetClient::new(env, token_id).mint(to, &amount);
    }

    fn advance_past_operation_delay(env: &Env, client: &FundingPoolClient<'_>) {
        let delay = client.get_operation_delay();
        env.ledger().with_mut(|l| l.timestamp += delay + 1);
    }

    fn propose_and_execute_set_collateral_config(
        env: &Env,
        client: &FundingPoolClient<'_>,
        admin: &Address,
        threshold: i128,
        collateral_bps: u32,
    ) {
        let proposal_id = client.propose_operation(
            admin,
            &AdminOperation::SetCollateralConfig(threshold, collateral_bps),
        );
        advance_past_operation_delay(env, client);
        client.execute_operation(admin, &proposal_id);
    }

    fn propose_and_execute_remove_token(
        env: &Env,
        client: &FundingPoolClient<'_>,
        admin: &Address,
        token: &Address,
    ) {
        let proposal_id =
            client.propose_operation(admin, &AdminOperation::RemoveToken(token.clone()));
        advance_past_operation_delay(env, client);
        client.execute_operation(admin, &proposal_id);
    }

    fn propose_and_execute_seize_collateral(
        env: &Env,
        client: &FundingPoolClient<'_>,
        admin: &Address,
        invoice_id: u64,
    ) {
        let proposal_id =
            client.propose_operation(admin, &AdminOperation::SeizeCollateral(invoice_id));
        advance_past_operation_delay(env, client);
        client.execute_operation(admin, &proposal_id);
    }

    #[test]
    fn test_vault_deposit_withdraw() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, share_token) = setup(&env);
        let investor1 = Address::generate(&env);
        let investor2 = Address::generate(&env);

        mint(&env, &usdc_id, &investor1, 1000);
        mint(&env, &usdc_id, &investor2, 1000);

        client.deposit(&investor1, &usdc_id, &1000, &None);

        let shares1: i128 = env.invoke_contract(
            &share_token,
            &Symbol::new(&env, "balance"),
            soroban_sdk::vec![&env, investor1.clone().into_val(&env)],
        );
        assert_eq!(shares1, 1000);

        let tt = client.get_token_totals(&usdc_id);
        assert_eq!(tt.pool_value, 1000);

        client.deposit(&investor2, &usdc_id, &500, &None);

        let shares2: i128 = env.invoke_contract(
            &share_token,
            &Symbol::new(&env, "balance"),
            soroban_sdk::vec![&env, investor2.clone().into_val(&env)],
        );
        assert_eq!(shares2, 500);

        client.withdraw(&investor1, &usdc_id, &1000);
        let bal = soroban_sdk::token::Client::new(&env, &usdc_id).balance(&investor1);
        assert_eq!(bal, 1000);
    }

    #[test]
    fn test_yield_accumulation() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 10000);
        mint(&env, &usdc_id, &sme, 10000);

        client.deposit(&investor, &usdc_id, &10000, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &5000i128,
            &sme,
            &(env.ledger().timestamp() + 200_000),
            &usdc_id,
        );

        env.ledger().with_mut(|l| l.timestamp += 100_000);
        let amount_due = client.estimate_repayment(&1u64, &None);
        client.repay_invoice(&1u64, &sme, &amount_due);

        let tt = client.get_token_totals(&usdc_id);
        assert!(tt.pool_value > 10000);

        // When investor withdraws their 10000 shares, they should get > 10000 underlying!
        client.withdraw(&investor, &usdc_id, &10000);
        let bal = soroban_sdk::token::Client::new(&env, &usdc_id).balance(&investor);
        assert_eq!(bal, tt.pool_value); // Investor got everything because they owned 100% shares
    }

    #[test]
    fn test_repay_invoice_interest_rounds_half_up() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        let principal: i128 = 12_499;
        mint(&env, &usdc_id, &investor, principal);
        mint(&env, &usdc_id, &sme, principal * 2);

        client.deposit(&investor, &usdc_id, &principal, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + SECS_PER_YEAR),
            &usdc_id,
        );

        env.ledger().with_mut(|l| l.timestamp += SECS_PER_YEAR);

        let expected_interest: i128 = 1_000;
        let total_due = client.estimate_repayment(&1u64, &None);
        assert_eq!(total_due, principal + expected_interest);

        client.repay_invoice(&1u64, &sme, &total_due);

        let tt = client.get_token_totals(&usdc_id);
        assert_eq!(tt.pool_value, principal + expected_interest);
        assert_eq!(tt.total_paid_out, principal + expected_interest);
    }

    #[test]
    fn test_factoring_fee_is_charged_and_tracked_separately() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        let principal: i128 = 1_000_000_000;
        mint(&env, &usdc_id, &investor, principal);
        // sme needs to repay principal + interest + fee
        mint(&env, &usdc_id, &sme, principal * 2);

        // Set factoring fee to 2.5%
        client.set_factoring_fee(&admin, &250);
        client.deposit(&investor, &usdc_id, &principal, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 30 * 86_400),
            &usdc_id,
        );

        let funded = client.get_funded_invoice(&1u64).unwrap();
        let expected_fee = principal * 250 / BPS_DENOM as i128;
        assert_eq!(funded.factoring_fee, expected_fee);

        env.ledger().with_mut(|l| l.timestamp += 30 * 86_400);

        let expected_interest = 6_575_342i128;
        let expected_total_due = principal + expected_interest as i128 + expected_fee;

        assert_eq!(client.estimate_repayment(&1u64, &None), expected_total_due);

        client.repay_invoice(&1u64, &sme, &expected_total_due);

        let tt = client.get_token_totals(&usdc_id);
        assert_eq!(tt.total_fee_revenue, expected_fee);
        assert_eq!(tt.total_paid_out, expected_total_due);
        // pool_value grew by the yield
        assert!(tt.pool_value >= principal);
    }

    // #784: fund an invoice, let its factoring fee accrue as protocol
    // revenue, then withdraw it to the treasury and confirm the treasury's
    // actual on-chain token balance increased (not just internal counters).
    #[test]
    fn test_fund_invoice_then_withdraw_fees_increases_treasury_balance() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);
        let treasury = Address::generate(&env);

        let principal: i128 = 1_000_000_000;
        mint(&env, &usdc_id, &investor, principal);
        mint(&env, &usdc_id, &sme, principal * 2);

        client.set_factoring_fee(&admin, &250); // 2.5%
        client.set_treasury(&admin, &treasury);
        client.deposit(&investor, &usdc_id, &principal, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 30 * 86_400),
            &usdc_id,
        );

        let expected_fee = principal * 250 / BPS_DENOM as i128;
        env.ledger().with_mut(|l| l.timestamp += 30 * 86_400);
        let total_due = client.estimate_repayment(&1u64, &None);
        client.repay_invoice(&1u64, &sme, &total_due);

        // Fee accrued as protocol revenue, separate from investor pool_value.
        assert_eq!(client.get_protocol_revenue(&usdc_id), expected_fee);

        let token_client = soroban_sdk::token::Client::new(&env, &usdc_id);
        assert_eq!(token_client.balance(&treasury), 0);

        client.withdraw_revenue(&admin, &usdc_id, &expected_fee);

        // Treasury's real token balance increased by exactly the fee withdrawn.
        assert_eq!(token_client.balance(&treasury), expected_fee);
        assert_eq!(client.get_protocol_revenue(&usdc_id), 0);
    }

    // ── #773: loyalty bonus APY for long-term depositors ─────────────────────

    #[test]
    fn test_get_deposit_info_defaults_to_tier_one_for_fresh_deposit() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 10_000);
        client.deposit(&investor, &usdc_id, &10_000, &None);

        let info = client.get_deposit_info(&investor, &usdc_id);
        assert_eq!(info.tier, 1);
        assert_eq!(info.bonus_bps, 0);
        assert_eq!(info.days_active, 0);
        assert_eq!(info.next_tier_days, Some(31));
        assert_eq!(info.effective_apy_bps, info.base_apy_bps);
    }

    #[test]
    fn test_loyalty_tier_after_100_days_is_tier_three() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 10_000);
        client.deposit(&investor, &usdc_id, &10_000, &None);

        env.ledger().with_mut(|l| l.timestamp += 100 * SECS_PER_DAY);

        let info = client.get_deposit_info(&investor, &usdc_id);
        // 91-365 days => tier 3, +150 bps per the default ladder.
        assert_eq!(info.tier, 3);
        assert_eq!(info.bonus_bps, 150);
        assert_eq!(info.days_active, 100);
        assert_eq!(info.next_tier_days, Some(366));
        assert_eq!(info.effective_apy_bps, info.base_apy_bps + 150);
    }

    #[test]
    fn test_loyalty_timer_resets_after_full_withdrawal_and_redeposit() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 20_000);
        client.deposit(&investor, &usdc_id, &10_000, &None);

        env.ledger().with_mut(|l| l.timestamp += 100 * SECS_PER_DAY);
        assert_eq!(client.get_deposit_info(&investor, &usdc_id).tier, 3);

        // Fully exit, then re-deposit — tenure should restart from zero.
        // This is the pool's first (and only) deposit for this token, so
        // shares minted == usdc_received == 10_000 exactly.
        client.withdraw(&investor, &usdc_id, &10_000);
        client.deposit(&investor, &usdc_id, &10_000, &None);

        let info = client.get_deposit_info(&investor, &usdc_id);
        assert_eq!(info.tier, 1);
        assert_eq!(info.bonus_bps, 0);
        assert_eq!(info.days_active, 0);
    }

    #[test]
    fn test_set_loyalty_tiers_rejects_bad_configs() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);

        // Empty ladder rejected.
        let empty: Vec<LoyaltyTier> = Vec::new(&env);
        assert_eq!(
            client
                .try_set_loyalty_tiers(&admin, &empty)
                .unwrap_err()
                .unwrap(),
            PoolError::InvalidLoyaltyTiers
        );

        // Non-ascending min_days rejected.
        let mut out_of_order: Vec<LoyaltyTier> = Vec::new(&env);
        out_of_order.push_back(LoyaltyTier {
            min_days: 30,
            bonus_bps: 50,
        });
        out_of_order.push_back(LoyaltyTier {
            min_days: 10,
            bonus_bps: 100,
        });
        assert_eq!(
            client
                .try_set_loyalty_tiers(&admin, &out_of_order)
                .unwrap_err()
                .unwrap(),
            PoolError::InvalidLoyaltyTiers
        );

        // Bonus above the sanity ceiling rejected.
        let mut too_generous: Vec<LoyaltyTier> = Vec::new(&env);
        too_generous.push_back(LoyaltyTier {
            min_days: 0,
            bonus_bps: MAX_LOYALTY_BONUS_BPS + 1,
        });
        assert_eq!(
            client
                .try_set_loyalty_tiers(&admin, &too_generous)
                .unwrap_err()
                .unwrap(),
            PoolError::InvalidLoyaltyTiers
        );

        // A valid custom ladder is accepted and overrides the default.
        let mut custom: Vec<LoyaltyTier> = Vec::new(&env);
        custom.push_back(LoyaltyTier {
            min_days: 0,
            bonus_bps: 0,
        });
        custom.push_back(LoyaltyTier {
            min_days: 7,
            bonus_bps: 1_000,
        });
        client.set_loyalty_tiers(&admin, &custom);
        assert_eq!(client.get_loyalty_tiers(), custom);
    }

    #[test]
    fn test_claim_yield_applies_loyalty_bonus_funded_from_protocol_revenue() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        let principal: i128 = 1_000_000_000;
        mint(&env, &usdc_id, &investor, principal);
        mint(&env, &usdc_id, &sme, principal * 2);

        client.set_factoring_fee(&admin, &250); // 2.5% — funds protocol_revenue
        client.deposit(&investor, &usdc_id, &principal, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 30 * 86_400),
            &usdc_id,
        );

        // Age the position into Tier 3 (+150 bps) before yield is realized.
        env.ledger().with_mut(|l| l.timestamp += 100 * SECS_PER_DAY);
        let total_due = client.estimate_repayment(&1u64, &None);
        client.repay_invoice(&1u64, &sme, &total_due);

        let protocol_revenue_before = client.get_protocol_revenue(&usdc_id);
        assert!(protocol_revenue_before > 0);

        let token_client = soroban_sdk::token::Client::new(&env, &usdc_id);
        let balance_before = token_client.balance(&investor);
        client.claim_yield(&investor, &usdc_id);
        let claimed = token_client.balance(&investor) - balance_before;

        // Bonus was paid out of protocol_revenue, so it dropped by the
        // difference between what was actually claimed and the raw
        // reward_per_share entitlement.
        let protocol_revenue_after = client.get_protocol_revenue(&usdc_id);
        assert!(protocol_revenue_after < protocol_revenue_before);
        assert!(claimed > 0);
    }

    // #799: referral program integration — the pool contract pays a
    // referrer their configured cut of a referee's factoring fee.
    #[test]
    fn test_referral_reward_paid_on_invoice_repayment() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);
        let referrer = Address::generate(&env);

        let referral_id = env.register(referral::ReferralContract, ());
        let referral_client = referral::ReferralContractClient::new(&env, &referral_id);
        referral_client.initialize(&admin, &client.address);
        referral_client.register(&sme, &referrer);
        client.set_referral_registry(&admin, &referral_id);

        let principal: i128 = 1_000_000_000;
        mint(&env, &usdc_id, &investor, principal);
        mint(&env, &usdc_id, &sme, principal * 2);

        // 2.5% factoring fee; default referral borrow reward is 5% of that fee.
        client.set_factoring_fee(&admin, &250);
        client.deposit(&investor, &usdc_id, &principal, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 30 * 86_400),
            &usdc_id,
        );

        let expected_fee = principal * 250 / BPS_DENOM as i128;
        let expected_reward = expected_fee * 500 / BPS_DENOM as i128;

        env.ledger().with_mut(|l| l.timestamp += 30 * 86_400);
        let total_due = client.estimate_repayment(&1u64, &None);
        client.repay_invoice(&1u64, &sme, &total_due);

        // Referrer's reward is credited and actually funded on the referral contract.
        assert_eq!(
            referral_client.get_pending_reward(&referrer, &usdc_id),
            expected_reward
        );
        assert_eq!(referral_client.get_stats(&referrer).referral_count, 1);
        let usdc_client = soroban_sdk::token::Client::new(&env, &usdc_id);
        assert_eq!(usdc_client.balance(&referral_id), expected_reward);

        // The reward is carved out of protocol_revenue, not paid on top of it.
        let tt = client.get_token_totals(&usdc_id);
        assert_eq!(tt.total_fee_revenue, expected_fee);
        assert_eq!(tt.protocol_revenue, expected_fee - expected_reward);

        // Referrer can claim it.
        let claimed = referral_client.claim_rewards(&referrer, &usdc_id);
        assert_eq!(claimed, expected_reward);
        assert_eq!(usdc_client.balance(&referrer), expected_reward);
    }

    #[test]
    fn test_referral_activates_on_first_deposit() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let referrer = Address::generate(&env);

        let referral_id = env.register(referral::ReferralContract, ());
        let referral_client = referral::ReferralContractClient::new(&env, &referral_id);
        referral_client.initialize(&admin, &client.address);
        referral_client.register(&investor, &referrer);
        client.set_referral_registry(&admin, &referral_id);

        mint(&env, &usdc_id, &investor, 10_000);
        client.deposit(&investor, &usdc_id, &1_000i128, &None);

        // No pool-level yield fee exists yet, so the deposit earns no
        // reward — but the referral is still activated/counted.
        assert_eq!(referral_client.get_stats(&referrer).referral_count, 1);
        assert_eq!(referral_client.get_pending_reward(&referrer, &usdc_id), 0);
    }

    #[test]
    fn test_compound_interest_rounds_once_for_days_and_remainder() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        let principal: i128 = 1_000_000_000;
        mint(&env, &usdc_id, &investor, principal);
        mint(&env, &usdc_id, &sme, principal * 2);

        client.set_compound_interest(&admin, &true);
        client.deposit(&investor, &usdc_id, &principal, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 90_000),
            &usdc_id,
        );

        env.ledger().with_mut(|l| l.timestamp += 90_000);

        assert_eq!(client.estimate_repayment(&1u64, &None), 1_000_228_313);
    }

    #[test]
    fn test_fee_tier_resolution_uses_high_credit_score_lower_fee() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        let credit_score_contract = env.register(DummyCreditScoreContract, ());
        client.set_credit_score_contract(&admin, &credit_score_contract);
        client.set_fee_tier(
            &admin,
            &1u32,
            &FeeTier {
                min_amount: 0,
                max_amount: 1_000_000_000_000,
                min_credit_score: 700,
                fee_bps: 100,
            },
        );
        client.set_fee_tier(
            &admin,
            &2u32,
            &FeeTier {
                min_amount: 0,
                max_amount: 1_000_000_000_000,
                min_credit_score: 0,
                fee_bps: 250,
            },
        );

        mint(&env, &usdc_id, &investor, 1_000_000_000);
        mint(&env, &usdc_id, &sme, 2_000_000_000);
        client.deposit(&investor, &usdc_id, &1_000_000_000, &None);

        client.fund_invoice(
            &admin,
            &1u64,
            &500_000_000i128,
            &sme,
            &(env.ledger().timestamp() + 30 * 86_400),
            &usdc_id,
        );

        let funded = client.get_funded_invoice(&1u64).unwrap();
        assert_eq!(
            funded.factoring_fee,
            500_000_000i128 * 100 / BPS_DENOM as i128
        );
    }

    #[test]
    fn test_fee_tier_crud_and_list() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);

        let tier = FeeTier {
            min_amount: 0,
            max_amount: 100_000,
            min_credit_score: 500,
            fee_bps: 150,
        };
        client.set_fee_tier(&admin, &1u32, &tier);
        let stored = client.get_fee_tier(&1u32).expect("tier exists");
        assert_eq!(stored.fee_bps, 150);

        let list = client.list_fee_tiers();
        assert_eq!(list.len(), 1);
        assert_eq!(list.get(0).unwrap().0, 1u32);

        client.remove_fee_tier(&admin, &1u32);
        assert!(client.get_fee_tier(&1u32).is_none());
    }

    // ---- Issue #61: Edge-Case Tests ----

    #[test]
    fn test_deposit_zero_amount_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let result = client.try_deposit(&investor, &usdc_id, &0i128, &None);
        assert_eq!(result, Err(Ok(PoolError::ZeroAmount)));
    }

    #[test]
    fn test_deposit_negative_amount_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let result = client.try_deposit(&investor, &usdc_id, &-100i128, &None);
        assert_eq!(result, Err(Ok(PoolError::NegativeAmount)));
    }

    #[test]
    fn test_deposit_non_whitelisted_token_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let unknown_token = Address::generate(&env);
        let result = client.try_deposit(&investor, &unknown_token, &1_000i128, &None);
        assert_eq!(result, Err(Ok(PoolError::TokenNotAccepted)));
    }

    #[test]
    fn test_get_pool_balance_reflects_direct_token_transfer() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);

        // Sanity: nothing deposited yet.
        assert_eq!(client.get_pool_balance(&usdc_id), 0);
        assert_eq!(client.get_token_totals(&usdc_id).pool_value, 0);

        // Tokens sent directly to the pool contract address, bypassing
        // deposit() entirely (e.g. a donation or accidental transfer).
        mint(&env, &usdc_id, &client.address, 5_000i128);

        // get_pool_balance() must reflect the live token balance immediately,
        // even though the internal `pool_value` counter (deposited/deployed
        // capital accounting) never saw this transfer since deposit() was
        // never called.
        assert_eq!(client.get_pool_balance(&usdc_id), 5_000i128);
        assert_eq!(client.get_token_totals(&usdc_id).pool_value, 0);
    }

    #[test]
    fn test_get_asset_price_uses_fresh_reflector_price() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);

        let oracle_id = env.register(MockReflector, ());
        let oracle_client = MockReflectorClient::new(&env, &oracle_id);
        client.set_oracle_contract(&admin, &oracle_id);

        let now = env.ledger().timestamp();
        oracle_client.set_price(
            &ReflectorAsset::Stellar(usdc_id.clone()),
            &1_000_000i128,
            &now,
        );

        assert_eq!(client.get_asset_price(&usdc_id), 1_000_000i128);
    }

    #[test]
    fn test_get_asset_price_falls_back_when_reflector_price_is_stale() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);

        let oracle_id = env.register(MockReflector, ());
        let oracle_client = MockReflectorClient::new(&env, &oracle_id);
        client.set_oracle_contract(&admin, &oracle_id);
        client.set_fallback_price(&admin, &usdc_id, &500_000i128);

        // Default staleness threshold is 3600s; this price is 7200s old.
        let now = env.ledger().timestamp();
        oracle_client.set_price(
            &ReflectorAsset::Stellar(usdc_id.clone()),
            &1_000_000i128,
            &now.saturating_sub(7_200),
        );

        assert_eq!(client.get_asset_price(&usdc_id), 500_000i128);
    }

    #[test]
    fn test_get_asset_price_falls_back_when_oracle_unconfigured() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);

        client.set_fallback_price(&admin, &usdc_id, &250_000i128);

        assert_eq!(client.get_asset_price(&usdc_id), 250_000i128);
    }

    #[test]
    fn test_get_asset_price_errors_when_no_price_available() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);

        let result = client.try_get_asset_price(&usdc_id);
        assert_eq!(result, Err(Ok(PoolError::OraclePriceUnavailable)));
    }

    // #1036: multi-asset, oracle-priced collateral (funding-gate side; the
    // risk-response side — check_collateral_risk/liquidate_collateral/
    // get_live_collateral_ratio — lives in contracts/auction, see its tests).

    // Registers a second accepted token (`xlm_id`) distinct from the pool's
    // usdc funding token — the fixture shared by the cross-asset tests below.
    // The cross-asset ratio check reads prices via pool's own
    // get_asset_price (fallback prices set directly here, no oracle
    // contract registered), the same path the risk-response satellite in
    // contracts/auction reads for its own ongoing monitoring.
    fn setup_cross_asset_collateral(
        env: &Env,
    ) -> (FundingPoolClient<'_>, Address, Address, Address) {
        let (client, admin, usdc_id, share_token) = setup(env);

        let xlm_admin = Address::generate(env);
        let xlm_id = env.register_stellar_asset_contract_v2(xlm_admin).address();
        client.add_token(&admin, &xlm_id, &share_token);

        (client, admin, usdc_id, xlm_id)
    }

    #[test]
    fn test_cross_asset_collateral_ratio_uses_oracle_prices() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, xlm_id) = setup_cross_asset_collateral(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        // threshold=0 => collateral required on every principal; 20% collateral_bps.
        propose_and_execute_set_collateral_config(&env, &client, &admin, 0i128, 2_000u32);

        let principal: i128 = 10_000;
        let required = client.required_collateral_for(&principal); // 2,000 usdc-equivalent

        client.set_fallback_price(&admin, &usdc_id, &1_000_000i128);
        // xlm priced at half of usdc: 4,000 xlm covers exactly a 2,000-usdc-value
        // requirement (4,000 * 500,000 == 2,000 * 1,000,000).
        client.set_fallback_price(&admin, &xlm_id, &500_000i128);
        let xlm_amount = required * 2;

        mint(&env, &usdc_id, &investor, 20_000);
        mint(&env, &xlm_id, &sme, xlm_amount);
        client.deposit(&investor, &usdc_id, &20_000, &None);

        client.deposit_collateral(&1u64, &sme, &xlm_id, &xlm_amount);

        // Funding succeeds even though the deposited token differs from the
        // invoice's funding token: fund_invoice computes the ratio inline via
        // get_asset_price, and it covers the requirement exactly.
        let now = env.ledger().timestamp();
        client.fund_invoice(&admin, &1u64, &principal, &sme, &(now + 10_000), &usdc_id);

        assert!(client.get_funded_invoice(&1u64).is_some());
    }

    #[test]
    fn test_cross_asset_collateral_insufficient_value_rejected_at_funding() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, xlm_id) = setup_cross_asset_collateral(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        propose_and_execute_set_collateral_config(&env, &client, &admin, 0i128, 2_000u32);

        let principal: i128 = 10_000;
        client.set_fallback_price(&admin, &usdc_id, &1_000_000i128);
        client.set_fallback_price(&admin, &xlm_id, &500_000i128);

        // Only half the xlm needed to cover the 2,000-usdc-value requirement.
        let xlm_amount = client.required_collateral_for(&principal); // 2,000, not 4,000

        mint(&env, &usdc_id, &investor, 20_000);
        mint(&env, &xlm_id, &sme, xlm_amount);
        client.deposit(&investor, &usdc_id, &20_000, &None);
        client.deposit_collateral(&1u64, &sme, &xlm_id, &xlm_amount);

        let now = env.ledger().timestamp();
        let result =
            client.try_fund_invoice(&admin, &1u64, &principal, &sme, &(now + 10_000), &usdc_id);
        assert_eq!(result, Err(Ok(PoolError::InvalidAmount)));
    }

    #[test]
    fn test_deposit_mint_failure_does_not_take_stablecoin() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let contract_id = env.register(FundingPool, ());
        let client = FundingPoolClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let usdc_id = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let invoice_contract = env.register(DummyInvoice, ());
        DummyInvoiceClient::new(&env, &invoice_contract).set_pool(&contract_id);
        let failing_share = env.register(FailingMintShare, ());
        client.initialize(&admin, &usdc_id, &failing_share, &invoice_contract);
        client.set_max_investor_concentration(&admin, &10_000u32);

        let investor = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 1_000);
        let token_client = token::Client::new(&env, &usdc_id);
        let balance_before = token_client.balance(&investor);

        let result = client.try_deposit(&investor, &usdc_id, &1_000i128, &None);
        assert!(result.is_err());
        assert_eq!(token_client.balance(&investor), balance_before);
        assert_eq!(client.get_token_totals(&usdc_id).pool_value, 0);
    }

    #[test]
    fn test_withdraw_zero_shares_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 1_000);
        client.deposit(&investor, &usdc_id, &1_000, &None);
        let result = client.try_withdraw(&investor, &usdc_id, &0i128);
        assert_eq!(result, Err(Ok(PoolError::InvalidAmount)));
    }

    #[test]
    fn test_withdraw_more_than_balance_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 500);
        client.deposit(&investor, &usdc_id, &500, &None);
        // Attempt to withdraw more shares than owned
        let result = client.try_withdraw(&investor, &usdc_id, &1_000i128);
        assert_eq!(result, Err(Ok(PoolError::InvalidAmount)));
    }

    #[test]
    fn test_withdraw_insufficient_liquidity_rejected_before_burning_shares() {
        // #782: 90% of the pool deployed into a funded invoice, leaving only
        // 10% liquid. An investor's shares are worth the full deposit, but
        // the pool cannot actually pay out that much — withdraw() must
        // reject with InsufficientLiquidity *before* burning any shares,
        // not fail at the token transfer after accounting already changed.
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 1_000);
        client.deposit(&investor, &usdc_id, &1_000, &None);

        client.fund_invoice(
            &admin,
            &1u64,
            &900i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        assert_eq!(client.available_liquidity(&usdc_id), 100);

        let share_client = token::Client::new(&env, &share_token);
        let shares_before = share_client.balance(&investor);

        let result = client.try_withdraw(&investor, &usdc_id, &shares_before);
        assert_eq!(result, Err(Ok(PoolError::InsufficientLiquidity)));

        // Shares must NOT be burned and pool_value must be untouched.
        assert_eq!(share_client.balance(&investor), shares_before);
        assert_eq!(client.get_token_totals(&usdc_id).pool_value, 1_000);

        // A withdrawal that fits within the 100 still-liquid units succeeds.
        client.withdraw(&investor, &usdc_id, &100);
        assert_eq!(share_client.balance(&investor), shares_before - 100);
    }

    #[test]
    fn test_withdraw_transfer_failure_rolls_back_position() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let contract_id = env.register(FundingPool, ());
        let client = FundingPoolClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let token = env.register(PanickingOutgoingToken, ());
        let token_client = PanickingOutgoingTokenClient::new(&env, &token);
        token_client.set_pool(&contract_id);
        let invoice_contract = env.register(DummyInvoice, ());
        DummyInvoiceClient::new(&env, &invoice_contract).set_pool(&contract_id);
        let share_token = env.register(DummyShare, ());
        client.initialize(&admin, &token, &share_token, &invoice_contract);
        client.set_max_investor_concentration(&admin, &10_000u32);

        let investor = Address::generate(&env);
        token_client.set_balance(&investor, &1_000);
        client.deposit(&investor, &token, &1_000, &None);

        let shares_before: i128 = env.invoke_contract(
            &share_token,
            &Symbol::new(&env, "balance"),
            soroban_sdk::vec![&env, investor.clone().into_val(&env)],
        );
        let totals_before = client.get_token_totals(&token);
        let pool_balance_before = token_client.balance(&contract_id);

        let result = client.try_withdraw(&investor, &token, &500i128);
        assert!(result.is_err());
        let shares_after: i128 = env.invoke_contract(
            &share_token,
            &Symbol::new(&env, "balance"),
            soroban_sdk::vec![&env, investor.clone().into_val(&env)],
        );
        let totals_after = client.get_token_totals(&token);
        assert_eq!(shares_after, shares_before);
        assert_eq!(totals_after.pool_value, totals_before.pool_value);
        assert_eq!(token_client.balance(&contract_id), pool_balance_before);
        assert_eq!(token_client.balance(&investor), 0);
    }

    #[test]
    fn test_fund_invoice_zero_principal_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let sme = Address::generate(&env);
        let result = client.try_fund_invoice(
            &admin,
            &1u64,
            &0i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        assert_eq!(result, Err(Ok(PoolError::InvalidAmount)));
    }

    #[test]
    fn test_fund_invoice_insufficient_liquidity_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 500);
        client.deposit(&investor, &usdc_id, &500, &None);
        // Try to fund more than available in pool
        let result = client.try_fund_invoice(
            &admin,
            &1u64,
            &1_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        assert_eq!(result, Err(Ok(PoolError::InsufficientLiquidity)));
    }

    #[test]
    fn test_fund_invoice_transfer_failure_rolls_back_storage() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let contract_id = env.register(FundingPool, ());
        let client = FundingPoolClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let token = env.register(PanickingOutgoingToken, ());
        let token_client = PanickingOutgoingTokenClient::new(&env, &token);
        token_client.set_pool(&contract_id);
        let invoice_contract = env.register(DummyInvoice, ());
        DummyInvoiceClient::new(&env, &invoice_contract).set_pool(&contract_id);
        let share_token = env.register(DummyShare, ());
        client.initialize(&admin, &token, &share_token, &invoice_contract);
        client.set_max_investor_concentration(&admin, &10_000u32);

        let investor = Address::generate(&env);
        let sme = Address::generate(&env);
        token_client.set_balance(&investor, &2_000);
        client.deposit(&investor, &token, &2_000, &None);

        let result = client.try_fund_invoice(
            &admin,
            &1u64,
            &1_000i128,
            &sme,
            &(env.ledger().timestamp() + 86_400),
            &token,
        );
        assert!(result.is_err());
        assert!(client.get_funded_invoice(&1u64).is_none());
        assert_eq!(client.get_token_totals(&token).total_deployed, 0);
        assert_eq!(client.get_storage_stats().active_funded_invoices, 0);
        assert_eq!(token_client.balance(&contract_id), 2_000);
        assert_eq!(token_client.balance(&sme), 0);
    }

    #[test]
    fn test_fund_invoice_prioritizes_liquidity_before_token_validation() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let sme = Address::generate(&env);
        let unknown_token = Address::generate(&env);

        let result = client.try_fund_invoice(
            &admin,
            &1u64,
            &1_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &unknown_token,
        );
        assert_eq!(result, Err(Ok(PoolError::InsufficientLiquidity)));
    }

    #[test]
    fn test_fund_invoice_returns_token_not_accepted_when_liquidity_exists() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let sme = Address::generate(&env);
        let unknown_token = Address::generate(&env);
        env.as_contract(&client.address, || {
            env.storage().instance().set(
                &DataKey::TokenTotals(unknown_token.clone()),
                &PoolTokenTotals {
                    pool_value: 5_000,
                    total_deployed: 0,
                    total_paid_out: 0,
                    total_fee_revenue: 0,
                    reward_per_share: 0,
                    protocol_revenue: 0,
                },
            );
        });

        let result = client.try_fund_invoice(
            &admin,
            &1u64,
            &1_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &unknown_token,
        );
        assert_eq!(result, Err(Ok(PoolError::TokenNotAccepted)));
    }

    #[test]
    fn test_fund_invoice_partial_accumulates_principal() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 2_000);
        client.deposit(&investor, &usdc_id, &2_000, &None);
        // First funding
        client.fund_invoice(
            &admin,
            &1u64,
            &300i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        // Second funding on same invoice — partial funding, must accumulate
        let result = client.try_fund_invoice(
            &admin,
            &1u64,
            &200i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        assert!(result.is_ok());
        // Verify the accumulated principal
        let record = client.get_funded_invoice(&1u64).unwrap();
        assert_eq!(record.principal, 500);
    }

    #[test]
    fn test_double_repay_invoice_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 1_000);
        mint(&env, &usdc_id, &sme, 2_000);
        client.deposit(&investor, &usdc_id, &1_000, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &1_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        let amount_due = client.estimate_repayment(&1u64, &None);
        client.repay_invoice(&1u64, &sme, &amount_due);
        // Second repay must return AlreadyFullyRepaid
        let result = client.try_repay_invoice(&1u64, &sme, &amount_due);
        assert_eq!(result, Err(Ok(PoolError::AlreadyFullyRepaid)));
    }

    #[test]
    fn test_fund_invoice_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let sme = Address::generate(&env);
        let attacker = Address::generate(&env);
        let result = client.try_fund_invoice(
            &attacker,
            &1u64,
            &100i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
    }

    #[test]
    fn test_set_yield_above_50_percent_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let result = client.try_propose_yield_change(&admin, &5_001u32);
        assert_eq!(result, Err(Ok(PoolError::InvalidAmount)));
    }

    #[test]
    fn test_set_yield_at_boundary_50_percent() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        // Allow a large one-time step so we can test the 50% ceiling independently.
        client.set_yield_change_policy(&admin, &1u64, &5_000u32, &3_600u64);
        env.ledger()
            .with_mut(|l| l.timestamp += DEFAULT_YIELD_CHANGE_COOLDOWN_SECS);
        client.propose_yield_change(&admin, &5_000u32);
        env.ledger().with_mut(|l| l.timestamp += 3_601u64);
        client.execute_yield_change();
        assert_eq!(client.get_config().yield_bps, 5_000);
    }

    #[test]
    fn test_set_yield_cooldown_enforced() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);

        // setup() sets timestamp; first change must wait out cooldown
        env.ledger()
            .with_mut(|l| l.timestamp += DEFAULT_YIELD_CHANGE_COOLDOWN_SECS);
        client.propose_yield_change(&admin, &900u32);
        env.ledger()
            .with_mut(|l| l.timestamp += DEFAULT_YIELD_TIMELOCK_SECS);
        client.execute_yield_change();

        // immediate second change should fail
        let result = client.try_propose_yield_change(&admin, &950u32);
        assert_eq!(result, Err(Ok(PoolError::InvalidAmount)));
    }

    #[test]
    fn test_set_yield_max_step_enforced() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);

        env.ledger()
            .with_mut(|l| l.timestamp += DEFAULT_YIELD_CHANGE_COOLDOWN_SECS);
        // DEFAULT_YIELD_BPS = 800, max step = 200 => delta 301 should fail
        let result = client.try_propose_yield_change(&admin, &1_101u32);
        assert_eq!(result, Err(Ok(PoolError::InvalidAmount)));
    }

    #[test]
    fn test_add_token_and_remove_unused() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let token_admin2 = Address::generate(&env);
        let new_token = env
            .register_stellar_asset_contract_v2(token_admin2)
            .address();
        let new_share = env.register(DummyShare, ());
        client.add_token(&admin, &new_token, &new_share);
        let tokens = client.accepted_tokens();
        assert_eq!(tokens.len(), 2);
        propose_and_execute_remove_token(&env, &client, &admin, &new_token);
        let tokens = client.accepted_tokens();
        assert_eq!(tokens.len(), 1);
    }

    #[test]
    fn test_remove_token_with_balance_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 1_000);
        client.deposit(&investor, &usdc_id, &1_000, &None);
        // pool has a non-zero balance — token removal must fail at execute time
        let proposal_id =
            client.propose_operation(&admin, &AdminOperation::RemoveToken(usdc_id.clone()));
        advance_past_operation_delay(&env, &client);
        let result = client.try_execute_operation(&admin, &proposal_id);
        assert_eq!(result, Err(Ok(PoolError::TokenHasActiveBalances)));
    }

    // ---- #222: Pool Token Removal Safety Checks Tests ----

    #[test]
    fn test_remove_token_zero_balances_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);

        // Add a second token to test removal
        let token_admin2 = Address::generate(&env);
        let new_token = env
            .register_stellar_asset_contract_v2(token_admin2)
            .address();
        let new_share = env.register(DummyShare, ());
        client.add_token(&admin, &new_token, &new_share);

        // Verify token was added
        let tokens = client.accepted_tokens();
        assert_eq!(tokens.len(), 2);

        // Remove token with zero balances should succeed
        propose_and_execute_remove_token(&env, &client, &admin, &new_token);

        // Verify token was removed
        let tokens_after = client.accepted_tokens();
        assert_eq!(tokens_after.len(), 1);

        // Verify the removed token is not in the list
        let mut found = false;
        for i in 0..tokens_after.len() {
            if tokens_after.get(i).unwrap() == new_token {
                found = true;
                break;
            }
        }
        assert!(!found);
    }

    #[test]
    fn test_remove_token_deposited_balance_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);

        // Add a second token
        let token_admin2 = Address::generate(&env);
        let new_token = env
            .register_stellar_asset_contract_v2(token_admin2)
            .address();
        let new_share = env.register(DummyShare, ());
        client.add_token(&admin, &new_token, &new_share);

        // Deposit into the new token to create non-zero balance
        let investor = Address::generate(&env);
        mint(&env, &new_token, &investor, 1_000);
        client.deposit(&investor, &new_token, &1_000, &None);

        // Attempt to remove token with deposited balance should fail at execute time
        let proposal_id =
            client.propose_operation(&admin, &AdminOperation::RemoveToken(new_token.clone()));
        advance_past_operation_delay(&env, &client);
        let result = client.try_execute_operation(&admin, &proposal_id);
        assert_eq!(result, Err(Ok(PoolError::TokenHasActiveBalances)));

        // Verify token is still in accepted list
        let tokens = client.accepted_tokens();
        assert_eq!(tokens.len(), 2);
    }

    #[test]
    fn test_remove_token_deployed_capital_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);

        // Add a second token
        let token_admin2 = Address::generate(&env);
        let new_token = env
            .register_stellar_asset_contract_v2(token_admin2)
            .address();
        let new_share = env.register(DummyShare, ());
        client.add_token(&admin, &new_token, &new_share);

        // Setup: deposit, fund invoice (deployed > 0)
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);
        mint(&env, &new_token, &investor, 2_000);
        mint(&env, &new_token, &sme, 1_000);

        client.deposit(&investor, &new_token, &2_000, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &1_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &new_token,
        );

        // Verify state: total_deployed > 0
        let tt = client.get_token_totals(&new_token);
        assert!(tt.total_deployed > 0);

        // Attempt to remove token with deployed capital should fail at execute time
        let proposal_id =
            client.propose_operation(&admin, &AdminOperation::RemoveToken(new_token.clone()));
        advance_past_operation_delay(&env, &client);
        let result = client.try_execute_operation(&admin, &proposal_id);
        assert_eq!(result, Err(Ok(PoolError::TokenHasDeployedCapital)));

        // Verify token is still in accepted list
        let tokens = client.accepted_tokens();
        assert_eq!(tokens.len(), 2);
    }

    #[test]
    fn test_remove_token_unauthorized_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);

        // Add a second token
        let token_admin2 = Address::generate(&env);
        let new_token = env
            .register_stellar_asset_contract_v2(token_admin2)
            .address();
        let new_share = env.register(DummyShare, ());
        client.add_token(&admin, &new_token, &new_share);

        // Non-admin attempts to remove token
        let attacker = Address::generate(&env);
        let result = client.try_remove_token(&attacker, &new_token);
        assert_eq!(result, Err(Ok(PoolError::Unauthorized)));

        // Verify token is still in accepted list
        let tokens = client.accepted_tokens();
        assert_eq!(tokens.len(), 2);
    }

    // ---- Collateral Tests ----

    #[test]
    fn test_default_collateral_config() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let cfg = client.get_collateral_config();
        assert_eq!(cfg.threshold, DEFAULT_COLLATERAL_THRESHOLD);
        assert_eq!(cfg.collateral_bps, DEFAULT_COLLATERAL_BPS);
    }

    #[test]
    fn test_set_collateral_config() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        // Set threshold to 5000 USDC, 10% collateral
        propose_and_execute_set_collateral_config(
            &env,
            &client,
            &admin,
            50_000_000_000i128,
            1_000u32,
        );
        let cfg = client.get_collateral_config();
        assert_eq!(cfg.threshold, 50_000_000_000i128);
        assert_eq!(cfg.collateral_bps, 1_000u32);
    }

    #[test]
    fn test_set_collateral_config_over_100_percent_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        // #742: invalid params are rejected at propose time, not direct-call time.
        let result = client.try_propose_operation(
            &admin,
            &AdminOperation::SetCollateralConfig(1_000i128, 10_001u32),
        );
        assert_eq!(result, Err(Ok(PoolError::InvalidCollateralBps)));
    }

    #[test]
    fn test_set_collateral_config_zero_collateral_bps_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let result = client.try_propose_operation(
            &admin,
            &AdminOperation::SetCollateralConfig(1_000i128, 0u32),
        );
        assert_eq!(result, Err(Ok(PoolError::InvalidCollateralBps)));
    }

    #[test]
    fn test_required_collateral_below_threshold_is_zero() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        // Default threshold is 100_000_000_000 (10,000 USDC); 1000 USDC is below it
        let req = client.required_collateral_for(&1_000_000_000i128);
        assert_eq!(req, 0);
    }

    #[test]
    fn test_required_collateral_above_threshold() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        // Lower threshold to 500 USDC, 20% collateral
        propose_and_execute_set_collateral_config(
            &env,
            &client,
            &admin,
            5_000_000_000i128,
            2_000u32,
        );
        // 1000 USDC principal → 200 USDC collateral
        let req = client.required_collateral_for(&10_000_000_000i128);
        assert_eq!(req, 2_000_000_000i128); // 20% of 10,000 USDC
    }

    #[test]
    fn test_low_value_invoice_funded_without_collateral() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 5_000);
        mint(&env, &usdc_id, &sme, 5_000);
        client.deposit(&investor, &usdc_id, &5_000, &None);

        // Principal (5000) is well below default threshold (100_000_000_000)
        // so no collateral needed
        client.fund_invoice(
            &admin,
            &1u64,
            &5_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        let fi = client.get_funded_invoice(&1u64).unwrap();
        assert_eq!(fi.repaid_amount, 0i128);
    }

    #[test]
    fn test_high_value_invoice_requires_collateral() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        // Lower threshold so our test amounts trigger it
        propose_and_execute_set_collateral_config(&env, &client, &admin, 1_000i128, 2_000u32);

        mint(&env, &usdc_id, &investor, 10_000);
        client.deposit(&investor, &usdc_id, &10_000, &None);

        // Try to fund without depositing collateral first — must return CollateralNotFound
        let result = client.try_fund_invoice(
            &admin,
            &1u64,
            &5_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        assert_eq!(result, Err(Ok(PoolError::CollateralNotFound)));
    }

    #[test]
    fn test_deposit_collateral_and_fund_high_value_invoice() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        // Threshold = 1000, 20% collateral
        propose_and_execute_set_collateral_config(&env, &client, &admin, 1_000i128, 2_000u32);

        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal); // 1000
        assert_eq!(required, 1_000);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, required);

        client.deposit(&investor, &usdc_id, &10_000, &None);

        // SME deposits collateral
        client.deposit_collateral(&1u64, &sme, &usdc_id, &required);

        let col = client.get_collateral_deposit(&1u64).unwrap();
        assert_eq!(col.amount, required);
        assert!(!col.settled);
        assert_eq!(col.posted_at, env.ledger().timestamp());
        assert_eq!(col.released_at, 0);
        assert_eq!(col.seized_at, 0);

        // Now funding should succeed
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        let fi = client.get_funded_invoice(&1u64).unwrap();
        assert_eq!(fi.repaid_amount, 0i128);
    }

    #[test]
    fn test_borrower_can_top_up_active_collateral() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);
        let principal = 5_000i128;
        let initial_collateral = 1_000i128;
        let top_up = 500i128;

        propose_and_execute_set_collateral_config(&env, &client, &admin, 1_000, 2_000);
        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, initial_collateral + top_up);
        client.deposit(&investor, &usdc_id, &10_000, &None);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &initial_collateral);
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );

        client.top_up_collateral(&1u64, &sme, &usdc_id, &top_up);

        let collateral = client.get_collateral_deposit(&1u64).unwrap();
        assert_eq!(collateral.amount, initial_collateral + top_up);
        assert!(!collateral.settled);
    }

    #[test]
    fn test_collateral_returned_on_repayment() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        propose_and_execute_set_collateral_config(&env, &client, &admin, 1_000i128, 2_000u32);

        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, principal * 2 + required);

        client.deposit(&investor, &usdc_id, &10_000, &None);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &required);
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );

        let sme_balance_before = token::Client::new(&env, &usdc_id).balance(&sme);

        let amount_due = client.estimate_repayment(&1u64, &None);
        client.repay_invoice(&1u64, &sme, &amount_due);

        let sme_balance_after = token::Client::new(&env, &usdc_id).balance(&sme);
        // SME should have gotten collateral back (minus repayment cost)
        // sme_balance_after = sme_balance_before - total_due + collateral_returned
        let col = client.get_collateral_deposit(&1u64).unwrap();
        assert!(col.settled);
        assert_eq!(col.released_at, env.ledger().timestamp());
        assert_eq!(col.seized_at, 0);
        // Net: sme paid total_due but got collateral back
        assert!(sme_balance_after > sme_balance_before - principal);
    }

    // #379: CollateralDeposit TTL extended on settlement so records survive
    // at least 90 days after the invoice is closed.
    #[test]
    fn test_collateral_record_exists_after_repayment() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        propose_and_execute_set_collateral_config(&env, &client, &admin, 1_000i128, 2_000u32);
        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, principal * 2 + required);

        client.deposit(&investor, &usdc_id, &10_000, &None);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &required);
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );

        let amount_due = client.estimate_repayment(&1u64, &None);
        client.repay_invoice(&1u64, &sme, &amount_due);

        // Record must still be queryable after settlement (TTL was extended)
        let col = client.get_collateral_deposit(&1u64);
        assert!(
            col.is_some(),
            "collateral record must exist after repayment"
        );
        assert!(col.unwrap().settled);
    }

    #[test]
    fn test_collateral_record_exists_after_seizure() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);
        let invoice_contract = client.get_config().invoice_contract;

        propose_and_execute_set_collateral_config(&env, &client, &admin, 1_000i128, 2_000u32);
        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, required);

        client.deposit(&investor, &usdc_id, &10_000, &None);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &required);
        let due_date = env.ledger().timestamp() + 1_000;
        client.fund_invoice(&admin, &1u64, &principal, &sme, &due_date, &usdc_id);

        // Mark invoice defaulted via dummy invoice contract
        DummyInvoiceClient::new(&env, &invoice_contract).set_invoice_defaulted(&1u64, &true);

        propose_and_execute_seize_collateral(&env, &client, &admin, 1u64);

        // Record must still be queryable after seizure (90-day TTL applied)
        let col = client.get_collateral_deposit(&1u64);
        assert!(col.is_some(), "collateral record must exist after seizure");
        assert!(col.unwrap().settled);
    }

    #[test]
    fn test_estimate_repayment_respects_updated_due_date() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);
        let invoice_contract = client.get_config().invoice_contract;

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, 20_000);
        client.deposit(&investor, &usdc_id, &10_000, &None);

        let initial_due_date = env.ledger().timestamp() + SECS_PER_DAY;
        client.fund_invoice(&admin, &1u64, &5_000i128, &sme, &initial_due_date, &usdc_id);

        env.ledger()
            .with_mut(|l| l.timestamp = initial_due_date + (5 * SECS_PER_DAY));
        let capped_amount = client.estimate_repayment(&1u64, &None);

        let extended_due_date = initial_due_date + (10 * SECS_PER_DAY);
        client.update_invoice_due_date(&invoice_contract, &1u64, &extended_due_date);
        let extended_amount = client.estimate_repayment(&1u64, &None);

        assert!(extended_amount > capped_amount);
    }

    #[test]
    fn test_double_deposit_collateral_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let sme = Address::generate(&env);

        propose_and_execute_set_collateral_config(&env, &client, &admin, 1_000i128, 2_000u32);
        mint(&env, &usdc_id, &sme, 5_000);

        client.deposit_collateral(&1u64, &sme, &usdc_id, &1_000);
        let result = client.try_deposit_collateral(&1u64, &sme, &usdc_id, &1_000);
        assert_eq!(result, Err(Ok(PoolError::StorageCorrupted)));
    }

    // #791: a borrower must not be able to post an arbitrary, non-whitelisted
    // token as collateral (e.g. a worthless token they mint themselves) to
    // inflate their apparent collateral ratio. `deposit_collateral` already
    // calls `assert_accepted_token`, but that guard had no dedicated
    // regression test — this locks the behaviour in.
    #[test]
    fn test_deposit_collateral_rejects_unsupported_token() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let sme = Address::generate(&env);

        propose_and_execute_set_collateral_config(&env, &client, &admin, 1_000i128, 2_000u32);

        // Attacker deploys their own worthless token — never registered via add_token.
        let attacker_token_admin = Address::generate(&env);
        let worthless_token = env
            .register_stellar_asset_contract_v2(attacker_token_admin)
            .address();
        mint(&env, &worthless_token, &sme, 1_000_000);

        let result = client.try_deposit_collateral(&1u64, &sme, &worthless_token, &1_000);
        assert_eq!(result, Err(Ok(PoolError::TokenNotAccepted)));

        // No collateral record should have been created for the rejected deposit.
        assert!(client.get_collateral_deposit(&1u64).is_none());
    }

    #[test]
    fn test_insufficient_collateral_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        // 20% collateral required on anything >= 1000
        propose_and_execute_set_collateral_config(&env, &client, &admin, 1_000i128, 2_000u32);

        let principal: i128 = 5_000;
        // Required = 1000, but we only deposit 500
        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, 500);

        client.deposit(&investor, &usdc_id, &10_000, &None);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &500);

        let result = client.try_fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        assert_eq!(result, Err(Ok(PoolError::InvalidAmount)));
    }

    #[test]
    fn test_seize_collateral_after_repayment_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        propose_and_execute_set_collateral_config(&env, &client, &admin, 1_000i128, 2_000u32);
        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, principal * 2 + required);

        client.deposit(&investor, &usdc_id, &10_000, &None);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &required);
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        let amount_due = client.estimate_repayment(&1u64, &None);
        client.repay_invoice(&1u64, &sme, &amount_due);

        // Trying to seize after repayment must return AlreadyFullyRepaid at execute time
        let proposal_id = client.propose_operation(&admin, &AdminOperation::SeizeCollateral(1u64));
        advance_past_operation_delay(&env, &client);
        let result = client.try_execute_operation(&admin, &proposal_id);
        assert_eq!(result, Err(Ok(PoolError::AlreadyFullyRepaid)));
    }

    // ---- Issue #105: Comprehensive Access Control Tests ----

    // --- Admin-gated function guards ---

    #[test]
    fn test_pause_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let attacker = Address::generate(&env);
        let result = client.try_pause(&attacker);
        assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
    }

    #[test]
    fn test_unpause_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        client.pause(&admin);
        let attacker = Address::generate(&env);
        let result = client.try_unpause(&attacker);
        assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
    }

    #[test]
    fn test_add_token_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let attacker = Address::generate(&env);
        let ta = Address::generate(&env);
        let new_token = env.register_stellar_asset_contract_v2(ta).address();
        let new_share = env.register(DummyShare, ());
        let result = client.try_add_token(&attacker, &new_token, &new_share);
        assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
    }

    #[test]
    fn test_add_token_rejects_non_standard_decimals() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let new_token = env.register(DummyToken6Decimals, ());
        let new_share = env.register(DummyShare, ());
        let result = client.try_add_token(&admin, &new_token, &new_share);
        assert_eq!(result, Err(Ok(PoolError::UnsupportedTokenDecimals)));
    }

    #[test]
    fn test_remove_token_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let ta2 = Address::generate(&env);
        let new_token = env.register_stellar_asset_contract_v2(ta2).address();
        let new_share = env.register(DummyShare, ());
        client.add_token(&admin, &new_token, &new_share);
        let attacker = Address::generate(&env);
        let result = client.try_remove_token(&attacker, &new_token);
        assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
    }

    #[test]
    fn test_set_yield_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let attacker = Address::generate(&env);
        let result = client.try_propose_yield_change(&attacker, &500u32);
        assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
    }

    #[test]
    fn test_set_factoring_fee_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let attacker = Address::generate(&env);
        let result = client.try_set_factoring_fee(&attacker, &100u32);
        assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
    }

    #[test]
    fn test_set_compound_interest_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let attacker = Address::generate(&env);
        let result = client.try_set_compound_interest(&attacker, &true);
        assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
    }

    #[test]
    fn test_set_collateral_config_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let attacker = Address::generate(&env);
        let result = client.try_set_collateral_config(&attacker, &1_000i128, &2_000u32);
        assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
    }

    #[test]
    fn test_set_exchange_rate_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        client.set_rate_bounds(&admin, &usdc_id, &9_500u32, &10_500u32);
        let attacker = Address::generate(&env);
        let result = client.try_set_exchange_rate(&attacker, &usdc_id, &10_000u32);
        assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
    }

    #[test]
    fn test_set_exchange_rate_within_bounds_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);

        client.set_rate_bounds(&admin, &usdc_id, &9_500u32, &10_500u32);
        client.set_exchange_rate(&admin, &usdc_id, &10_200u32);

        assert_eq!(client.get_exchange_rate(&usdc_id), 10_200u32);
        let bounds = client.get_rate_bounds(&usdc_id);
        assert_eq!(bounds.min_bps, 9_500u32);
        assert_eq!(bounds.max_bps, 10_500u32);
    }

    #[test]
    fn test_set_exchange_rate_outside_bounds_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);

        client.set_rate_bounds(&admin, &usdc_id, &9_500u32, &10_500u32);
        let result = client.try_set_exchange_rate(&admin, &usdc_id, &10_600u32);
        assert_eq!(result, Err(Ok(PoolError::InvalidAmount)));
    }

    #[test]
    fn test_set_rate_bounds_invalid_order_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);

        let result = client.try_set_rate_bounds(&admin, &usdc_id, &10_500u32, &9_500u32);
        assert_eq!(result, Err(Ok(PoolError::InvalidAmount)));
    }

    #[test]
    fn test_yield_calc_no_overflow_large_principal() {
        let interest = calculate_interest(
            1_000_000_000_000_000u128,
            5_000u32,
            5 * SECS_PER_YEAR,
            false,
        )
        .unwrap();
        assert!(interest > 0);
        assert!(interest < 3_000_000_000_000_000u128);
    }

    #[test]
    fn test_yield_calc_precision_small_amounts() {
        let interest = calculate_interest(1u128, 800u32, 86_400u64, false).unwrap();
        assert_eq!(interest, 0u128);
    }

    #[test]
    fn test_set_kyc_required_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let attacker = Address::generate(&env);
        let result = client.try_set_kyc_required(&attacker, &true);
        assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
    }

    #[test]
    fn test_set_investor_kyc_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let attacker = Address::generate(&env);
        let investor = Address::generate(&env);
        let result = client.try_set_investor_kyc(&attacker, &investor, &true);
        assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
    }

    #[test]
    fn test_propose_upgrade_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let attacker = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[0u8; 32]);
        let result = client.try_propose_upgrade(&attacker, &hash);
        assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
    }

    #[test]
    fn test_fund_multiple_invoices_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 2_000);
        client.deposit(&investor, &usdc_id, &2_000, &None);

        let mut requests = Vec::new(&env);
        requests.push_back(FundingRequest {
            invoice_id: 1u64,
            principal: 500,
            sme,
            due_date: env.ledger().timestamp() + 10_000,
            token: usdc_id,
        });
        let attacker = Address::generate(&env);
        let result = client.try_fund_multiple_invoices(&attacker, &requests);
        assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
    }

    #[test]
    fn test_fund_invoices_batch_funds_five_invoices() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 10_000);
        client.deposit(&investor, &usdc_id, &10_000, &None);

        let mut requests = Vec::new(&env);
        for invoice_id in 1u64..=5u64 {
            requests.push_back(FundingRequest {
                invoice_id,
                principal: 1_000,
                sme: sme.clone(),
                due_date: env.ledger().timestamp() + 86_400,
                token: usdc_id.clone(),
            });
        }

        client.fund_invoices_batch(&admin, &requests);

        for invoice_id in 1u64..=5u64 {
            assert!(client.get_funded_invoice(&invoice_id).is_some());
        }
        let stats = client.get_storage_stats();
        assert_eq!(stats.active_funded_invoices, 5);
    }

    #[test]
    fn test_fund_invoices_batch_rejects_more_than_twenty() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let sme = Address::generate(&env);

        let mut requests = Vec::new(&env);
        for invoice_id in 1u64..=21u64 {
            requests.push_back(FundingRequest {
                invoice_id,
                principal: 1,
                sme: sme.clone(),
                due_date: env.ledger().timestamp() + 86_400,
                token: usdc_id.clone(),
            });
        }

        let result = client.try_fund_invoices_batch(&admin, &requests);
        assert_eq!(result, Err(Ok(PoolError::BatchTooLarge)));
    }

    #[test]
    fn test_repay_invoices_batch_repays_multiple_invoices() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, 10_000);
        client.deposit(&investor, &usdc_id, &10_000, &None);

        let mut fund_requests = Vec::new(&env);
        for invoice_id in 1u64..=3u64 {
            fund_requests.push_back(FundingRequest {
                invoice_id,
                principal: 1_000,
                sme: sme.clone(),
                due_date: env.ledger().timestamp() + 86_400,
                token: usdc_id.clone(),
            });
        }
        client.fund_invoices_batch(&admin, &fund_requests);

        let mut repayments = Vec::new(&env);
        for invoice_id in 1u64..=3u64 {
            let amount = client.estimate_repayment(&invoice_id, &None);
            repayments.push_back(RepaymentRequest { invoice_id, amount });
        }
        client.repay_invoices_batch(&sme, &repayments);

        for invoice_id in 1u64..=3u64 {
            let record = client.get_funded_invoice(&invoice_id).unwrap();
            assert_eq!(record.repaid_amount, record.principal);
        }
    }

    #[test]
    fn test_get_funded_invoices_batch_returns_records_and_none() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 10_000);
        client.deposit(&investor, &usdc_id, &10_000, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &1_000i128,
            &sme,
            &(env.ledger().timestamp() + 86_400),
            &usdc_id,
        );

        let ids = soroban_sdk::vec![&env, 1u64, 99u64];
        let records = client.get_funded_invoices_batch(&ids);

        assert_eq!(records.len(), 2);
        assert_eq!(records.get(0).unwrap().unwrap().invoice_id, 1);
        assert!(records.get(1).unwrap().is_none());
    }

    #[test]
    fn test_get_funded_invoices_batch_rejects_oversized_batch() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let mut ids = Vec::new(&env);
        for invoice_id in 1u64..=21u64 {
            ids.push_back(invoice_id);
        }

        let result = client.try_get_funded_invoices_batch(&ids);

        if let Err(Ok(err)) = result {
            assert_eq!(err, PoolError::BatchTooLarge);
        } else {
            panic!("Expected BatchTooLarge error");
        }
    }

    #[test]
    fn test_total_due_overflow_returns_amount_overflow() {
        let env = Env::default();
        let token = Address::generate(&env);
        let sme = Address::generate(&env);
        let record = FundedInvoice {
            invoice_id: 1,
            sme,
            token,
            principal: i128::MAX,
            funded_at: 0,
            factoring_fee: 0,
            due_date: u64::MAX,
            repaid_amount: 0,
            co_funding_round_id: None,
            // #863: interest now accrues at the locked rate, so the extreme
            // value lives here to keep exercising the overflow path.
            locked_yield_bps: u32::MAX,
        };
        let config = PoolConfig {
            invoice_contract: Address::generate(&env),
            admin: Address::generate(&env),
            yield_bps: u32::MAX,
            factoring_fee_bps: 0,
            compound_interest: false,
            last_yield_change_at: 0,
            yield_change_cooldown_secs: DEFAULT_YIELD_CHANGE_COOLDOWN_SECS,
            max_yield_change_bps: DEFAULT_MAX_YIELD_CHANGE_BPS,
            proposed_yield_bps: 0,
            yield_proposal_at: 0,
            yield_timelock_secs: DEFAULT_YIELD_TIMELOCK_SECS,
            min_deposit_amount: DEFAULT_MIN_DEPOSIT_AMOUNT,
            max_single_investor_bps: DEFAULT_MAX_SINGLE_INVESTOR_BPS,
            max_single_withdrawal_bps: DEFAULT_MAX_SINGLE_WITHDRAWAL_BPS,
            withdrawal_cooldown_secs: DEFAULT_WITHDRAWAL_COOLDOWN_SECS,
            max_utilization_bps: DEFAULT_MAX_UTILIZATION_BPS,
            utilization_warning_bps: DEFAULT_UTILIZATION_WARNING_BPS,
            max_withdrawal_queue_age_days: DEFAULT_MAX_WITHDRAWAL_QUEUE_AGE_DAYS,
            max_withdrawal_queue_depth: DEFAULT_MAX_WITHDRAWAL_QUEUE_DEPTH,
        };

        assert_eq!(
            calculate_total_due(&record, &config, u64::MAX),
            Err(PoolError::AmountOverflow)
        );
    }

    #[test]
    fn test_calculate_reward_delta_overflow_returns_amount_overflow() {
        assert_eq!(
            calculate_reward_delta(i128::MAX, 1),
            Err(PoolError::AmountOverflow)
        );
    }

    #[test]
    fn test_calculate_reward_delta_large_values_succeed() {
        let total_interest = 1_000_000_000_000_000_000i128;
        let total_shares = 2_000_000_000_000i128;
        let reward_delta = calculate_reward_delta(total_interest, total_shares).unwrap();
        assert_eq!(reward_delta, 500_000_000_000_000_000i128);
    }

    #[test]
    fn test_calculate_reward_delta_multiple_large_inputs() {
        let cases: [(i128, i128); 4] = [
            (10_000_000_000_000i128, 10_000i128),
            (1_000_000_000_000_000i128, 500_000i128),
            (8_500_000_000_000_000_000i128, 3_000_000_000i128),
            (90_000_000_000_000_000i128, 9_000_000i128),
        ];
        for (total_interest, total_shares) in cases {
            let expected = (total_interest * REWARD_PRECISION) / total_shares;
            let actual = calculate_reward_delta(total_interest, total_shares).unwrap();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn test_seize_collateral_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        propose_and_execute_set_collateral_config(&env, &client, &admin, 1_000i128, 2_000u32);
        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal);
        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, required);
        client.deposit(&investor, &usdc_id, &10_000, &None);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &required);
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        let attacker = Address::generate(&env);
        // Non-admin is rejected by require_admin before the proposal-flow gate.
        let result = client.try_seize_collateral(&attacker, &1u64);
        assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
    }

    #[test]
    fn test_cleanup_funded_invoice_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 1_000);
        mint(&env, &usdc_id, &sme, 2_000);
        client.deposit(&investor, &usdc_id, &1_000, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &1_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        let amount_due = client.estimate_repayment(&1u64, &None);
        client.repay_invoice(&1u64, &sme, &amount_due);
        let attacker = Address::generate(&env);
        let result = client.try_cleanup_funded_invoice(&attacker, &1u64);
        assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
    }

    // --- Pause mechanism tests ---

    #[test]
    #[should_panic(expected = "Error(Contract, #12)")]
    fn test_fund_invoice_when_paused_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 2_000);
        client.deposit(&investor, &usdc_id, &2_000, &None);
        client.pause(&admin);
        client.fund_invoice(
            &admin,
            &1u64,
            &1_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
    }

    #[test]
    fn test_repay_invoice_allowed_when_paused() {
        // #779: repayment must stay open during an emergency pause so
        // borrowers can always exit their debt, even while new deposits and
        // funding are frozen.
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 1_000);
        mint(&env, &usdc_id, &sme, 2_000);
        client.deposit(&investor, &usdc_id, &1_000, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &1_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        client.pause(&admin);
        assert!(client.is_paused());
        let amount_due = client.estimate_repayment(&1u64, &None);
        client.repay_invoice(&1u64, &sme, &amount_due);
        let fi = client.get_funded_invoice(&1u64).unwrap();
        assert!(fi.repaid_amount >= amount_due);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #12)")]
    fn test_deposit_collateral_when_paused_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let sme = Address::generate(&env);

        propose_and_execute_set_collateral_config(&env, &client, &admin, 1_000i128, 2_000u32);
        mint(&env, &usdc_id, &sme, 1_000);
        client.pause(&admin);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &1_000);
    }

    #[test]
    fn test_pause_and_unpause_restores_operations() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 2_000);
        mint(&env, &usdc_id, &sme, 2_000);
        client.deposit(&investor, &usdc_id, &2_000, &None);

        client.pause(&admin);
        assert!(client.is_paused());

        client.unpause(&admin);
        assert!(!client.is_paused());

        client.fund_invoice(
            &admin,
            &1u64,
            &1_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        let amount_due = client.estimate_repayment(&1u64, &None);
        client.repay_invoice(&1u64, &sme, &amount_due);
        let fi = client.get_funded_invoice(&1u64).unwrap();
        assert!(fi.repaid_amount >= amount_due);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #12)")]
    fn test_deposit_blocked_when_paused() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 1_000);

        client.pause(&admin);
        client.deposit(&investor, &usdc_id, &1_000, &None);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #12)")]
    fn test_withdraw_blocked_when_paused() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 1_000);
        client.deposit(&investor, &usdc_id, &1_000, &None);
        client.pause(&admin);

        client.withdraw(&investor, &usdc_id, &100);
    }

    #[test]
    fn test_admin_ops_allowed_when_paused() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        client.pause(&admin);
        assert!(client.is_paused());

        env.ledger()
            .with_mut(|l| l.timestamp += DEFAULT_YIELD_CHANGE_COOLDOWN_SECS);
        client.propose_yield_change(&admin, &900u32);
        env.ledger()
            .with_mut(|l| l.timestamp += DEFAULT_YIELD_TIMELOCK_SECS);
        client.execute_yield_change();
        assert_eq!(client.get_config().yield_bps, 900u32);

        client.unpause(&admin);
        assert!(!client.is_paused());
    }

    // --- KYC gate tests ---

    #[test]
    fn test_deposit_when_kyc_required_unapproved_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        client.set_kyc_required(&admin, &true);
        mint(&env, &usdc_id, &investor, 1_000);
        // Investor never started KYC → KycNotRequested (#337)
        let result = client.try_deposit(&investor, &usdc_id, &1_000, &None);
        assert_eq!(result, Err(Ok(PoolError::KycNotRequested)));
    }

    #[test]
    fn test_deposit_when_kyc_required_and_approved_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        client.set_kyc_required(&admin, &true);
        client.set_investor_kyc(&admin, &investor, &true);
        mint(&env, &usdc_id, &investor, 1_000);
        client.deposit(&investor, &usdc_id, &1_000, &None);

        let tt = client.get_token_totals(&usdc_id);
        assert_eq!(tt.pool_value, 1_000);
    }

    #[test]
    fn test_kyc_revocation_blocks_deposit() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        client.set_kyc_required(&admin, &true);
        client.set_investor_kyc(&admin, &investor, &true);
        mint(&env, &usdc_id, &investor, 2_000);
        client.deposit(&investor, &usdc_id, &1_000, &None);

        // Revoke KYC — subsequent deposit must be blocked with KycRejected (#337)
        client.set_investor_kyc(&admin, &investor, &false);
        let result = client.try_deposit(&investor, &usdc_id, &1_000, &None);
        assert_eq!(result, Err(Ok(PoolError::KycRejected)));
    }

    #[test]
    fn test_kyc_not_required_allows_any_investor() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        // KYC disabled by default — any investor can deposit
        assert!(!client.kyc_required());
        mint(&env, &usdc_id, &investor, 500);
        client.deposit(&investor, &usdc_id, &500, &None);

        let tt = client.get_token_totals(&usdc_id);
        assert_eq!(tt.pool_value, 500);
    }

    #[test]
    fn test_kyc_required_flag_toggle_blocks_and_restores_deposit() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 3_000);
        client.set_kyc_required(&admin, &true);
        // Investor never set KYC → KycNotRequested (#337)
        let blocked = client.try_deposit(&investor, &usdc_id, &1_000, &None);
        assert_eq!(blocked, Err(Ok(PoolError::KycNotRequested)));

        client.set_kyc_required(&admin, &false);
        client.deposit(&investor, &usdc_id, &1_000, &None);

        client.set_kyc_required(&admin, &true);
        let blocked_again = client.try_deposit(&investor, &usdc_id, &1_000, &None);
        assert_eq!(blocked_again, Err(Ok(PoolError::KycNotRequested)));
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #12)")]
    fn test_deposit_when_paused_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        client.pause(&admin);
        mint(&env, &usdc_id, &investor, 1000);
        client.deposit(&investor, &usdc_id, &1000, &None); // Should panic
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #12)")]
    fn test_withdraw_when_paused_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 1000);
        client.deposit(&investor, &usdc_id, &1000, &None);
        client.pause(&admin);
        client.withdraw(&investor, &usdc_id, &500); // Should panic
    }

    #[test]
    fn test_pause_events_emitted() {
        // #779: the paused/unpaused events must carry both the pausing admin
        // and a timestamp, not just the admin address.
        use soroban_sdk::testutils::Events;
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);

        client.pause(&admin);
        // env.events().all() only returns events from the most recent
        // invocation, so this must be checked before any other call (e.g.
        // is_paused) intervenes.
        let pause_ts = env.ledger().timestamp();
        let expected_paused: soroban_sdk::Vec<soroban_sdk::Val> =
            (EVT, symbol_short!("paused")).into_val(&env);
        let paused_event = env
            .events()
            .all()
            .iter()
            .find(|e| e.1 == expected_paused)
            .expect("paused event not emitted");
        let (event_admin, event_ts): (Address, u64) = paused_event.2.into_val(&env);
        assert_eq!(event_admin, admin);
        assert_eq!(event_ts, pause_ts);
        assert!(client.is_paused());

        client.unpause(&admin);
        let unpause_ts = env.ledger().timestamp();
        let expected_unpaused: soroban_sdk::Vec<soroban_sdk::Val> =
            (EVT, symbol_short!("unpaused")).into_val(&env);
        let unpaused_event = env
            .events()
            .all()
            .iter()
            .find(|e| e.1 == expected_unpaused)
            .expect("unpaused event not emitted");
        let (event_admin, event_ts): (Address, u64) = unpaused_event.2.into_val(&env);
        assert_eq!(event_admin, admin);
        assert_eq!(event_ts, unpause_ts);
        assert!(!client.is_paused());
    }

    // ---- Issue #138: Partial Repayment Tests ----

    #[test]
    fn test_partial_repayment_two_installments() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, 10_000);

        client.deposit(&investor, &usdc_id, &10_000, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &5_000i128,
            &sme,
            &(env.ledger().timestamp() + 50_000),
            &usdc_id,
        );

        env.ledger().with_mut(|l| l.timestamp += 10_000);
        let total_due = client.estimate_repayment(&1u64, &None);
        let half = total_due / 2;

        // First partial payment
        client.repay_invoice(&1u64, &sme, &half);
        let fi = client.get_funded_invoice(&1u64).unwrap();
        assert_eq!(fi.repaid_amount, half);

        // Invoice still active — total_deployed unchanged
        let tt = client.get_token_totals(&usdc_id);
        assert_eq!(tt.total_deployed, 5_000i128);

        // Second payment clears the rest
        let remaining = client.estimate_repayment(&1u64, &None);
        client.repay_invoice(&1u64, &sme, &remaining);

        let fi2 = client.get_funded_invoice(&1u64).unwrap();
        assert!(fi2.repaid_amount >= total_due);

        let tt2 = client.get_token_totals(&usdc_id);
        assert_eq!(tt2.total_deployed, 0);
        assert!(tt2.pool_value >= 10_000);
    }

    #[test]
    fn test_partial_repayment_does_not_transition_prematurely() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 5_000);
        mint(&env, &usdc_id, &sme, 5_000);

        client.deposit(&investor, &usdc_id, &5_000, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &3_000i128,
            &sme,
            &(env.ledger().timestamp() + 50_000),
            &usdc_id,
        );

        env.ledger().with_mut(|l| l.timestamp += 5_000);
        let total_due = client.estimate_repayment(&1u64, &None);

        // Partial payment — less than total
        client.repay_invoice(&1u64, &sme, &(total_due / 3));

        // Invoice record still exists; pool still shows it as deployed
        let fi = client.get_funded_invoice(&1u64).unwrap();
        assert!(fi.repaid_amount < total_due);
        let tt = client.get_token_totals(&usdc_id);
        assert_eq!(tt.total_deployed, 3_000i128);
    }

    #[test]
    fn test_overpayment_is_capped_at_outstanding_balance() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 5_000);
        mint(&env, &usdc_id, &sme, 10_000);

        client.deposit(&investor, &usdc_id, &5_000, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &2_000i128,
            &sme,
            &(env.ledger().timestamp() + 50_000),
            &usdc_id,
        );

        env.ledger().with_mut(|l| l.timestamp += 5_000);
        let total_due = client.estimate_repayment(&1u64, &None);

        // The final payment is capped; the excess remains in the borrower's wallet.
        client.repay_invoice(&1u64, &sme, &(total_due + 1));
        let funded = client.get_funded_invoice(&1u64).unwrap();
        assert_eq!(funded.repaid_amount, total_due);
    }

    #[test]
    fn test_double_full_repayment_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 5_000);
        mint(&env, &usdc_id, &sme, 10_000);

        client.deposit(&investor, &usdc_id, &5_000, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &2_000i128,
            &sme,
            &(env.ledger().timestamp() + 50_000),
            &usdc_id,
        );

        env.ledger().with_mut(|l| l.timestamp += 5_000);
        let total_due = client.estimate_repayment(&1u64, &None);
        client.repay_invoice(&1u64, &sme, &total_due);

        // Second full repayment must be rejected
        let result = client.try_repay_invoice(&1u64, &sme, &total_due);
        assert_eq!(result, Err(Ok(PoolError::AlreadyFullyRepaid)));
    }

    // ---- Issue #275: Utilization rate alerts and auto-pause threshold ----

    #[test]
    fn test_utilization_zero_when_no_deployment() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 1_000);
        client.deposit(&investor, &usdc_id, &1_000, &None);
        assert_eq!(client.get_utilization(&usdc_id), 0u32);
    }

    #[test]
    fn test_utilization_calculated_correctly() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 10_000);
        client.deposit(&investor, &usdc_id, &10_000, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &5_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        // 5000 deployed / 10000 pool_value = 50% = 5000 bps
        assert_eq!(client.get_utilization(&usdc_id), 5_000u32);
    }

    #[test]
    fn test_fund_invoice_rejected_when_utilization_limit_exceeded() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 10_000);
        client.deposit(&investor, &usdc_id, &10_000, &None);

        // Set max utilization to 50%
        client
            .try_set_max_utilization(&admin, &5_000u32)
            .unwrap()
            .unwrap();

        // Fund 5000 (50%) — should succeed exactly at limit
        client.fund_invoice(
            &admin,
            &1u64,
            &5_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );

        // Fund 1 more — would push to 50.01%, exceeding limit
        let result = client.try_fund_invoice(
            &admin,
            &2u64,
            &1i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_set_max_utilization_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let attacker = Address::generate(&env);
        let result = client.try_set_max_utilization(&attacker, &5_000u32);
        assert!(result.is_err());
    }

    #[test]
    fn test_util_warn_emitted_only_on_threshold_crossing() {
        // #653: the util_warn event is edge-triggered — emitted once when a
        // funding call pushes utilization across the warning threshold (default
        // 80%), and not re-emitted on further funding while utilization stays
        // above it.
        use soroban_sdk::testutils::Events;
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 10_000);
        client.deposit(&investor, &usdc_id, &10_000, &None);

        let expected_topics: soroban_sdk::Vec<soroban_sdk::Val> =
            (EVT, symbol_short!("util_warn")).into_val(&env);
        // env.events().all() returns the events from the most recent invocation,
        // so checking it right after each call tells us whether *that* funding
        // emitted the warning.
        let warns_in_last_call = |env: &Env| {
            env.events()
                .all()
                .iter()
                .filter(|e| e.1 == expected_topics)
                .count()
        };

        // Fund 7000 (70%) — below the warning threshold, no event.
        client.fund_invoice(
            &admin,
            &1u64,
            &7_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        assert_eq!(warns_in_last_call(&env), 0);

        // Fund 1000 more → 80%, crossing the threshold — exactly one warning.
        client.fund_invoice(
            &admin,
            &2u64,
            &1_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        assert_eq!(warns_in_last_call(&env), 1);

        // Fund 500 more → 85%, still above the threshold — no re-emission.
        client.fund_invoice(
            &admin,
            &3u64,
            &500i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        assert_eq!(warns_in_last_call(&env), 0);
    }

    // ---- Issue #411: Share minting uses exchange rate ----

    #[test]
    fn test_deposit_after_yield_accrual_mints_correct_shares() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, share_token) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let sme = Address::generate(&env);

        // Alice deposits 1000 USDC → receives 1000 shares (1:1 initially)
        mint(&env, &usdc_id, &alice, 1000);
        client.deposit(&alice, &usdc_id, &1000, &None);

        let alice_shares: i128 = env.invoke_contract(
            &share_token,
            &Symbol::new(&env, "balance"),
            soroban_sdk::vec![&env, alice.clone().into_val(&env)],
        );
        assert_eq!(alice_shares, 1000);

        // Pool accrues yield via a funded invoice
        let due_date = env.ledger().timestamp() + 1_000_000;
        mint(&env, &usdc_id, &sme, 500);
        client.fund_invoice(&admin, &1u64, &500, &sme, &due_date, &usdc_id);
        env.ledger().with_mut(|l| l.timestamp += 1_000_000);
        let total_due = client.estimate_repayment(&1u64, &None);
        client.repay_invoice(&1u64, &sme, &total_due);

        let tt = client.get_token_totals(&usdc_id);
        let pool_value_after_yield = tt.pool_value;
        assert!(pool_value_after_yield > 1000);

        // Bob deposits an amount equal to the pool value
        let bob_deposit = pool_value_after_yield;
        mint(&env, &usdc_id, &bob, bob_deposit);
        client.deposit(&bob, &usdc_id, &bob_deposit, &None);

        // Bob should receive shares proportional to the exchange rate:
        // shares = deposit * total_shares / pool_value
        let total_shares_before_bob = 1000;
        let expected_bob_shares = (bob_deposit * total_shares_before_bob) / pool_value_after_yield;
        let bob_shares: i128 = env.invoke_contract(
            &share_token,
            &Symbol::new(&env, "balance"),
            soroban_sdk::vec![&env, bob.clone().into_val(&env)],
        );
        assert_eq!(bob_shares, expected_bob_shares);

        // Alice's share of pool is not diluted
        let total_shares_after: i128 = env.invoke_contract(
            &share_token,
            &Symbol::new(&env, "total_supply"),
            Vec::new(&env),
        );
        let alice_share_bps = (alice_shares * 10_000) / total_shares_after;
        let fair_share_bps = (alice_shares * 10_000) / (alice_shares + bob_shares);
        assert_eq!(alice_share_bps, fair_share_bps);
    }

    // ---- Issue #415: Factoring fee precision ----

    #[test]
    fn test_factoring_fee_small_invoice_no_precision_loss() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        // 1 USDC invoice (100 stroops) with 25 BPS fee
        let principal: i128 = 100;
        let fee_bps: u32 = 25;

        client.set_factoring_fee(&admin, &fee_bps);
        mint(&env, &usdc_id, &investor, 1000);
        mint(&env, &usdc_id, &sme, 1000);
        client.deposit(&investor, &usdc_id, &1000, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 86400),
            &usdc_id,
        );

        let funded = client.get_funded_invoice(&1u64).unwrap();
        // Ceiling division: fee rounds up to 1 stroop
        let numerator = principal as u128 * fee_bps as u128;
        let expected_fee = (numerator + BPS_DENOM as u128 - 1) / BPS_DENOM as u128;
        assert_eq!(funded.factoring_fee, expected_fee as i128);
        assert!(
            funded.factoring_fee > 0,
            "fee should be non-zero for any fee_bps > 0"
        );
        // Fee must be ≤ principal for small amounts
        assert!(funded.factoring_fee <= funded.principal);
    }

    #[test]
    fn test_factoring_fee_tiny_invoice_rounds_up() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        // 50 stroops (minimal amount) with 100 BPS (1%) fee
        // (50 × 100) / 10_000 = 0 before fix → ceil gives 1 stroop
        let principal: i128 = 50;
        let fee_bps: u32 = 100;

        client.set_factoring_fee(&admin, &fee_bps);
        mint(&env, &usdc_id, &investor, 1000);
        mint(&env, &usdc_id, &sme, 1000);
        client.deposit(&investor, &usdc_id, &1000, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 86400),
            &usdc_id,
        );

        let funded = client.get_funded_invoice(&1u64).unwrap();
        assert!(
            funded.factoring_fee > 0,
            "even minimal invoice should have non-zero fee"
        );
        assert!(funded.factoring_fee <= funded.principal);
    }

    #[test]
    fn test_factoring_fee_large_invoice_precise() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        // 10M stroops (1000 USDC) with 25 BPS fee → 25_000 stroops
        let principal: i128 = 10_000_000;
        let fee_bps: u32 = 25;

        client.set_factoring_fee(&admin, &fee_bps);
        mint(&env, &usdc_id, &investor, principal);
        mint(&env, &usdc_id, &sme, principal * 2);
        client.deposit(&investor, &usdc_id, &principal, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 86400),
            &usdc_id,
        );

        let funded = client.get_funded_invoice(&1u64).unwrap();
        let expected_fee = principal * fee_bps as i128 / BPS_DENOM as i128;
        assert_eq!(funded.factoring_fee, expected_fee);
    }

    #[test]
    fn test_factoring_fee_always_below_principal() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        // Max fee (10_000 bps = 100%) on a 1-stroop invoice
        client.set_factoring_fee(&admin, &BPS_DENOM);
        mint(&env, &usdc_id, &investor, 1000);
        mint(&env, &usdc_id, &sme, 1000);
        client.deposit(&investor, &usdc_id, &1000, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &1,
            &sme,
            &(env.ledger().timestamp() + 86400),
            &usdc_id,
        );

        let funded = client.get_funded_invoice(&1u64).unwrap();
        assert!(funded.factoring_fee <= funded.principal);
    }

    // ---- Issue #414: O(log n) compound interest via fixed_point_pow ----

    #[test]
    fn test_fixed_point_pow_identity() {
        // (1 + 0)^n = 1
        let precision = 10000u128;
        let result = fixed_point_pow(precision, 365, precision);
        assert_eq!(result, precision);
    }

    #[test]
    fn test_fixed_point_pow_zero_exp() {
        let precision = 10000u128;
        let result = fixed_point_pow(precision + 1, 0, precision);
        assert_eq!(result, precision);
    }

    #[test]
    fn test_div_round_half_up_rejects_zero_denominator() {
        assert_eq!(div_round_half_up(1, 0), Err(PoolError::AmountOverflow));
    }

    #[test]
    fn test_compound_interest_matches_loop_for_one_day() {
        let principal: u128 = 10_000_000;
        let yield_bps: u32 = 800;
        let elapsed_secs: u64 = 86400; // 1 day

        let interest = calculate_interest(principal, yield_bps, elapsed_secs, true).unwrap();
        let simple = calculate_interest(principal, yield_bps, elapsed_secs, false).unwrap();
        // For 1 day, compound == simple
        assert_eq!(interest, simple);
    }

    #[test]
    fn test_compound_interest_matches_loop_for_many_days() {
        let principal: u128 = 10_000_000_000;
        let yield_bps: u32 = 800;
        let days = 365u64;
        let elapsed_secs = days * 86400;

        let compound_interest =
            calculate_interest(principal, yield_bps, elapsed_secs, true).unwrap();

        // Calculate using the old loop method for comparison
        let denominator = BPS_DENOM as u128 * SECS_PER_YEAR as u128;
        let mut loop_amount = principal;
        let daily_rate_num = yield_bps as u128 * 86400;
        for _ in 0..days {
            let accrued = loop_amount * daily_rate_num / denominator;
            loop_amount += accrued;
        }
        let loop_interest = loop_amount - principal;

        let diff = if compound_interest > loop_interest {
            compound_interest - loop_interest
        } else {
            loop_interest - compound_interest
        };
        // The loop method accumulates integer-division rounding across 365 iterations,
        // while fixed_point_pow uses O(log n) multiplications and is more precise.
        assert!(diff <= 10_000, "diff={} > 10_000", diff);
    }

    #[test]
    fn test_compound_interest_long_period_no_overflow() {
        let principal: u128 = 1_000_000_000;
        let yield_bps: u32 = 800;
        // 3650 days ≈ 10 years
        let elapsed_secs = 3650u64 * 86400;

        let interest = calculate_interest(principal, yield_bps, elapsed_secs, true).unwrap();
        // At 8% APY for 10 years, interest should be less than principal * 2
        assert!(interest < principal * 2);
        assert!(interest > 0);
    }

    #[test]
    fn test_compound_interest_days_remainder_matches_loop() {
        let principal: u128 = 50_000_000_000;
        let yield_bps: u32 = 1200;
        let days = 100u64;
        let extra_secs = 43200u64; // half day
        let elapsed_secs = days * 86400 + extra_secs;

        let compound = calculate_interest(principal, yield_bps, elapsed_secs, true).unwrap();

        // Old loop method
        let denominator = BPS_DENOM as u128 * SECS_PER_YEAR as u128;
        let mut loop_amount = principal;
        let daily_rate_num = yield_bps as u128 * 86400;
        for _ in 0..days {
            let accrued = loop_amount * daily_rate_num / denominator;
            loop_amount += accrued;
        }
        let remaining_secs = elapsed_secs % 86400;
        if remaining_secs > 0 {
            let accrued = loop_amount * yield_bps as u128 * remaining_secs as u128 / denominator;
            loop_amount += accrued;
        }
        let loop_interest = loop_amount - principal;

        let diff = if compound > loop_interest {
            compound - loop_interest
        } else {
            loop_interest - compound
        };
        // The loop method accumulates integer-division rounding across iterations;
        // fixed_point_pow with O(log n) multiplications is more precise.
        assert!(
            diff <= 10_000,
            "compound={} loop={} diff={}",
            compound,
            loop_interest,
            diff
        );
    }

    // ---- Issue #416: Reentrancy guard on all state-changing functions ----

    #[test]
    #[should_panic(expected = "reentrant call")]
    fn test_reentrancy_guard_blocks_reentrant_call() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        // Acquire the reentrancy lock manually via env.as_contract (no client call)
        // so the guard is set before the guarded external call enters the contract.
        env.as_contract(&client.address, || {
            FundingPool::non_reentrant_start(&env);
        });

        mint(&env, &usdc_id, &investor, 1000);
        client.deposit(&investor, &usdc_id, &1000, &None);
    }

    #[test]
    #[should_panic(expected = "reentrant call")]
    fn test_repay_invoice_reentrancy_guard_blocks_when_set() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 1_000);
        mint(&env, &usdc_id, &sme, 2_000);
        client.deposit(&investor, &usdc_id, &1_000, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &1_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        let amount_due = client.estimate_repayment(&1u64, &None);

        env.as_contract(&client.address, || {
            env.storage()
                .instance()
                .set(&DataKey::ReentrancyGuard, &true);
        });

        client.repay_invoice(&1u64, &sme, &amount_due);
    }

    #[test]
    fn test_repay_invoice_reentrancy_guard_cleared_after_success() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 1_000);
        mint(&env, &usdc_id, &sme, 2_000);
        client.deposit(&investor, &usdc_id, &1_000, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &1_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        let amount_due = client.estimate_repayment(&1u64, &None);

        client.repay_invoice(&1u64, &sme, &amount_due);

        env.as_contract(&client.address, || {
            let guard = env
                .storage()
                .instance()
                .get::<DataKey, bool>(&DataKey::ReentrancyGuard)
                .unwrap_or(false);
            assert!(!guard);
        });
    }

    #[test]
    fn test_reentrancy_guard_releases_after_call() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 1000);
        client.deposit(&investor, &usdc_id, &1000, &None);

        // After a successful deposit, the guard should be released
        let result = client.try_deposit(&investor, &usdc_id, &1000, &None);
        // The second deposit should fail due to insufficient balance, not reentrancy
        assert_ne!(result, Err(Ok(PoolError::TokenNotAccepted)));
    }

    #[test]
    fn test_claim_yield_guarded_against_reentrancy() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 10000);
        mint(&env, &usdc_id, &sme, 10000);
        client.deposit(&investor, &usdc_id, &10000, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &5000,
            &sme,
            &(env.ledger().timestamp() + 86400),
            &usdc_id,
        );
        env.ledger().with_mut(|l| l.timestamp += 100_000);
        let amount_due = client.estimate_repayment(&1u64, &None);
        client.repay_invoice(&1u64, &sme, &amount_due);

        // claim_yield should succeed — guard acquired and released correctly
        client.claim_yield(&investor, &usdc_id);
    }

    #[test]
    fn test_seize_collateral_guarded_against_reentrancy() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        propose_and_execute_set_collateral_config(&env, &client, &admin, 1_000i128, 2_000u32);
        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, required);
        client.deposit(&investor, &usdc_id, &10_000, &None);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &required);

        let due_date = env.ledger().timestamp() + 10_000;
        client.fund_invoice(&admin, &1u64, &principal, &sme, &due_date, &usdc_id);
        // Mark invoice as defaulted for the test dummy
        let invoice_contract_id = client.get_config().invoice_contract;
        DummyInvoiceClient::new(&env, &invoice_contract_id).set_invoice_defaulted(&1u64, &true);
        env.ledger().with_mut(|l| l.timestamp = due_date + 1);

        // seize_collateral should succeed — guard acquired and released
        propose_and_execute_seize_collateral(&env, &client, &admin, 1u64);
    }

    #[test]
    fn test_withdraw_revenue_guarded_against_reentrancy() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);

        client.set_treasury(&admin, &admin);
        // Mint tokens to the pool so it can transfer protocol revenue
        let pool_address = client.address.clone();
        mint(&env, &usdc_id, &pool_address, 1000);
        // Directly push protocol revenue for testing
        env.as_contract(&pool_address, || {
            let tt_key = DataKey::TokenTotals(usdc_id.clone());
            let mut tt: PoolTokenTotals = env.storage().instance().get(&tt_key).unwrap_or_default();
            tt.protocol_revenue = 500;
            env.storage().instance().set(&tt_key, &tt);
        });

        // withdraw_revenue should succeed — guard acquired and released
        client.withdraw_revenue(&admin, &usdc_id, &500);
    }

    #[test]
    fn test_deposit_collateral_guarded_against_reentrancy() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let sme = Address::generate(&env);

        propose_and_execute_set_collateral_config(&env, &client, &admin, 1_000i128, 2_000u32);
        mint(&env, &usdc_id, &sme, 5_000);

        // deposit_collateral should succeed — guard acquired and released
        client.deposit_collateral(&1u64, &sme, &usdc_id, &1_000);

        let col = client.get_collateral_deposit(&1u64).unwrap();
        assert_eq!(col.amount, 1_000);
    }

    #[test]
    fn test_cleanup_funded_invoice_guarded_against_reentrancy() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 5_000);
        mint(&env, &usdc_id, &sme, 10_000);
        client.deposit(&investor, &usdc_id, &5_000, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &5_000,
            &sme,
            &(env.ledger().timestamp() + 86400),
            &usdc_id,
        );
        let amount_due = client.estimate_repayment(&1u64, &None);
        client.repay_invoice(&1u64, &sme, &amount_due);

        // cleanup_funded_invoice should succeed — guard acquired and released
        client.cleanup_funded_invoice(&admin, &1u64);
    }

    #[test]
    fn test_cancel_withdrawal_request_guarded_against_reentrancy() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        // Need liquidity shortfall to force queued withdrawal
        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, 10_000);
        client.deposit(&investor, &usdc_id, &10_000, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &10_000,
            &sme,
            &(env.ledger().timestamp() + 86400),
            &usdc_id,
        );

        // Request withdrawal when no liquidity available → gets queued
        let request_id = client.request_withdrawal(&investor, &usdc_id, &5_000);
        assert!(request_id > 0);

        // cancel_withdrawal_request should succeed
        client.cancel_withdrawal_request(&investor, &usdc_id);
    }

    #[test]
    fn test_withdrawal_request_counter_preserves_queue() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, 10_000);
        client.deposit(&investor, &usdc_id, &10_000, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &10_000,
            &sme,
            &(env.ledger().timestamp() + 86400),
            &usdc_id,
        );

        let request_id = client.request_withdrawal(&investor, &usdc_id, &5_000);
        let queue = client.get_withdrawal_queue(&usdc_id);

        assert_eq!(request_id, 1);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.get(0).unwrap().investor, investor);
    }

    #[test]
    fn test_cancel_withdrawal_request_by_token_removes_pending_request() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, 10_000);
        client.deposit(&investor, &usdc_id, &10_000, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &10_000,
            &sme,
            &(env.ledger().timestamp() + 86400),
            &usdc_id,
        );

        client.request_withdrawal(&investor, &usdc_id, &5_000);
        client.cancel_withdrawal_request(&investor, &usdc_id);

        assert_eq!(client.get_withdrawal_queue(&usdc_id).len(), 0);
        assert_eq!(
            client.try_cancel_withdrawal_request(&investor, &usdc_id),
            Err(Ok(PoolError::WithdrawalRequestNotFound))
        );
    }

    #[test]
    fn test_repayment_processes_withdrawal_queue_pro_rata() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, share_token) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &alice, 10_000);
        mint(&env, &usdc_id, &bob, 10_000);
        mint(&env, &usdc_id, &sme, 20_000);
        client.deposit(&alice, &usdc_id, &10_000, &None);
        client.deposit(&bob, &usdc_id, &10_000, &None);
        let due_date = env.ledger().timestamp() + SECS_PER_DAY;
        client.fund_invoice(&admin, &1u64, &10_000, &sme, &due_date, &usdc_id);
        client.fund_invoice(&admin, &2u64, &10_000, &sme, &due_date, &usdc_id);

        client.request_withdrawal(&alice, &usdc_id, &10_000);
        client.request_withdrawal(&bob, &usdc_id, &10_000);
        client.repay_invoice(&2u64, &sme, &10_000);

        let alice_shares: i128 = env.invoke_contract(
            &share_token,
            &Symbol::new(&env, "balance"),
            soroban_sdk::vec![&env, alice.clone().into_val(&env)],
        );
        let bob_shares: i128 = env.invoke_contract(
            &share_token,
            &Symbol::new(&env, "balance"),
            soroban_sdk::vec![&env, bob.clone().into_val(&env)],
        );
        let queue = client.get_withdrawal_queue(&usdc_id);

        assert_eq!(alice_shares, 5_000);
        assert_eq!(bob_shares, 5_000);
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.get(0).unwrap().shares, 5_000);
        assert_eq!(queue.get(1).unwrap().shares, 5_000);
    }

    #[test]
    fn test_update_invoice_due_date_guarded_against_reentrancy() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);
        let invoice_contract = client.get_config().invoice_contract;

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, 10_000);
        client.deposit(&investor, &usdc_id, &10_000, &None);
        client.fund_invoice(
            &admin,
            &1u64,
            &5_000,
            &sme,
            &(env.ledger().timestamp() + 86400),
            &usdc_id,
        );

        // update_invoice_due_date should succeed — guard acquired and released
        let new_due = env.ledger().timestamp() + (10 * 86400);
        client.update_invoice_due_date(&invoice_contract, &1u64, &new_due);

        let record = client.get_funded_invoice(&1u64).unwrap();
        assert_eq!(record.due_date, new_due);
    }

    // ── #337: KYC tri-state tests ─────────────────────────────────────────────

    #[test]
    fn test_approve_investor_kyc_allows_deposit() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        client.set_kyc_required(&admin, &true);
        client.approve_investor_kyc(&admin, &investor);
        assert_eq!(
            client.get_investor_kyc_status(&investor),
            KycStatus::Approved
        );
        assert!(client.get_investor_kyc(&investor));

        mint(&env, &usdc_id, &investor, 1_000);
        client.deposit(&investor, &usdc_id, &1_000, &None);
        assert_eq!(client.get_token_totals(&usdc_id).pool_value, 1_000);
    }

    #[test]
    fn test_reject_investor_kyc_returns_kyc_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        client.set_kyc_required(&admin, &true);
        client.reject_investor_kyc(&admin, &investor);
        assert_eq!(
            client.get_investor_kyc_status(&investor),
            KycStatus::Rejected
        );
        assert!(!client.get_investor_kyc(&investor));

        mint(&env, &usdc_id, &investor, 1_000);
        let result = client.try_deposit(&investor, &usdc_id, &1_000, &None);
        assert_eq!(result, Err(Ok(PoolError::KycRejected)));
    }

    #[test]
    fn test_kyc_not_requested_returns_distinct_error() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        client.set_kyc_required(&admin, &true);
        assert_eq!(
            client.get_investor_kyc_status(&investor),
            KycStatus::NotRequested
        );

        mint(&env, &usdc_id, &investor, 1_000);
        let result = client.try_deposit(&investor, &usdc_id, &1_000, &None);
        assert_eq!(result, Err(Ok(PoolError::KycNotRequested)));
    }

    #[test]
    fn test_set_investor_kyc_true_maps_to_approved() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        client.set_investor_kyc(&admin, &investor, &true);
        assert_eq!(
            client.get_investor_kyc_status(&investor),
            KycStatus::Approved
        );
        assert!(client.get_investor_kyc(&investor));
    }

    #[test]
    fn test_set_investor_kyc_false_maps_to_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        client.set_investor_kyc(&admin, &investor, &false);
        assert_eq!(
            client.get_investor_kyc_status(&investor),
            KycStatus::Rejected
        );
        assert!(!client.get_investor_kyc(&investor));
    }

    // ── #338: upgrade timelock tests ─────────────────────────────────────────

    #[test]
    fn test_pool_upgrade_timelock_default_is_24h() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        assert_eq!(client.get_upgrade_timelock(), UPGRADE_TIMELOCK_SECS);
    }

    #[test]
    fn test_pool_set_upgrade_timelock_configures_value() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        client.set_upgrade_timelock(&admin, &7_200u64);
        assert_eq!(client.get_upgrade_timelock(), 7_200u64);
    }

    #[test]
    fn test_pool_set_upgrade_timelock_below_minimum_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let result = client.try_set_upgrade_timelock(&admin, &(MIN_UPGRADE_TIMELOCK_SECS - 1));
        assert_eq!(result, Err(Ok(PoolError::InvalidUpgradeTimelock)));
    }

    #[test]
    fn test_pool_execute_upgrade_before_timelock_fails() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        client.set_upgrade_timelock(&admin, &7_200u64);

        let hash = BytesN::from_array(&env, &[1u8; 32]);
        client.propose_upgrade(&admin, &hash);

        env.ledger().with_mut(|l| l.timestamp += 3_600);
        let result = client.try_execute_upgrade(&admin);
        assert_eq!(result, Err(Ok(PoolError::UpgradeTimelockNotExpired)));
    }

    // ── #340: WASM hash validation tests ─────────────────────────────────────

    #[test]
    fn test_pool_propose_upgrade_zero_hash_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
        let result = client.try_propose_upgrade(&admin, &zero_hash);
        assert_eq!(result, Err(Ok(PoolError::InvalidWasmHash)));
    }

    #[test]
    fn test_pool_propose_upgrade_nonzero_hash_accepted() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let valid_hash = BytesN::from_array(&env, &[42u8; 32]);
        client.propose_upgrade(&admin, &valid_hash);
    }

    // ── #565: pool admin key rotation timelock tests ──────────────────────────

    #[test]
    fn test_pool_propose_admin_change_stores_pending() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let new_admin = Address::generate(&env);
        client.propose_admin_change(&admin, &new_admin);
    }

    #[test]
    fn test_pool_finalize_admin_change_before_timelock_fails() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let new_admin = Address::generate(&env);
        client.propose_admin_change(&admin, &new_admin);
        let result = client.try_finalize_admin_change(&admin);
        assert_eq!(result, Err(Ok(PoolError::AdminChangeTimelockNotExpired)));
    }

    #[test]
    fn test_pool_finalize_admin_change_after_timelock_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let new_admin = Address::generate(&env);
        client.propose_admin_change(&admin, &new_admin);
        env.ledger()
            .with_mut(|l| l.timestamp += ADMIN_CHANGE_TIMELOCK_SECS + 1);
        client.finalize_admin_change(&admin);
        // New admin should now be able to perform admin operations
        let result = client.try_pause(&admin);
        assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
        client.pause(&new_admin);
    }

    #[test]
    fn test_pool_cancel_admin_change_removes_pending() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let new_admin = Address::generate(&env);
        client.propose_admin_change(&admin, &new_admin);
        client.cancel_admin_change(&admin);
        // After cancel, finalize should fail with NoAdminChangeProposed
        let result = client.try_finalize_admin_change(&admin);
        assert_eq!(result, Err(Ok(PoolError::NoAdminChangeProposed)));
    }

    #[test]
    fn test_pool_finalize_without_proposal_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let result = client.try_finalize_admin_change(&admin);
        assert_eq!(result, Err(Ok(PoolError::NoAdminChangeProposed)));
    }

    #[test]
    fn test_pool_cancel_without_proposal_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let result = client.try_cancel_admin_change(&admin);
        assert_eq!(result, Err(Ok(PoolError::NoAdminChangeProposed)));
    }

    // ── #623: seize_collateral accounting & validation tests ────────────────────

    #[test]
    fn test_seize_collateral_happy_path_accounting() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);
        let invoice_contract = client.get_config().invoice_contract;

        propose_and_execute_set_collateral_config(&env, &client, &admin, 1_000i128, 2_000u32);
        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal); // 1,000

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, required);

        client.deposit(&investor, &usdc_id, &10_000, &None);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &required);
        let due_date = env.ledger().timestamp() + 1_000;
        client.fund_invoice(&admin, &1u64, &principal, &sme, &due_date, &usdc_id);

        // Mark invoice defaulted
        DummyInvoiceClient::new(&env, &invoice_contract).set_invoice_defaulted(&1u64, &true);

        let tt_before = client.get_token_totals(&usdc_id);
        let pool_token_client = token::Client::new(&env, &usdc_id);
        let pool_balance_before = pool_token_client.balance(&client.address);

        propose_and_execute_seize_collateral(&env, &client, &admin, 1u64);

        // 1. Check pool value is written down by the unrecovered shortfall
        // (principal minus recovered collateral), and total deployed
        // decreased by the full principal.
        let tt_after = client.get_token_totals(&usdc_id);
        assert_eq!(
            tt_after.pool_value,
            tt_before.pool_value - principal + required
        );
        assert_eq!(
            tt_after.total_deployed,
            tt_before.total_deployed - principal
        );

        // 2. Check the pool contract still holds the collateral amount (retained, not returned to SME)
        let pool_balance_after = pool_token_client.balance(&client.address);
        assert_eq!(pool_balance_after, pool_balance_before); // no tokens transferred out

        // 3. Verify CollateralDeposit.settled = true and seized_at is updated
        let col = client.get_collateral_deposit(&1u64).unwrap();
        assert!(col.settled);
        assert_eq!(col.seized_at, env.ledger().timestamp());
    }

    #[test]
    fn test_liquidate_invoice_permissionless_happy_path() {
        // #1037: any address (a keeper, not the admin) can trigger liquidation
        // directly once the invoice is Defaulted — no governance proposal needed.
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);
        let keeper = Address::generate(&env);
        let invoice_contract = client.get_config().invoice_contract;

        propose_and_execute_set_collateral_config(&env, &client, &admin, 1_000i128, 2_000u32);
        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, required);

        client.deposit(&investor, &usdc_id, &10_000, &None);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &required);
        let due_date = env.ledger().timestamp() + 1_000;
        client.fund_invoice(&admin, &1u64, &principal, &sme, &due_date, &usdc_id);

        DummyInvoiceClient::new(&env, &invoice_contract).set_invoice_defaulted(&1u64, &true);

        // Keeper (not admin, no proposal) triggers liquidation directly.
        client.liquidate_invoice(&keeper, &1u64);

        let col = client.get_collateral_deposit(&1u64).unwrap();
        assert!(col.settled);
        assert_eq!(col.seized_at, env.ledger().timestamp());
    }

    #[test]
    fn test_liquidate_invoice_not_defaulted_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);
        let keeper = Address::generate(&env);

        propose_and_execute_set_collateral_config(&env, &client, &admin, 1_000i128, 2_000u32);
        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, required);

        client.deposit(&investor, &usdc_id, &10_000, &None);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &required);
        let due_date = env.ledger().timestamp() + 1_000;
        client.fund_invoice(&admin, &1u64, &principal, &sme, &due_date, &usdc_id);

        // Not yet marked Defaulted — permissionless trigger must reject it.
        let result = client.try_liquidate_invoice(&keeper, &1u64);
        assert_eq!(result, Err(Ok(PoolError::NotDefaulted)));
    }

    #[test]
    fn test_liquidate_invoice_double_liquidation_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);
        let keeper = Address::generate(&env);
        let invoice_contract = client.get_config().invoice_contract;

        propose_and_execute_set_collateral_config(&env, &client, &admin, 1_000i128, 2_000u32);
        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, required);

        client.deposit(&investor, &usdc_id, &10_000, &None);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &required);
        let due_date = env.ledger().timestamp() + 1_000;
        client.fund_invoice(&admin, &1u64, &principal, &sme, &due_date, &usdc_id);

        DummyInvoiceClient::new(&env, &invoice_contract).set_invoice_defaulted(&1u64, &true);

        client.liquidate_invoice(&keeper, &1u64);

        // Second permissionless call on the same already-seized invoice is rejected.
        let result = client.try_liquidate_invoice(&keeper, &1u64);
        assert_eq!(result, Err(Ok(PoolError::CollateralAlreadySettled)));

        // Also rejected via the governance path — same underlying idempotency guard.
        let proposal_id = client.propose_operation(&admin, &AdminOperation::SeizeCollateral(1u64));
        advance_past_operation_delay(&env, &client);
        let exec_result = client.try_execute_operation(&admin, &proposal_id);
        assert_eq!(exec_result, Err(Ok(PoolError::CollateralAlreadySettled)));
    }

    #[test]
    fn test_seize_collateral_already_settled_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);
        let invoice_contract = client.get_config().invoice_contract;

        propose_and_execute_set_collateral_config(&env, &client, &admin, 1_000i128, 2_000u32);
        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, required);

        client.deposit(&investor, &usdc_id, &10_000, &None);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &required);
        let due_date = env.ledger().timestamp() + 1_000;
        client.fund_invoice(&admin, &1u64, &principal, &sme, &due_date, &usdc_id);

        DummyInvoiceClient::new(&env, &invoice_contract).set_invoice_defaulted(&1u64, &true);

        propose_and_execute_seize_collateral(&env, &client, &admin, 1u64);

        // Try second time — should fail with CollateralAlreadySettled at execute time
        let proposal_id = client.propose_operation(&admin, &AdminOperation::SeizeCollateral(1u64));
        advance_past_operation_delay(&env, &client);
        let result = client.try_execute_operation(&admin, &proposal_id);
        assert_eq!(result, Err(Ok(PoolError::CollateralAlreadySettled)));
    }

    #[test]
    fn test_seize_collateral_not_defaulted_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);
        let invoice_contract = client.get_config().invoice_contract;

        propose_and_execute_set_collateral_config(&env, &client, &admin, 1_000i128, 2_000u32);
        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, required);

        client.deposit(&investor, &usdc_id, &10_000, &None);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &required);
        let due_date = env.ledger().timestamp() + 1_000;
        client.fund_invoice(&admin, &1u64, &principal, &sme, &due_date, &usdc_id);

        // Do NOT mark invoice defaulted
        DummyInvoiceClient::new(&env, &invoice_contract).set_invoice_defaulted(&1u64, &false);

        let proposal_id = client.propose_operation(&admin, &AdminOperation::SeizeCollateral(1u64));
        advance_past_operation_delay(&env, &client);
        let result = client.try_execute_operation(&admin, &proposal_id);
        assert_eq!(result, Err(Ok(PoolError::NotDefaulted)));
    }

    // ── #622: property tests for calculate_interest ──────────────────────────
    //
    // These exercise calculate_interest() directly (it's a private fn in this
    // module) with many pseudo-random inputs instead of a handful of fixed
    // cases. We avoid pulling in the `proptest`/`quickcheck` crates as a new
    // dev-dependency and instead use a tiny deterministic PRNG (splitmix64-style
    // LCG) seeded per test, which keeps failures reproducible without adding
    // build dependencies. "Valid ranges" below mirror what the contract itself
    // enforces elsewhere: yield_bps is capped at 5_000 (see `set_yield` /
    // `propose_yield_change`), and duration is bounded at 10 years, matching
    // `test_compound_interest_long_period_no_overflow` above — this is the
    // longest duration already demonstrated not to overflow `fixed_point_pow`'s
    // unchecked squaring at the maximum yield rate.

    struct TestRng(u64);

    impl TestRng {
        fn new(seed: u64) -> Self {
            TestRng(seed)
        }

        fn next_u64(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
            z ^ (z >> 31)
        }

        fn range_u64(&mut self, lo: u64, hi: u64) -> u64 {
            debug_assert!(hi >= lo);
            let span = hi - lo + 1;
            lo + (self.next_u64() % span)
        }
    }

    const PROP_MAX_PRINCIPAL: u128 = 1_000_000_000_000_000; // 1e15 stroops (~100M tokens at 7 decimals)
    const PROP_MAX_YIELD_BPS: u64 = 5_000; // matches the contract's hard cap (set_yield / propose_yield_change)
    const PROP_MAX_ELAPSED_SECS: u64 = SECS_PER_YEAR * 10; // matches test_compound_interest_long_period_no_overflow

    #[test]
    fn test_property_compound_interest_always_gte_simple() {
        let mut rng = TestRng::new(0x622_0001);
        for _ in 0..2_000 {
            let principal = rng.range_u64(1, PROP_MAX_PRINCIPAL as u64) as u128;
            let yield_bps = rng.range_u64(1, PROP_MAX_YIELD_BPS) as u32;
            let elapsed_secs = rng.range_u64(1, PROP_MAX_ELAPSED_SECS);

            let simple = calculate_interest(principal, yield_bps, elapsed_secs, false)
                .expect("simple interest must not overflow within valid ranges");
            let compound = calculate_interest(principal, yield_bps, elapsed_secs, true)
                .expect("compound interest must not overflow within valid ranges");

            assert!(
                compound >= simple,
                "compound ({}) < simple ({}) for principal={}, yield_bps={}, elapsed_secs={}",
                compound,
                simple,
                principal,
                yield_bps,
                elapsed_secs
            );
        }
    }

    #[test]
    fn test_property_zero_elapsed_secs_yields_zero_interest() {
        let mut rng = TestRng::new(0x622_0002);
        for _ in 0..1_000 {
            let principal = rng.range_u64(1, PROP_MAX_PRINCIPAL as u64) as u128;
            let yield_bps = rng.range_u64(1, PROP_MAX_YIELD_BPS) as u32;

            let simple = calculate_interest(principal, yield_bps, 0, false)
                .expect("simple interest at elapsed_secs=0 must not overflow");
            let compound = calculate_interest(principal, yield_bps, 0, true)
                .expect("compound interest at elapsed_secs=0 must not overflow");

            assert_eq!(
                simple, 0,
                "simple interest must be 0 at elapsed_secs=0 (principal={}, yield_bps={})",
                principal, yield_bps
            );
            assert_eq!(
                compound, 0,
                "compound interest must be 0 at elapsed_secs=0 (principal={}, yield_bps={})",
                principal, yield_bps
            );
        }
    }

    #[test]
    fn test_property_no_overflow_panic_on_edge_case_large_inputs() {
        let edge_principals: [u128; 5] = [
            1,
            2,
            PROP_MAX_PRINCIPAL / 2,
            PROP_MAX_PRINCIPAL - 1,
            PROP_MAX_PRINCIPAL,
        ];
        let edge_yields: [u32; 4] = [
            1,
            2,
            (PROP_MAX_YIELD_BPS / 2) as u32,
            PROP_MAX_YIELD_BPS as u32,
        ];
        let edge_elapsed: [u64; 5] = [
            1,
            SECS_PER_DAY,
            SECS_PER_YEAR,
            PROP_MAX_ELAPSED_SECS / 2,
            PROP_MAX_ELAPSED_SECS,
        ];

        for &principal in edge_principals.iter() {
            for &yield_bps in edge_yields.iter() {
                for &elapsed_secs in edge_elapsed.iter() {
                    for &is_compound in [false, true].iter() {
                        let result =
                            calculate_interest(principal, yield_bps, elapsed_secs, is_compound);
                        assert!(
                            result.is_ok(),
                            "calculate_interest unexpectedly errored for \
                             principal={}, yield_bps={}, elapsed_secs={}, is_compound={}: {:?}",
                            principal,
                            yield_bps,
                            elapsed_secs,
                            is_compound,
                            result
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_property_compound_gte_simple_at_input_boundaries() {
        let principals: [u128; 3] = [1, PROP_MAX_PRINCIPAL / 2, PROP_MAX_PRINCIPAL];
        let yields: [u32; 3] = [
            1,
            (PROP_MAX_YIELD_BPS / 2) as u32,
            PROP_MAX_YIELD_BPS as u32,
        ];
        let elapsed: [u64; 3] = [1, PROP_MAX_ELAPSED_SECS / 2, PROP_MAX_ELAPSED_SECS];

        for &principal in principals.iter() {
            for &yield_bps in yields.iter() {
                for &elapsed_secs in elapsed.iter() {
                    let simple = calculate_interest(principal, yield_bps, elapsed_secs, false)
                        .expect("simple interest must not overflow within valid ranges");
                    let compound = calculate_interest(principal, yield_bps, elapsed_secs, true)
                        .expect("compound interest must not overflow within valid ranges");
                    assert!(
                        compound >= simple,
                        "compound ({}) < simple ({}) for principal={}, yield_bps={}, elapsed_secs={}",
                        compound,
                        simple,
                        principal,
                        yield_bps,
                        elapsed_secs
                    );
                }
            }
        }
    }
}
