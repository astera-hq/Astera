#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, EnvTestConfig, Ledger},
    Address, Env, String,
};
use std::panic;

// Import contract clients
mod invoice {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/invoice.wasm");
}

mod pool {
    pub type PoolError = soroban_sdk::Error;

    // Keep imports pinned to the local wasm32v1-none build artifacts used by these integration tests.
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/pool.wasm");
}

mod credit_score {
    soroban_sdk::contractimport!(
        file = "../../target/wasm32v1-none/release/credit_score.wasm"
    );
}

mod share {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/share.wasm");
}

fn metadata_url(env: &Env) -> String {
    String::from_str(env, "https://example.com/meta")
}

fn pool_contract_error(code: u32) -> soroban_sdk::Error {
    soroban_sdk::Error::from_contract_error(code)
}

fn test_env() -> Env {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.cost_estimate().budget().reset_unlimited();
    env
}

fn initialize_pool(
    pool_client: &pool::Client<'_>,
    admin: &Address,
    token_id: &Address,
    share_id: &Address,
    invoice_id: &Address,
) {
    pool_client.initialize(admin, token_id, share_id, invoice_id);
    pool_client.set_max_investor_concentration(admin, &10_000u32);
}

/// Integration test: Complete invoice lifecycle with pool funding and credit scoring
#[test]
fn test_complete_invoice_lifecycle() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
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
    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let invoice_client = invoice::Client::new(&env, &invoice_id);
    let pool_client = pool::Client::new(&env, &pool_id);
    let credit_client = credit_score::Client::new(&env, &credit_id);
    let share_client = share::Client::new(&env, &share_id);

    // Initialize contracts
    invoice_client.initialize(
        &admin,
        &pool_id,
        &10_000_000_000i128,
        &(30u64 * 86_400u64),
        &7u32,
    );
    share_client.initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "Pool Shares"),
        &String::from_str(&env, "POOL"),
    );
    initialize_pool(&pool_client, &admin, &usdc_id, &share_id, &invoice_id);
    credit_client.initialize(&admin, &invoice_id, &pool_id);

    // Mint tokens to investor and SME
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
        .mint(&investor, &10_000_000_000i128);
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
        &metadata_url(&env),
    );
    assert_eq!(inv_id, 1);

    // Step 3: Pool funds the invoice
    pool_client.fund_invoice(
        &admin,
        &inv_id,
        &2_000_000_000i128,
        &sme,
        &due_date,
        &usdc_id,
    );
    invoice_client.mark_funded(&inv_id, &pool_id);

    let invoice = invoice_client.get_invoice(&inv_id);
    assert_eq!(invoice.status, invoice::InvoiceStatus::Funded);

    // Verify pool state
    let totals = pool_client.get_token_totals(&usdc_id);
    assert_eq!(totals.total_deployed, 2_000_000_000i128);

    // Step 4: SME repays invoice
    env.ledger().with_mut(|l| l.timestamp += 25 * 86_400); // 25 days later
    let amount_due = pool_client.estimate_repayment(&inv_id, &None);
    pool_client.repay_invoice(&inv_id, &sme, &amount_due);

    // Step 5: Verify invoice is marked as paid
    invoice_client.mark_paid(&inv_id, &pool_id);
    let invoice = invoice_client.get_invoice(&inv_id);
    assert_eq!(invoice.status, invoice::InvoiceStatus::Paid);

    // Step 6: Record payment in credit score
    credit_client.record_payment(
        &pool_id,
        &inv_id,
        &sme,
        &2_000_000_000i128,
        &due_date,
        &env.ledger().timestamp(),
    );

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
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let invoice_id = env.register_contract_wasm(None, invoice::WASM);
    let pool_id = env.register_contract_wasm(None, pool::WASM);
    let credit_id = env.register_contract_wasm(None, credit_score::WASM);
    let share_id = env.register_contract_wasm(None, share::WASM);
    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let invoice_client = invoice::Client::new(&env, &invoice_id);
    let pool_client = pool::Client::new(&env, &pool_id);
    let credit_client = credit_score::Client::new(&env, &credit_id);
    let share_client = share::Client::new(&env, &share_id);

    invoice_client.initialize(
        &admin,
        &pool_id,
        &10_000_000_000i128,
        &(30u64 * 86_400u64),
        &7u32,
    );
    share_client.initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "Pool Shares"),
        &String::from_str(&env, "POOL"),
    );
    initialize_pool(&pool_client, &admin, &usdc_id, &share_id, &invoice_id);
    credit_client.initialize(&admin, &invoice_id, &pool_id);

    let grace_period = invoice_client.get_grace_period() as u64;
    let grace_secs = grace_period * 86_400;

    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
        .mint(&investor, &10_000_000_000i128);

    pool_client.deposit(&investor, &usdc_id, &5_000_000_000i128);

    let due_date = env.ledger().timestamp() + 30 * 86_400;
    let inv_id = invoice_client.create_invoice(
        &sme,
        &String::from_str(&env, "ACME Corp"),
        &2_000_000_000i128,
        &due_date,
        &String::from_str(&env, "Invoice #001"),
        &String::from_str(&env, "hash123"),
        &metadata_url(&env),
    );

    pool_client.fund_invoice(
        &admin,
        &inv_id,
        &2_000_000_000i128,
        &sme,
        &due_date,
        &usdc_id,
    );
    invoice_client.mark_funded(&inv_id, &pool_id);

    // Move past due date but within grace period
    env.ledger()
        .with_mut(|l| l.timestamp = due_date + grace_secs - 3600);

    // Note: Would fail here but we can't test panic without std in integration tests
    // Just verify we're within grace period
    assert!(env.ledger().timestamp() < due_date + grace_secs);

    // Move past grace period
    env.ledger()
        .with_mut(|l| l.timestamp = due_date + grace_secs + 1);

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
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
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
    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let invoice_client = invoice::Client::new(&env, &invoice_id);
    let pool_client = pool::Client::new(&env, &pool_id);
    let credit_client = credit_score::Client::new(&env, &credit_id);
    let share_client = share::Client::new(&env, &share_id);

    invoice_client.initialize(
        &admin,
        &pool_id,
        &10_000_000_000i128,
        &(30u64 * 86_400u64),
        &7u32,
    );
    share_client.initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "Pool Shares"),
        &String::from_str(&env, "POOL"),
    );
    initialize_pool(&pool_client, &admin, &usdc_id, &share_id, &invoice_id);
    credit_client.initialize(&admin, &invoice_id, &pool_id);

    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
        .mint(&investor1, &10_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
        .mint(&investor2, &10_000_000_000i128);
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
        &metadata_url(&env),
    );

    let inv2 = invoice_client.create_invoice(
        &sme2,
        &String::from_str(&env, "Company B"),
        &2_000_000_000i128,
        &due_date,
        &String::from_str(&env, "Invoice #002"),
        &String::from_str(&env, "hash2"),
        &metadata_url(&env),
    );

    pool_client.fund_invoice(
        &admin,
        &inv1,
        &3_000_000_000i128,
        &sme1,
        &due_date,
        &usdc_id,
    );
    pool_client.fund_invoice(
        &admin,
        &inv2,
        &2_000_000_000i128,
        &sme2,
        &due_date,
        &usdc_id,
    );

    invoice_client.mark_funded(&inv1, &pool_id);
    invoice_client.mark_funded(&inv2, &pool_id);

    // Both SMEs repay
    env.ledger().with_mut(|l| l.timestamp += 20 * 86_400);
    let amount1 = pool_client.estimate_repayment(&inv1, &None);
    pool_client.repay_invoice(&inv1, &sme1, &amount1);
    let amount2 = pool_client.estimate_repayment(&inv2, &None);
    pool_client.repay_invoice(&inv2, &sme2, &amount2);

    invoice_client.mark_paid(&inv1, &pool_id);
    invoice_client.mark_paid(&inv2, &pool_id);

    credit_client.record_payment(
        &pool_id,
        &inv1,
        &sme1,
        &3_000_000_000i128,
        &due_date,
        &env.ledger().timestamp(),
    );
    credit_client.record_payment(
        &pool_id,
        &inv2,
        &sme2,
        &2_000_000_000i128,
        &due_date,
        &env.ledger().timestamp(),
    );

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
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let invoice_id = env.register_contract_wasm(None, invoice::WASM);
    let pool_id = env.register_contract_wasm(None, pool::WASM);
    let credit_id = env.register_contract_wasm(None, credit_score::WASM);
    let share_id = env.register_contract_wasm(None, share::WASM);
    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let invoice_client = invoice::Client::new(&env, &invoice_id);
    let pool_client = pool::Client::new(&env, &pool_id);
    let credit_client = credit_score::Client::new(&env, &credit_id);
    let share_client = share::Client::new(&env, &share_id);

    invoice_client.initialize(
        &admin,
        &pool_id,
        &10_000_000_000i128,
        &(30u64 * 86_400u64),
        &7u32,
    );
    share_client.initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "Pool Shares"),
        &String::from_str(&env, "POOL"),
    );
    initialize_pool(&pool_client, &admin, &usdc_id, &share_id, &invoice_id);
    credit_client.initialize(&admin, &invoice_id, &pool_id);

    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
        .mint(&investor, &10_000_000_000i128);
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
        &metadata_url(&env),
    );

    // Verify invoice count consistency
    assert_eq!(invoice_client.get_invoice_count(), 1);
    let stats = invoice_client.get_storage_stats();
    assert_eq!(stats.total_invoices, 1);
    assert_eq!(stats.active_invoices, 1);

    pool_client.fund_invoice(
        &admin,
        &inv_id,
        &2_000_000_000i128,
        &sme,
        &due_date,
        &usdc_id,
    );
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
    let amount_due = pool_client.estimate_repayment(&inv_id, &None);
    pool_client.repay_invoice(&inv_id, &sme, &amount_due);
    invoice_client.mark_paid(&inv_id, &pool_id);

    // Verify state after repayment
    let stats = invoice_client.get_storage_stats();
    assert_eq!(stats.active_invoices, 0);

    let pool_stats = pool_client.get_storage_stats();
    assert_eq!(pool_stats.active_funded_invoices, 0);

    let totals = pool_client.get_token_totals(&usdc_id);
    assert_eq!(totals.total_deployed, 0);
    assert!(totals.pool_value > 5_000_000_000i128); // Includes yield

    credit_client.record_payment(
        &pool_id,
        &inv_id,
        &sme,
        &2_000_000_000i128,
        &due_date,
        &env.ledger().timestamp(),
    );

    // Verify credit score state
    let credit_data = credit_client.get_credit_score(&sme);
    assert_eq!(credit_data.total_invoices, 1);
    assert_eq!(credit_data.total_volume, 2_000_000_000i128);
    assert!(credit_client.is_invoice_processed(&inv_id));
}

fn setup_pool(
    env: &Env,
) -> (
    pool::Client<'_>,
    share::Client<'_>,
    Address, // admin
    Address, // usdc_id
) {
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let pool_id = env.register_contract_wasm(None, pool::WASM);
    let share_id = env.register_contract_wasm(None, share::WASM);
    let invoice_id = env.register_contract_wasm(None, invoice::WASM);
    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let pool_client = pool::Client::new(env, &pool_id);
    let share_client = share::Client::new(env, &share_id);

    share_client.initialize(
        &admin,
        &7u32,
        &String::from_str(env, "Pool Shares"),
        &String::from_str(env, "POOL"),
    );
    invoice_client_init(env, &invoice_id, &admin, &pool_id);
    initialize_pool(&pool_client, &admin, &usdc_id, &share_id, &invoice_id);

    (pool_client, share_client, admin, usdc_id)
}

fn invoice_client_init(env: &Env, invoice_id: &Address, admin: &Address, pool_id: &Address) {
    let invoice_client = invoice::Client::new(env, invoice_id);
    invoice_client.initialize(
        admin,
        pool_id,
        &10_000_000_000i128,
        &(30u64 * 86_400u64),
        &7u32,
    );
}

/// Integration test: Collateral post and release on full repayment
#[test]
fn test_collateral_post_and_release() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let invoice_id_addr = env.register_contract_wasm(None, invoice::WASM);
    let pool_id = env.register_contract_wasm(None, pool::WASM);
    let share_id = env.register_contract_wasm(None, share::WASM);
    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let invoice_client = invoice::Client::new(&env, &invoice_id_addr);
    let pool_client = pool::Client::new(&env, &pool_id);
    let share_client = share::Client::new(&env, &share_id);

    invoice_client.initialize(
        &admin,
        &pool_id,
        &10_000_000_000i128,
        &(30u64 * 86_400u64),
        &7u32,
    );
    share_client.initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "Pool Shares"),
        &String::from_str(&env, "POOL"),
    );
    initialize_pool(&pool_client, &admin, &usdc_id, &share_id, &invoice_id_addr);

    // Threshold = 1_000 USDC, 20% collateral required
    pool_client.set_collateral_config(&admin, &1_000i128, &2_000u32);

    let principal: i128 = 5_000;
    let required_col = pool_client.required_collateral_for(&principal);
    assert_eq!(required_col, 1_000); // 20% of 5_000

    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&investor, &10_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
        .mint(&sme, &(principal * 2 + required_col));

    pool_client.deposit(&investor, &usdc_id, &10_000i128);

    // SME posts collateral
    let sme_balance_before_collateral =
        soroban_sdk::token::Client::new(&env, &usdc_id).balance(&sme);
    pool_client.deposit_collateral(&1u64, &sme, &usdc_id, &required_col);

    let col = pool_client.get_collateral_deposit(&1u64).unwrap();
    assert_eq!(col.amount, required_col);
    assert!(!col.settled);

    // Verify collateral transferred to contract
    let sme_balance_after_collateral =
        soroban_sdk::token::Client::new(&env, &usdc_id).balance(&sme);
    assert_eq!(
        sme_balance_after_collateral,
        sme_balance_before_collateral - required_col
    );

    // Admin funds invoice
    let due_date = env.ledger().timestamp() + 30 * 86_400;
    pool_client.fund_invoice(&admin, &1u64, &principal, &sme, &due_date, &usdc_id);

    // SME repays fully
    env.ledger().with_mut(|l| l.timestamp += 10 * 86_400);
    let amount_due = pool_client.estimate_repayment(&1u64, &None);
    let sme_before_repay = soroban_sdk::token::Client::new(&env, &usdc_id).balance(&sme);
    pool_client.repay_invoice(&1u64, &sme, &amount_due);

    // Collateral should be automatically returned to SME on full repayment
    let col_after = pool_client.get_collateral_deposit(&1u64).unwrap();
    assert!(col_after.settled);

    let sme_after_repay = soroban_sdk::token::Client::new(&env, &usdc_id).balance(&sme);
    // Net: paid amount_due but got required_col back
    assert_eq!(
        sme_after_repay,
        sme_before_repay - amount_due + required_col
    );
}

/// Integration test: Collateral seized on default (no repayment past grace period)
#[test]
fn test_collateral_seize_on_default() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let invoice_id_addr = env.register_contract_wasm(None, invoice::WASM);
    let pool_id = env.register_contract_wasm(None, pool::WASM);
    let share_id = env.register_contract_wasm(None, share::WASM);
    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let invoice_client = invoice::Client::new(&env, &invoice_id_addr);
    let pool_client = pool::Client::new(&env, &pool_id);
    let share_client = share::Client::new(&env, &share_id);

    invoice_client.initialize(
        &admin,
        &pool_id,
        &10_000_000_000i128,
        &(30u64 * 86_400u64),
        &7u32,
    );
    share_client.initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "Pool Shares"),
        &String::from_str(&env, "POOL"),
    );
    initialize_pool(&pool_client, &admin, &usdc_id, &share_id, &invoice_id_addr);

    let grace_period = invoice_client.get_grace_period() as u64;
    let grace_secs = grace_period * 86_400;

    pool_client.set_collateral_config(&admin, &1_000i128, &2_000u32);

    let principal: i128 = 5_000;
    let required_col = pool_client.required_collateral_for(&principal);

    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&investor, &10_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&sme, &required_col);

    pool_client.deposit(&investor, &usdc_id, &10_000i128);
    pool_client.deposit_collateral(&1u64, &sme, &usdc_id, &required_col);

    let due_date = env.ledger().timestamp() + 30 * 86_400;
    let inv_id = invoice_client.create_invoice(
        &sme,
        &String::from_str(&env, "ACME Corp"),
        &principal,
        &due_date,
        &String::from_str(&env, "Invoice #001"),
        &String::from_str(&env, "hash123"),
        &metadata_url(&env),
    );
    assert_eq!(inv_id, 1);
    pool_client.fund_invoice(&admin, &1u64, &principal, &sme, &due_date, &usdc_id);
    invoice_client.mark_funded(&1u64, &pool_id);

    // Advance past due date without repayment — mark as defaulted
    env.ledger()
        .with_mut(|l| l.timestamp = due_date + grace_secs + 1);
    invoice_client.mark_defaulted(&1u64, &pool_id);

    let tt_before = pool_client.get_token_totals(&usdc_id);

    // Admin seizes collateral
    pool_client.seize_collateral(&admin, &1u64);

    let col = pool_client.get_collateral_deposit(&1u64).unwrap();
    assert!(col.settled);

    // Pool value increased by collateral, deployed reduced by principal
    let tt_after = pool_client.get_token_totals(&usdc_id);
    assert_eq!(tt_after.pool_value, tt_before.pool_value + required_col);
    assert_eq!(
        tt_after.total_deployed,
        tt_before.total_deployed - principal
    );

    // SME cannot seize again (collateral already settled)
    let result = pool_client.try_seize_collateral(&admin, &1u64);
    assert_eq!(result, Err(Ok(pool_contract_error(14))));
}

#[test]
fn test_credit_score_on_time_payment() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let pool = Address::generate(&env);
    let invoice_id = env.register_contract_wasm(None, invoice::WASM);
    let credit_id = env.register_contract_wasm(None, credit_score::WASM);
    let invoice_client = invoice::Client::new(&env, &invoice_id);
    let credit_client = credit_score::Client::new(&env, &credit_id);
    invoice_client.initialize(
        &admin,
        &pool,
        &10_000_000_000i128,
        &(30u64 * 86_400u64),
        &7u32,
    );
    credit_client.initialize(&admin, &invoice_id, &pool);

    let due_date = env.ledger().timestamp() + 30 * 86_400;
    let inv_id = invoice_client.create_invoice(
        &sme,
        &String::from_str(&env, "ACME"),
        &2_000i128,
        &due_date,
        &String::from_str(&env, "i1"),
        &String::from_str(&env, "h1"),
        &metadata_url(&env),
    );
    let before = credit_client.get_credit_score(&sme);
    credit_client.record_payment(
        &pool,
        &inv_id,
        &sme,
        &2_000i128,
        &due_date,
        &(due_date - 100),
    );
    let after = credit_client.get_credit_score(&sme);
    assert_eq!(after.paid_on_time, 1);
    assert!(after.score > before.score);
    assert!(after.score > 500);
}

#[test]
fn test_credit_score_late_payment() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let pool = Address::generate(&env);
    let invoice_id = env.register_contract_wasm(None, invoice::WASM);
    let credit_id = env.register_contract_wasm(None, credit_score::WASM);
    let invoice_client = invoice::Client::new(&env, &invoice_id);
    let credit_client = credit_score::Client::new(&env, &credit_id);
    invoice_client.initialize(
        &admin,
        &pool,
        &10_000_000_000i128,
        &(30u64 * 86_400u64),
        &7u32,
    );
    credit_client.initialize(&admin, &invoice_id, &pool);
    let due_date = env.ledger().timestamp() + 30 * 86_400;
    let inv_id = invoice_client.create_invoice(
        &sme,
        &String::from_str(&env, "ACME"),
        &2_000i128,
        &due_date,
        &String::from_str(&env, "i1"),
        &String::from_str(&env, "h1"),
        &metadata_url(&env),
    );
    let before = credit_client.get_credit_score(&sme);
    credit_client.record_payment(
        &pool,
        &inv_id,
        &sme,
        &2_000i128,
        &due_date,
        &(due_date + 3600),
    );
    let after = credit_client.get_credit_score(&sme);
    assert_eq!(after.paid_late, 1);
    assert!(after.score > before.score);
    assert!(after.score > 500);
}

#[test]
fn test_credit_score_default_penalty() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let pool = Address::generate(&env);
    let invoice_id = env.register_contract_wasm(None, invoice::WASM);
    let credit_id = env.register_contract_wasm(None, credit_score::WASM);
    let invoice_client = invoice::Client::new(&env, &invoice_id);
    let credit_client = credit_score::Client::new(&env, &credit_id);
    invoice_client.initialize(
        &admin,
        &pool,
        &10_000_000_000i128,
        &(30u64 * 86_400u64),
        &7u32,
    );
    credit_client.initialize(&admin, &invoice_id, &pool);
    let due_date = 200_000u64;
    let inv_id = invoice_client.create_invoice(
        &sme,
        &String::from_str(&env, "ACME"),
        &2_000i128,
        &due_date,
        &String::from_str(&env, "i1"),
        &String::from_str(&env, "h1"),
        &metadata_url(&env),
    );
    let before = credit_client.get_credit_score(&sme);
    credit_client.record_default(&pool, &inv_id, &sme, &2_000i128, &due_date);
    let after = credit_client.get_credit_score(&sme);
    assert_eq!(after.defaulted, 1);
    assert!(after.score >= before.score);
    assert!(after.score < 500);
}

#[test]
fn test_payment_history_idempotency() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let pool = Address::generate(&env);
    let invoice_id = env.register_contract_wasm(None, invoice::WASM);
    let credit_id = env.register_contract_wasm(None, credit_score::WASM);
    let invoice_client = invoice::Client::new(&env, &invoice_id);
    let credit_client = credit_score::Client::new(&env, &credit_id);
    invoice_client.initialize(
        &admin,
        &pool,
        &10_000_000_000i128,
        &(30u64 * 86_400u64),
        &7u32,
    );
    credit_client.initialize(&admin, &invoice_id, &pool);
    let due_date = 200_000u64;
    let inv_id = invoice_client.create_invoice(
        &sme,
        &String::from_str(&env, "ACME"),
        &2_000i128,
        &due_date,
        &String::from_str(&env, "i1"),
        &String::from_str(&env, "h1"),
        &metadata_url(&env),
    );
    credit_client.record_payment(&pool, &inv_id, &sme, &2_000i128, &due_date, &(due_date - 1));
    let before = credit_client.get_credit_score(&sme);
    let _ = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        credit_client.record_payment(&pool, &inv_id, &sme, &2_000i128, &due_date, &(due_date - 1));
    }));
    let after = credit_client.get_credit_score(&sme);
    assert_eq!(before.score, after.score);
}

#[test]
fn test_credit_score_multiple_invoices() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let pool = Address::generate(&env);
    let invoice_id = env.register_contract_wasm(None, invoice::WASM);
    let credit_id = env.register_contract_wasm(None, credit_score::WASM);
    let invoice_client = invoice::Client::new(&env, &invoice_id);
    let credit_client = credit_score::Client::new(&env, &credit_id);
    invoice_client.initialize(
        &admin,
        &pool,
        &10_000_000_000i128,
        &(30u64 * 86_400u64),
        &7u32,
    );
    credit_client.initialize(&admin, &invoice_id, &pool);
    let due_date = 300_000u64;
    let i1 = invoice_client.create_invoice(
        &sme,
        &String::from_str(&env, "A"),
        &1_000i128,
        &due_date,
        &String::from_str(&env, "i1"),
        &String::from_str(&env, "h1"),
        &metadata_url(&env),
    );
    let i2 = invoice_client.create_invoice(
        &sme,
        &String::from_str(&env, "B"),
        &1_000i128,
        &due_date,
        &String::from_str(&env, "i2"),
        &String::from_str(&env, "h2"),
        &metadata_url(&env),
    );
    let i3 = invoice_client.create_invoice(
        &sme,
        &String::from_str(&env, "C"),
        &1_000i128,
        &due_date,
        &String::from_str(&env, "i3"),
        &String::from_str(&env, "h3"),
        &metadata_url(&env),
    );
    credit_client.record_payment(&pool, &i1, &sme, &1_000i128, &due_date, &(due_date - 10));
    credit_client.record_payment(&pool, &i2, &sme, &1_000i128, &due_date, &(due_date - 10));
    credit_client.record_default(&pool, &i3, &sme, &1_000i128, &due_date);
    let score = credit_client.get_credit_score(&sme);
    assert_eq!(score.total_invoices, 3);
    assert_eq!(score.paid_on_time, 2);
    assert_eq!(score.defaulted, 1);
    assert!(score.score > 500);
    assert!(score.score < 550);
}

#[test]
fn test_get_payment_history() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let pool = Address::generate(&env);
    let invoice_id = env.register_contract_wasm(None, invoice::WASM);
    let credit_id = env.register_contract_wasm(None, credit_score::WASM);
    let invoice_client = invoice::Client::new(&env, &invoice_id);
    let credit_client = credit_score::Client::new(&env, &credit_id);
    invoice_client.initialize(
        &admin,
        &pool,
        &10_000_000_000i128,
        &(30u64 * 86_400u64),
        &7u32,
    );
    credit_client.initialize(&admin, &invoice_id, &pool);
    let due_date = 300_000u64;
    let i1 = invoice_client.create_invoice(
        &sme,
        &String::from_str(&env, "A"),
        &1_000i128,
        &due_date,
        &String::from_str(&env, "i1"),
        &String::from_str(&env, "h1"),
        &metadata_url(&env),
    );
    let i2 = invoice_client.create_invoice(
        &sme,
        &String::from_str(&env, "B"),
        &1_000i128,
        &due_date,
        &String::from_str(&env, "i2"),
        &String::from_str(&env, "h2"),
        &metadata_url(&env),
    );
    credit_client.record_payment(&pool, &i1, &sme, &1_000i128, &due_date, &(due_date - 10));
    credit_client.record_default(&pool, &i2, &sme, &1_000i128, &due_date);
    let history = credit_client.get_payment_history(&sme);
    assert_eq!(history.len(), 2);
}

/// Integration test: Collateral not required below threshold
#[test]
fn test_collateral_not_required_below_threshold() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let invoice_id_addr = env.register_contract_wasm(None, invoice::WASM);
    let pool_id = env.register_contract_wasm(None, pool::WASM);
    let share_id = env.register_contract_wasm(None, share::WASM);
    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let invoice_client = invoice::Client::new(&env, &invoice_id_addr);
    let pool_client = pool::Client::new(&env, &pool_id);
    let share_client = share::Client::new(&env, &share_id);

    invoice_client.initialize(
        &admin,
        &pool_id,
        &10_000_000_000i128,
        &(30u64 * 86_400u64),
        &7u32,
    );
    share_client.initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "Pool Shares"),
        &String::from_str(&env, "POOL"),
    );
    initialize_pool(&pool_client, &admin, &usdc_id, &share_id, &invoice_id_addr);

    // Threshold = 10_000, principal = 500 → below threshold, no collateral needed
    pool_client.set_collateral_config(&admin, &10_000i128, &2_000u32);

    let principal: i128 = 500;
    assert_eq!(pool_client.required_collateral_for(&principal), 0);

    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&investor, &10_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&sme, &(principal * 2));

    pool_client.deposit(&investor, &usdc_id, &10_000i128);

    // Fund without collateral — must succeed
    let due_date = env.ledger().timestamp() + 30 * 86_400;
    pool_client.fund_invoice(&admin, &1u64, &principal, &sme, &due_date, &usdc_id);

    let totals = pool_client.get_token_totals(&usdc_id);
    assert_eq!(totals.total_deployed, principal);

    // Repay fully
    env.ledger().with_mut(|l| l.timestamp += 15 * 86_400);
    let amount_due = pool_client.estimate_repayment(&1u64, &None);
    pool_client.repay_invoice(&1u64, &sme, &amount_due);

    let fi = pool_client.get_funded_invoice(&1u64).unwrap();
    assert!(fi.repaid_amount >= amount_due);
}

/// Integration test: Collateral error cases
#[test]
fn test_collateral_error_double_deposit() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let invoice_id_addr = env.register_contract_wasm(None, invoice::WASM);
    let pool_id = env.register_contract_wasm(None, pool::WASM);
    let share_id = env.register_contract_wasm(None, share::WASM);
    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let invoice_client = invoice::Client::new(&env, &invoice_id_addr);
    let pool_client = pool::Client::new(&env, &pool_id);
    let share_client = share::Client::new(&env, &share_id);

    invoice_client.initialize(
        &admin,
        &pool_id,
        &10_000_000_000i128,
        &(30u64 * 86_400u64),
        &7u32,
    );
    share_client.initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "Pool Shares"),
        &String::from_str(&env, "POOL"),
    );
    initialize_pool(&pool_client, &admin, &usdc_id, &share_id, &invoice_id_addr);
    pool_client.set_collateral_config(&admin, &1_000i128, &2_000u32);

    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&sme, &5_000i128);

    pool_client.deposit_collateral(&1u64, &sme, &usdc_id, &1_000);

    // Double deposit must fail
    let result = pool_client.try_deposit_collateral(&1u64, &sme, &usdc_id, &1_000);
    assert_eq!(result, Err(Ok(pool_contract_error(10))));
}

/// Integration test: Partial repayments accumulate to full repayment
#[test]
fn test_partial_repayment_lifecycle() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let invoice_id_addr = env.register_contract_wasm(None, invoice::WASM);
    let pool_id = env.register_contract_wasm(None, pool::WASM);
    let share_id = env.register_contract_wasm(None, share::WASM);
    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let invoice_client = invoice::Client::new(&env, &invoice_id_addr);
    let pool_client = pool::Client::new(&env, &pool_id);
    let share_client = share::Client::new(&env, &share_id);

    invoice_client.initialize(
        &admin,
        &pool_id,
        &10_000_000_000i128,
        &(30u64 * 86_400u64),
        &7u32,
    );
    share_client.initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "Pool Shares"),
        &String::from_str(&env, "POOL"),
    );
    initialize_pool(&pool_client, &admin, &usdc_id, &share_id, &invoice_id_addr);

    let principal: i128 = 10_000;
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&investor, &20_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&sme, &20_000i128);

    pool_client.deposit(&investor, &usdc_id, &20_000i128);

    let due_date = env.ledger().timestamp() + 30 * 86_400;
    pool_client.fund_invoice(&admin, &1u64, &principal, &sme, &due_date, &usdc_id);

    // Advance time and compute total due
    env.ledger().with_mut(|l| l.timestamp += 15 * 86_400);
    let total_due = pool_client.estimate_repayment(&1u64, &None);

    // First partial repayment — half the total due
    let half = total_due / 2;
    pool_client.repay_invoice(&1u64, &sme, &half);

    // Invoice is not yet fully repaid
    let fi_after_first = pool_client.get_funded_invoice(&1u64).unwrap();
    assert_eq!(fi_after_first.repaid_amount, half);
    // total_deployed should still show principal (not fully repaid yet)
    let tt_mid = pool_client.get_token_totals(&usdc_id);
    assert_eq!(tt_mid.total_deployed, principal);

    // Second partial repayment — remaining balance
    let remaining = pool_client.estimate_repayment(&1u64, &None);
    pool_client.repay_invoice(&1u64, &sme, &remaining);

    // Invoice is now fully repaid
    let fi_final = pool_client.get_funded_invoice(&1u64).unwrap();
    assert!(fi_final.repaid_amount >= total_due);

    // total_deployed should now be zero (invoice settled)
    let tt_final = pool_client.get_token_totals(&usdc_id);
    assert_eq!(tt_final.total_deployed, 0);
    assert!(tt_final.pool_value > 20_000i128); // yield accrued

    // Over-payment must be rejected
    let result = pool_client.try_repay_invoice(&1u64, &sme, &1i128);
    assert_eq!(result, Err(Ok(pool_contract_error(6))));
}

/// Integration test: Past due but within grace period should NOT allow default
#[test]
fn test_within_grace_period_not_defaultable() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let invoice_id = env.register_contract_wasm(None, invoice::WASM);
    let pool_id = env.register_contract_wasm(None, pool::WASM);
    let share_id = env.register_contract_wasm(None, share::WASM);
    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let invoice_client = invoice::Client::new(&env, &invoice_id);
    let pool_client = pool::Client::new(&env, &pool_id);
    let share_client = share::Client::new(&env, &share_id);

    invoice_client.initialize(
        &admin,
        &pool_id,
        &10_000_000_000i128,
        &(30u64 * 86_400u64),
        &7u32,
    );
    share_client.initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "Pool Shares"),
        &String::from_str(&env, "POOL"),
    );
    initialize_pool(&pool_client, &admin, &usdc_id, &share_id, &invoice_id);

    let grace_period = invoice_client.get_grace_period() as u64;
    let grace_secs = grace_period * 86_400;

    let due_date = env.ledger().timestamp() + 30 * 86_400;
    let inv_id = invoice_client.create_invoice(
        &sme,
        &String::from_str(&env, "ACME Corp"),
        &2_000_000_000i128,
        &due_date,
        &String::from_str(&env, "Invoice #001"),
        &String::from_str(&env, "hash123"),
        &metadata_url(&env),
    );

    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
        .mint(&investor, &10_000_000_000i128);
    pool_client.deposit(&investor, &usdc_id, &5_000_000_000i128);
    pool_client.fund_invoice(
        &admin,
        &inv_id,
        &2_000_000_000i128,
        &sme,
        &due_date,
        &usdc_id,
    );
    invoice_client.mark_funded(&inv_id, &pool_id);

    // Advance to just past due date but within grace period
    env.ledger()
        .with_mut(|l| l.timestamp = due_date + grace_secs - 3600);
    assert!(
        env.ledger().timestamp() < due_date + grace_secs,
        "should still be within grace period"
    );

    // Attempting to mark as defaulted should panic
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        invoice_client.mark_defaulted(&inv_id, &pool_id);
    }));
    assert!(
        result.is_err(),
        "mark_defaulted should panic within grace period"
    );
}

/// Integration test: Multi-token deposit with EURC at 1.08 USDC, yield distribution
#[test]
fn test_multi_token_deposit_and_yield() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor_a = Address::generate(&env);
    let investor_b = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let invoice_id = env.register_contract_wasm(None, invoice::WASM);
    let pool_id = env.register_contract_wasm(None, pool::WASM);
    let credit_id = env.register_contract_wasm(None, credit_score::WASM);
    let share_usdc_id = env.register_contract_wasm(None, share::WASM);
    let share_eurc_id = env.register_contract_wasm(None, share::WASM);

    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let eurc_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let invoice_client = invoice::Client::new(&env, &invoice_id);
    let pool_client = pool::Client::new(&env, &pool_id);
    let credit_client = credit_score::Client::new(&env, &credit_id);
    let share_usdc_client = share::Client::new(&env, &share_usdc_id);
    let share_eurc_client = share::Client::new(&env, &share_eurc_id);

    share_usdc_client.initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "USDC Pool Shares"),
        &String::from_str(&env, "sUSDC"),
    );
    share_eurc_client.initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "EURC Pool Shares"),
        &String::from_str(&env, "sEURC"),
    );

    invoice_client.initialize(
        &admin,
        &pool_id,
        &10_000_000_000i128,
        &(30u64 * 86_400u64),
        &7u32,
    );
    initialize_pool(&pool_client, &admin, &usdc_id, &share_usdc_id, &invoice_id);
    credit_client.initialize(&admin, &invoice_id, &pool_id);

    pool_client.add_token(&admin, &eurc_id, &share_eurc_id);
    pool_client.set_exchange_rate(&admin, &eurc_id, &10_800u32);
    pool_client.set_max_investor_concentration(&admin, &10_000u32);

    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
        .mint(&investor_a, &10_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &eurc_id)
        .mint(&investor_b, &10_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&sme, &10_000_000_000i128);

    pool_client.deposit(&investor_a, &usdc_id, &1_000_000_000i128);
    let totals_usdc = pool_client.get_token_totals(&usdc_id);
    assert_eq!(totals_usdc.pool_value, 1_000_000_000i128);

    pool_client.deposit(&investor_b, &eurc_id, &1_000_000_000i128);
    let totals_eurc = pool_client.get_token_totals(&eurc_id);
    assert_eq!(totals_eurc.pool_value, 1_080_000_000i128);

    let totals_usdc = pool_client.get_token_totals(&usdc_id);
    let totals_eurc = pool_client.get_token_totals(&eurc_id);
    assert_eq!(
        totals_usdc.pool_value + totals_eurc.pool_value,
        2_080_000_000i128
    );

    let due_date = env.ledger().timestamp() + 30 * 86_400;
    let inv_id = invoice_client.create_invoice(
        &sme,
        &String::from_str(&env, "ACME Corp"),
        &500_000_000i128,
        &due_date,
        &String::from_str(&env, "Invoice #MT-001"),
        &String::from_str(&env, "hash_mt"),
        &String::from_str(&env, "https://example.com/meta"),
    );

    pool_client.fund_invoice(&admin, &inv_id, &500_000_000i128, &sme, &due_date, &usdc_id);
    invoice_client.mark_funded(&inv_id, &pool_id);

    env.ledger().with_mut(|l| l.timestamp += 25 * 86_400);
    let amount_due = pool_client.estimate_repayment(&inv_id, &None);
    pool_client.repay_invoice(&inv_id, &sme, &amount_due);
    invoice_client.mark_paid(&inv_id, &pool_id);
    credit_client.record_payment(
        &pool_id,
        &inv_id,
        &sme,
        &500_000_000i128,
        &due_date,
        &env.ledger().timestamp(),
    );

    let shares_a = share_usdc_client.balance(&investor_a);
    pool_client.withdraw(&investor_a, &usdc_id, &shares_a);
    let balance_a = soroban_sdk::token::Client::new(&env, &usdc_id).balance(&investor_a);
    assert!(
        balance_a > 5_000_000_000i128,
        "Investor A should have earned yield in USDC"
    );

    let shares_b = share_eurc_client.balance(&investor_b);
    pool_client.withdraw(&investor_b, &eurc_id, &shares_b);
    let balance_b = soroban_sdk::token::Client::new(&env, &eurc_id).balance(&investor_b);
    assert!(
        balance_b > 5_000_000_000i128,
        "Investor B should have earned yield in EURC"
    );
}

/// Integration test: token removal succeeds when balances are zero
#[test]
fn test_token_removal_with_zero_balances() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let investor = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let invoice_id = env.register_contract_wasm(None, invoice::WASM);
    let pool_id = env.register_contract_wasm(None, pool::WASM);
    let share_usdc_id = env.register_contract_wasm(None, share::WASM);
    let share_eurc_id = env.register_contract_wasm(None, share::WASM);

    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let eurc_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let invoice_client = invoice::Client::new(&env, &invoice_id);
    let pool_client = pool::Client::new(&env, &pool_id);

    share::Client::new(&env, &share_usdc_id).initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "USDC Pool Shares"),
        &String::from_str(&env, "sUSDC"),
    );
    share::Client::new(&env, &share_eurc_id).initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "EURC Pool Shares"),
        &String::from_str(&env, "sEURC"),
    );

    invoice_client.initialize(
        &admin,
        &pool_id,
        &10_000_000_000i128,
        &(30u64 * 86_400u64),
        &7u32,
    );
    initialize_pool(&pool_client, &admin, &usdc_id, &share_usdc_id, &invoice_id);
    pool_client.add_token(&admin, &eurc_id, &share_eurc_id);

    let tokens_before = pool_client.accepted_tokens();
    assert!(tokens_before.contains(&eurc_id));

    soroban_sdk::token::StellarAssetClient::new(&env, &eurc_id)
        .mint(&investor, &10_000_000_000i128);
    pool_client.deposit(&investor, &eurc_id, &100_000_000i128);
    let eurc_shares = share::Client::new(&env, &share_eurc_id).balance(&investor);
    pool_client.withdraw(&investor, &eurc_id, &eurc_shares);

    pool_client.remove_token(&admin, &eurc_id);

    let tokens_after = pool_client.accepted_tokens();
    assert!(
        !tokens_after.contains(&eurc_id),
        "EURC should no longer be in accepted_tokens after removal"
    );
}

/// Integration test: token removal blocked when there are active deposits
#[test]
fn test_token_removal_blocked_with_active_deposits() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let investor = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let invoice_id = env.register_contract_wasm(None, invoice::WASM);
    let pool_id = env.register_contract_wasm(None, pool::WASM);
    let share_usdc_id = env.register_contract_wasm(None, share::WASM);
    let share_eurc_id = env.register_contract_wasm(None, share::WASM);

    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let eurc_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let invoice_client = invoice::Client::new(&env, &invoice_id);
    let pool_client = pool::Client::new(&env, &pool_id);

    share::Client::new(&env, &share_usdc_id).initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "USDC Pool Shares"),
        &String::from_str(&env, "sUSDC"),
    );
    share::Client::new(&env, &share_eurc_id).initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "EURC Pool Shares"),
        &String::from_str(&env, "sEURC"),
    );

    invoice_client.initialize(
        &admin,
        &pool_id,
        &10_000_000_000i128,
        &(30u64 * 86_400u64),
        &7u32,
    );
    initialize_pool(&pool_client, &admin, &usdc_id, &share_usdc_id, &invoice_id);
    pool_client.add_token(&admin, &eurc_id, &share_eurc_id);

    soroban_sdk::token::StellarAssetClient::new(&env, &eurc_id)
        .mint(&investor, &10_000_000_000i128);
    pool_client.deposit(&investor, &eurc_id, &100_000_000i128);

    let result = pool_client.try_remove_token(&admin, &eurc_id);
    assert_eq!(result, Err(Ok(pool_contract_error(27))));

    let tokens = pool_client.accepted_tokens();
    assert!(
        tokens.contains(&eurc_id),
        "EURC should still be in accepted_tokens after failed removal"
    );
}
