#![cfg(test)]

use insurance::{
    CollateralDepositView, CreditScoreData, FundedInvoiceView, InsuranceError, InsuranceReserve,
    InsuranceReserveClient, PremiumConfig, RiskTier,
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

    let caller = Address::generate(&env);
    let payout = h.client.file_claim(&caller, &1u64);
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

    let caller = Address::generate(&env);
    let payout = h.client.file_claim(&caller, &1u64);

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

    let caller = Address::generate(&env);
    h.client.file_claim(&caller, &1u64);
    let result = h.client.try_file_claim(&caller, &1u64);
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

    let caller = Address::generate(&env);
    let result = h.client.try_file_claim(&caller, &1u64);
    assert_eq!(result, Err(Ok(InsuranceError::InvoiceNotDefaulted)));
}

#[test]
fn test_file_claim_no_coverage_found() {
    let env = Env::default();
    env.mock_all_auths();
    let h = setup(&env);
    let caller = Address::generate(&env);
    let result = h.client.try_file_claim(&caller, &999u64);
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

    let caller = Address::generate(&env);
    let payout = h.client.file_claim(&caller, &1u64);
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

    let caller = Address::generate(&env);
    let result = h.client.try_file_claim(&caller, &1u64);
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
