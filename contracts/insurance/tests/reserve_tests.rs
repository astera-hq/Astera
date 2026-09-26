#![cfg(test)]

use insurance::{
    CollateralDepositView, CreditScoreData, FundedInvoiceView, InsuranceError, InsuranceReserve,
    InsuranceReserveClient, PremiumConfig, RiskTier, BPS_DENOM, MAX_RISK_MULTIPLIER_BPS,
};
use soroban_sdk::{
    contract, contractimpl, symbol_short, testutils::Address as _, Address, Env, Vec,
};

fn default_premium_config(env: &Env) -> PremiumConfig {
    let mut tiers = Vec::new(env);
    // Worse (lower) score bands carry a higher multiplier.
    tiers.push_back(RiskTier {
        min_score: 750,
        max_score: 850,
        risk_multiplier_bps: 8_000, // 0.8x — best tier, cheapest
    });
    tiers.push_back(RiskTier {
        min_score: 650,
        max_score: 749,
        risk_multiplier_bps: 12_000,
    });
    tiers.push_back(RiskTier {
        min_score: 550,
        max_score: 649,
        risk_multiplier_bps: 18_000,
    });
    tiers.push_back(RiskTier {
        min_score: 200,
        max_score: 549,
        risk_multiplier_bps: 30_000, // 3.0x — worst tier, most expensive
    });
    PremiumConfig {
        base_rate_bps: 200, // 2%
        tenor_bps_per_day: 10,
        risk_tiers: tiers,
        default_risk_multiplier_bps: 40_000, // worse than the worst tier
        min_premium_bps: 10,
        max_premium_bps: 5_000,
        default_coverage_bps: 8_000, // 80%
    }
}

// ---- Dummy contracts for cross-contract wiring ----
// Each contract test file in this repo defines its own minimal dummies
// (see contracts/pool/tests/fuzz_tests.rs) rather than importing test-only
// types from the crate under test.

#[contract]
pub struct DummyCreditScore;
#[contractimpl]
impl DummyCreditScore {
    pub fn set_score(env: Env, sme: Address, score: u32) {
        env.storage().persistent().set(&sme, &score);
    }
    pub fn get_credit_score(env: Env, sme: Address) -> CreditScoreData {
        let score: u32 = env.storage().persistent().get(&sme).unwrap_or(300);
        CreditScoreData {
            sme,
            score,
            total_invoices: 0,
            paid_on_time: 0,
            paid_late: 0,
            defaulted: 0,
            total_volume: 0,
            average_payment_days: 0,
            last_updated: 0,
            score_version: 0,
            config_version: 0,
            is_stale: false,
            blended_score: score,
        }
    }
}

#[contract]
pub struct DummyInvoice;
#[contractimpl]
impl DummyInvoice {
    pub fn set_defaulted(env: Env, id: u64, defaulted: bool) {
        env.storage()
            .persistent()
            .set(&(symbol_short!("dflt"), id), &defaulted);
    }
    pub fn is_invoice_defaulted(env: Env, id: u64) -> bool {
        env.storage()
            .persistent()
            .get(&(symbol_short!("dflt"), id))
            .unwrap_or(false)
    }
}

#[contract]
pub struct DummyPool;
#[contractimpl]
impl DummyPool {
    pub fn set_funded_invoice(env: Env, invoice: FundedInvoiceView) {
        env.storage()
            .persistent()
            .set(&(symbol_short!("fnd"), invoice.invoice_id), &invoice);
    }
    pub fn get_funded_invoice(env: Env, invoice_id: u64) -> Option<FundedInvoiceView> {
        env.storage()
            .persistent()
            .get(&(symbol_short!("fnd"), invoice_id))
    }
    pub fn set_collateral_deposit(env: Env, deposit: CollateralDepositView) {
        env.storage()
            .persistent()
            .set(&(symbol_short!("col"), deposit.invoice_id), &deposit);
    }
    pub fn get_collateral_deposit(env: Env, invoice_id: u64) -> Option<CollateralDepositView> {
        env.storage()
            .persistent()
            .get(&(symbol_short!("col"), invoice_id))
    }
    pub fn receive_insurance_payout(
        env: Env,
        insurance: Address,
        token: Address,
        invoice_id: u64,
        amount: i128,
    ) {
        insurance.require_auth();
        let _ = token;
        env.storage()
            .persistent()
            .set(&(symbol_short!("payout"), invoice_id), &amount);
    }
    pub fn last_payout(env: Env, invoice_id: u64) -> Option<i128> {
        env.storage()
            .persistent()
            .get(&(symbol_short!("payout"), invoice_id))
    }
}

fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    soroban_sdk::token::StellarAssetClient::new(env, token_id).mint(to, &amount);
}

struct Harness<'a> {
    client: InsuranceReserveClient<'a>,
    admin: Address,
    pool_id: Address,
    pool_client: DummyPoolClient<'a>,
    invoice_client: DummyInvoiceClient<'a>,
    credit_client: DummyCreditScoreClient<'a>,
    token_id: Address,
}

fn setup(env: &Env) -> Harness<'_> {
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let pool_id = env.register(DummyPool, ());
    let invoice_id_addr = env.register(DummyInvoice, ());
    let credit_id = env.register(DummyCreditScore, ());

    let insurance_id = env.register(InsuranceReserve, ());
    let client = InsuranceReserveClient::new(env, &insurance_id);
    client.initialize(&admin, &pool_id, &invoice_id_addr);
    client.set_premium_config(&admin, &default_premium_config(env));
    client.set_credit_score_contract(&admin, &credit_id);

    Harness {
        client,
        admin,
        pool_id: pool_id.clone(),
        pool_client: DummyPoolClient::new(env, &pool_id),
        invoice_client: DummyInvoiceClient::new(env, &invoice_id_addr),
        credit_client: DummyCreditScoreClient::new(env, &credit_id),
        token_id,
    }
}

#[test]
fn test_purchase_coverage_and_reserve_status() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    let sme = Address::generate(&env);
    let payer = h.pool_id.clone();

    mint(&env, &h.token_id, &payer, 1_000_000);
    h.credit_client.set_score(&sme, &700);
    h.pool_client.set_funded_invoice(&FundedInvoiceView {
        invoice_id: 1,
        sme: sme.clone(),
        token: h.token_id.clone(),
        principal: 10_000,
        funded_at: 0,
        factoring_fee: 0,
        due_date: 30 * 86_400,
        repaid_amount: 0,
    });

    let record = h.client.purchase_coverage(
        &payer,
        &1u64,
        &10_000i128,
        &sme,
        &(30u64 * 86_400u64),
        &h.token_id,
    );
    assert_eq!(record.invoice_id, 1);
    assert!(record.premium_paid > 0);
    assert_eq!(record.coverage_bps, 8_000);

    let status = h.client.get_reserve_status(&h.token_id);
    assert_eq!(status.total_reserves, record.premium_paid);
    assert_eq!(status.total_premiums_collected, record.premium_paid);
    assert_eq!(status.total_covered_exposure, 8_000); // 80% of 10_000
}

#[test]
fn test_purchase_coverage_rejects_double_coverage() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    let sme = Address::generate(&env);
    let payer = h.pool_id.clone();
    mint(&env, &h.token_id, &payer, 1_000_000);
    h.pool_client.set_funded_invoice(&FundedInvoiceView {
        invoice_id: 1,
        sme: sme.clone(),
        token: h.token_id.clone(),
        principal: 10_000,
        funded_at: 0,
        factoring_fee: 0,
        due_date: 30 * 86_400,
        repaid_amount: 0,
    });
    h.client.purchase_coverage(
        &payer,
        &1u64,
        &10_000i128,
        &sme,
        &(30u64 * 86_400u64),
        &h.token_id,
    );
    let result = h.client.try_purchase_coverage(
        &payer,
        &1u64,
        &10_000i128,
        &sme,
        &(30u64 * 86_400u64),
        &h.token_id,
    );
    assert_eq!(result, Err(Ok(InsuranceError::AlreadyCovered)));
}

#[test]
fn test_non_pool_cannot_squat_coverage_record() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    let attacker = Address::generate(&env);
    let sme = Address::generate(&env);
    mint(&env, &h.token_id, &attacker, 1_000_000);

    let result = h.client.try_purchase_coverage(
        &attacker,
        &42u64,
        &1i128,
        &sme,
        &(30u64 * 86_400u64),
        &h.token_id,
    );
    assert_eq!(result, Err(Ok(InsuranceError::Unauthorized)));
    assert_eq!(h.client.get_coverage_record(&42u64), None);

    mint(&env, &h.token_id, &h.pool_id, 1_000_000);
    let record = h.client.purchase_coverage(
        &h.pool_id,
        &42u64,
        &10_000i128,
        &sme,
        &(30u64 * 86_400u64),
        &h.token_id,
    );
    assert_eq!(record.invoice_id, 42);
}

#[test]
fn test_reserve_without_configured_minimum_is_healthy() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);

    let health = h.client.check_reserve_health(&h.token_id);
    assert_eq!(health.min_reserve_amount, 0);
    assert!(health.is_healthy);
    assert!(!health.needs_top_up);
}

#[test]
fn test_coverage_ratio_floor_blocks_new_purchases() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    // A thin reserve with a high floor: the very first purchase already
    // pushes projected exposure far above what the (zero) reserve can back.
    h.client
        .set_min_coverage_ratio(&h.admin, &h.token_id, &5_000u32);

    let sme = Address::generate(&env);
    let payer = h.pool_id.clone();
    mint(&env, &h.token_id, &payer, 1_000_000);
    h.pool_client.set_funded_invoice(&FundedInvoiceView {
        invoice_id: 1,
        sme: sme.clone(),
        token: h.token_id.clone(),
        principal: 10_000_000,
        funded_at: 0,
        factoring_fee: 0,
        due_date: 30 * 86_400,
        repaid_amount: 0,
    });

    let result = h.client.try_purchase_coverage(
        &payer,
        &1u64,
        &10_000_000i128,
        &sme,
        &(30u64 * 86_400u64),
        &h.token_id,
    );
    assert_eq!(result, Err(Ok(InsuranceError::CoverageRatioFloorBreached)));
}

#[test]
fn test_coverage_ratio_floor_allows_purchase_once_reserve_seeded() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    h.client
        .set_min_coverage_ratio(&h.admin, &h.token_id, &5_000u32);

    // Seed the reserve generously first so the floor isn't breached.
    mint(&env, &h.token_id, &h.admin, 10_000_000);
    h.client
        .fund_reserve_from_treasury(&h.admin, &h.token_id, &5_000_000i128);

    let sme = Address::generate(&env);
    let payer = h.pool_id.clone();
    mint(&env, &h.token_id, &payer, 1_000_000);
    h.pool_client.set_funded_invoice(&FundedInvoiceView {
        invoice_id: 1,
        sme: sme.clone(),
        token: h.token_id.clone(),
        principal: 10_000,
        funded_at: 0,
        factoring_fee: 0,
        due_date: 30 * 86_400,
        repaid_amount: 0,
    });

    let record = h.client.purchase_coverage(
        &payer,
        &1u64,
        &10_000i128,
        &sme,
        &(30u64 * 86_400u64),
        &h.token_id,
    );
    assert!(record.premium_paid > 0);
}

fn cover_and_default(env: &Env, h: &Harness, invoice_id: u64, principal: i128) -> Address {
    let sme = Address::generate(env);
    let payer = h.pool_id.clone();
    mint(env, &h.token_id, &payer, 1_000_000_000);
    h.pool_client.set_funded_invoice(&FundedInvoiceView {
        invoice_id,
        sme: sme.clone(),
        token: h.token_id.clone(),
        principal,
        funded_at: 0,
        factoring_fee: 0,
        due_date: 30 * 86_400,
        repaid_amount: 0,
    });
    h.client.purchase_coverage(
        &payer,
        &invoice_id,
        &principal,
        &sme,
        &(30u64 * 86_400u64),
        &h.token_id,
    );
    h.invoice_client.set_defaulted(&invoice_id, &true);
    sme
}

#[test]
fn test_file_claim_full_payout_when_solvent() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    let principal = 10_000i128;
    cover_and_default(&env, &h, 1, principal);

    // Seed the reserve well beyond the nominal covered amount (8_000).
    mint(&env, &h.token_id, &h.admin, 1_000_000);
    h.client
        .fund_reserve_from_treasury(&h.admin, &h.token_id, &100_000i128);

    let payout = h.client.file_claim(&1u64);
    assert_eq!(payout, 8_000); // full nominal coverage (80% of 10_000), shortfall is 10_000

    let record = h.client.get_coverage_record(&1u64).unwrap();
    assert!(record.claimed);
    assert_eq!(h.pool_client.last_payout(&1u64), Some(8_000));

    let status = h.client.get_reserve_status(&h.token_id);
    assert_eq!(status.total_claims_paid, 8_000);
    assert_eq!(status.total_covered_exposure, 0);
}

/// Acceptance criterion: a claim against an insolvent reserve pays out exactly
/// total_reserves (not the nominal covered amount) and leaves the reserve at
/// zero without panicking.
#[test]
fn test_file_claim_partial_payout_when_insolvent_does_not_panic() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    let principal = 10_000i128;
    cover_and_default(&env, &h, 1, principal);

    // Reserve holds far less than the nominal covered amount (8_000) —
    // the premium alone (a couple hundred units) is all that's in there.
    let status_before = h.client.get_reserve_status(&h.token_id);
    assert!(status_before.total_reserves < 8_000);
    assert!(status_before.total_reserves > 0);

    let payout = h.client.file_claim(&1u64);

    // Must pay out exactly total_reserves, not the nominal covered amount.
    assert_eq!(payout, status_before.total_reserves);

    let status_after = h.client.get_reserve_status(&h.token_id);
    assert_eq!(status_after.total_reserves, 0);
    assert_eq!(status_after.total_claims_paid, status_before.total_reserves);
}

#[test]
fn test_file_claim_rejects_double_claim() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    let principal = 10_000i128;
    cover_and_default(&env, &h, 1, principal);
    mint(&env, &h.token_id, &h.admin, 1_000_000);
    h.client
        .fund_reserve_from_treasury(&h.admin, &h.token_id, &100_000i128);

    h.client.file_claim(&1u64);
    let result = h.client.try_file_claim(&1u64);
    assert_eq!(result, Err(Ok(InsuranceError::AlreadyClaimed)));
}

#[test]
fn test_file_claim_rejects_before_default() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    let sme = Address::generate(&env);
    let payer = h.pool_id.clone();
    let principal = 10_000i128;
    mint(&env, &h.token_id, &payer, 1_000_000);
    h.pool_client.set_funded_invoice(&FundedInvoiceView {
        invoice_id: 1,
        sme: sme.clone(),
        token: h.token_id.clone(),
        principal,
        funded_at: 0,
        factoring_fee: 0,
        due_date: 30 * 86_400,
        repaid_amount: 0,
    });
    h.client.purchase_coverage(
        &payer,
        &1u64,
        &principal,
        &sme,
        &(30u64 * 86_400u64),
        &h.token_id,
    );
    // Never marked defaulted.

    let result = h.client.try_file_claim(&1u64);
    assert_eq!(result, Err(Ok(InsuranceError::InvoiceNotDefaulted)));
}

#[test]
fn test_file_claim_no_coverage_found() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    let result = h.client.try_file_claim(&999u64);
    assert_eq!(result, Err(Ok(InsuranceError::NoCoverageFound)));
}

#[test]
fn test_file_claim_accounts_for_collateral_recovery() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    let principal = 10_000i128;
    cover_and_default(&env, &h, 1, principal);
    mint(&env, &h.token_id, &h.admin, 1_000_000);
    h.client
        .fund_reserve_from_treasury(&h.admin, &h.token_id, &100_000i128);

    // Collateral already recovered 6_000 of the 10_000 owed — shortfall is
    // 4_000, below the nominal 8_000 covered amount, so the claim should pay
    // only 4_000 (insurance covers the gap after collateral, not double-pay).
    h.pool_client
        .set_collateral_deposit(&CollateralDepositView {
            invoice_id: 1,
            depositor: Address::generate(&env),
            token: h.token_id.clone(),
            amount: 6_000,
            settled: true,
            posted_at: 0,
            released_at: 0,
            seized_at: 0,
        });

    let payout = h.client.file_claim(&1u64);
    assert_eq!(payout, 4_000);
}

#[test]
fn test_file_claim_no_shortfall_after_full_collateral_recovery() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    let principal = 10_000i128;
    cover_and_default(&env, &h, 1, principal);
    mint(&env, &h.token_id, &h.admin, 1_000_000);
    h.client
        .fund_reserve_from_treasury(&h.admin, &h.token_id, &100_000i128);

    h.pool_client
        .set_collateral_deposit(&CollateralDepositView {
            invoice_id: 1,
            depositor: Address::generate(&env),
            token: h.token_id.clone(),
            amount: 10_000,
            settled: true,
            posted_at: 0,
            released_at: 0,
            seized_at: 0,
        });

    let result = h.client.try_file_claim(&1u64);
    assert_eq!(result, Err(Ok(InsuranceError::NoShortfall)));
}

#[test]
fn test_pause_blocks_purchase_coverage() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    h.client.pause(&h.admin);

    let sme = Address::generate(&env);
    let payer = h.pool_id.clone();
    mint(&env, &h.token_id, &payer, 1_000_000);
    let result = h.client.try_purchase_coverage(
        &payer,
        &1u64,
        &10_000i128,
        &sme,
        &(30u64 * 86_400u64),
        &h.token_id,
    );
    assert_eq!(result, Err(Ok(InsuranceError::ContractPaused)));
}

#[test]
fn test_non_admin_cannot_set_premium_config() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    let attacker = Address::generate(&env);
    let result = h
        .client
        .try_set_premium_config(&attacker, &default_premium_config(&env));
    assert_eq!(result, Err(Ok(InsuranceError::Unauthorized)));
}

// ── #1417: zero premium must not buy real coverage ───────────────────────────

#[test]
fn test_purchase_coverage_rejects_zero_premium() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);

    // Force a zero premium: zero base rate and zero floor clamp every
    // principal to 0, reproducing min_premium_bps = 0 + dust rounding.
    let mut zero_cfg = default_premium_config(&env);
    zero_cfg.base_rate_bps = 0;
    zero_cfg.tenor_bps_per_day = 0;
    zero_cfg.min_premium_bps = 0;
    h.client.set_premium_config(&h.admin, &zero_cfg);

    let sme = Address::generate(&env);
    let payer = h.pool_id.clone();
    mint(&env, &h.token_id, &payer, 1_000_000);

    let result = h.client.try_purchase_coverage(
        &payer,
        &1u64,
        &10_000i128,
        &sme,
        &(30u64 * 86_400u64),
        &h.token_id,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));

    // No exposure booked, no record written, reserves untouched.
    let status = h.client.get_reserve_status(&h.token_id);
    assert_eq!(status.total_covered_exposure, 0);
    assert_eq!(status.total_reserves, 0);
    assert!(h.client.get_coverage_record(&1u64).is_none());
}

#[test]
fn test_purchase_coverage_rejects_dust_principal_flooring_to_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    // Default config (min 10 bps) still floors a dust principal of 1 to a
    // zero premium via integer division — must be rejected, not free cover.
    let sme = Address::generate(&env);
    let payer = h.pool_id.clone();
    mint(&env, &h.token_id, &payer, 1_000_000);

    let result = h.client.try_purchase_coverage(
        &payer,
        &2u64,
        &1i128,
        &sme,
        &(30u64 * 86_400u64),
        &h.token_id,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));
    assert!(h.client.get_coverage_record(&2u64).is_none());
}

// ── premium config validation ────────────────────────────────────────────────

fn tier(min_score: u32, max_score: u32, risk_multiplier_bps: u32) -> RiskTier {
    RiskTier {
        min_score,
        max_score,
        risk_multiplier_bps,
    }
}

fn config_with_tiers(env: &Env, tiers: &[RiskTier]) -> PremiumConfig {
    let mut cfg = default_premium_config(env);
    let mut v = Vec::new(env);
    for t in tiers {
        v.push_back(t.clone());
    }
    cfg.risk_tiers = v;
    cfg
}

fn assert_config_rejected(h: &Harness<'_>, cfg: &PremiumConfig) {
    let result = h.client.try_set_premium_config(&h.admin, cfg);
    assert_eq!(result, Err(Ok(InsuranceError::InvalidPremiumConfig)));
}

#[test]
fn test_set_premium_config_rejects_inverted_tier_range() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    let cfg = config_with_tiers(&env, &[tier(600, 500, 10_000)]);
    assert_config_rejected(&h, &cfg);
}

#[test]
fn test_set_premium_config_rejects_overlapping_tiers() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    // Overlap on 600..=650, supplied out of order.
    let cfg = config_with_tiers(&env, &[tier(600, 750, 10_000), tier(200, 650, 20_000)]);
    assert_config_rejected(&h, &cfg);
    // A tier fully containing another is also an overlap.
    let cfg = config_with_tiers(&env, &[tier(200, 850, 10_000), tier(600, 650, 20_000)]);
    assert_config_rejected(&h, &cfg);
    // Sharing a single boundary score overlaps too (bounds are inclusive).
    let cfg = config_with_tiers(&env, &[tier(200, 500, 10_000), tier(500, 850, 20_000)]);
    assert_config_rejected(&h, &cfg);
}

#[test]
fn test_set_premium_config_rejects_zero_tier_multiplier() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    let cfg = config_with_tiers(&env, &[tier(200, 850, 0)]);
    assert_config_rejected(&h, &cfg);
}

#[test]
fn test_set_premium_config_rejects_oversized_tier_multiplier() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    let cfg = config_with_tiers(&env, &[tier(200, 850, MAX_RISK_MULTIPLIER_BPS + 1)]);
    assert_config_rejected(&h, &cfg);
}

#[test]
fn test_set_premium_config_allows_gaps_between_tiers() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    // Scores 501..=599 are uncovered and price at default_risk_multiplier_bps.
    let cfg = config_with_tiers(&env, &[tier(200, 500, 30_000), tier(600, 850, 10_000)]);
    h.client.set_premium_config(&h.admin, &cfg);
    assert_eq!(h.client.get_premium_config(), Some(cfg));
}

#[test]
fn test_set_premium_config_accepts_boundary_values() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    let mut cfg = default_premium_config(&env);
    cfg.base_rate_bps = BPS_DENOM;
    cfg.tenor_bps_per_day = BPS_DENOM;
    cfg.min_premium_bps = BPS_DENOM;
    cfg.max_premium_bps = BPS_DENOM;
    cfg.default_risk_multiplier_bps = MAX_RISK_MULTIPLIER_BPS;
    h.client.set_premium_config(&h.admin, &cfg);
}

#[test]
fn test_set_premium_config_rejects_out_of_range_rate_fields() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);

    let mut cfg = default_premium_config(&env);
    cfg.base_rate_bps = BPS_DENOM + 1;
    assert_config_rejected(&h, &cfg);

    let mut cfg = default_premium_config(&env);
    cfg.tenor_bps_per_day = BPS_DENOM + 1;
    assert_config_rejected(&h, &cfg);

    let mut cfg = default_premium_config(&env);
    cfg.min_premium_bps = BPS_DENOM + 1;
    cfg.max_premium_bps = BPS_DENOM + 1;
    assert_config_rejected(&h, &cfg);

    let mut cfg = default_premium_config(&env);
    cfg.max_premium_bps = BPS_DENOM + 1;
    assert_config_rejected(&h, &cfg);

    let mut cfg = default_premium_config(&env);
    cfg.default_risk_multiplier_bps = 0;
    assert_config_rejected(&h, &cfg);

    let mut cfg = default_premium_config(&env);
    cfg.default_risk_multiplier_bps = MAX_RISK_MULTIPLIER_BPS + 1;
    assert_config_rejected(&h, &cfg);
}

#[test]
fn test_rejected_premium_config_leaves_existing_config_untouched() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    let before = h.client.get_premium_config();
    let mut cfg = default_premium_config(&env);
    cfg.base_rate_bps = 1_000_000;
    assert_config_rejected(&h, &cfg);
    assert_eq!(h.client.get_premium_config(), before);
}

// ── missing credit score must not be priced as a real score ──────────────────

// Default config: base 200 bps, min 10 bps, max 5_000 bps, and a 200..=549
// tier at 3.0x. On a 1_000_000 principal with zero tenor the base premium is
// 20_000, so a real score of 300 prices at 60_000 (3.0x) while a missing score
// must price at default_risk_multiplier_bps (4.0x) = 80_000.
const PRINCIPAL: i128 = 1_000_000;
const PREMIUM_AT_TIER_300: i128 = 60_000;
const PREMIUM_AT_DEFAULT_MULTIPLIER: i128 = 80_000;

fn insurance_without_credit_score<'a>(
    env: &'a Env,
    h: &Harness<'_>,
) -> (InsuranceReserveClient<'a>, Address) {
    let admin = Address::generate(env);
    let id = env.register(InsuranceReserve, ());
    let client = InsuranceReserveClient::new(env, &id);
    client.initialize(&admin, &h.pool_id, &Address::generate(env));
    client.set_premium_config(&admin, &default_premium_config(env));
    (client, admin)
}

#[test]
fn test_real_score_of_300_uses_tier_multiplier() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    let sme = Address::generate(&env);
    h.credit_client.set_score(&sme, &300);
    assert_eq!(h.client.estimate_premium(&PRINCIPAL, &sme, &0u32), PREMIUM_AT_TIER_300);
}

#[test]
fn test_unset_credit_score_contract_uses_default_multiplier() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    let (client, _admin) = insurance_without_credit_score(&env, &h);
    let sme = Address::generate(&env);
    assert_eq!(
        client.estimate_premium(&PRINCIPAL, &sme, &0u32),
        PREMIUM_AT_DEFAULT_MULTIPLIER
    );
}

#[test]
fn test_failed_credit_score_call_uses_default_multiplier() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    let (client, admin) = insurance_without_credit_score(&env, &h);
    // A contract with no `get_credit_score` makes the cross-contract call fail.
    let not_a_credit_contract = env.register(DummyPool, ());
    client.set_credit_score_contract(&admin, &not_a_credit_contract);
    let sme = Address::generate(&env);
    assert_eq!(
        client.estimate_premium(&PRINCIPAL, &sme, &0u32),
        PREMIUM_AT_DEFAULT_MULTIPLIER
    );
}
