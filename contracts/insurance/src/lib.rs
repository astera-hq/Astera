#![no_std]

#[cfg(test)]
extern crate std;

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, panic_with_error,
    symbol_short, token, Address, Env, Symbol, Vec,
};

// === Issue #866: protocol-level default insurance reserve ===
//
// Design choices (documented per the issue's explicit asks):
//
// - `purchase_coverage` is paid by an explicit `payer` address rather than always the SME.
//   The pool contract calls it non-fatally right after `fund_invoice_request` succeeds,
//   passing `payer = pool's own contract address` and drawing the premium out of that
//   token's accrued `protocol_revenue` (see contracts/pool/src/lib.rs). This avoids
//   needing the SME as a co-signer inside `fund_invoice` (which is admin-initiated and
//   the SME is not part of the auth stack), while still satisfying "risk-based premiums
//   funded by protocol revenue" from the issue description. `payer` is a parameter (not
//   hardcoded) so a future flow could let the SME pay directly by passing its own address
//   and signing. `due_date` is likewise a caller-supplied parameter rather than looked up
//   via a callback into pool — see the reentrancy note below.
// - `file_claim` pays the pool contract directly (not individual investors). The payout is
//   a plain token transfer from the reserve to the pool, followed by a call into
//   `pool.receive_insurance_payout` so the pool credits that token's `pool_value` — the
//   same accounting bucket collateral seizure already credits. This is the "simpler,
//   generally-correct" design named in the issue: it reuses the pool's existing
//   pro-rata share accounting instead of insurance needing to know about tranches/investors.
// - `file_claim` is permissionless (any caller) since it independently re-derives the
//   default status and shortfall from the invoice/pool contracts rather than trusting the
//   caller. Pool does *not* call it internally, and `purchase_coverage` does *not* call back
//   into pool for invoice data either: Soroban disallows A→B→A re-entrancy, and both of
//   these flows are themselves invoked *from* pool (purchase_coverage from
//   `fund_invoice_request`, file_claim would have been called from
//   `execute_seize_collateral`) — a callback into pool while pool is still on the call
//   stack would trap. So `purchase_coverage` takes `due_date` directly as a parameter, and
//   `file_claim` is designed to be called permissionlessly as a separate, later transaction
//   (by a keeper, the SME, or the frontend) rather than chained automatically off seizure.

pub const BPS_DENOM: u32 = 10_000;
const SECS_PER_DAY: u64 = 86_400;

/// Ceiling for risk multipliers (tier and default), in bps. Multipliers are
/// relative (10_000 = 1.0x) and legitimately exceed `BPS_DENOM`, so they need
/// their own cap: 100_000 = 10x.
pub const MAX_RISK_MULTIPLIER_BPS: u32 = 100_000;

const INSTANCE_LIFETIME_THRESHOLD: u32 = 17_280 * 30; // ~30 days at 5s/ledger
const INSTANCE_BUMP_AMOUNT: u32 = 17_280 * 60; // ~60 days
const PERSISTENT_LIFETIME_THRESHOLD: u32 = 17_280 * 30; // ~30 days at 5s/ledger
const PERSISTENT_BUMP_AMOUNT: u32 = 17_280 * 60; // ~60 days

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum InsuranceError {
    AlreadyInitialized = 0,
    NotInitialized = 1,
    Unauthorized = 2,
    ContractPaused = 3,
    InvalidAmount = 4,
    InvalidCoverageBps = 5,
    InvalidPremiumConfig = 6,
    InvalidMinCoverageRatio = 7,
    // Purchasing this coverage would push coverage_ratio_bps below the configured floor.
    CoverageRatioFloorBreached = 8,
    AlreadyCovered = 9,
    NoCoverageFound = 10,
    // The invoice contract does not (yet) report this invoice as defaulted.
    InvoiceNotDefaulted = 11,
    // Owed amount is already fully covered by collateral recovery; nothing to claim.
    NoShortfall = 12,
    AlreadyClaimed = 13,
    AmountOverflow = 14,
    FundedInvoiceNotFound = 15,
    PoolCallFailed = 16,
    /// #936: reserve has insufficient funds to cover the full nominal payout.
    InsufficientReserves = 17,
}

type InsuranceResult<T> = Result<T, InsuranceError>;

// ---- Types ----

/// A single credit-score band and its risk multiplier (bps, 10_000 = 1.0x).
/// Mirrors pool's `FeeTier` pattern but inverted: a *worse* (lower) score band
/// carries a *higher* multiplier, since it is used for risk-based premium pricing
/// rather than fee discounting.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RiskTier {
    /// Inclusive lower bound of the credit-score band this tier covers.
    pub min_score: u32,
    /// Inclusive upper bound of the credit-score band this tier covers.
    pub max_score: u32,
    /// Risk multiplier applied to the base premium, in bps (10_000 = 1.0x).
    pub risk_multiplier_bps: u32,
}

/// Pure pricing configuration. Every field here is a plain number/Vec passed by
/// value, so `calculate_premium` can be exercised in unit/property tests without
/// any contract storage.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PremiumConfig {
    /// Base annualized premium rate, in bps of principal (e.g. 200 = 2%/year).
    pub base_rate_bps: u32,
    /// Extra premium accrued per day of invoice tenor, in bps applied
    /// multiplicatively on top of the risk-adjusted base (longer exposure = higher premium).
    pub tenor_bps_per_day: u32,
    /// Credit-score risk tiers, worst-to-best or any order — resolved by range match.
    pub risk_tiers: Vec<RiskTier>,
    /// Multiplier used when the SME's credit score doesn't fall inside any
    /// configured tier (including when credit_score data is unavailable) —
    /// deliberately conservative (high) since missing data is itself a risk signal.
    pub default_risk_multiplier_bps: u32,
    /// Floor on the computed premium, in bps of principal.
    pub min_premium_bps: u32,
    /// Ceiling on the computed premium, in bps of principal.
    pub max_premium_bps: u32,
    /// Fraction of principal newly-purchased coverage insures, in bps
    /// (10_000 = 100%). May be <100% — partial coverage at a lower premium.
    pub default_coverage_bps: u32,
}

/// Per-token reserve solvency state.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ReserveFund {
    pub total_reserves: i128,
    pub total_premiums_collected: i128,
    pub total_claims_paid: i128,
    /// Sum of `coverage_bps * principal / 10_000` across all outstanding
    /// (purchased, unclaimed) coverage — the reserve's total covered exposure.
    pub total_covered_exposure: i128,
    /// Reserves as a fraction of total covered exposure, in bps. A solvency
    /// signal: 10_000 = fully backed 1:1, below that = under-reserved.
    /// Recomputed on every purchase/claim; 10_000 when exposure is zero.
    pub coverage_ratio_bps: u32,
    /// Admin-configured floor below which new coverage cannot be sold.
    pub min_coverage_ratio_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CoverageRecord {
    pub invoice_id: u64,
    pub token: Address,
    pub principal: i128,
    pub premium_paid: i128,
    pub coverage_bps: u32,
    pub purchased_at: u64,
    pub claimed: bool,
}

/// #937: A single historical claim entry recording the payout details.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ClaimHistoryItem {
    pub invoice_id: u64,
    pub token: Address,
    pub payout: i128,
    pub shortfalls: i128,
    pub timestamp: u64,
}

/// #938: Health status of a token's reserve, returned by `check_reserve_health`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ReserveHealth {
    pub token: Address,
    pub total_reserves: i128,
    pub coverage_ratio_bps: u32,
    pub min_reserve_amount: i128,
    pub is_healthy: bool,
    pub needs_top_up: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct Config {
    pub admin: Address,
    pub pool_contract: Address,
    pub invoice_contract: Address,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Config,
    CreditScoreContract,
    PremiumConfig,
    ReserveFund(Address),
    CoverageRecord(u64),
    Paused,
    ReentrancyGuard,
    /// #937: number of claim history entries for a given invoice.
    ClaimHistoryCount(u64),
    /// #937: individual claim history entry keyed by (invoice_id, index).
    ClaimHistoryEntry(u64, u32),
    /// #938: minimum reserve amount below which top-up is flagged.
    MinReserveAmount(Address),
}

// ---- Cross-contract client mirrors ----
// Local, minimal mirrors of the other contracts' public interfaces — Soroban
// decodes struct return values by field name, so a subset of fields is enough
// (same convention pool uses for its own CreditScoreData/InvoiceContract mirrors).

#[contractclient(name = "CreditScoreClient")]
pub trait CreditScoreContract {
    fn get_credit_score(env: Env, sme: Address) -> CreditScoreData;
}

/// Local mirror of credit_score::CreditScoreResponse — see FundedInvoiceView
/// for why every field must be reproduced.
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
    pub config_version: u32,
    pub is_stale: bool,
    /// #935: Blended score incorporating external attestations. Used for
    /// premium pricing when available; falls back to `score` (internal-only)
    /// when the credit_score contract doesn't return this field (pre-v2).
    pub blended_score: u32,
}

#[contractclient(name = "InvoiceContractClient")]
pub trait InvoiceContract {
    fn is_invoice_defaulted(env: Env, id: u64) -> bool;
    fn get_invoice_verification_state(env: Env, id: u64) -> (bool, i128);
}

#[contractclient(name = "PoolContractClient")]
pub trait PoolContract {
    fn get_funded_invoice(env: Env, invoice_id: u64) -> Option<FundedInvoiceView>;
    fn get_collateral_deposit(env: Env, invoice_id: u64) -> Option<CollateralDepositView>;
    fn receive_insurance_payout(
        env: Env,
        insurance: Address,
        token: Address,
        invoice_id: u64,
        amount: i128,
    );
}

/// Local mirror of pool::FundedInvoice. Cross-contract *return-value*
/// decoding requires the field set to match the callee's struct exactly (not
/// a subset), so every field is reproduced here even though only a few are
/// read.
#[contracttype]
#[derive(Clone)]
pub struct FundedInvoiceView {
    pub invoice_id: u64,
    pub sme: Address,
    pub token: Address,
    pub principal: i128,
    pub funded_at: u64,
    pub factoring_fee: i128,
    pub due_date: u64,
    pub repaid_amount: i128,
}

/// Local mirror of pool::CollateralDeposit — see FundedInvoiceView for why
/// every field must be reproduced.
#[contracttype]
#[derive(Clone)]
pub struct CollateralDepositView {
    pub invoice_id: u64,
    pub depositor: Address,
    pub token: Address,
    pub amount: i128,
    pub settled: bool,
    pub posted_at: u64,
    pub released_at: u64,
    pub seized_at: u64,
    /// #1329: actual liquidation proceeds from Dutch auction settlement,
    /// denominated in proceeds_token (distinct from the collateral token).
    /// Zero if not yet settled, or pre-settlement if pool hasn't been updated yet.
    pub settled_price: i128,
}

// ---- Pure pricing engine ----

fn resolve_risk_multiplier_bps(score: u32, config: &PremiumConfig) -> u32 {
    for i in 0..config.risk_tiers.len() {
        if let Some(tier) = config.risk_tiers.get(i) {
            if score >= tier.min_score && score <= tier.max_score {
                return tier.risk_multiplier_bps;
            }
        }
    }
    config.default_risk_multiplier_bps
}

/// Pure premium-pricing function — no contract storage, no Env I/O beyond what's
/// already inside `config`. Deliberately kept free of `env.storage()` calls so it
/// can be exercised directly by unit/property tests (see tests/premium_pricing_tests.rs).
///
/// Monotonic by construction: a worse (lower) credit score can only match a tier
/// with an equal-or-higher `risk_multiplier_bps` (or fall back to the conservative
/// default), and a longer tenor can only add equal-or-more `tenor_bps_per_day`
/// extra bps — so, before clamping, premium is non-decreasing as score worsens or
/// tenor lengthens. Clamping to `[min_premium_bps, max_premium_bps]` can only ever
/// flatten that monotonic curve at the edges, never invert it.
pub fn calculate_premium(
    principal: i128,
    sme_credit_score: u32,
    invoice_tenor_days: u32,
    config: &PremiumConfig,
) -> u128 {
    let risk_multiplier_bps = resolve_risk_multiplier_bps(sme_credit_score, config);
    calculate_premium_with_multiplier(principal, risk_multiplier_bps, invoice_tenor_days, config)
}

/// Same pricing as `calculate_premium`, but with the risk multiplier already
/// resolved. Lets callers price a missing credit score with
/// `default_risk_multiplier_bps` directly instead of pushing a sentinel score
/// through tier matching, where it could collide with a real tier.
fn calculate_premium_with_multiplier(
    principal: i128,
    risk_multiplier_bps: u32,
    invoice_tenor_days: u32,
    config: &PremiumConfig,
) -> u128 {
    if principal <= 0 {
        return 0;
    }
    let principal = principal as u128;
    let denom = BPS_DENOM as u128;

    let risk_multiplier_bps = risk_multiplier_bps as u128;
    let base = principal.saturating_mul(config.base_rate_bps as u128) / denom;
    let risk_adjusted = base.saturating_mul(risk_multiplier_bps) / denom;

    let tenor_extra_bps =
        (invoice_tenor_days as u128).saturating_mul(config.tenor_bps_per_day as u128);
    let tenor_bonus = risk_adjusted.saturating_mul(tenor_extra_bps) / denom;
    let raw_premium = risk_adjusted.saturating_add(tenor_bonus);

    let min_premium = principal.saturating_mul(config.min_premium_bps as u128) / denom;
    let max_premium = principal.saturating_mul(config.max_premium_bps as u128) / denom;

    raw_premium.max(min_premium).min(max_premium)
}

// ---- Internal helpers ----

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

fn require_not_paused(env: &Env) -> InsuranceResult<()> {
    if env
        .storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Paused)
        .unwrap_or(false)
    {
        return Err(InsuranceError::ContractPaused);
    }
    Ok(())
}

macro_rules! non_reentrant {
    ($env:expr, $body:block) => {{
        InsuranceReserve::non_reentrant_start($env);
        let result = { $body };
        InsuranceReserve::non_reentrant_end($env);
        result
    }};
}

const EVT: Symbol = symbol_short!("INSURNCE");

#[contract]
pub struct InsuranceReserve;

#[contractimpl]
impl InsuranceReserve {
    pub fn initialize(
        env: Env,
        admin: Address,
        pool_contract: Address,
        invoice_contract: Address,
    ) -> Result<(), InsuranceError> {
        if env.storage().instance().has(&DataKey::Config) {
            return Err(InsuranceError::AlreadyInitialized);
        }
        admin.require_auth();
        let config = Config {
            admin: admin.clone(),
            pool_contract,
            invoice_contract,
        };
        env.storage().instance().set(&DataKey::Config, &config);
        bump_instance(&env);
        env.events().publish((EVT, symbol_short!("init")), admin);
        Ok(())
    }

    // ---- Admin ----

    pub fn set_premium_config(
        env: Env,
        admin: Address,
        config: PremiumConfig,
    ) -> Result<(), InsuranceError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        Self::validate_premium_config(&config)?;
        if config.default_coverage_bps == 0 || config.default_coverage_bps > BPS_DENOM {
            return Err(InsuranceError::InvalidCoverageBps);
        }
        env.storage()
            .instance()
            .set(&DataKey::PremiumConfig, &config);
        env.events().publish((EVT, symbol_short!("cfg_set")), admin);
        Ok(())
    }

    pub fn get_premium_config(env: Env) -> Option<PremiumConfig> {
        env.storage().instance().get(&DataKey::PremiumConfig)
    }

    pub fn set_min_coverage_ratio(
        env: Env,
        admin: Address,
        token: Address,
        min_ratio_bps: u32,
    ) -> Result<(), InsuranceError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        if min_ratio_bps > BPS_DENOM {
            return Err(InsuranceError::InvalidMinCoverageRatio);
        }
        let mut reserve = Self::load_reserve(&env, &token);
        reserve.min_coverage_ratio_bps = min_ratio_bps;
        env.storage()
            .instance()
            .set(&DataKey::ReserveFund(token.clone()), &reserve);
        env.events().publish(
            (EVT, symbol_short!("mcr_set")),
            (admin, token, min_ratio_bps),
        );
        Ok(())
    }

    pub fn set_credit_score_contract(
        env: Env,
        admin: Address,
        credit_score_contract: Address,
    ) -> Result<(), InsuranceError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        env.storage()
            .instance()
            .set(&DataKey::CreditScoreContract, &credit_score_contract);
        Ok(())
    }

    pub fn get_credit_score_contract(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::CreditScoreContract)
    }

    pub fn get_config(env: Env) -> Option<Config> {
        env.storage().instance().get(&DataKey::Config)
    }

    /// Bootstrap/top-up path — e.g. seeded from `pool.withdraw_revenue` proceeds.
    /// `admin` must have already authorized this specific transfer (same
    /// authorization the transfer itself requires).
    pub fn fund_reserve_from_treasury(
        env: Env,
        admin: Address,
        token: Address,
        amount: i128,
    ) -> Result<(), InsuranceError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        if amount <= 0 {
            return Err(InsuranceError::InvalidAmount);
        }
        non_reentrant!(&env, {
            let token_client = token::Client::new(&env, &token);
            token_client.transfer(&admin, &env.current_contract_address(), &amount);

            let mut reserve = Self::load_reserve(&env, &token);
            reserve.total_reserves = reserve
                .total_reserves
                .checked_add(amount)
                .ok_or(InsuranceError::AmountOverflow)?;
            Self::recompute_ratio(&mut reserve);
            env.storage()
                .instance()
                .set(&DataKey::ReserveFund(token.clone()), &reserve);

            env.events()
                .publish((EVT, symbol_short!("funded")), (admin, token, amount));
            Ok(())
        })
    }

    pub fn pause(env: Env, admin: Address) -> Result<(), InsuranceError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        bump_instance(&env);
        env.storage().instance().set(&DataKey::Paused, &true);
        env.events().publish((EVT, symbol_short!("paused")), admin);
        Ok(())
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), InsuranceError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        bump_instance(&env);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.events()
            .publish((EVT, symbol_short!("unpaused")), admin);
        Ok(())
    }

    pub fn rotate_admin(
        env: Env,
        current_admin: Address,
        new_admin: Address,
    ) -> Result<(), InsuranceError> {
        current_admin.require_auth();
        new_admin.require_auth();
        Self::require_admin(&env, &current_admin)?;
        bump_instance(&env);
        let mut config: Config = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(InsuranceError::NotInitialized)?;
        config.admin = new_admin.clone();
        env.storage().instance().set(&DataKey::Config, &config);
        env.events().publish(
            (EVT, symbol_short!("adm_rot")),
            (current_admin, new_admin),
        );
        Ok(())
    }

    // ---- Core flows ----

    /// Called by the pool contract (or, in principle, the SME directly) at
    /// invoice-funding time. Non-fatal from the caller's point of view — pool
    /// calls this via `try_purchase_coverage` so a temporary outage here never
    /// blocks funding (see contracts/pool/src/lib.rs `fund_invoice_request`).
    pub fn purchase_coverage(
        env: Env,
        payer: Address,
        invoice_id: u64,
        principal: i128,
        sme: Address,
        due_date: u64,
        token: Address,
    ) -> Result<CoverageRecord, InsuranceError> {
        payer.require_auth();
        bump_instance(&env);
        require_not_paused(&env)?;

        let config: Config = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(InsuranceError::NotInitialized)?;
        if payer != config.pool_contract {
            return Err(InsuranceError::Unauthorized);
        }

        if principal <= 0 {
            return Err(InsuranceError::InvalidAmount);
        }

        let invoice_client = InvoiceContractClient::new(&env, &config.invoice_contract);
        let _ = invoice_client.get_invoice_verification_state(&invoice_id);

        if env
            .storage()
            .persistent()
            .get::<_, CoverageRecord>(&DataKey::CoverageRecord(invoice_id))
            .is_some()
        {
            return Err(InsuranceError::AlreadyCovered);
        }

        let config: PremiumConfig = env
            .storage()
            .instance()
            .get(&DataKey::PremiumConfig)
            .ok_or(InsuranceError::InvalidPremiumConfig)?;

        // Tenor is derived from `due_date` (supplied by the caller — the pool
        // contract at funding time, in the intended flow) rather than looked
        // up via a cross-contract call back into pool: `purchase_coverage` is
        // itself called *from* pool's `fund_invoice_request`, and Soroban
        // disallows A→B→A re-entrancy, so pool cannot be re-entered here. A
        // dishonest non-pool caller could misreport `due_date` to mis-price
        // their own premium, but the claim payout is still independently
        // capped by the actual shortfall/reserves at claim time (see
        // `file_claim`), so this can only ever cost the payer, never the
        // reserve.
        let tenor_days = due_date
            .saturating_sub(env.ledger().timestamp())
            .saturating_div(SECS_PER_DAY) as u32;

        let premium = Self::price_premium(&env, principal, &sme, tenor_days, &config)?;
        // #1417: reject a premium that floored to zero for a positive principal
        // (e.g. min_premium_bps = 0 with dust principal). Otherwise a zero-value
        // transfer would still book full covered exposure — real claimable risk
        // for no premium.
        if premium <= 0 {
            return Err(InsuranceError::InvalidAmount);
        }

        let coverage_bps = config.default_coverage_bps;
        let covered_exposure = principal
            .checked_mul(coverage_bps as i128)
            .and_then(|v| v.checked_div(BPS_DENOM as i128))
            .ok_or(InsuranceError::AmountOverflow)?;

        let mut reserve = Self::load_reserve(&env, &token);
        let new_exposure = reserve
            .total_covered_exposure
            .checked_add(covered_exposure)
            .ok_or(InsuranceError::AmountOverflow)?;
        let prospective_reserves = reserve
            .total_reserves
            .checked_add(premium)
            .ok_or(InsuranceError::AmountOverflow)?;
        let prospective_ratio_bps = Self::ratio_bps(prospective_reserves, new_exposure);
        if new_exposure > 0 && prospective_ratio_bps < reserve.min_coverage_ratio_bps {
            return Err(InsuranceError::CoverageRatioFloorBreached);
        }

        non_reentrant!(&env, {
            let token_client = token::Client::new(&env, &token);
            token_client.transfer(&payer, &env.current_contract_address(), &premium);

            reserve.total_reserves = prospective_reserves;
            reserve.total_premiums_collected = reserve
                .total_premiums_collected
                .checked_add(premium)
                .ok_or(InsuranceError::AmountOverflow)?;
            reserve.total_covered_exposure = new_exposure;
            Self::recompute_ratio(&mut reserve);
            env.storage()
                .instance()
                .set(&DataKey::ReserveFund(token.clone()), &reserve);

            let record = CoverageRecord {
                invoice_id,
                token: token.clone(),
                principal,
                premium_paid: premium,
                coverage_bps,
                purchased_at: env.ledger().timestamp(),
                claimed: false,
            };
            let key = DataKey::CoverageRecord(invoice_id);
            env.storage()
                .persistent()
                .set(&key, &record);
            env.storage().persistent().extend_ttl(&key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);

            env.events().publish(
                (EVT, symbol_short!("covered")),
                (invoice_id, payer.clone(), premium, coverage_bps),
            );
            Ok(record)
        })
    }

    /// Called when an invoice repays in full (on time or late). Releases the
    /// coverage and reduces the reserve's total_covered_exposure accordingly.
    /// The invoice_id must not already have a claimed coverage record.
    pub fn release_coverage(env: Env, invoice_id: u64) -> Result<(), InsuranceError> {
        bump_instance(&env);
        require_not_paused(&env)?;

        let mut record: CoverageRecord = env
            .storage()
            .instance()
            .get(&DataKey::CoverageRecord(invoice_id))
            .ok_or(InsuranceError::NoCoverageFound)?;

        // Cannot release coverage that was already claimed
        if record.claimed {
            return Err(InsuranceError::AlreadyClaimed);
        }

        let covered_exposure = record
            .principal
            .checked_mul(record.coverage_bps as i128)
            .and_then(|v| v.checked_div(BPS_DENOM as i128))
            .ok_or(InsuranceError::AmountOverflow)?;

        let mut reserve = Self::load_reserve(&env, &record.token);
        reserve.total_covered_exposure = reserve
            .total_covered_exposure
            .checked_sub(covered_exposure)
            .unwrap_or(0)
            .max(0);
        Self::recompute_ratio(&mut reserve);
        env.storage()
            .instance()
            .set(&DataKey::ReserveFund(record.token.clone()), &reserve);

        // Mark as claimed so it can't be released again or have a claim filed
        record.claimed = true;
        env.storage()
            .instance()
            .set(&DataKey::CoverageRecord(invoice_id), &record);

        env.events()
            .publish((EVT, symbol_short!("released")), (invoice_id, covered_exposure));
        Ok(())
    }

    /// Permissionless — re-derives default status and shortfall itself rather
    /// than trusting the caller. Pool does *not* call this internally (it
    /// would re-enter pool while pool is still on the call stack seizing
    /// collateral, which Soroban disallows) — instead anyone (a keeper, the
    /// SME, the frontend) files it directly against this contract as a
    /// follow-up call once collateral has been seized.
    pub fn file_claim(env: Env, invoice_id: u64) -> Result<i128, InsuranceError> {
        bump_instance(&env);
        require_not_paused(&env)?;

        let record: CoverageRecord = env
            .storage()
            .persistent()
            .get(&DataKey::CoverageRecord(invoice_id))
            .ok_or(InsuranceError::NoCoverageFound)?;
        let record_key = DataKey::CoverageRecord(invoice_id);
        env.storage().persistent().extend_ttl(&record_key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);

        let cfg: Config = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(InsuranceError::NotInitialized)?;

        let invoice_client = InvoiceContractClient::new(&env, &cfg.invoice_contract);
        if !invoice_client.is_invoice_defaulted(&invoice_id) {
            return Err(InsuranceError::InvoiceNotDefaulted);
        }

        let pool_client = PoolContractClient::new(&env, &cfg.pool_contract);
        let funded = pool_client
            .get_funded_invoice(&invoice_id)
            .ok_or(InsuranceError::FundedInvoiceNotFound)?;
        let owed = funded
            .principal
            .checked_sub(funded.repaid_amount)
            .ok_or(InsuranceError::AmountOverflow)?
            .max(0);
        let recovered = pool_client
            .get_collateral_deposit(&invoice_id)
            .filter(|c| c.settled)
            .map(|c| c.settled_price)
            .unwrap_or(0);
        let shortfall = owed
            .checked_sub(recovered)
            .ok_or(InsuranceError::AmountOverflow)?
            .max(0);
        if shortfall <= 0 {
            return Err(InsuranceError::NoShortfall);
        }

        let nominal_covered = record
            .principal
            .checked_mul(record.coverage_bps as i128)
            .and_then(|v| v.checked_div(BPS_DENOM as i128))
            .ok_or(InsuranceError::AmountOverflow)?;

        let mut reserve = Self::load_reserve(&env, &record.token);

        // #936: reject the claim upfront when the reserve is depleted — a
        // partial payout on a zero-balance reserve would transfer nothing and
        // still mark the coverage as claimed, wasting the claim slot.
        if reserve.total_reserves <= 0 {
            return Err(InsuranceError::InsufficientReserves);
        }

        // Never pay out more than: what's nominally covered, the actual
        // shortfall (don't double-pay past collateral recovery), or what the
        // reserve actually holds (insolvency degrades to a partial payout
        // instead of panicking or overdrawing).
        let payout = nominal_covered
            .min(shortfall)
            .min(reserve.total_reserves)
            .max(0);
        if payout <= 0 {
            return Err(InsuranceError::NoShortfall);
        }

        non_reentrant!(&env, {
            let token_client = token::Client::new(&env, &record.token);
            token_client.transfer(&env.current_contract_address(), &cfg.pool_contract, &payout);
            pool_client.receive_insurance_payout(
                &env.current_contract_address(),
                &record.token,
                &invoice_id,
                &payout,
            );

            reserve.total_reserves = reserve
                .total_reserves
                .checked_sub(payout)
                .ok_or(InsuranceError::AmountOverflow)?;
            reserve.total_claims_paid = reserve
                .total_claims_paid
                .checked_add(payout)
                .ok_or(InsuranceError::AmountOverflow)?;
            reserve.total_covered_exposure = reserve
                .total_covered_exposure
                .checked_sub(nominal_covered)
                .unwrap_or(0)
                .max(0);
            Self::recompute_ratio(&mut reserve);
            env.storage()
                .instance()
                .set(&DataKey::ReserveFund(record.token.clone()), &reserve);

            let hist_count: u32 = env
                .storage()
                .persistent()
                .get(&DataKey::ClaimHistoryCount(invoice_id))
                .unwrap_or(0);
            let item = ClaimHistoryItem {
                invoice_id,
                token: record.token.clone(),
                payout,
                shortfalls: shortfall,
                timestamp: env.ledger().timestamp(),
            };
            let hist_key = DataKey::ClaimHistoryEntry(invoice_id, hist_count);
            env.storage().persistent().set(
                &hist_key,
                &item,
            );
            env.storage().persistent().extend_ttl(&hist_key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);

            let count_key = DataKey::ClaimHistoryCount(invoice_id);
            env.storage().persistent().set(
                &count_key,
                &(hist_count + 1),
            );
            env.storage().persistent().extend_ttl(&count_key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);

            env.events()
                .publish((EVT, symbol_short!("claimed")), (invoice_id, payout));
            Ok(payout)
        })
    }

    // ---- Views ----

    pub fn get_reserve_status(env: Env, token: Address) -> ReserveFund {
        Self::load_reserve(&env, &token)
    }

    pub fn get_coverage_record(env: Env, invoice_id: u64) -> Option<CoverageRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::CoverageRecord(invoice_id))
    }

    /// #937: Return the full claim history for an invoice.
    pub fn get_claim_history(env: Env, invoice_id: u64) -> Vec<ClaimHistoryItem> {
        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ClaimHistoryCount(invoice_id))
            .unwrap_or(0);
        let mut items = Vec::new(&env);
        let mut i: u32 = 0;
        while i < count {
            if let Some(item) = env
                .storage()
                .persistent()
                .get(&DataKey::ClaimHistoryEntry(invoice_id, i))
            {
                items.push_back(item);
            }
            i += 1;
        }
        items
    }

    /// #938: Check reserve health against the configured minimum. Returns a
    /// `ReserveHealth` struct indicating whether the reserve needs topping up.
    pub fn check_reserve_health(env: Env, token: Address) -> ReserveHealth {
        let reserve = Self::load_reserve(&env, &token);
        let min_amount: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::MinReserveAmount(token.clone()))
            .unwrap_or(0);
        // With no configured floor there is no minimum for the reserve to
        // violate. Treat that state as healthy, consistent with needs_top_up.
        let is_healthy = reserve.total_reserves >= min_amount;
        ReserveHealth {
            token,
            total_reserves: reserve.total_reserves,
            coverage_ratio_bps: reserve.coverage_ratio_bps,
            min_reserve_amount: min_amount,
            is_healthy,
            needs_top_up: min_amount > 0 && reserve.total_reserves < min_amount,
        }
    }

    // ---- Admin: #938 min reserve config ----

    /// Set the minimum reserve amount for a token. When reserves drop below
    /// this threshold, `check_reserve_health` signals that a top-up is needed.
    pub fn set_min_reserve_amount(
        env: Env,
        admin: Address,
        token: Address,
        min_amount: i128,
    ) -> Result<(), InsuranceError> {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin)?;
        if min_amount < 0 {
            return Err(InsuranceError::InvalidAmount);
        }
        let key = DataKey::MinReserveAmount(token.clone());
        env.storage()
            .persistent()
            .set(&key, &min_amount);
        env.storage().persistent().extend_ttl(&key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        env.events().publish(
            (EVT, symbol_short!("min_rsv")),
            (admin, token, min_amount),
        );
        Ok(())
    }

    pub fn get_min_reserve_amount(env: Env, token: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::MinReserveAmount(token))
            .unwrap_or(0)
    }

    /// Read-only quote — no storage writes, no auth. `tenor_days` is supplied
    /// directly (unlike `purchase_coverage`, there may be no funded invoice yet
    /// to derive it from).
    pub fn estimate_premium(
        env: Env,
        principal: i128,
        sme: Address,
        tenor_days: u32,
    ) -> Result<i128, InsuranceError> {
        let config: PremiumConfig = env
            .storage()
            .instance()
            .get(&DataKey::PremiumConfig)
            .ok_or(InsuranceError::InvalidPremiumConfig)?;
        Self::price_premium(&env, principal, &sme, tenor_days, &config)
    }

    // ---- Internal ----

    fn validate_premium_config(config: &PremiumConfig) -> InsuranceResult<()> {
        if config.max_premium_bps < config.min_premium_bps {
            return Err(InsuranceError::InvalidPremiumConfig);
        }
        // Rate fields are fractions of principal (or of the risk-adjusted
        // premium), so none may exceed 100%.
        if config.base_rate_bps > BPS_DENOM
            || config.tenor_bps_per_day > BPS_DENOM
            || config.min_premium_bps > BPS_DENOM
            || config.max_premium_bps > BPS_DENOM
        {
            return Err(InsuranceError::InvalidPremiumConfig);
        }
        if config.default_risk_multiplier_bps == 0
            || config.default_risk_multiplier_bps > MAX_RISK_MULTIPLIER_BPS
        {
            return Err(InsuranceError::InvalidPremiumConfig);
        }

        // Tiers may be supplied in any order, so overlap is checked pairwise.
        // Gaps between tiers are allowed: uncovered scores intentionally price
        // at `default_risk_multiplier_bps`.
        let n = config.risk_tiers.len();
        for i in 0..n {
            let tier = config.risk_tiers.get(i).expect("storage corrupted");
            if tier.min_score > tier.max_score
                || tier.risk_multiplier_bps == 0
                || tier.risk_multiplier_bps > MAX_RISK_MULTIPLIER_BPS
            {
                return Err(InsuranceError::InvalidPremiumConfig);
            }
            for j in (i + 1)..n {
                let other = config.risk_tiers.get(j).expect("storage corrupted");
                if tier.min_score <= other.max_score && other.min_score <= tier.max_score {
                    return Err(InsuranceError::InvalidPremiumConfig);
                }
            }
        }
        Ok(())
    }

    /// Prices a premium for `sme`. A missing credit score (contract unset or
    /// call failed) bypasses tier matching and uses the config's conservative
    /// `default_risk_multiplier_bps`, so "no data" can never be priced as a
    /// real score that happens to fall inside a tier.
    fn price_premium(
        env: &Env,
        principal: i128,
        sme: &Address,
        tenor_days: u32,
        config: &PremiumConfig,
    ) -> InsuranceResult<i128> {
        let multiplier_bps = match Self::resolve_credit_score(env, sme) {
            Some(score) => resolve_risk_multiplier_bps(score, config),
            None => config.default_risk_multiplier_bps,
        };
        calculate_premium_with_multiplier(principal, multiplier_bps, tenor_days, config)
            .try_into()
            .map_err(|_| InsuranceError::AmountOverflow)
    }

    fn resolve_credit_score(env: &Env, sme: &Address) -> Option<u32> {
        let cs_contract: Option<Address> =
            env.storage().instance().get(&DataKey::CreditScoreContract);
        let addr = cs_contract?;
        let client = CreditScoreClient::new(env, &addr);
        match client.try_get_credit_score(sme) {
            // #935: prefer blended_score (includes external attestations)
            // over the raw internal score for more accurate risk pricing.
            Ok(Ok(data)) => Some(if data.blended_score > 0 {
                data.blended_score
            } else {
                // Pre-v2 credit_score contracts don't return blended_score;
                // fall back to the internal score.
                data.score
            }),
            _ => None,
        }
    }

    fn load_reserve(env: &Env, token: &Address) -> ReserveFund {
        env.storage()
            .instance()
            .get(&DataKey::ReserveFund(token.clone()))
            .unwrap_or_default()
    }

    fn ratio_bps(reserves: i128, exposure: i128) -> u32 {
        if exposure <= 0 {
            return BPS_DENOM;
        }
        let ratio = reserves
            .max(0)
            .saturating_mul(BPS_DENOM as i128)
            .checked_div(exposure);
        match ratio {
            Some(r) if r >= 0 => r.min(u32::MAX as i128) as u32,
            _ => 0,
        }
    }

    fn recompute_ratio(reserve: &mut ReserveFund) {
        reserve.coverage_ratio_bps =
            Self::ratio_bps(reserve.total_reserves, reserve.total_covered_exposure);
    }

    fn require_admin(env: &Env, admin: &Address) -> InsuranceResult<()> {
        let config: Config = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(InsuranceError::NotInitialized)?;
        if admin != &config.admin {
            return Err(InsuranceError::Unauthorized);
        }
        Ok(())
    }

    fn non_reentrant_start(env: &Env) {
        if env
            .storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::ReentrancyGuard)
            .unwrap_or(false)
        {
            panic_with_error!(env, InsuranceError::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyGuard, &true);
    }

    fn non_reentrant_end(env: &Env) {
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyGuard, &false);
    }
}
