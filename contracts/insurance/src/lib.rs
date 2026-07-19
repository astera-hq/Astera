#![no_std]

#[cfg(test)]
extern crate std;

use soroban_sdk::{
    contract, contracterror, contractimpl, contractclient, contracttype, panic_with_error,
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
///  funded by protocol revenue" from the issue description. `payer` is a parameter (not
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

const INSTANCE_LIFETIME_THRESHOLD: u32 = 17_280 * 30; // ~30 days at 5s/ledger
const INSTANCE_BUMP_AMOUNT: u32 = 17_280 * 60; // ~60 days

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
}

#[contractclient(name = "InvoiceContractClient")]
pub trait InvoiceContract {
    fn is_invoice_defaulted(env: Env, id: u64) -> bool;
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
}

// ---- Pure pricing engine ----

fn resolve_risk_multiplier_bps(score: u32, config: &PremiumConfig) -> u32 {
    for i in 0..config.risk_tiers.len() {
        let tier = config.risk_tiers.get(i).expect("storage corrupted");
        if score >= tier.min_score && score <= tier.max_score {
            return tier.risk_multiplier_bps;
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
    if principal <= 0 {
        return 0;
    }
    let principal = principal as u128;
    let denom = BPS_DENOM as u128;

    let risk_multiplier_bps = resolve_risk_multiplier_bps(sme_credit_score, config) as u128;
    let base = principal.saturating_mul(config.base_rate_bps as u128) / denom;
    let risk_adjusted = base.saturating_mul(risk_multiplier_bps) / denom;

    let tenor_extra_bps = (invoice_tenor_days as u128).saturating_mul(config.tenor_bps_per_day as u128);
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
        if config.max_premium_bps < config.min_premium_bps {
            return Err(InsuranceError::InvalidPremiumConfig);
        }
        if config.default_coverage_bps == 0 || config.default_coverage_bps > BPS_DENOM {
            return Err(InsuranceError::InvalidCoverageBps);
        }
        env.storage().instance().set(&DataKey::PremiumConfig, &config);
        env.events()
            .publish((EVT, symbol_short!("cfg_set")), admin);
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
        env.events()
            .publish((EVT, symbol_short!("mcr_set")), (admin, token, min_ratio_bps));
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

        if principal <= 0 {
            return Err(InsuranceError::InvalidAmount);
        }
        if env
            .storage()
            .instance()
            .has(&DataKey::CoverageRecord(invoice_id))
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

        let score = Self::resolve_credit_score(&env, &sme);
        let premium = calculate_premium(principal, score, tenor_days, &config);
        let premium: i128 = premium
            .try_into()
            .map_err(|_| InsuranceError::AmountOverflow)?;

        let coverage_bps = config.default_coverage_bps;
        let covered_exposure = (principal as i128)
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
            env.storage()
                .instance()
                .set(&DataKey::CoverageRecord(invoice_id), &record);

            env.events().publish(
                (EVT, symbol_short!("covered")),
                (invoice_id, payer.clone(), premium, coverage_bps),
            );
            Ok(record)
        })
    }

    /// Permissionless — re-derives default status and shortfall itself rather
    /// than trusting the caller. Pool does *not* call this internally (it
    /// would re-enter pool while pool is still on the call stack seizing
    /// collateral, which Soroban disallows) — instead anyone (a keeper, the
    /// SME, the frontend) files it directly against this contract as a
    /// follow-up call once collateral has been seized.
    pub fn file_claim(env: Env, _caller: Address, invoice_id: u64) -> Result<i128, InsuranceError> {
        bump_instance(&env);
        require_not_paused(&env)?;

        let mut record: CoverageRecord = env
            .storage()
            .instance()
            .get(&DataKey::CoverageRecord(invoice_id))
            .ok_or(InsuranceError::NoCoverageFound)?;
        if record.claimed {
            return Err(InsuranceError::AlreadyClaimed);
        }

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
            .map(|c| c.amount)
            .unwrap_or(0);
        let shortfall = owed.checked_sub(recovered).ok_or(InsuranceError::AmountOverflow)?.max(0);
        if shortfall <= 0 {
            return Err(InsuranceError::NoShortfall);
        }

        let nominal_covered = record
            .principal
            .checked_mul(record.coverage_bps as i128)
            .and_then(|v| v.checked_div(BPS_DENOM as i128))
            .ok_or(InsuranceError::AmountOverflow)?;

        let mut reserve = Self::load_reserve(&env, &record.token);
        // Never pay out more than: what's nominally covered, the actual
        // shortfall (don't double-pay past collateral recovery), or what the
        // reserve actually holds (insolvency degrades to a partial payout
        // instead of panicking or overdrawing).
        let payout = nominal_covered.min(shortfall).min(reserve.total_reserves).max(0);
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

            record.claimed = true;
            env.storage()
                .instance()
                .set(&DataKey::CoverageRecord(invoice_id), &record);

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
        env.storage().instance().get(&DataKey::CoverageRecord(invoice_id))
    }

    /// Read-only quote — no storage writes, no auth. `tenor_days` is supplied
    /// directly (unlike `purchase_coverage`, there may be no funded invoice yet
    /// to derive it from).
    pub fn estimate_premium(
        env: Env,
        principal: i128,
        sme: Address,
        tenor_days: u32,
        _token: Address,
    ) -> Result<i128, InsuranceError> {
        let config: PremiumConfig = env
            .storage()
            .instance()
            .get(&DataKey::PremiumConfig)
            .ok_or(InsuranceError::InvalidPremiumConfig)?;
        let score = Self::resolve_credit_score(&env, &sme);
        let premium = calculate_premium(principal, score, tenor_days, &config);
        premium.try_into().map_err(|_| InsuranceError::AmountOverflow)
    }

    // ---- Internal ----

    fn resolve_credit_score(env: &Env, sme: &Address) -> u32 {
        const DEFAULT_SCORE_WHEN_UNAVAILABLE: u32 = 300; // conservative: near the bottom of 200-850

        let cs_contract: Option<Address> = env.storage().instance().get(&DataKey::CreditScoreContract);
        match cs_contract {
            Some(addr) => {
                let client = CreditScoreClient::new(env, &addr);
                match client.try_get_credit_score(sme) {
                    Ok(Ok(data)) => data.score,
                    _ => DEFAULT_SCORE_WHEN_UNAVAILABLE,
                }
            }
            None => DEFAULT_SCORE_WHEN_UNAVAILABLE,
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
        let ratio = (reserves.max(0) as i128)
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
        env.storage().instance().set(&DataKey::ReentrancyGuard, &true);
    }

    fn non_reentrant_end(env: &Env) {
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyGuard, &false);
    }
}

