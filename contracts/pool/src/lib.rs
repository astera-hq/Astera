// === AUTHORIZED CALLERS ===
// - Admin: pause(), unpause(), set_yield(), add_token(), admin-only setters
// - Pool contract: N/A (this is the pool contract)
// - Invoice contract: may call pool for state reads
// - Anyone: public view functions

// IMPLEMENTATION APPROACH for #222 - Pool Token Removal Safety Checks
//
// RECON FINDINGS:
// - remove_token() currently: checks admin auth, finds token in accepted list, removes if pool_value=0 and total_deployed=0
// - TokenTotals: PoolTokenTotals struct with fields: pool_value, total_deployed, total_paid_out, total_fee_revenue, reward_per_share, protocol_revenue
// - get_token_totals(): returns PoolTokenTotals struct
// - get_withdrawal_queue(): DOES NOT EXIST - no withdrawal queue functionality found in codebase
// - PoolError: #[contracterror] enum, next code #[u32] = 21 (after InsufficientCoFundShare = 20)
// - Storage pattern: DataKey::TokenTotals uses instance storage, no TTL
// - Auth pattern: admin.require_auth() + Self::require_admin(&env, &admin)
// - Share token burn: no existing burn logic found, share tokens handled via external contract calls
// - Tests use: Env::default(), env.mock_all_auths(), FundingPoolClient::new(), client.try_method() for error testing
//
// STRATEGY:
// 1. Add 3 safety checks before existing removal logic using exact storage helpers
// 2. Extend PoolError with TokenHasActiveBalances(#21), TokenHasDeployedCapital(#22),
//    TokenHasPendingWithdrawals(#23) following exact error pattern
// 3. Since no withdrawal queue exists, will skip that check for now and add placeholder
// 4. Tests: 4 new unit tests matching exact test framework from recon
//
// FILES: modify contracts/pool/src/lib.rs only + new tests
// UNRESOLVED: No withdrawal queue found - will implement basic check structure

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, token,
    Address, BytesN, Env, IntoVal, Symbol, Vec,
};

use soroban_sdk::contractclient;

/// Semantic version of this pool contract (#237).
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

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PoolError {
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
}

type PoolResult<T> = Result<T, PoolError>;

const DEFAULT_YIELD_BPS: u32 = 800;
const DEFAULT_FACTORING_FEE_BPS: u32 = 0;
const BPS_DENOM: u32 = 10_000;
const SECS_PER_YEAR: u64 = 31_536_000;
// #367: Stellar-native tokens use 7 decimal places (stroops)
const EXPECTED_DECIMALS: u32 = 7;
const SECS_PER_DAY: u64 = 86_400;
// #275: default max utilization — disabled (10_000 bps = 100%).
// Many flows legitimately deploy 100% of available liquidity.
const DEFAULT_MAX_UTILIZATION_BPS: u32 = 10_000;
// #275: warning threshold — 80% (8_000 bps)
const DEFAULT_UTILIZATION_WARNING_BPS: u32 = 8_000;
/// Default collateral threshold: invoices >= 10,000 USDC (7 decimals) require collateral.
const DEFAULT_COLLATERAL_THRESHOLD: i128 = 100_000_000_000; // 10,000 USDC
/// Default collateral ratio: 20% of principal (2000 bps).
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

const LEDGERS_PER_DAY: u32 = 17_280;
const ACTIVE_INVOICE_TTL: u32 = LEDGERS_PER_DAY * 365;
const COMPLETED_INVOICE_TTL: u32 = LEDGERS_PER_DAY * 30;
// Collateral records are kept for 90 days after settlement (repayment or
// seizure) so auditors and the SME have time to verify the record exists.
// This is longer than COMPLETED_INVOICE_TTL (30 days) because collateral
// disputes may arise well after the invoice is closed.
const SETTLEMENT_COLLATERAL_TTL: u32 = LEDGERS_PER_DAY * 90;
const INSTANCE_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 30;
const INSTANCE_LIFETIME_THRESHOLD: u32 = LEDGERS_PER_DAY * 7;
const UPGRADE_TIMELOCK_SECS: u64 = 86400; // 24 hours
/// Current on-chain storage schema version (#397). Bump by one and add a
/// matching arm in `run_migration` whenever the persistent storage layout
/// changes so a deployed contract can migrate state after a WASM upgrade.
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
}

#[contracttype]
#[derive(Clone, Default)]
pub struct PoolTokenTotals {
    pub pool_value: i128,
    pub total_deployed: i128,
    pub total_paid_out: i128,
    pub total_fee_revenue: i128,
    /// Cumulative interest earned per share unit, scaled by REWARD_PRECISION.
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

/// Scaling factor for reward_per_share to maintain precision with integer arithmetic.
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
#[derive(Clone)]
pub struct InvestorPosition {
    pub deposited: i128,
    pub available: i128,
    pub deployed: i128,
    pub earned: i128,
    pub deposit_count: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct FundedInvoice {
    pub invoice_id: u64,
    pub sme: Address,
    pub token: Address,
    pub principal: i128,
    pub funded_at: u64,
    /// Protocol fee locked when the invoice becomes fully funded.
    pub factoring_fee: i128,
    pub due_date: u64,
    /// Total amount repaid so far (supports partial repayments)
    pub repaid_amount: i128,
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

/// Collateral configuration: threshold above which collateral is required,
/// and the required ratio expressed in basis points of the principal.
#[contracttype]
#[derive(Clone)]
pub struct CollateralConfig {
    /// Minimum principal amount (inclusive) that triggers the collateral requirement.
    /// Invoices with principal >= this value must have collateral deposited before funding.
    pub threshold: i128,
    /// Required collateral as a fraction of principal, in basis points (e.g. 2000 = 20%).
    pub collateral_bps: u32,
}

/// Record of collateral deposited for a specific invoice.
#[contracttype]
#[derive(Clone)]
pub struct CollateralDeposit {
    /// The invoice this collateral secures.
    pub invoice_id: u64,
    /// Address that deposited the collateral (typically the SME).
    pub depositor: Address,
    /// Stablecoin token used for collateral.
    pub token: Address,
    /// Amount of collateral locked.
    pub amount: i128,
    /// Whether the collateral has been settled (returned or seized).
    pub settled: bool,
    /// When the collateral was originally posted.
    pub posted_at: u64,
    /// When the collateral was released after repayment (0 = not released).
    pub released_at: u64,
    /// When the collateral was seized after default (0 = not seized).
    pub seized_at: u64,
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
    /// Stores each investor's reward_per_share snapshot at last claim: (investor, token) -> i128
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
    /// Semantic version stored during initialize() (#237).
    ContractVersion,
    /// Migration level, incremented per migration run (#237).
    MigrationVersion,
    /// Withdrawal queue for low-liquidity scenarios (#217)
    WithdrawalQueue(Address),
    /// Withdrawal request data (#217)
    WithdrawalRequest(Address, u64), // (investor, request_id)
}

const EVT: Symbol = symbol_short!("POOL");

#[contractclient(name = "CreditScoreClient")]
pub trait CreditScoreContract {
    fn get_credit_score(env: Env, sme: Address) -> CreditScoreData;
}

/// Cross-contract interface to the invoice contract (#385, #386).
#[contractclient(name = "InvoiceContractClient")]
pub trait InvoiceContract {
    fn get_authorized_pool(env: Env) -> Address;
    fn is_invoice_defaulted(env: Env, id: u64) -> bool;
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
        panic!("contract is paused");
    }
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

/// Fixed-point exponentiation: computes `(base / precision)^exp * precision`
/// using O(log exp) fast exponentiation (exponentiation by squaring).
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
        return Ok(numerator / denominator);
    }
    let elapsed_days = elapsed_secs / 86400;
    let mut amount = principal;
    if elapsed_days > 0 {
        let daily_rate_num = yield_bps as u128 * 86400;
        let num = denominator
            .checked_add(daily_rate_num)
            .ok_or(PoolError::AmountOverflow)?;
        let growth_factor = fixed_point_pow(num, elapsed_days, denominator);
        amount = principal
            .checked_mul(growth_factor)
            .ok_or(PoolError::AmountOverflow)?
            / denominator;
    }
    let remaining_secs = elapsed_secs % 86400;
    if remaining_secs > 0 {
        let accrued = amount
            .checked_mul(yield_bps as u128)
            .and_then(|value| value.checked_mul(remaining_secs as u128))
            .ok_or(PoolError::AmountOverflow)?
            / denominator;
        amount = amount
            .checked_add(accrued)
            .ok_or(PoolError::AmountOverflow)?;
    }
    amount
        .checked_sub(principal)
        .ok_or(PoolError::AmountOverflow)
}

fn u128_to_i128(value: u128) -> PoolResult<i128> {
    if value > i128::MAX as u128 {
        return Err(PoolError::AmountOverflow);
    }
    Ok(value as i128)
}

fn calculate_factoring_fee(principal: i128, factoring_fee_bps: u32) -> PoolResult<i128> {
    let fee = (principal as u128)
        .checked_mul(factoring_fee_bps as u128)
        .ok_or(PoolError::AmountOverflow)?
        / BPS_DENOM as u128;
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
    let total_interest = calculate_interest(
        record.principal as u128,
        config.yield_bps,
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

/// #367: Retrieve token configuration including decimals, with fallback to EXPECTED_DECIMALS
fn get_token_config(env: &Env, token: &Address) -> PoolResult<TokenConfig> {
    env.storage()
        .instance()
        .get(&DataKey::TokenConfig(token.clone()))
        .ok_or(PoolError::StorageCorrupted)
}

/// #367: Normalize amount from token decimal precision to stroops (7 decimals)
fn normalize_to_stroops(amount: i128, token_decimals: u32) -> i128 {
    if token_decimals >= EXPECTED_DECIMALS {
        // If token has more decimals than 7, divide down
        amount / (10i128.pow(token_decimals - EXPECTED_DECIMALS))
    } else {
        // If token has fewer decimals than 7, multiply up
        amount * (10i128.pow(EXPECTED_DECIMALS - token_decimals))
    }
}

/// #367: Denormalize amount from stroops (7 decimals) back to token precision
fn denormalize_from_stroops(amount: i128, token_decimals: u32) -> i128 {
    if token_decimals >= EXPECTED_DECIMALS {
        // If token has more decimals than 7, multiply up
        amount * (10i128.pow(token_decimals - EXPECTED_DECIMALS))
    } else {
        // If token has fewer decimals than 7, divide down
        amount / (10i128.pow(EXPECTED_DECIMALS - token_decimals))
    }
}

/// Returns the required collateral amount for `principal` given the pool's collateral config.
/// Returns 0 if the principal is below the threshold (no collateral required).
fn get_credit_score_contract(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::CreditScoreContract)
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

fn calculate_reward_delta(total_interest: i128, total_shares: i128) -> PoolResult<i128> {
    total_interest
        .checked_mul(REWARD_PRECISION)
        .and_then(|value| value.checked_div(total_shares))
        .ok_or(PoolError::AmountOverflow)
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
    if env
        .storage()
        .persistent()
        .has(&DataKey::FundedInvoice(request.invoice_id))
    {
        return Err(PoolError::StorageCorrupted);
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
    let factoring_fee = resolve_factoring_fee(
        env,
        config,
        request.principal,
        request.sme.clone(),
        &request.token,
    )?;
    let funded = FundedInvoice {
        invoice_id: request.invoice_id,
        sme: request.sme.clone(),
        token: request.token.clone(),
        principal: request.principal,
        funded_at: now,
        factoring_fee,
        due_date: request.due_date,
        repaid_amount: 0i128,
    };

    // Persist invoice record and update totals/stats.
    env.storage()
        .persistent()
        .set(&DataKey::FundedInvoice(request.invoice_id), &funded);
    set_funded_invoice_ttl(env, request.invoice_id, false);

    tt.total_deployed = tt
        .total_deployed
        .checked_add(request.principal)
        .ok_or(PoolError::AmountOverflow)?;
    env.storage().instance().set(&token_totals_key, &tt);

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
        if utilization > config.utilization_warning_bps {
            env.events().publish(
                (EVT, symbol_short!("high_util")),
                (request.token.clone(), utilization),
            );
        }
    }

    stats.total_funded_invoices = stats
        .total_funded_invoices
        .checked_add(1)
        .ok_or(PoolError::AmountOverflow)?;
    stats.active_funded_invoices = stats
        .active_funded_invoices
        .checked_add(1)
        .ok_or(PoolError::AmountOverflow)?;

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
    Ok(())
}

/// Macro that wraps a function body with the reentrancy guard.
/// The guard is cleared after the body executes (or on error, since
/// Soroban rolls back all storage changes on panic/error).
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
            panic!("already initialized");
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

    /// Returns the semantic version of this deployed pool contract (#237).
    pub fn version(env: Env) -> PoolContractVersion {
        env.storage()
            .instance()
            .get(&DataKey::ContractVersion)
            .unwrap_or_else(parse_pool_version)
    }

    /// Returns the applied storage-schema migration level (#397).
    pub fn migration_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::MigrationVersion)
            .unwrap_or(0)
    }

    /// Run pending storage migrations after a WASM upgrade (#397).
    ///
    /// Admin-only and idempotent: once the contract has reached
    /// `CURRENT_MIGRATION_VERSION` further calls are a no-op. Each migration
    /// step transforms the persistent storage layout for one schema version
    /// and is meant to be invoked manually after `execute_upgrade`.
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
        // Pause policy: all user state-changing actions are blocked while paused,
        // including deposit, withdraw, funding, and repayment. Admin emergency
        // controls (set_yield, set_investor_kyc, unpause) remain available.
        env.storage().instance().set(&DataKey::Paused, &true);
        bump_instance(&env);
        env.events().publish((EVT, symbol_short!("paused")), admin);
        Ok(())
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), PoolError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        env.storage().instance().set(&DataKey::Paused, &false);
        bump_instance(&env);
        env.events()
            .publish((EVT, symbol_short!("unpaused")), admin);
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
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;

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

        // #367: Fetch and validate token decimals
        let token_client = token::Client::new(&env, &token);
        let token_decimals = token_client.decimals();
        if token_decimals != EXPECTED_DECIMALS {
            return Err(PoolError::UnsupportedTokenDecimals);
        }

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

    /// Removes a token from the accepted token list. Fails if:
    /// - Token has non-zero deposited balance (TokenHasActiveBalances)
    /// - Token has deployed capital (TokenHasDeployedCapital)
    /// - Token has pending withdrawals (TokenHasPendingWithdrawals)
    pub fn remove_token(env: Env, admin: Address, token: Address) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;

        let tokens: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AcceptedTokens)
            .ok_or(PoolError::NotInitialized)?;

        let mut new_tokens: Vec<Address> = Vec::new(&env);
        let mut found = false;
        for i in 0..tokens.len() {
            let t = tokens.get(i).ok_or(PoolError::StorageCorrupted)?;
            if t == token {
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
            .unwrap_or(Vec::new(&env));
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
            &Symbol::new(&env, "total_supply"),
            Vec::new(&env),
        );

        if total_shares > 0 {
            return Err(PoolError::TokenHasActiveBalances);
        }

        env.storage()
            .instance()
            .set(&DataKey::AcceptedTokens, &new_tokens);
        env.events()
            .publish((EVT, symbol_short!("rm_token")), (admin, token));
        Ok(())
    }

    pub fn deposit(
        env: Env,
        investor: Address,
        token: Address,
        amount: i128,
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

        // #109: enforce KYC check when required
        let kyc_required: bool = env
            .storage()
            .instance()
            .get(&DataKey::KycRequired)
            .unwrap_or(false);
        if kyc_required {
            let approved: bool = env
                .storage()
                .persistent()
                .get(&DataKey::InvestorKyc(investor.clone()))
                .unwrap_or(false);
            if !approved {
                return Err(PoolError::KycNotApproved);
            }
        }

        // Record balance before transfer (CEI: check-effects-interactions)
        let token_client = token::Client::new(&env, &token);
        let balance_before = token_client.balance(&env.current_contract_address());

        // Transfer tokens
        token_client.transfer(&investor, &env.current_contract_address(), &amount);

        // Verify exact amount received (handles fee-on-transfer tokens)
        let balance_after = token_client.balance(&env.current_contract_address());
        let received = balance_after.wrapping_sub(balance_before);
        if received != amount {
            return Err(PoolError::TransferMismatch);
        }

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
            .unwrap_or(InvestorPosition {
                deposited: 0,
                available: 0,
                deployed: 0,
                earned: 0,
                deposit_count: 0,
            });

        // Normalise received amount to USDC equivalent using stored exchange rate
        let rate_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ExchangeRate(token.clone()))
            .unwrap_or(10_000u32);
        let usdc_received = (received * rate_bps as i128) / 10_000i128;

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

        // #233: update investor position — track in USDC terms to match pool_value
        investor_position.deposited += usdc_received;
        investor_position.deposit_count += 1;
        env.storage()
            .persistent()
            .set(&investor_pos_key, &investor_position);

        Self::non_reentrant_end(&env);

        env.events().publish(
            (EVT, symbol_short!("deposit")),
            (investor, received, shares_to_mint, env.ledger().timestamp()),
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
        let usdc_amount = (shares * tt.pool_value) / total_shares;
        let rate_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ExchangeRate(token.clone()))
            .unwrap_or(10_000u32);
        let amount = (usdc_amount * 10_000i128) / rate_bps as i128;

        let available_liquidity = tt.pool_value - tt.total_deployed;
        if available_liquidity < usdc_amount {
            return Err(PoolError::InvalidAmount);
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

        // Transfer actual token amount LAST - interaction
        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&env.current_contract_address(), &investor, &amount);

        Self::non_reentrant_end(&env); // <- ADD GUARD END

        env.events().publish(
            (EVT, symbol_short!("withdraw")),
            (investor, amount, shares, now),
        );
        Ok(())
    }

    /// Request a withdrawal when liquidity is insufficient (#217)
    ///
    /// If liquidity is available, processes immediately like withdraw()
    /// If not, queues the request for FIFO processing when funds become available
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
            return Err(PoolError::InvalidAmount);
        }
        Self::assert_accepted_token(&env, &token)?;

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
        let amount = (shares * tt.pool_value) / total_shares;
        let available_liquidity = tt.pool_value - tt.total_deployed;

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
            // Insufficient liquidity - queue the request
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

            env.events().publish(
                (EVT, symbol_short!("wd_queue")),
                (investor, shares, request_id),
            );
        }

        Self::non_reentrant_end(&env);
        Ok(request_id)
    }

    /// Cancel a pending withdrawal request
    pub fn cancel_withdrawal_request(
        env: Env,
        investor: Address,
        request_id: u64,
    ) -> Result<(), PoolError> {
        investor.require_auth();
        bump_instance(&env);

        non_reentrant!(&env, {
            let request_key = DataKey::WithdrawalRequest(investor.clone(), request_id);
            let request: WithdrawalRequest = env
                .storage()
                .persistent()
                .get(&request_key)
                .ok_or(PoolError::WithdrawalRequestNotFound)?;

            // Remove from queue
            let queue_key = DataKey::WithdrawalQueue(request.token.clone());
            let queue: Vec<WithdrawalRequest> = env
                .storage()
                .persistent()
                .get(&queue_key)
                .unwrap_or(Vec::new(&env));

            let mut new_queue = Vec::new(&env);
            for req in queue.iter() {
                if !(req.investor == investor && req.request_id == request_id) {
                    new_queue.push_back(req);
                }
            }
            env.storage().persistent().set(&queue_key, &new_queue);

            // Remove individual request
            env.storage().persistent().remove(&request_key);

            env.events()
                .publish((EVT, symbol_short!("wd_cncl")), (investor, request_id));
            Ok(())
        })
    }

    /// Get the current withdrawal queue for a token
    pub fn get_withdrawal_queue(env: Env, token: Address) -> Vec<WithdrawalRequest> {
        bump_instance(&env);
        let queue_key = DataKey::WithdrawalQueue(token);
        env.storage()
            .persistent()
            .get(&queue_key)
            .unwrap_or(Vec::new(&env))
    }

    /// Process withdrawal immediately (helper function)
    fn process_immediate_withdrawal(
        env: &Env,
        investor: Address,
        token: Address,
        shares: i128,
        amount: i128,
        mut tt: PoolTokenTotals,
        share_token: Address,
    ) -> Result<(), PoolError> {
        // Burn shares FIRST
        let mut burn_args = Vec::new(env);
        burn_args.push_back(investor.clone().into_val(env));
        burn_args.push_back(shares.into_val(env));
        let _: () = env.invoke_contract(&share_token, &Symbol::new(env, "burn"), burn_args);

        // Update state
        tt.pool_value -= amount;
        let token_totals_key = DataKey::TokenTotals(token.clone());
        env.storage().instance().set(&token_totals_key, &tt);

        // Transfer LAST
        let token_client = token::Client::new(env, &token);
        token_client.transfer(&env.current_contract_address(), &investor, &amount);

        env.events()
            .publish((EVT, symbol_short!("wd_full")), (investor, amount, shares));
        Ok(())
    }

    /// Generate unique request ID for withdrawal requests
    fn generate_request_id(env: &Env, token: &Address) -> u64 {
        let counter_key = DataKey::WithdrawalQueue(token.clone());
        let current_count: u64 = env.storage().persistent().get(&counter_key).unwrap_or(0);
        let new_id = current_count + 1;
        env.storage().persistent().set(&counter_key, &new_id);
        new_id
    }

    /// Process withdrawal queue after repayments (call from repay_invoice)
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

        let mut processed = Vec::new(env);
        let mut remaining_amount = available_amount;
        let mut tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&DataKey::TokenTotals(token.clone()))
            .unwrap_or_default();

        // Compute total shares once for proportional withdrawals.
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

        for request in queue.iter() {
            let request_amount = (request.shares * tt.pool_value) / total_shares;
            if remaining_amount >= request_amount {
                // Process this request
                // `share_token` already resolved above.

                // Burn shares
                let mut burn_args = Vec::new(env);
                burn_args.push_back(request.investor.clone().into_val(env));
                burn_args.push_back(request.shares.into_val(env));
                let _: () = env.invoke_contract(&share_token, &Symbol::new(env, "burn"), burn_args);

                // Update pool totals
                tt.pool_value -= request_amount;
                remaining_amount -= request_amount;

                // Transfer tokens
                let token_client = token::Client::new(env, &token);
                token_client.transfer(
                    &env.current_contract_address(),
                    &request.investor,
                    &request_amount,
                );

                // Remove individual request
                let request_key =
                    DataKey::WithdrawalRequest(request.investor.clone(), request.request_id);
                env.storage().persistent().remove(&request_key);

                env.events().publish(
                    (EVT, symbol_short!("wd_full")),
                    (request.investor, request_amount, request.shares),
                );
            } else {
                // Can't process this request, keep it in queue
                processed.push_back(request);
            }
        }

        // Update queue with remaining unprocessed requests
        env.storage().persistent().set(&queue_key, &processed);

        // Update token totals
        let token_totals_key = DataKey::TokenTotals(token);
        env.storage().instance().set(&token_totals_key, &tt);

        Ok(())
    }

    /// Claim accrued yield for `investor` on `token`.
    ///
    /// Uses a reward-per-share accumulator pattern: each fully-repaid invoice
    /// increments `reward_per_share`; investors claim the delta since their last
    /// snapshot proportional to their share balance.
    pub fn claim_yield(env: Env, investor: Address, token: Address) -> Result<(), PoolError> {
        investor.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);

        non_reentrant!(&env, {
            let token_totals_key = DataKey::TokenTotals(token.clone());
            let tt: PoolTokenTotals = env
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

            // Update snapshot before transfer (checks-effects-interactions).
            env.storage()
                .persistent()
                .set(&snapshot_key, &tt.reward_per_share);

            if claimable > 0 {
                let token_client = token::Client::new(&env, &token);
                token_client.transfer(&env.current_contract_address(), &investor, &claimable);
            }

            env.events().publish(
                (EVT, symbol_short!("yld_claim")),
                (investor, token, claimable),
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

        if env
            .storage()
            .persistent()
            .has(&DataKey::FundedInvoice(invoice_id))
        {
            return Err(PoolError::StorageCorrupted);
        }
        let accepted_tokens: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AcceptedTokens)
            .ok_or(PoolError::NotInitialized)?;

        // Collateral check: high-value invoices must have collateral deposited first.
        let collateral_cfg: CollateralConfig = env
            .storage()
            .instance()
            .get(&DataKey::CollateralConfig)
            .unwrap_or(CollateralConfig {
                threshold: DEFAULT_COLLATERAL_THRESHOLD,
                collateral_bps: DEFAULT_COLLATERAL_BPS,
            });
        let req_collateral = required_collateral(principal, &collateral_cfg);
        if req_collateral > 0 {
            let deposit: Option<CollateralDeposit> = env
                .storage()
                .persistent()
                .get(&DataKey::CollateralDeposit(invoice_id));
            match deposit {
                None => return Err(PoolError::CollateralNotFound),
                Some(d) => {
                    if d.settled {
                        return Err(PoolError::CollateralAlreadySettled);
                    }
                    if d.amount < req_collateral {
                        return Err(PoolError::InvalidAmount);
                    }
                }
            }
        }

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
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;

        Self::non_reentrant_start(&env);

        if requests.is_empty() {
            return Err(PoolError::InvalidAmount);
        }
        if requests.len() > MAX_BATCH_SIZE {
            return Err(PoolError::BatchTooLarge);
        }

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

        Self::non_reentrant_start(env); // <- ADD GUARD START

        let config: PoolConfig = get_config_cached(env)?;
        let funded_invoice_key = DataKey::FundedInvoice(invoice_id);
        let mut record: FundedInvoice = env
            .storage()
            .persistent()
            .get(&funded_invoice_key)
            .ok_or(PoolError::InvoiceNotFound)?;

        let now = env.ledger().timestamp();
        let (total_interest, total_due) = calculate_total_due(&record, &config, now)?;
        let total_interest_i128 = u128_to_i128(total_interest)?;

        if record.repaid_amount >= total_due {
            return Err(PoolError::AlreadyFullyRepaid);
        }
        let new_repaid_amount = record
            .repaid_amount
            .checked_add(amount)
            .ok_or(PoolError::AmountOverflow)?;
        if new_repaid_amount > total_due {
            return Err(PoolError::Overpayment);
        }

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

        if fully_repaid {
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
        token_client.transfer(&payer, &env.current_contract_address(), &amount);

        // Handle collateral release after main transfer
        if fully_repaid {
            release_collateral(env, invoice_id, &payer, now);
        }

        Self::non_reentrant_end(env); // <- ADD GUARD END

        if fully_repaid {
            // #217: Process withdrawal queue after repayment
            let available_amount = total_interest_i128
                .checked_add(record.factoring_fee)
                .ok_or(PoolError::AmountOverflow)?;
            if let Err(e) =
                Self::process_withdrawal_queue(env, record.token.clone(), available_amount)
            {
                // Log error but don't fail the repayment
                // `format!` is unavailable in `no_std`; keep a lightweight log.
                let _ = e;
                env.logs().add("Failed to process withdrawal queue", &[]);
            }

            env.events().publish(
                (EVT, symbol_short!("repaid")),
                (invoice_id, record.principal, total_interest_i128, now),
            );
        } else {
            env.events().publish(
                (EVT, symbol_short!("part_pay")),
                (invoice_id, amount, record.repaid_amount, now),
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
        Self::require_not_paused(&env);
        Self::repay_invoice_request(&env, invoice_id, payer, amount)
    }

    // ---- Collateral management ----

    /// Admin sets the collateral configuration.
    /// `threshold` — minimum principal (inclusive) that requires collateral.
    /// `collateral_bps` — required collateral as % of principal in basis points (max 10000 = 100%).
    pub fn set_collateral_config(
        env: Env,
        admin: Address,
        threshold: i128,
        collateral_bps: u32,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin)?;
        if threshold < 0 {
            return Err(PoolError::InvalidThreshold);
        }
        if collateral_bps == 0 || collateral_bps > BPS_DENOM {
            return Err(PoolError::InvalidThreshold);
        }
        let cfg = CollateralConfig {
            threshold,
            collateral_bps,
        };
        env.storage()
            .instance()
            .set(&DataKey::CollateralConfig, &cfg);
        env.events().publish(
            (EVT, symbol_short!("col_cfg")),
            (admin, threshold, collateral_bps),
        );
        Ok(())
    }

    /// Returns the current collateral configuration.
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

    /// Returns the required collateral amount for a given principal under current config.
    /// Returns 0 if no collateral is required.
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

    /// SME (or any party) deposits collateral for a high-value invoice before it can be funded.
    /// The collateral is held by the pool contract until the invoice is repaid (returned)
    /// or defaulted (seized to protect investors).
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

            let record = CollateralDeposit {
                invoice_id,
                depositor: depositor.clone(),
                token: token.clone(),
                amount,
                settled: false,
                posted_at: env.ledger().timestamp(),
                released_at: 0,
                seized_at: 0,
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

            env.events().publish(
                (EVT, symbol_short!("col_dep")),
                (
                    invoice_id,
                    depositor.clone(),
                    token,
                    amount,
                    depositor,
                    env.ledger().timestamp(),
                ),
            );
            Ok(())
        })
    }

    /// Returns the collateral deposit record for an invoice, if any.
    pub fn get_collateral_deposit(env: Env, invoice_id: u64) -> Option<CollateralDeposit> {
        bump_instance(&env);
        env.storage()
            .persistent()
            .get(&DataKey::CollateralDeposit(invoice_id))
    }

    /// Admin seizes collateral for a defaulted invoice, transferring it to the pool
    /// to partially compensate investors for the loss.
    /// Can only be called after the invoice has been marked as defaulted (repaid == false
    /// and the invoice is past due + grace period).
    pub fn seize_collateral(env: Env, admin: Address, invoice_id: u64) -> Result<(), PoolError> {
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

            if record.repaid_amount >= total_due {
                return Err(PoolError::AlreadyFullyRepaid);
            }

            // #386: require the invoice to be explicitly in Defaulted status before seizing.
            // Prevents premature seizure on invoices that are merely overdue but not yet defaulted.
            let invoice_client = InvoiceContractClient::new(&env, &config.invoice_contract);
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

            // The seized collateral reduces the effective loss: add it to pool_value and
            // reduce total_deployed by the original principal (the invoice is now a loss).
            tt.pool_value += col.amount;
            tt.total_deployed -= record.principal;
            env.storage().instance().set(&token_totals_key, &tt);

            col.settled = true;
            col.seized_at = now;
            env.storage()
                .persistent()
                .set(&DataKey::CollateralDeposit(invoice_id), &col);
            // Use SETTLEMENT_COLLATERAL_TTL (90 days) so the seizure record
            // outlives COMPLETED_INVOICE_TTL (30 days) and remains queryable
            // for the full post-default audit window.
            env.storage().persistent().extend_ttl(
                &DataKey::CollateralDeposit(invoice_id),
                SETTLEMENT_COLLATERAL_TTL,
                SETTLEMENT_COLLATERAL_TTL,
            );

            // #386: emit status-triggered seizure event (invoice was Defaulted).
            env.events().publish(
                (EVT, Symbol::new(&env, "col_seiz_default")),
                (invoice_id, col.depositor, col.amount, admin, now),
            );
            Ok(())
        })
    }

    /// Direct yield setter (single-step, subject to cooldown and max-step guards).
    /// Used in tests and for small adjustments that don't require the full timelock flow.
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

    /// Admin proposes a new yield rate.
    /// Stores (proposed_yield_bps, proposal_timestamp) and emits an event.
    /// Minimum delay: 48 hours (configurable via set_yield_timelock()).
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

    /// Anyone can call after the delay period has passed.
    /// Reads the proposal, verifies delay, updates yield_bps, clears proposal.
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

    /// Admin can cancel a pending yield proposal before execution.
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

    /// Set the yield change policy: cooldown, max step, and timelock duration.
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
            env.storage()
                .persistent()
                .get(&pos_key)
                .unwrap_or(InvestorPosition {
                    deposited: 0,
                    available: 0,
                    deployed: 0,
                    earned: 0,
                    deposit_count: 0,
                });
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

    // ---- #247: co-fund share transfer (secondary market) ----

    /// Returns the co-fund share (in bps, 0-10_000) that `investor` holds in `invoice_id`.
    pub fn get_co_fund_share(env: Env, invoice_id: u64, investor: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::CoFundShare(invoice_id, investor))
            .unwrap_or(0)
    }

    /// Transfer `bps` basis points of the caller's co-fund share in `invoice_id` to `to`.
    /// bps=10_000 transfers 100% of the caller's share.
    /// Only allowed on invoices that are currently funded (not yet fully repaid).
    /// If KYC is enabled on the pool, `to` must be KYC-approved.
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

        if bps == 0 || bps > BPS_DENOM {
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

            // KYC check on recipient if pool requires it
            let kyc_required: bool = env
                .storage()
                .instance()
                .get(&DataKey::KycRequired)
                .unwrap_or(false);
            if kyc_required {
                let approved: bool = env
                    .storage()
                    .persistent()
                    .get(&DataKey::InvestorKyc(to.clone()))
                    .unwrap_or(false);
                if !approved {
                    return Err(PoolError::KycNotApproved);
                }
            }

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

    /// #275: returns utilization for a token in basis points (0-10_000).
    pub fn get_utilization(env: Env, token: Address) -> u32 {
        let tt = Self::get_token_totals(env, token);
        if tt.pool_value <= 0 {
            return 0;
        }
        ((tt.total_deployed as u128 * 10_000u128) / tt.pool_value as u128) as u32
    }

    /// #275: admin setter for max utilization (bps).
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

    pub fn estimate_repayment(env: Env, invoice_id: u64) -> Result<i128, PoolError> {
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

        let now = env.ledger().timestamp();
        let (_interest, total_due) = calculate_total_due(&record, &config, now)?;
        // Return remaining amount due (total - already repaid)
        let remaining = total_due - record.repaid_amount;
        if remaining < 0 {
            Ok(0)
        } else {
            Ok(remaining)
        }
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

    /// Set the USD exchange rate for a token (in bps, e.g. 10000 = 1:1 with USD).
    /// Used to normalise pool value across stablecoins for display/reporting.
    /// Oracle-backed validation is a planned follow-up; for now the admin must
    /// set explicit per-token bounds before changing a rate.
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

    /// Returns the USD exchange rate for `token` in bps (defaults to 10000 = 1:1).
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

    // ---- #109: Investor KYC / whitelist methods ----

    /// Toggle whether KYC is required before depositing.
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

    /// Returns whether KYC is currently required.
    pub fn kyc_required(env: Env) -> bool {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::KycRequired)
            .unwrap_or(false)
    }

    /// Approve or revoke a specific investor's KYC status.
    pub fn set_investor_kyc(
        env: Env,
        admin: Address,
        investor: Address,
        approved: bool,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        env.storage()
            .persistent()
            .set(&DataKey::InvestorKyc(investor.clone()), &approved);
        env.events()
            .publish((EVT, symbol_short!("kyc_set")), (admin, investor, approved));
        Ok(())
    }

    /// Returns whether `investor` has been KYC-approved.
    pub fn get_investor_kyc(env: Env, investor: Address) -> bool {
        bump_instance(&env);
        env.storage()
            .persistent()
            .get(&DataKey::InvestorKyc(investor))
            .unwrap_or(false)
    }

    pub fn propose_upgrade(
        env: Env,
        admin: Address,
        wasm_hash: BytesN<32>,
    ) -> Result<(), PoolError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        env.storage()
            .instance()
            .set(&DataKey::ProposedWasmHash, &wasm_hash);
        env.storage()
            .instance()
            .set(&DataKey::UpgradeScheduledAt, &env.ledger().timestamp());
        env.events().publish(
            (EVT, symbol_short!("upg_prop")),
            (admin, env.ledger().timestamp() + UPGRADE_TIMELOCK_SECS),
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
        let now = env.ledger().timestamp();
        if now < scheduled_at + UPGRADE_TIMELOCK_SECS {
            return Err(PoolError::InvalidAmount);
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
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        BytesN, Env, IntoVal,
    };

    #[contract]
    pub struct DummyShare;
    #[contractimpl]
    impl DummyShare {
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

    // #367: Test token with 6 decimals (non-standard)
    #[contract]
    pub struct DummyToken6Decimals;
    #[contractimpl]
    impl DummyToken6Decimals {
        pub fn decimals(_env: Env) -> u32 {
            6
        }
    }

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
            let stored: Option<bool> = env.storage().persistent().get(&DataKey::FundedInvoice(id));
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
    }

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

    #[test]
    fn test_vault_deposit_withdraw() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, share_token) = setup(&env);
        let investor1 = Address::generate(&env);
        let investor2 = Address::generate(&env);

        mint(&env, &usdc_id, &investor1, 1000);
        mint(&env, &usdc_id, &investor2, 1000);

        client.deposit(&investor1, &usdc_id, &1000);

        let shares1: i128 = env.invoke_contract(
            &share_token,
            &Symbol::new(&env, "balance"),
            soroban_sdk::vec![&env, investor1.clone().into_val(&env)],
        );
        assert_eq!(shares1, 1000);

        let tt = client.get_token_totals(&usdc_id);
        assert_eq!(tt.pool_value, 1000);

        client.deposit(&investor2, &usdc_id, &500);

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

        client.deposit(&investor, &usdc_id, &10000);
        client.fund_invoice(
            &admin,
            &1u64,
            &5000i128,
            &sme,
            &(env.ledger().timestamp() + 200_000),
            &usdc_id,
        );

        env.ledger().with_mut(|l| l.timestamp += 100_000);
        let amount_due = client.estimate_repayment(&1u64);
        client.repay_invoice(&1u64, &sme, &amount_due);

        let tt = client.get_token_totals(&usdc_id);
        assert!(tt.pool_value > 10000);

        // When investor withdraws their 10000 shares, they should get > 10000 underlying!
        client.withdraw(&investor, &usdc_id, &10000);
        let bal = soroban_sdk::token::Client::new(&env, &usdc_id).balance(&investor);
        assert_eq!(bal, tt.pool_value); // Investor got everything because they owned 100% shares
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
        client.deposit(&investor, &usdc_id, &principal);
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

        let expected_interest =
            (principal as u128 * DEFAULT_YIELD_BPS as u128 * (30 * 86_400) as u128)
                / (BPS_DENOM as u128 * SECS_PER_YEAR as u128);
        let expected_total_due = principal + expected_interest as i128 + expected_fee;

        assert_eq!(client.estimate_repayment(&1u64), expected_total_due);

        client.repay_invoice(&1u64, &sme, &expected_total_due);

        let tt = client.get_token_totals(&usdc_id);
        assert_eq!(tt.total_fee_revenue, expected_fee);
        assert_eq!(tt.total_paid_out, expected_total_due);
        // pool_value grew by the yield
        assert!(tt.pool_value >= principal);
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
        client.deposit(&investor, &usdc_id, &1_000_000_000);

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
        let result = client.try_deposit(&investor, &usdc_id, &0i128);
        assert_eq!(result, Err(Ok(PoolError::ZeroAmount)));
    }

    #[test]
    fn test_deposit_negative_amount_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let result = client.try_deposit(&investor, &usdc_id, &-100i128);
        assert_eq!(result, Err(Ok(PoolError::NegativeAmount)));
    }

    #[test]
    fn test_deposit_non_whitelisted_token_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let unknown_token = Address::generate(&env);
        let result = client.try_deposit(&investor, &unknown_token, &1_000i128);
        assert_eq!(result, Err(Ok(PoolError::TokenNotAccepted)));
    }

    #[test]
    fn test_withdraw_zero_shares_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 1_000);
        client.deposit(&investor, &usdc_id, &1_000);
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
        client.deposit(&investor, &usdc_id, &500);
        // Attempt to withdraw more shares than owned
        let result = client.try_withdraw(&investor, &usdc_id, &1_000i128);
        assert_eq!(result, Err(Ok(PoolError::InvalidAmount)));
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
        client.deposit(&investor, &usdc_id, &500);
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
    fn test_fund_invoice_duplicate_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 2_000);
        client.deposit(&investor, &usdc_id, &2_000);
        client.fund_invoice(
            &admin,
            &1u64,
            &500i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        // Second fund on same invoice_id must return StorageCorrupted
        let result = client.try_fund_invoice(
            &admin,
            &1u64,
            &500i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        assert_eq!(result, Err(Ok(PoolError::StorageCorrupted)));
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
        client.deposit(&investor, &usdc_id, &1_000);
        client.fund_invoice(
            &admin,
            &1u64,
            &1_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        let amount_due = client.estimate_repayment(&1u64);
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
        client.remove_token(&admin, &new_token);
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
        client.deposit(&investor, &usdc_id, &1_000);
        // pool has a non-zero balance — token removal must fail
        let result = client.try_remove_token(&admin, &usdc_id);
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
        client.remove_token(&admin, &new_token);

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
        client.deposit(&investor, &new_token, &1_000);

        // Attempt to remove token with deposited balance should fail
        let result = client.try_remove_token(&admin, &new_token);
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

        client.deposit(&investor, &new_token, &2_000);
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

        // Attempt to remove token with deployed capital should fail
        let result = client.try_remove_token(&admin, &new_token);
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
        client.set_collateral_config(&admin, &50_000_000_000i128, &1_000u32);
        let cfg = client.get_collateral_config();
        assert_eq!(cfg.threshold, 50_000_000_000i128);
        assert_eq!(cfg.collateral_bps, 1_000u32);
    }

    #[test]
    fn test_set_collateral_config_over_100_percent_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let result = client.try_set_collateral_config(&admin, &1_000i128, &10_001u32);
        assert_eq!(result, Err(Ok(PoolError::InvalidThreshold)));
    }

    #[test]
    fn test_set_collateral_config_zero_collateral_bps_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let result = client.try_set_collateral_config(&admin, &1_000i128, &0u32);
        assert_eq!(result, Err(Ok(PoolError::InvalidThreshold)));
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
        client.set_collateral_config(&admin, &5_000_000_000i128, &2_000u32);
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
        client.deposit(&investor, &usdc_id, &5_000);

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
        client.set_collateral_config(&admin, &1_000i128, &2_000u32);

        mint(&env, &usdc_id, &investor, 10_000);
        client.deposit(&investor, &usdc_id, &10_000);

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
        client.set_collateral_config(&admin, &1_000i128, &2_000u32);

        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal); // 1000
        assert_eq!(required, 1_000);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, required);

        client.deposit(&investor, &usdc_id, &10_000);

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
    fn test_collateral_returned_on_repayment() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        client.set_collateral_config(&admin, &1_000i128, &2_000u32);

        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, principal * 2 + required);

        client.deposit(&investor, &usdc_id, &10_000);
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

        let amount_due = client.estimate_repayment(&1u64);
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

        client.set_collateral_config(&admin, &1_000i128, &2_000u32);
        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, principal * 2 + required);

        client.deposit(&investor, &usdc_id, &10_000);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &required);
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );

        let amount_due = client.estimate_repayment(&1u64);
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

        client.set_collateral_config(&admin, &1_000i128, &2_000u32);
        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, required);

        client.deposit(&investor, &usdc_id, &10_000);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &required);
        let due_date = env.ledger().timestamp() + 1_000;
        client.fund_invoice(&admin, &1u64, &principal, &sme, &due_date, &usdc_id);

        // Mark invoice defaulted via dummy invoice contract
        DummyInvoiceClient::new(&env, &invoice_contract).set_invoice_defaulted(&1u64, &true);

        client.seize_collateral(&admin, &1u64).unwrap();

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
        client.deposit(&investor, &usdc_id, &10_000);

        let initial_due_date = env.ledger().timestamp() + SECS_PER_DAY;
        client.fund_invoice(&admin, &1u64, &5_000i128, &sme, &initial_due_date, &usdc_id);

        env.ledger()
            .with_mut(|l| l.timestamp = initial_due_date + (5 * SECS_PER_DAY));
        let capped_amount = client.estimate_repayment(&1u64);

        let extended_due_date = initial_due_date + (10 * SECS_PER_DAY);
        client.update_invoice_due_date(&invoice_contract, &1u64, &extended_due_date);
        let extended_amount = client.estimate_repayment(&1u64);

        assert!(extended_amount > capped_amount);
    }

    #[test]
    fn test_double_deposit_collateral_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let sme = Address::generate(&env);

        client.set_collateral_config(&admin, &1_000i128, &2_000u32);
        mint(&env, &usdc_id, &sme, 5_000);

        client.deposit_collateral(&1u64, &sme, &usdc_id, &1_000);
        let result = client.try_deposit_collateral(&1u64, &sme, &usdc_id, &1_000);
        assert_eq!(result, Err(Ok(PoolError::StorageCorrupted)));
    }

    #[test]
    fn test_insufficient_collateral_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        // 20% collateral required on anything >= 1000
        client.set_collateral_config(&admin, &1_000i128, &2_000u32);

        let principal: i128 = 5_000;
        // Required = 1000, but we only deposit 500
        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, 500);

        client.deposit(&investor, &usdc_id, &10_000);
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

        client.set_collateral_config(&admin, &1_000i128, &2_000u32);
        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, principal * 2 + required);

        client.deposit(&investor, &usdc_id, &10_000);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &required);
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        let amount_due = client.estimate_repayment(&1u64);
        client.repay_invoice(&1u64, &sme, &amount_due);

        // Trying to seize after repayment must return AlreadyFullyRepaid
        let result = client.try_seize_collateral(&admin, &1u64);
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
        client.deposit(&investor, &usdc_id, &2_000);

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
        client.deposit(&investor, &usdc_id, &10_000);

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
        client.deposit(&investor, &usdc_id, &10_000);

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
            let amount = client.estimate_repayment(&invoice_id);
            repayments.push_back(RepaymentRequest { invoice_id, amount });
        }
        client.repay_invoices_batch(&sme, &repayments);

        for invoice_id in 1u64..=3u64 {
            let record = client.get_funded_invoice(&invoice_id).unwrap();
            assert_eq!(record.repaid_amount, record.principal);
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

        client.set_collateral_config(&admin, &1_000i128, &2_000u32);
        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal);
        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, required);
        client.deposit(&investor, &usdc_id, &10_000);
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
        client.deposit(&investor, &usdc_id, &1_000);
        client.fund_invoice(
            &admin,
            &1u64,
            &1_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        let amount_due = client.estimate_repayment(&1u64);
        client.repay_invoice(&1u64, &sme, &amount_due);
        let attacker = Address::generate(&env);
        let result = client.try_cleanup_funded_invoice(&attacker, &1u64);
        assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
    }

    // --- Pause mechanism tests ---

    #[test]
    #[should_panic(expected = "contract is paused")]
    fn test_fund_invoice_when_paused_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 2_000);
        client.deposit(&investor, &usdc_id, &2_000);
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
    #[should_panic(expected = "contract is paused")]
    fn test_repay_invoice_when_paused_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 1_000);
        mint(&env, &usdc_id, &sme, 2_000);
        client.deposit(&investor, &usdc_id, &1_000);
        client.fund_invoice(
            &admin,
            &1u64,
            &1_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        client.pause(&admin);
        let amount_due = client.estimate_repayment(&1u64);
        client.repay_invoice(&1u64, &sme, &amount_due);
    }

    #[test]
    #[should_panic(expected = "contract is paused")]
    fn test_deposit_collateral_when_paused_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let sme = Address::generate(&env);

        client.set_collateral_config(&admin, &1_000i128, &2_000u32);
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
        client.deposit(&investor, &usdc_id, &2_000);

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
        let amount_due = client.estimate_repayment(&1u64);
        client.repay_invoice(&1u64, &sme, &amount_due);
        let fi = client.get_funded_invoice(&1u64).unwrap();
        assert!(fi.repaid_amount >= amount_due);
    }

    #[test]
    #[should_panic(expected = "contract is paused")]
    fn test_deposit_blocked_when_paused() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 1_000);

        client.pause(&admin);
        client.deposit(&investor, &usdc_id, &1_000);
    }

    #[test]
    #[should_panic(expected = "contract is paused")]
    fn test_withdraw_blocked_when_paused() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 1_000);
        client.deposit(&investor, &usdc_id, &1_000);
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
        let result = client.try_deposit(&investor, &usdc_id, &1_000);
        assert_eq!(result, Err(Ok(PoolError::KycNotApproved)));
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
        client.deposit(&investor, &usdc_id, &1_000);

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
        client.deposit(&investor, &usdc_id, &1_000);

        // Revoke KYC — subsequent deposit must be blocked
        client.set_investor_kyc(&admin, &investor, &false);
        let result = client.try_deposit(&investor, &usdc_id, &1_000);
        assert_eq!(result, Err(Ok(PoolError::KycNotApproved)));
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
        client.deposit(&investor, &usdc_id, &500);

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
        let blocked = client.try_deposit(&investor, &usdc_id, &1_000);
        assert_eq!(blocked, Err(Ok(PoolError::KycNotApproved)));

        client.set_kyc_required(&admin, &false);
        client.deposit(&investor, &usdc_id, &1_000);

        client.set_kyc_required(&admin, &true);
        let blocked_again = client.try_deposit(&investor, &usdc_id, &1_000);
        assert_eq!(blocked_again, Err(Ok(PoolError::KycNotApproved)));
    }

    #[test]
    #[should_panic(expected = "contract is paused")]
    fn test_deposit_when_paused_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        client.pause(&admin);
        mint(&env, &usdc_id, &investor, 1000);
        client.deposit(&investor, &usdc_id, &1000); // Should panic
    }

    #[test]
    #[should_panic(expected = "contract is paused")]
    fn test_withdraw_when_paused_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 1000);
        client.deposit(&investor, &usdc_id, &1000);
        client.pause(&admin);
        client.withdraw(&investor, &usdc_id, &500); // Should panic
    }

    #[test]
    fn test_pause_events_emitted() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);

        client.pause(&admin);
        assert!(client.is_paused());

        client.unpause(&admin);
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

        client.deposit(&investor, &usdc_id, &10_000);
        client.fund_invoice(
            &admin,
            &1u64,
            &5_000i128,
            &sme,
            &(env.ledger().timestamp() + 50_000),
            &usdc_id,
        );

        env.ledger().with_mut(|l| l.timestamp += 10_000);
        let total_due = client.estimate_repayment(&1u64);
        let half = total_due / 2;

        // First partial payment
        client.repay_invoice(&1u64, &sme, &half);
        let fi = client.get_funded_invoice(&1u64).unwrap();
        assert_eq!(fi.repaid_amount, half);

        // Invoice still active — total_deployed unchanged
        let tt = client.get_token_totals(&usdc_id);
        assert_eq!(tt.total_deployed, 5_000i128);

        // Second payment clears the rest
        let remaining = client.estimate_repayment(&1u64);
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

        client.deposit(&investor, &usdc_id, &5_000);
        client.fund_invoice(
            &admin,
            &1u64,
            &3_000i128,
            &sme,
            &(env.ledger().timestamp() + 50_000),
            &usdc_id,
        );

        env.ledger().with_mut(|l| l.timestamp += 5_000);
        let total_due = client.estimate_repayment(&1u64);

        // Partial payment — less than total
        client.repay_invoice(&1u64, &sme, &(total_due / 3));

        // Invoice record still exists; pool still shows it as deployed
        let fi = client.get_funded_invoice(&1u64).unwrap();
        assert!(fi.repaid_amount < total_due);
        let tt = client.get_token_totals(&usdc_id);
        assert_eq!(tt.total_deployed, 3_000i128);
    }

    #[test]
    fn test_overpayment_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 5_000);
        mint(&env, &usdc_id, &sme, 10_000);

        client.deposit(&investor, &usdc_id, &5_000);
        client.fund_invoice(
            &admin,
            &1u64,
            &2_000i128,
            &sme,
            &(env.ledger().timestamp() + 50_000),
            &usdc_id,
        );

        env.ledger().with_mut(|l| l.timestamp += 5_000);
        let total_due = client.estimate_repayment(&1u64);

        // Attempt to pay more than due
        let result = client.try_repay_invoice(&1u64, &sme, &(total_due + 1));
        assert_eq!(result, Err(Ok(PoolError::Overpayment)));
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

        client.deposit(&investor, &usdc_id, &5_000);
        client.fund_invoice(
            &admin,
            &1u64,
            &2_000i128,
            &sme,
            &(env.ledger().timestamp() + 50_000),
            &usdc_id,
        );

        env.ledger().with_mut(|l| l.timestamp += 5_000);
        let total_due = client.estimate_repayment(&1u64);
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
        client.deposit(&investor, &usdc_id, &1_000);
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
        client.deposit(&investor, &usdc_id, &10_000);
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
        client.deposit(&investor, &usdc_id, &10_000);

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
        client.deposit(&alice, &usdc_id, &1000);

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
        let total_due = client.estimate_repayment(&1u64);
        client.repay_invoice(&1u64, &sme, &total_due);

        let tt = client.get_token_totals(&usdc_id);
        let pool_value_after_yield = tt.pool_value;
        assert!(pool_value_after_yield > 1000);

        // Bob deposits an amount equal to the pool value
        let bob_deposit = pool_value_after_yield;
        mint(&env, &usdc_id, &bob, bob_deposit);
        client.deposit(&bob, &usdc_id, &bob_deposit);

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
        client.deposit(&investor, &usdc_id, &1000);
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 86400),
            &usdc_id,
        );

        let funded = client.get_funded_invoice(&1u64).unwrap();
        let expected_fee = (principal as u128 * fee_bps as u128) / BPS_DENOM as u128;
        assert_eq!(funded.factoring_fee, expected_fee as i128);
        // Fee must be ≤ principal for small amounts
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
        client.deposit(&investor, &usdc_id, &principal);
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
        client.deposit(&investor, &usdc_id, &1000);
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
        client.deposit(&investor, &usdc_id, &1000);
    }

    #[test]
    fn test_reentrancy_guard_releases_after_call() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 1000);
        client.deposit(&investor, &usdc_id, &1000);

        // After a successful deposit, the guard should be released
        let result = client.try_deposit(&investor, &usdc_id, &1000);
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
        client.deposit(&investor, &usdc_id, &10000);
        client.fund_invoice(
            &admin,
            &1u64,
            &5000,
            &sme,
            &(env.ledger().timestamp() + 86400),
            &usdc_id,
        );
        env.ledger().with_mut(|l| l.timestamp += 100_000);
        let amount_due = client.estimate_repayment(&1u64);
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

        client.set_collateral_config(&admin, &1_000i128, &2_000u32);
        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, required);
        client.deposit(&investor, &usdc_id, &10_000);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &required);

        let due_date = env.ledger().timestamp() + 10_000;
        client.fund_invoice(&admin, &1u64, &principal, &sme, &due_date, &usdc_id);
        // Mark invoice as defaulted for the test dummy
        let invoice_contract_id = client.get_config().invoice_contract;
        DummyInvoiceClient::new(&env, &invoice_contract_id).set_invoice_defaulted(&1u64, &true);
        env.ledger().with_mut(|l| l.timestamp = due_date + 1);

        // seize_collateral should succeed — guard acquired and released
        client.seize_collateral(&admin, &1u64);
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

        client.set_collateral_config(&admin, &1_000i128, &2_000u32);
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
        client.deposit(&investor, &usdc_id, &5_000);
        client.fund_invoice(
            &admin,
            &1u64,
            &5_000,
            &sme,
            &(env.ledger().timestamp() + 86400),
            &usdc_id,
        );
        let amount_due = client.estimate_repayment(&1u64);
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
        client.deposit(&investor, &usdc_id, &10_000);
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
        client.cancel_withdrawal_request(&investor, &request_id);
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
        client.deposit(&investor, &usdc_id, &10_000);
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
}
