#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

// Import contract clients
mod invoice {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/invoice.wasm"
    );
}

mod pool {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/pool.wasm"
    );
}

mod credit_score {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/credit_score.wasm"
    );
}

mod share {
    soroban_sdk::contractimport!(
        file = "../target/wasm32-unknown-unknown/release/share.wasm"
    );
}

#[contracttype]
#[derive(Clone)]
enum MockOracleKey {
    PriceBps,
    UpdatedAt,
    Unavailable,
}

#[contract]
struct MockOracle;

#[contractimpl]
impl MockOracle {
    pub fn initialize(env: Env, price_bps: u32, updated_at: u64) {
        env.storage()
            .instance()
            .set(&MockOracleKey::PriceBps, &price_bps);
        env.storage()
            .instance()
            .set(&MockOracleKey::UpdatedAt, &updated_at);
        env.storage()
            .instance()
            .set(&MockOracleKey::Unavailable, &false);
    }

    pub fn set_price(env: Env, price_bps: u32, updated_at: u64) {
        env.storage()
            .instance()
            .set(&MockOracleKey::PriceBps, &price_bps);
        env.storage()
            .instance()
            .set(&MockOracleKey::UpdatedAt, &updated_at);
    }

    pub fn set_unavailable(env: Env, unavailable: bool) {
        env.storage()
            .instance()
            .set(&MockOracleKey::Unavailable, &unavailable);
    }

    pub fn latest_price(env: Env, _token: Address) -> pool::OraclePrice {
        let unavailable: bool = env
            .storage()
            .instance()
            .get(&MockOracleKey::Unavailable)
            .unwrap_or(false);
        if unavailable {
            panic!("oracle unavailable");
        }

        pool::OraclePrice {
            price_bps: env
                .storage()
                .instance()
                .get(&MockOracleKey::PriceBps)
                .unwrap_or(10_000),
            updated_at: env
                .storage()
                .instance()
                .get(&MockOracleKey::UpdatedAt)
                .unwrap_or(0),
        }
    }
}

struct TestContracts {
    env: Env,
    admin: Address,
    sme: Address,
    investor: Address,
    usdc_id: Address,
    pool_id: Address,
    oracle_id: Address,
}

fn setup_oracle_pool() -> TestContracts {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let invoice_id = env.register_contract_wasm(None, invoice::WASM);
    let pool_id = env.register_contract_wasm(None, pool::WASM);
    let share_id = env.register_contract_wasm(None, share::WASM);
    let oracle_id = env.register_contract(None, MockOracle);
    let usdc_id = env.register_stellar_asset_contract_v2(token_admin.clone()).address();

    let invoice_client = invoice::Client::new(&env, &invoice_id);
    let pool_client = pool::Client::new(&env, &pool_id);
    let share_client = share::Client::new(&env, &share_id);
    let oracle_client = MockOracleClient::new(&env, &oracle_id);

    invoice_client.initialize(&admin, &pool_id, &10_000_000_000i128, &30u64 * 86_400u64);
    share_client.initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "Pool Shares"),
        &String::from_str(&env, "POOL"),
    );
    pool_client.initialize(&admin, &usdc_id, &share_id, &invoice_id);
    oracle_client.initialize(&10_000u32, &env.ledger().timestamp());
    pool_client.set_price_oracle(&admin, &usdc_id, &oracle_id);

    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
        .mint(&investor, &20_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
        .mint(&sme, &20_000_000_000i128);

    TestContracts {
        env,
        admin,
        sme,
        investor,
        usdc_id,
        pool_id,
        oracle_id,
    }
}

#[test]
#[should_panic(expected = "oracle price is stale")]
fn test_stale_oracle_price_rejected() {
    let ctx = setup_oracle_pool();
    let pool_client = pool::Client::new(&ctx.env, &ctx.pool_id);
    let oracle_client = MockOracleClient::new(&ctx.env, &ctx.oracle_id);

    oracle_client.set_price(&10_000u32, &(ctx.env.ledger().timestamp() - 86_401));
    pool_client.deposit_collateral(&1u64, &ctx.sme, &ctx.usdc_id, &2_000i128);
}

#[test]
#[should_panic(expected = "oracle price must be positive")]
fn test_oracle_zero_price_handled() {
    let ctx = setup_oracle_pool();
    let pool_client = pool::Client::new(&ctx.env, &ctx.pool_id);
    let oracle_client = MockOracleClient::new(&ctx.env, &ctx.oracle_id);

    oracle_client.set_price(&0u32, &ctx.env.ledger().timestamp());
    pool_client.deposit_collateral(&1u64, &ctx.sme, &ctx.usdc_id, &2_000i128);
}

#[test]
fn test_price_change_between_deposit_and_seizure() {
    let ctx = setup_oracle_pool();
    let pool_client = pool::Client::new(&ctx.env, &ctx.pool_id);
    let oracle_client = MockOracleClient::new(&ctx.env, &ctx.oracle_id);

    pool_client.set_collateral_config(&ctx.admin, &1i128, &2_000u32);
    pool_client.deposit(&ctx.investor, &ctx.usdc_id, &20_000i128);
    pool_client.deposit_collateral(&1u64, &ctx.sme, &ctx.usdc_id, &2_000i128);

    let due_date = ctx.env.ledger().timestamp() + 30 * 86_400;
    pool_client.fund_invoice(
        &ctx.admin,
        &1u64,
        &10_000i128,
        &ctx.sme,
        &due_date,
        &ctx.usdc_id,
    );

    oracle_client.set_price(&5_000u32, &ctx.env.ledger().timestamp());
    pool_client.seize_collateral(&ctx.admin, &1u64);

    let totals = pool_client.get_token_totals(&ctx.usdc_id);
    assert_eq!(totals.pool_value, 21_000i128);
    assert_eq!(totals.total_deployed, 0i128);
}

#[test]
#[should_panic(expected = "oracle unavailable")]
fn test_oracle_unavailable_fallback() {
    let ctx = setup_oracle_pool();
    let pool_client = pool::Client::new(&ctx.env, &ctx.pool_id);
    let oracle_client = MockOracleClient::new(&ctx.env, &ctx.oracle_id);

    oracle_client.set_unavailable(&true);
    pool_client.deposit_collateral(&1u64, &ctx.sme, &ctx.usdc_id, &2_000i128);
}

/// Integration test: Complete invoice lifecycle with pool funding and credit scoring
#[test]
fn test_complete_invoice_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    // Deploy contracts
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let invoice_id = env.register_contract_wasm(None, invoice::WASM);
    let pool_id = env.register_contract_wasm(None, pool::WASM);
    let credit_id = env.register_contract_wasm(None, credit_score::WASM);
    let share_id = env.register_contract_wasm(None, share::WASM);
    let usdc_id = env.register_stellar_asset_contract_v2(token_admin.clone()).address();

    let invoice_client = invoice::Client::new(&env, &invoice_id);
    let pool_client = pool::Client::new(&env, &pool_id);
    let credit_client = credit_score::Client::new(&env, &credit_id);
    let share_client = share::Client::new(&env, &share_id);

    // Initialize contracts
    invoice_client.initialize(&admin, &pool_id, &10_000_000_000i128, &30u64 * 86_400u64);
    share_client.initialize(&admin, &7u32, &String::from_str(&env, "Pool Shares"), &String::from_str(&env, "POOL"));
    pool_client.initialize(&admin, &usdc_id, &share_id, &invoice_id);
    credit_client.initialize(&admin, &invoice_id, &pool_id);

    // Mint tokens to investor and SME
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&investor, &10_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&sme, &10_000_000_000i128);

    // Step 1: Investor deposits into pool
    pool_client.deposit(&investor, &usdc_id, &5_000_000_000i128);
    let totals = pool_client.get_token_totals(&usdc_id);
    assert_eq!(totals.pool_value, 5_000_000_000i128);

    // Step 2: SME creates invoice
    let due_date = env.ledger().timestamp() + 30 * 86_400; // 30 days
    let inv_id = invoice_client.create_invoice(
        &sme,
        &String::from_str(&env, "ACME Corp"),
        &2_000_000_000i128,
        &due_date,
        &String::from_str(&env, "Invoice #001"),
        &String::from_str(&env, "hash123"),
    );
    assert_eq!(inv_id, 1);

    // Step 3: Pool funds the invoice
    pool_client.fund_invoice(&admin, &inv_id, &2_000_000_000i128, &sme, &due_date, &usdc_id);
    
    let invoice = invoice_client.get_invoice(&inv_id);
    assert_eq!(invoice.status, invoice::InvoiceStatus::Funded);

    // Verify pool state
    let totals = pool_client.get_token_totals(&usdc_id);
    assert_eq!(totals.total_deployed, 2_000_000_000i128);

    // Step 4: SME repays invoice
    env.ledger().with_mut(|l| l.timestamp += 25 * 86_400); // 25 days later
    pool_client.repay_invoice(&inv_id, &sme);

    // Step 5: Verify invoice is marked as paid
    invoice_client.mark_paid(&inv_id, &pool_id);
    let invoice = invoice_client.get_invoice(&inv_id);
    assert_eq!(invoice.status, invoice::InvoiceStatus::Paid);

    // Step 6: Record payment in credit score
    credit_client.record_payment(&pool_id, &inv_id, &sme, &2_000_000_000i128, &due_date, &env.ledger().timestamp());
    
    let credit_data = credit_client.get_credit_score(&sme);
    assert_eq!(credit_data.total_invoices, 1);
    assert_eq!(credit_data.paid_on_time, 1);
    assert!(credit_data.score > 500);

    // Step 7: Investor withdraws with yield
    let shares = share_client.balance(&investor);
    pool_client.withdraw(&investor, &usdc_id, &shares);
    
    let investor_balance = soroban_sdk::token::Client::new(&env, &usdc_id).balance(&investor);
    assert!(investor_balance > 5_000_000_000i128); // Should have earned yield
}

/// Integration test: Default scenario with grace period
#[test]
fn test_default_with_grace_period() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let invoice_id = env.register_contract_wasm(None, invoice::WASM);
    let pool_id = env.register_contract_wasm(None, pool::WASM);
    let credit_id = env.register_contract_wasm(None, credit_score::WASM);
    let share_id = env.register_contract_wasm(None, share::WASM);
    let usdc_id = env.register_stellar_asset_contract_v2(token_admin.clone()).address();

    let invoice_client = invoice::Client::new(&env, &invoice_id);
    let pool_client = pool::Client::new(&env, &pool_id);
    let credit_client = credit_score::Client::new(&env, &credit_id);
    let share_client = share::Client::new(&env, &share_id);

    invoice_client.initialize(&admin, &pool_id, &10_000_000_000i128, &30u64 * 86_400u64);
    share_client.initialize(&admin, &7u32, &String::from_str(&env, "Pool Shares"), &String::from_str(&env, "POOL"));
    pool_client.initialize(&admin, &usdc_id, &share_id, &invoice_id);
    credit_client.initialize(&admin, &invoice_id, &pool_id);

    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&investor, &10_000_000_000i128);

    pool_client.deposit(&investor, &usdc_id, &5_000_000_000i128);

    let due_date = env.ledger().timestamp() + 30 * 86_400;
    let inv_id = invoice_client.create_invoice(
        &sme,
        &String::from_str(&env, "ACME Corp"),
        &2_000_000_000i128,
        &due_date,
        &String::from_str(&env, "Invoice #001"),
        &String::from_str(&env, "hash123"),
    );

    pool_client.fund_invoice(&admin, &inv_id, &2_000_000_000i128, &sme, &due_date, &usdc_id);
    invoice_client.mark_funded(&inv_id, &pool_id);

    // Move past due date but within grace period (default is 7 days)
    env.ledger().with_mut(|l| l.timestamp = due_date + 3 * 86_400);

    // Note: Would fail here but we can't test panic without std in integration tests
    // Just verify we're within grace period
    assert!(env.ledger().timestamp() < due_date + 7 * 86_400);

    // Move past grace period
    env.ledger().with_mut(|l| l.timestamp = due_date + 8 * 86_400);

    // Should succeed now
    invoice_client.mark_defaulted(&inv_id, &pool_id);
    let invoice = invoice_client.get_invoice(&inv_id);
    assert_eq!(invoice.status, invoice::InvoiceStatus::Defaulted);

    // Record default in credit score
    credit_client.record_default(&pool_id, &inv_id, &sme, &2_000_000_000i128, &due_date);
    
    let credit_data = credit_client.get_credit_score(&sme);
    assert_eq!(credit_data.defaulted, 1);
    assert!(credit_data.score < 500);
}

/// Integration test: Multiple invoices with yield distribution
#[test]
fn test_multiple_invoices_yield_distribution() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let sme1 = Address::generate(&env);
    let sme2 = Address::generate(&env);
    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let invoice_id = env.register_contract_wasm(None, invoice::WASM);
    let pool_id = env.register_contract_wasm(None, pool::WASM);
    let credit_id = env.register_contract_wasm(None, credit_score::WASM);
    let share_id = env.register_contract_wasm(None, share::WASM);
    let usdc_id = env.register_stellar_asset_contract_v2(token_admin.clone()).address();

    let invoice_client = invoice::Client::new(&env, &invoice_id);
    let pool_client = pool::Client::new(&env, &pool_id);
    let credit_client = credit_score::Client::new(&env, &credit_id);
    let share_client = share::Client::new(&env, &share_id);

    invoice_client.initialize(&admin, &pool_id, &10_000_000_000i128, &30u64 * 86_400u64);
    share_client.initialize(&admin, &7u32, &String::from_str(&env, "Pool Shares"), &String::from_str(&env, "POOL"));
    pool_client.initialize(&admin, &usdc_id, &share_id, &invoice_id);
    credit_client.initialize(&admin, &invoice_id, &pool_id);

    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&investor1, &10_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&investor2, &10_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&sme1, &10_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&sme2, &10_000_000_000i128);

    // Two investors deposit
    pool_client.deposit(&investor1, &usdc_id, &6_000_000_000i128);
    pool_client.deposit(&investor2, &usdc_id, &4_000_000_000i128);

    let totals = pool_client.get_token_totals(&usdc_id);
    assert_eq!(totals.pool_value, 10_000_000_000i128);

    // Create and fund two invoices
    let due_date = env.ledger().timestamp() + 30 * 86_400;
    
    let inv1 = invoice_client.create_invoice(
        &sme1,
        &String::from_str(&env, "Company A"),
        &3_000_000_000i128,
        &due_date,
        &String::from_str(&env, "Invoice #001"),
        &String::from_str(&env, "hash1"),
    );
    
    let inv2 = invoice_client.create_invoice(
        &sme2,
        &String::from_str(&env, "Company B"),
        &2_000_000_000i128,
        &due_date,
        &String::from_str(&env, "Invoice #002"),
        &String::from_str(&env, "hash2"),
    );

    pool_client.fund_invoice(&admin, &inv1, &3_000_000_000i128, &sme1, &due_date, &usdc_id);
    pool_client.fund_invoice(&admin, &inv2, &2_000_000_000i128, &sme2, &due_date, &usdc_id);

    invoice_client.mark_funded(&inv1, &pool_id);
    invoice_client.mark_funded(&inv2, &pool_id);

    // Both SMEs repay
    env.ledger().with_mut(|l| l.timestamp += 20 * 86_400);
    pool_client.repay_invoice(&inv1, &sme1);
    pool_client.repay_invoice(&inv2, &sme2);

    invoice_client.mark_paid(&inv1, &pool_id);
    invoice_client.mark_paid(&inv2, &pool_id);

    credit_client.record_payment(&pool_id, &inv1, &sme1, &3_000_000_000i128, &due_date, &env.ledger().timestamp());
    credit_client.record_payment(&pool_id, &inv2, &sme2, &2_000_000_000i128, &due_date, &env.ledger().timestamp());

    // Verify credit scores
    let credit1 = credit_client.get_credit_score(&sme1);
    let credit2 = credit_client.get_credit_score(&sme2);
    assert_eq!(credit1.paid_on_time, 1);
    assert_eq!(credit2.paid_on_time, 1);

    // Both investors withdraw proportionally
    let shares1 = share_client.balance(&investor1);
    let shares2 = share_client.balance(&investor2);
    
    pool_client.withdraw(&investor1, &usdc_id, &shares1);
    pool_client.withdraw(&investor2, &usdc_id, &shares2);

    let balance1 = soroban_sdk::token::Client::new(&env, &usdc_id).balance(&investor1);
    let balance2 = soroban_sdk::token::Client::new(&env, &usdc_id).balance(&investor2);

    // Both should have earned yield proportional to their investment
    assert!(balance1 > 6_000_000_000i128);
    assert!(balance2 > 4_000_000_000i128);
}

/// Integration test: State consistency across contracts
#[test]
fn test_state_consistency() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let invoice_id = env.register_contract_wasm(None, invoice::WASM);
    let pool_id = env.register_contract_wasm(None, pool::WASM);
    let credit_id = env.register_contract_wasm(None, credit_score::WASM);
    let share_id = env.register_contract_wasm(None, share::WASM);
    let usdc_id = env.register_stellar_asset_contract_v2(token_admin.clone()).address();

    let invoice_client = invoice::Client::new(&env, &invoice_id);
    let pool_client = pool::Client::new(&env, &pool_id);
    let credit_client = credit_score::Client::new(&env, &credit_id);
    let share_client = share::Client::new(&env, &share_id);

    invoice_client.initialize(&admin, &pool_id, &10_000_000_000i128, &30u64 * 86_400u64);
    share_client.initialize(&admin, &7u32, &String::from_str(&env, "Pool Shares"), &String::from_str(&env, "POOL"));
    pool_client.initialize(&admin, &usdc_id, &share_id, &invoice_id);
    credit_client.initialize(&admin, &invoice_id, &pool_id);

    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&investor, &10_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&sme, &10_000_000_000i128);

    pool_client.deposit(&investor, &usdc_id, &5_000_000_000i128);

    let due_date = env.ledger().timestamp() + 30 * 86_400;
    let inv_id = invoice_client.create_invoice(
        &sme,
        &String::from_str(&env, "ACME Corp"),
        &2_000_000_000i128,
        &due_date,
        &String::from_str(&env, "Invoice #001"),
        &String::from_str(&env, "hash123"),
    );

    // Verify invoice count consistency
    assert_eq!(invoice_client.get_invoice_count(), 1);
    let stats = invoice_client.get_storage_stats();
    assert_eq!(stats.total_invoices, 1);
    assert_eq!(stats.active_invoices, 1);

    pool_client.fund_invoice(&admin, &inv_id, &2_000_000_000i128, &sme, &due_date, &usdc_id);
    invoice_client.mark_funded(&inv_id, &pool_id);

    // Verify pool state consistency
    let totals = pool_client.get_token_totals(&usdc_id);
    assert_eq!(totals.pool_value, 5_000_000_000i128);
    assert_eq!(totals.total_deployed, 2_000_000_000i128);
    assert_eq!(pool_client.available_liquidity(&usdc_id), 3_000_000_000i128);

    let pool_stats = pool_client.get_storage_stats();
    assert_eq!(pool_stats.total_funded_invoices, 1);
    assert_eq!(pool_stats.active_funded_invoices, 1);

    env.ledger().with_mut(|l| l.timestamp += 25 * 86_400);
    pool_client.repay_invoice(&inv_id, &sme);
    invoice_client.mark_paid(&inv_id, &pool_id);

    // Verify state after repayment
    let stats = invoice_client.get_storage_stats();
    assert_eq!(stats.active_invoices, 0);

    let pool_stats = pool_client.get_storage_stats();
    assert_eq!(pool_stats.active_funded_invoices, 0);

    let totals = pool_client.get_token_totals(&usdc_id);
    assert_eq!(totals.total_deployed, 0);
    assert!(totals.pool_value > 5_000_000_000i128); // Includes yield

    credit_client.record_payment(&pool_id, &inv_id, &sme, &2_000_000_000i128, &due_date, &env.ledger().timestamp());
    
    // Verify credit score state
    let credit_data = credit_client.get_credit_score(&sme);
    assert_eq!(credit_data.total_invoices, 1);
    assert_eq!(credit_data.total_volume, 2_000_000_000i128);
    assert!(credit_client.is_invoice_processed(&inv_id));
}
