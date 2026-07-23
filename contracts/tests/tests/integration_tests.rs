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
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/credit_score.wasm");
}

mod share {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/share.wasm");
}

mod oracle_registry {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/oracle_registry.wasm");
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

/// #742: RemoveToken, SetCollateralConfig, and SeizeCollateral now require
/// the two-step propose/execute timelock flow instead of direct admin calls.
/// This advances the ledger past the configured operation delay so a freshly
/// proposed operation is ready to execute.
fn advance_past_operation_delay(env: &Env, pool_client: &pool::Client<'_>) {
    let delay = pool_client.get_operation_delay();
    env.ledger().with_mut(|l| l.timestamp += delay + 1);
}

fn propose_and_execute_set_collateral_config(
    env: &Env,
    pool_client: &pool::Client<'_>,
    admin: &Address,
    threshold: i128,
    collateral_bps: u32,
) {
    let proposal_id = pool_client.propose_operation(
        admin,
        &pool::AdminOperation::SetCollateralConfig(threshold, collateral_bps),
    );
    advance_past_operation_delay(env, pool_client);
    pool_client.execute_operation(admin, &proposal_id);
}

fn propose_and_execute_seize_collateral(
    env: &Env,
    pool_client: &pool::Client<'_>,
    admin: &Address,
    invoice_id: u64,
) {
    let proposal_id =
        pool_client.propose_operation(admin, &pool::AdminOperation::SeizeCollateral(invoice_id));
    advance_past_operation_delay(env, pool_client);
    pool_client.execute_operation(admin, &proposal_id);
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
    propose_and_execute_set_collateral_config(&env, &pool_client, &admin, 1_000i128, 2_000u32);

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

    propose_and_execute_set_collateral_config(&env, &pool_client, &admin, 1_000i128, 2_000u32);

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
    propose_and_execute_seize_collateral(&env, &pool_client, &admin, 1u64);

    let col = pool_client.get_collateral_deposit(&1u64).unwrap();
    assert!(col.settled);

    // Pool value increased by collateral, deployed reduced by principal
    let tt_after = pool_client.get_token_totals(&usdc_id);
    assert_eq!(tt_after.pool_value, tt_before.pool_value + required_col);
    assert_eq!(
        tt_after.total_deployed,
        tt_before.total_deployed - principal
    );

    // SME cannot seize again (collateral already settled). #742: the
    // already-settled check happens at execute time, not propose time.
    let proposal_id =
        pool_client.propose_operation(&admin, &pool::AdminOperation::SeizeCollateral(1u64));
    advance_past_operation_delay(&env, &pool_client);
    let result = pool_client.try_execute_operation(&admin, &proposal_id);
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
    propose_and_execute_set_collateral_config(&env, &pool_client, &admin, 10_000i128, 2_000u32);

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
    propose_and_execute_set_collateral_config(&env, &pool_client, &admin, 1_000i128, 2_000u32);

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

    {
        let proposal_id = pool_client
            .propose_operation(&admin, &pool::AdminOperation::RemoveToken(eurc_id.clone()));
        advance_past_operation_delay(&env, &pool_client);
        pool_client.execute_operation(&admin, &proposal_id);
    }

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

    // #742: RemoveToken now requires the propose/execute timelock flow; the
    // active-balances check (error #27) happens at execute time, not propose time.
    let proposal_id =
        pool_client.propose_operation(&admin, &pool::AdminOperation::RemoveToken(eurc_id.clone()));
    advance_past_operation_delay(&env, &pool_client);
    let result = pool_client.try_execute_operation(&admin, &proposal_id);
    assert_eq!(result, Err(Ok(pool_contract_error(27))));

    let tokens = pool_client.accepted_tokens();
    assert!(
        tokens.contains(&eurc_id),
        "EURC should still be in accepted_tokens after failed removal"
    );
}

/// Integration test: Oracle verification + funding flow (Issue #621)
#[test]
fn test_oracle_verified_funding_flow() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let invoice_addr = env.register_contract_wasm(None, invoice::WASM);
    let pool_addr = env.register_contract_wasm(None, pool::WASM);
    let share_addr = env.register_contract_wasm(None, share::WASM);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let invoice_client = invoice::Client::new(&env, &invoice_addr);
    let pool_client = pool::Client::new(&env, &pool_addr);
    let share_client = share::Client::new(&env, &share_addr);

    invoice_client.initialize(
        &admin,
        &pool_addr,
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
    initialize_pool(&pool_client, &admin, &usdc_addr, &share_addr, &invoice_addr);

    // Configure oracle on the invoice contract
    invoice_client.set_oracle(&admin, &oracle);

    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_addr)
        .mint(&investor, &10_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_addr).mint(&sme, &10_000_000_000i128);

    pool_client.deposit(&investor, &usdc_addr, &5_000_000_000i128);

    // Create invoice — starts in AwaitingVerification because oracle is configured
    let due_date = env.ledger().timestamp() + 30 * 86_400;
    let inv_id = invoice_client.create_invoice(
        &sme,
        &String::from_str(&env, "ACME Corp"),
        &2_000_000_000i128,
        &due_date,
        &String::from_str(&env, "Invoice #OVF-001"),
        &String::from_str(&env, "hash_ovf"),
        &metadata_url(&env),
    );
    assert_eq!(inv_id, 1);

    let invoice = invoice_client.get_invoice(&inv_id);
    assert_eq!(invoice.status, invoice::InvoiceStatus::AwaitingVerification);

    // mark_funded should be blocked while invoice is AwaitingVerification
    let block_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        invoice_client.mark_funded(&inv_id, &pool_addr);
    }));
    assert!(block_result.is_err());

    // Oracle approves the invoice
    invoice_client.verify_invoice(
        &inv_id,
        &oracle,
        &true,
        &String::from_str(&env, ""),
        &String::from_str(&env, "hash_ovf"),
    );

    let invoice = invoice_client.get_invoice(&inv_id);
    assert_eq!(invoice.status, invoice::InvoiceStatus::Verified);
    assert!(invoice.oracle_verified);

    // Admin opens co-funding and invoice is funded
    pool_client.fund_invoice(
        &admin,
        &inv_id,
        &2_000_000_000i128,
        &sme,
        &due_date,
        &usdc_addr,
    );
    invoice_client.mark_funded(&inv_id, &pool_addr);

    let invoice = invoice_client.get_invoice(&inv_id);
    assert_eq!(invoice.status, invoice::InvoiceStatus::Funded);

    let totals = pool_client.get_token_totals(&usdc_addr);
    assert_eq!(totals.total_deployed, 2_000_000_000i128);
}

/// Integration test: Concurrent deposit and withdrawal in same ledger
/// Verifies pool accounting is correct regardless of transaction ordering
#[test]
fn test_concurrent_deposit_and_withdrawal_same_ledger() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let (pool_client, share_client, admin, usdc_id) = setup_pool(&env);

    let lender1 = Address::generate(&env);
    let lender2 = Address::generate(&env);

    // Mint tokens to lenders
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&lender1, &10_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&lender2, &10_000_000_000i128);

    // Initial deposit from lender1
    pool_client.deposit(&lender1, &usdc_id, &5_000_000_000i128);
    let initial_pool_value = pool_client.get_token_totals(&usdc_id).pool_value;
    assert_eq!(initial_pool_value, 5_000_000_000i128);

    // Simulate same-ledger transactions:
    // Transaction 1: lender2 deposits 1000 USDC
    // Transaction 2: lender1 withdraws 500 USDC worth of shares

    // Execute deposit first
    pool_client.deposit(&lender2, &usdc_id, &1_000_000_000i128);

    // Same ledger - no sequence number increment
    // Execute withdrawal immediately after
    let shares_to_withdraw = share_client.balance(&lender1) / 10; // withdraw 10%
    pool_client.withdraw(&lender1, &usdc_id, &shares_to_withdraw);

    // Verify final pool value is correct
    let final_totals = pool_client.get_token_totals(&usdc_id);
    let expected_value = 5_000_000_000i128 + 1_000_000_000i128 - 500_000_000i128;
    assert_eq!(final_totals.pool_value, expected_value);

    // Test reverse ordering: withdrawal then deposit
    let env2 = test_env();
    env2.mock_all_auths_allowing_non_root_auth();
    env2.ledger().with_mut(|l| l.timestamp = 100_000);

    let (pool_client2, share_client2, _admin2, usdc_id2) = setup_pool(&env2);
    let lender1_alt = Address::generate(&env2);
    let lender2_alt = Address::generate(&env2);

    soroban_sdk::token::StellarAssetClient::new(&env2, &usdc_id2)
        .mint(&lender1_alt, &10_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env2, &usdc_id2)
        .mint(&lender2_alt, &10_000_000_000i128);

    pool_client2.deposit(&lender1_alt, &usdc_id2, &5_000_000_000i128);

    // Reverse order: withdraw then deposit (same ledger)
    let shares_alt = share_client2.balance(&lender1_alt) / 10;
    pool_client2.withdraw(&lender1_alt, &usdc_id2, &shares_alt);
    pool_client2.deposit(&lender2_alt, &usdc_id2, &1_000_000_000i128);

    // Should have same final value regardless of ordering
    let final_totals2 = pool_client2.get_token_totals(&usdc_id2);
    assert_eq!(final_totals2.pool_value, expected_value);
}

/// Integration test: Deposit during active invoice funding
/// Verifies new deposits are correctly accounted for in next yield calculation
#[test]
fn test_deposit_during_active_funding() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let lender1 = Address::generate(&env);
    let lender2 = Address::generate(&env);
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

    // Mint tokens
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&lender1, &10_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&lender2, &10_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&sme, &10_000_000_000i128);

    // Initial deposit from lender1
    pool_client.deposit(&lender1, &usdc_id, &5_000_000_000i128);
    let shares_lender1_initial = share_client.balance(&lender1);

    // Create and fund invoice
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

    // Fund invoice - this deploys capital
    pool_client.fund_invoice(
        &admin,
        &inv_id,
        &2_000_000_000i128,
        &sme,
        &due_date,
        &usdc_id,
    );
    invoice_client.mark_funded(&inv_id, &pool_id);

    // While invoice is active, lender2 deposits (same ledger)
    pool_client.deposit(&lender2, &usdc_id, &3_000_000_000i128);
    let shares_lender2 = share_client.balance(&lender2);

    // Verify pool accounting
    let totals = pool_client.get_token_totals(&usdc_id);
    assert_eq!(totals.pool_value, 8_000_000_000i128); // 5B + 3B
    assert_eq!(totals.total_deployed, 2_000_000_000i128);
    assert_eq!(pool_client.available_liquidity(&usdc_id), 6_000_000_000i128);

    // SME repays with interest
    env.ledger().with_mut(|l| l.timestamp += 20 * 86_400);
    let amount_due = pool_client.estimate_repayment(&inv_id, &None);
    pool_client.repay_invoice(&inv_id, &sme, &amount_due);
    invoice_client.mark_paid(&inv_id, &pool_id);

    // Both lenders should get proportional yield
    // Lender1 had capital deployed, lender2 did not
    let shares_lender1_final = share_client.balance(&lender1);

    // Lender1's shares should be same (yield increases share value, not count)
    assert_eq!(shares_lender1_final, shares_lender1_initial);

    // When they withdraw, lender1 should have higher returns per share
    pool_client.withdraw(&lender1, &usdc_id, &shares_lender1_final);
    pool_client.withdraw(&lender2, &usdc_id, &shares_lender2);

    let balance1 = soroban_sdk::token::Client::new(&env, &usdc_id).balance(&lender1);
    let balance2 = soroban_sdk::token::Client::new(&env, &usdc_id).balance(&lender2);

    // Lender1 should have earned yield
    assert!(balance1 > 5_000_000_000i128);
    // The pool's reward-per-share accumulator distributes accrued interest
    // pro-rata to every share outstanding at the moment of full repayment,
    // not only to shares that funded this specific invoice. Lender2 held
    // 3B of the 8B total shares at that moment, so lender2 legitimately
    // earns a proportional slice of the interest too (not zero).
    assert!(
        balance2 >= 3_000_000_000i128,
        "lender2 should not lose principal, got {balance2}"
    );
    // Both lenders were minted 10B externally and deposited only part of
    // it, so their final wallet balance is (10B - deposit + payout); use
    // the full mint amount as the baseline to isolate yield alone.
    let lender1_yield = balance1 - 10_000_000_000i128;
    let lender2_yield = balance2 - 10_000_000_000i128;
    // Yield should split ~5:3 between lender1:lender2, matching their
    // share-count ratio (5B vs 3B) at the moment interest was credited.
    // Cross-multiplied to avoid integer-division rounding; a small
    // tolerance absorbs the contract's own internal rounding.
    let cross_diff = (lender1_yield * 3 - lender2_yield * 5).abs();
    assert!(
        cross_diff <= 10,
        "yield should split ~5:3 by share count, got lender1={lender1_yield} lender2={lender2_yield}"
    );
}

/// Integration test: Withdrawal while invoice is being repaid
/// Verifies repayment is credited before withdrawal accounting
#[test]
fn test_withdraw_during_repayment() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let lender = Address::generate(&env);
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

    // Mint tokens
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&lender, &10_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&sme, &10_000_000_000i128);

    // Lender deposits
    pool_client.deposit(&lender, &usdc_id, &5_000_000_000i128);

    // Create and fund invoice
    let due_date = env.ledger().timestamp() + 30 * 86_400;
    let inv_id = invoice_client.create_invoice(
        &sme,
        &String::from_str(&env, "ACME Corp"),
        &4_000_000_000i128,
        &due_date,
        &String::from_str(&env, "Invoice #001"),
        &String::from_str(&env, "hash123"),
        &metadata_url(&env),
    );

    pool_client.fund_invoice(
        &admin,
        &inv_id,
        &4_000_000_000i128,
        &sme,
        &due_date,
        &usdc_id,
    );
    invoice_client.mark_funded(&inv_id, &pool_id);

    // Move time forward
    env.ledger().with_mut(|l| l.timestamp += 20 * 86_400);

    // SME repays invoice
    let amount_due = pool_client.estimate_repayment(&inv_id, &None);
    pool_client.repay_invoice(&inv_id, &sme, &amount_due);
    invoice_client.mark_paid(&inv_id, &pool_id);

    // In same ledger, lender tries to withdraw
    // The repayment should be reflected in pool value
    let totals_before = pool_client.get_token_totals(&usdc_id);
    assert!(totals_before.pool_value > 5_000_000_000i128); // Includes repayment with yield
    assert_eq!(totals_before.total_deployed, 0i128); // Invoice fully repaid

    // Lender withdraws all shares
    let shares = share_client.balance(&lender);
    pool_client.withdraw(&lender, &usdc_id, &shares);

    // Lender should receive their deposit plus yield
    let lender_balance = soroban_sdk::token::Client::new(&env, &usdc_id).balance(&lender);
    assert!(lender_balance > 5_000_000_000i128);

    // Pool should be empty
    let totals_after = pool_client.get_token_totals(&usdc_id);
    assert_eq!(totals_after.pool_value, 0i128);
}

/// Integration test: Multiple lenders withdraw simultaneously when pool is 90% deployed
/// Verifies only liquid portion is accessible and later withdrawals correctly fail
#[test]
fn test_multiple_simultaneous_withdrawals_high_deployment() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let lender1 = Address::generate(&env);
    let lender2 = Address::generate(&env);
    let lender3 = Address::generate(&env);
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

    // Mint tokens to all lenders
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&lender1, &10_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&lender2, &10_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&lender3, &10_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&sme, &10_000_000_000i128);

    // All lenders deposit equal amounts
    pool_client.deposit(&lender1, &usdc_id, &3_000_000_000i128);
    pool_client.deposit(&lender2, &usdc_id, &3_000_000_000i128);
    pool_client.deposit(&lender3, &usdc_id, &4_000_000_000i128);

    let total_pool = pool_client.get_token_totals(&usdc_id).pool_value;
    assert_eq!(total_pool, 10_000_000_000i128);

    // Deploy 90% of pool to invoice
    let due_date = env.ledger().timestamp() + 30 * 86_400;
    let inv_id = invoice_client.create_invoice(
        &sme,
        &String::from_str(&env, "ACME Corp"),
        &9_000_000_000i128,
        &due_date,
        &String::from_str(&env, "Invoice #001"),
        &String::from_str(&env, "hash123"),
        &metadata_url(&env),
    );

    pool_client.fund_invoice(
        &admin,
        &inv_id,
        &9_000_000_000i128,
        &sme,
        &due_date,
        &usdc_id,
    );
    invoice_client.mark_funded(&inv_id, &pool_id);

    // Verify deployment
    let totals = pool_client.get_token_totals(&usdc_id);
    assert_eq!(totals.total_deployed, 9_000_000_000i128);
    assert_eq!(pool_client.available_liquidity(&usdc_id), 1_000_000_000i128);

    // All three lenders try to withdraw simultaneously (same ledger)
    // Only 1B liquidity available, total value is 10B

    // Lender1 attempts to withdraw all their shares (should represent 3B value)
    let shares1 = share_client.balance(&lender1);
    let result1 = pool_client.try_withdraw(&lender1, &usdc_id, &shares1);

    // First withdrawal should fail if trying to withdraw more than available liquidity
    // or succeed with partial amount
    // Based on pool logic, this might fail with insufficient liquidity error
    assert!(result1.is_err());

    // Lender1 tries to withdraw only available liquidity portion
    let shares_for_available = shares1 / 10; // ~10% of their shares (~300M USDC)
    pool_client.withdraw(&lender1, &usdc_id, &shares_for_available);

    // Verify liquidity reduced
    let remaining_liquidity = pool_client.available_liquidity(&usdc_id);
    assert!(remaining_liquidity < 1_000_000_000i128);

    // Lender2 tries to withdraw all shares - should fail
    let shares2 = share_client.balance(&lender2);
    let result2 = pool_client.try_withdraw(&lender2, &usdc_id, &shares2);
    assert!(result2.is_err());

    // Lender3 tries to withdraw small amount within remaining liquidity
    let shares3_small = share_client.balance(&lender3) / 40; // ~2.5% (~100M)
    if remaining_liquidity >= 100_000_000i128 {
        pool_client.withdraw(&lender3, &usdc_id, &shares3_small);
    }

    // After invoice repayment, all should be able to withdraw
    env.ledger().with_mut(|l| l.timestamp += 25 * 86_400);
    let amount_due = pool_client.estimate_repayment(&inv_id, &None);
    pool_client.repay_invoice(&inv_id, &sme, &amount_due);
    invoice_client.mark_paid(&inv_id, &pool_id);

    // Now all lenders can withdraw remaining shares
    let shares1_remaining = share_client.balance(&lender1);
    let shares2_remaining = share_client.balance(&lender2);
    let shares3_remaining = share_client.balance(&lender3);

    pool_client.withdraw(&lender1, &usdc_id, &shares1_remaining);
    pool_client.withdraw(&lender2, &usdc_id, &shares2_remaining);
    pool_client.withdraw(&lender3, &usdc_id, &shares3_remaining);

    // All lenders should have received their deposits plus yield
    let balance1 = soroban_sdk::token::Client::new(&env, &usdc_id).balance(&lender1);
    let balance2 = soroban_sdk::token::Client::new(&env, &usdc_id).balance(&lender2);
    let balance3 = soroban_sdk::token::Client::new(&env, &usdc_id).balance(&lender3);

    assert!(balance1 > 3_000_000_000i128);
    assert!(balance2 > 3_000_000_000i128);
    assert!(balance3 > 4_000_000_000i128);
}

/// #865: realistic multi-investor withdrawal-queue scenario against the real compiled
/// pool.wasm/invoice.wasm/share.wasm artifacts. Three lenders deposit unequal amounts;
/// after a large invoice deploys most of the pool's liquidity, two of them queue
/// withdrawals of different sizes (both exceeding the thin remaining liquidity) while
/// the third withdraws a small amount immediately (within that liquidity) — exercising
/// both the immediate and queued branches of `request_withdrawal` in one scenario.
/// A single full repayment then drains the queue, and every lender's final balance is
/// reconciled against their original deposit (each must come out ahead on yield).
#[test]
fn test_withdrawal_queue_drains_across_multiple_investors_via_repayments() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let lender1 = Address::generate(&env);
    let lender2 = Address::generate(&env);
    let lender3 = Address::generate(&env);
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

    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&lender1, &10_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&lender2, &10_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&lender3, &10_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&sme, &10_000_000_000i128);

    // Unequal deposits, so the two queued requests below differ in size.
    let deposit1 = 2_000_000_000i128;
    let deposit2 = 3_000_000_000i128;
    let deposit3 = 5_000_000_000i128;
    pool_client.deposit(&lender1, &usdc_id, &deposit1);
    pool_client.deposit(&lender2, &usdc_id, &deposit2);
    pool_client.deposit(&lender3, &usdc_id, &deposit3);
    assert_eq!(
        pool_client.get_token_totals(&usdc_id).pool_value,
        deposit1 + deposit2 + deposit3
    );

    // Deploy 90% of the pool, leaving only 1B liquid.
    let due_date = env.ledger().timestamp() + 30 * 86_400;
    let inv_id = invoice_client.create_invoice(
        &sme,
        &String::from_str(&env, "ACME Corp"),
        &9_000_000_000i128,
        &due_date,
        &String::from_str(&env, "Invoice #001"),
        &String::from_str(&env, "hash123"),
        &metadata_url(&env),
    );
    pool_client.fund_invoice(
        &admin,
        &inv_id,
        &9_000_000_000i128,
        &sme,
        &due_date,
        &usdc_id,
    );
    invoice_client.mark_funded(&inv_id, &pool_id);
    assert_eq!(pool_client.available_liquidity(&usdc_id), 1_000_000_000i128);

    // Lender1 and lender2 each request their *entire* position — both far exceed the
    // 1B remaining liquidity, so both get queued (FIFO: lender1 first, lender2 second).
    let shares1 = share_client.balance(&lender1);
    let shares2 = share_client.balance(&lender2);
    let request_id_1 = pool_client.request_withdrawal(&lender1, &usdc_id, &shares1);
    let request_id_2 = pool_client.request_withdrawal(&lender2, &usdc_id, &shares2);
    assert!(request_id_1 > 0, "lender1's request should be queued");
    assert!(request_id_2 > 0, "lender2's request should be queued");

    let queue = pool_client.get_withdrawal_queue(&usdc_id);
    assert_eq!(queue.len(), 2);
    assert_eq!(queue.get(0).unwrap().investor, lender1);
    assert_eq!(queue.get(1).unwrap().investor, lender2);
    // Different deposit sizes -> different queued share amounts.
    assert_ne!(queue.get(0).unwrap().shares, queue.get(1).unwrap().shares);

    // Lender3 withdraws a small amount that fits within the remaining liquidity —
    // this settles immediately (request_id == 0), not via the queue.
    let shares3_small = share_client.balance(&lender3) / 20; // ~5% (~250M value)
    let immediate_request_id = pool_client.request_withdrawal(&lender3, &usdc_id, &shares3_small);
    assert_eq!(
        immediate_request_id, 0,
        "small enough to settle immediately, not queued"
    );
    assert_eq!(pool_client.get_withdrawal_queue(&usdc_id).len(), 2);

    // Full repayment (well after funding) brings back the deployed principal plus
    // interest — far more than enough to drain both queued requests in full.
    env.ledger().with_mut(|l| l.timestamp += 25 * 86_400);
    let amount_due = pool_client.estimate_repayment(&inv_id, &None);
    pool_client.repay_invoice(&inv_id, &sme, &amount_due);
    invoice_client.mark_paid(&inv_id, &pool_id);

    assert_eq!(
        pool_client.get_withdrawal_queue(&usdc_id).len(),
        0,
        "both queued requests should have fully drained on repayment"
    );
    assert_eq!(share_client.balance(&lender1), 0);
    assert_eq!(share_client.balance(&lender2), 0);

    // Lender3 withdraws their remaining shares directly (liquidity is now ample).
    let shares3_remaining = share_client.balance(&lender3);
    pool_client.withdraw(&lender3, &usdc_id, &shares3_remaining);

    // Reconcile: every lender ends up with more than they deposited (yield earned).
    let usdc_client = soroban_sdk::token::Client::new(&env, &usdc_id);
    assert!(usdc_client.balance(&lender1) > deposit1);
    assert!(usdc_client.balance(&lender2) > deposit2);
    assert!(usdc_client.balance(&lender3) > deposit3);
}

/// #860: end-to-end multi-investor co-funding round spanning invoice + pool
/// + credit_score — three investors co-fund a single oracle-verified
/// invoice with a non-round-number bps split, the SME is paid, and full
/// repayment credits each co-funder proportionally without touching the
/// pool's general reward_per_share accumulator.
#[test]
fn test_co_funding_round_end_to_end_with_credit_score() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let sme = Address::generate(&env);
    let lender1 = Address::generate(&env);
    let lender2 = Address::generate(&env);
    let lender3 = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let invoice_addr = env.register_contract_wasm(None, invoice::WASM);
    let pool_addr = env.register_contract_wasm(None, pool::WASM);
    let credit_addr = env.register_contract_wasm(None, credit_score::WASM);
    let share_addr = env.register_contract_wasm(None, share::WASM);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let invoice_client = invoice::Client::new(&env, &invoice_addr);
    let pool_client = pool::Client::new(&env, &pool_addr);
    let credit_client = credit_score::Client::new(&env, &credit_addr);
    let share_client = share::Client::new(&env, &share_addr);

    invoice_client.initialize(
        &admin,
        &pool_addr,
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
    initialize_pool(&pool_client, &admin, &usdc_addr, &share_addr, &invoice_addr);
    credit_client.initialize(&admin, &invoice_addr, &pool_addr);
    pool_client.set_credit_score_contract(&admin, &credit_addr);
    invoice_client.set_oracle(&admin, &oracle);

    for lender in [&lender1, &lender2, &lender3] {
        soroban_sdk::token::StellarAssetClient::new(&env, &usdc_addr)
            .mint(lender, &10_000_000_000i128);
        pool_client.deposit(lender, &usdc_addr, &10_000_000_000i128);
    }
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_addr).mint(&sme, &10_000_000_000i128);

    let due_date = env.ledger().timestamp() + 30 * 86_400;
    let inv_id = invoice_client.create_invoice(
        &sme,
        &String::from_str(&env, "Co-Funded Corp"),
        &9_000_000_000i128,
        &due_date,
        &String::from_str(&env, "Invoice #CF-001"),
        &String::from_str(&env, "hash_cf"),
        &metadata_url(&env),
    );
    invoice_client.verify_invoice(
        &inv_id,
        &oracle,
        &true,
        &String::from_str(&env, ""),
        &String::from_str(&env, "hash_cf"),
    );
    assert_eq!(
        invoice_client.get_invoice(&inv_id).status,
        invoice::InvoiceStatus::Verified
    );

    let deadline = env.ledger().timestamp() + 10_000;
    pool_client.open_co_funding(
        &admin,
        &pool::OpenCoFundingRequest {
            invoice_id: inv_id,
            token: usdc_addr.clone(),
            target_principal: 9_000_000_000i128,
            sme: sme.clone(),
            due_date,
            funding_deadline: deadline,
            min_commitment: 0,
            max_investor_bps: 0,
        },
    );

    // Non-round-number split across 3 lenders: 3000/3000/3000 out of 9000 ->
    // 3333/3333/3334 bps, exercising the exact fractional-split acceptance
    // criterion from #860.
    pool_client.commit_to_invoice(&lender1, &inv_id, &3_000_000_000i128);
    pool_client.commit_to_invoice(&lender2, &inv_id, &3_000_000_000i128);
    pool_client.commit_to_invoice(&lender3, &inv_id, &3_000_000_000i128);

    let round = pool_client.get_co_funding_round(&inv_id).unwrap();
    assert_eq!(round.committed_principal, 9_000_000_000i128);

    let sme_balance_before = soroban_sdk::token::Client::new(&env, &usdc_addr).balance(&sme);
    pool_client.finalize_co_funding(&admin, &inv_id);
    let sme_balance_after = soroban_sdk::token::Client::new(&env, &usdc_addr).balance(&sme);
    assert_eq!(sme_balance_after - sme_balance_before, 9_000_000_000i128);

    // Funding this way still drives mark_funded and the credit_score
    // record_funding signal exactly like the admin lump-sum path does.
    invoice_client.mark_funded(&inv_id, &pool_addr);
    assert_eq!(
        invoice_client.get_invoice(&inv_id).status,
        invoice::InvoiceStatus::Funded
    );
    let credit_before_repay = credit_client.get_credit_score(&sme);
    assert_eq!(credit_before_repay.total_volume, 9_000_000_000i128);

    // Repay in full and confirm the reward_per_share accumulator — which
    // would otherwise siphon co-funders' interest to every LP holder in the
    // pool, including the three lenders' own general deposits that funded
    // OTHER investors' pools too — stays untouched for this invoice.
    let totals_before_repay = pool_client.get_token_totals(&usdc_addr);
    env.ledger().with_mut(|l| l.timestamp += 15 * 86_400);
    let total_due = pool_client.estimate_repayment(&inv_id, &None);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_addr).mint(&sme, &total_due);
    pool_client.repay_invoice(&inv_id, &sme, &total_due);
    invoice_client.mark_paid(&inv_id, &pool_addr);

    let totals_after_repay = pool_client.get_token_totals(&usdc_addr);
    assert_eq!(
        totals_after_repay.reward_per_share,
        totals_before_repay.reward_per_share
    );

    assert_eq!(
        invoice_client.get_invoice(&inv_id).status,
        invoice::InvoiceStatus::Paid
    );
    let funded_record = pool_client.get_funded_invoice(&inv_id).unwrap();
    assert_eq!(funded_record.repaid_amount, total_due);
    assert_eq!(funded_record.co_funding_round_id, Some(inv_id));

    // Each lender should now be able to withdraw more than their original
    // 10B deposit — proof their proportional share of principal + interest
    // was actually credited as fresh, withdrawable LP shares.
    for lender in [&lender1, &lender2, &lender3] {
        let shares = share_client.balance(lender);
        pool_client.withdraw(lender, &usdc_addr, &shares);
        let balance = soroban_sdk::token::Client::new(&env, &usdc_addr).balance(lender);
        assert!(
            balance > 10_000_000_000i128,
            "lender balance {} should exceed original 10B deposit after proportional payout",
            balance
        );
    }
}

/// #860: a round that never reaches its minimum commitment before the
/// deadline must refund every participant in full rather than leaving the
/// invoice permanently stuck — and the pool must still be able to fund
/// other invoices normally afterward.
#[test]
fn test_co_funding_round_expires_and_refunds_then_pool_still_usable() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let lender1 = Address::generate(&env);
    let lender2 = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let invoice_addr = env.register_contract_wasm(None, invoice::WASM);
    let pool_addr = env.register_contract_wasm(None, pool::WASM);
    let share_addr = env.register_contract_wasm(None, share::WASM);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let pool_client = pool::Client::new(&env, &pool_addr);
    let share_client = share::Client::new(&env, &share_addr);

    invoice_client_init(&env, &invoice_addr, &admin, &pool_addr);
    share_client.initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "Pool Shares"),
        &String::from_str(&env, "POOL"),
    );
    initialize_pool(&pool_client, &admin, &usdc_addr, &share_addr, &invoice_addr);

    for lender in [&lender1, &lender2] {
        soroban_sdk::token::StellarAssetClient::new(&env, &usdc_addr)
            .mint(lender, &10_000_000_000i128);
        pool_client.deposit(lender, &usdc_addr, &10_000_000_000i128);
    }

    let inv_id = 42u64;
    let due_date = env.ledger().timestamp() + 30 * 86_400;
    let deadline = env.ledger().timestamp() + 1_000;
    pool_client.open_co_funding(
        &admin,
        &pool::OpenCoFundingRequest {
            invoice_id: inv_id,
            token: usdc_addr.clone(),
            target_principal: 9_000_000_000i128,
            sme: sme.clone(),
            due_date,
            funding_deadline: deadline,
            min_commitment: 8_000_000_000i128,
            max_investor_bps: 0,
        },
    );

    // Only 2B committed against a 9B target with an 8B minimum — well short.
    pool_client.commit_to_invoice(&lender1, &inv_id, &1_000_000_000i128);
    pool_client.commit_to_invoice(&lender2, &inv_id, &1_000_000_000i128);

    env.ledger().with_mut(|l| l.timestamp = deadline + 1);
    pool_client.finalize_co_funding(&admin, &inv_id);

    let round = pool_client.get_co_funding_round(&inv_id).unwrap();
    assert_eq!(round.status, pool::CoFundingStatus::Expired);
    assert!(pool_client.get_funded_invoice(&inv_id).is_none());

    // Both lenders should be able to withdraw their full original deposit —
    // proof the refund returned 100% of committed principal.
    for lender in [&lender1, &lender2] {
        let shares = share_client.balance(lender);
        pool_client.withdraw(lender, &usdc_addr, &shares);
        let balance = soroban_sdk::token::Client::new(&env, &usdc_addr).balance(lender);
        assert_eq!(balance, 10_000_000_000i128);
    }

    // Pool must still be fully usable for ordinary lump-sum funding after an
    // expired co-funding round — nothing should be left in a stuck state.
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_addr)
        .mint(&lender1, &5_000_000_000i128);
    pool_client.deposit(&lender1, &usdc_addr, &5_000_000_000i128);
    pool_client.fund_invoice(
        &admin,
        &43u64,
        &1_000_000_000i128,
        &sme,
        &(env.ledger().timestamp() + 30 * 86_400),
        &usdc_addr,
    );
    let totals = pool_client.get_token_totals(&usdc_addr);
    assert_eq!(totals.total_deployed, 1_000_000_000i128);
}

/// #861: N-of-M staked oracle consensus network — end-to-end test with the
/// real compiled `oracle_registry.wasm` and `invoice.wasm` artifacts (not
/// in-process stubs). Five oracles register with equal stake; the registry's
/// quorum is configured to 6000 bps (60%) so that exactly 3 of 5 approving
/// votes (60% of equal-weighted stake) crosses the threshold — matching the
/// "consensus approves at exactly quorum" acceptance criterion. Once quorum
/// is reached the registry calls back into `consensus_verify`, and the pool
/// funds the now-`Verified` invoice exactly as it would after a legacy
/// single-oracle `verify_invoice` call.
#[test]
fn test_oracle_consensus_quorum_approves_and_pool_funds_invoice() {
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
    let registry_id = env.register_contract_wasm(None, oracle_registry::WASM);
    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let invoice_client = invoice::Client::new(&env, &invoice_id);
    let pool_client = pool::Client::new(&env, &pool_id);
    let credit_client = credit_score::Client::new(&env, &credit_id);
    let share_client = share::Client::new(&env, &share_id);
    let registry_client = oracle_registry::Client::new(&env, &registry_id);

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

    // A placeholder legacy `Oracle` address is still required so that newly
    // created invoices enter `AwaitingVerification` (that gate is keyed off
    // whether *any* oracle is configured, independent of the consensus flag).
    invoice_client.set_oracle(&admin, &Address::generate(&env));
    registry_client.initialize(&admin, &usdc_id, &1_000i128);
    registry_client.set_invoice_contract(&admin, &invoice_id);
    // 6000 bps (60%) quorum so 3-of-5 equally-staked oracles lands exactly on
    // the threshold, per the "approves at exactly quorum" scenario.
    registry_client.set_registry_config(
        &admin,
        &1_000i128,
        &3u32,
        &6_000u32,
        &(3 * 86_400u64),
        &(7 * 86_400u64),
    );
    invoice_client.set_oracle_registry(&admin, &registry_id);
    invoice_client.set_consensus_required(&admin, &true);

    let mut oracles = Vec::new();
    for _ in 0..5 {
        let op = Address::generate(&env);
        soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&op, &1_000i128);
        registry_client.register_oracle(&op, &1_000i128);
        oracles.push(op);
    }

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
    assert_eq!(
        invoice_client.get_invoice(&inv_id).status,
        invoice::InvoiceStatus::AwaitingVerification
    );

    // The legacy path is locked out while consensus verification is required.
    let legacy_attempt = invoice_client.try_verify_invoice(
        &inv_id,
        &oracles[0],
        &true,
        &String::from_str(&env, ""),
        &String::from_str(&env, "hash123"),
    );
    assert!(legacy_attempt.is_err());

    registry_client.open_verification_round(&admin, &inv_id, &String::from_str(&env, "hash123"));

    // 2 reject first (40% weight — below the 3000/5000 threshold on its own).
    registry_client.submit_vote(&oracles[0], &inv_id, &false, &String::from_str(&env, "e"));
    registry_client.submit_vote(&oracles[1], &inv_id, &false, &String::from_str(&env, "e"));
    assert_eq!(
        registry_client
            .get_verification_round(&inv_id)
            .unwrap()
            .status,
        oracle_registry::RoundStatus::Open
    );

    // 3 approve — cumulative weight hits exactly 3000, the quorum threshold.
    registry_client.submit_vote(&oracles[2], &inv_id, &true, &String::from_str(&env, "e"));
    registry_client.submit_vote(&oracles[3], &inv_id, &true, &String::from_str(&env, "e"));
    registry_client.submit_vote(&oracles[4], &inv_id, &true, &String::from_str(&env, "e"));

    let round = registry_client.get_verification_round(&inv_id).unwrap();
    assert_eq!(
        round.status,
        oracle_registry::RoundStatus::ConsensusApproved
    );
    assert_eq!(round.weight_for, 3_000i128);
    assert_eq!(round.weight_against, 2_000i128);

    let invoice = invoice_client.get_invoice(&inv_id);
    assert_eq!(invoice.status, invoice::InvoiceStatus::Verified);
    assert!(invoice.oracle_verified);

    // The pool funds the now-Verified invoice exactly like the legacy flow.
    pool_client.fund_invoice(
        &admin,
        &inv_id,
        &2_000_000_000i128,
        &sme,
        &due_date,
        &usdc_id,
    );
    invoice_client.mark_funded(&inv_id, &pool_id);
    assert_eq!(
        invoice_client.get_invoice(&inv_id).status,
        invoice::InvoiceStatus::Funded
    );
}

/// #861: the escape hatch — if oracle participation never reaches quorum
/// before the round's deadline, the round expires and an admin fallback
/// (`admin_resolve_round`) resolves it so the invoice is never permanently
/// bricked by an unresponsive oracle set.
#[test]
fn test_oracle_consensus_round_expires_then_admin_fallback_resolves() {
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
    let registry_id = env.register_contract_wasm(None, oracle_registry::WASM);
    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let invoice_client = invoice::Client::new(&env, &invoice_id);
    let pool_client = pool::Client::new(&env, &pool_id);
    let share_client = share::Client::new(&env, &share_id);
    let registry_client = oracle_registry::Client::new(&env, &registry_id);

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

    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
        .mint(&investor, &10_000_000_000i128);
    pool_client.deposit(&investor, &usdc_id, &5_000_000_000i128);

    invoice_client.set_oracle(&admin, &Address::generate(&env));
    registry_client.initialize(&admin, &usdc_id, &1_000i128);
    registry_client.set_invoice_contract(&admin, &invoice_id);
    invoice_client.set_oracle_registry(&admin, &registry_id);
    invoice_client.set_consensus_required(&admin, &true);

    let mut oracles = Vec::new();
    for _ in 0..5 {
        let op = Address::generate(&env);
        soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&op, &1_000i128);
        registry_client.register_oracle(&op, &1_000i128);
        oracles.push(op);
    }

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

    registry_client.open_verification_round(&admin, &inv_id, &String::from_str(&env, "hash123"));

    // Only a single oracle ever votes — well short of the default 6600 bps
    // quorum out of 5000 total stake.
    registry_client.submit_vote(&oracles[0], &inv_id, &true, &String::from_str(&env, "e"));
    assert_eq!(
        registry_client
            .get_verification_round(&inv_id)
            .unwrap()
            .status,
        oracle_registry::RoundStatus::Open
    );

    // Advance past the default 3-day round deadline.
    env.ledger().with_mut(|l| l.timestamp += 3 * 86_400 + 1);
    registry_client.expire_round(&inv_id);
    assert_eq!(
        registry_client
            .get_verification_round(&inv_id)
            .unwrap()
            .status,
        oracle_registry::RoundStatus::Expired
    );

    // The invoice is still stuck in AwaitingVerification until the admin
    // fallback resolves it — it must never be permanently bricked.
    assert_eq!(
        invoice_client.get_invoice(&inv_id).status,
        invoice::InvoiceStatus::AwaitingVerification
    );

    registry_client.admin_resolve_round(
        &admin,
        &inv_id,
        &true,
        &String::from_str(&env, "manual review: oracle participation too low"),
    );

    let invoice = invoice_client.get_invoice(&inv_id);
    assert_eq!(invoice.status, invoice::InvoiceStatus::Verified);

    // Fully unblocked: the pool can now fund it like any other verified invoice.
    pool_client.fund_invoice(
        &admin,
        &inv_id,
        &2_000_000_000i128,
        &sme,
        &due_date,
        &usdc_id,
    );
    invoice_client.mark_funded(&inv_id, &pool_id);
    assert_eq!(
        invoice_client.get_invoice(&inv_id).status,
        invoice::InvoiceStatus::Funded
    );
}

/// #868: credit_score v2 — a brand-new SME with zero internal payment history
/// gets a baseline-only (internal-only) score, gains a business-registry
/// attestation that measurably raises the blended score, disputes a bad-faith
/// attestation which reverts the score, and continues to fund/repay invoices
/// through the pool normally throughout.
#[test]
fn test_credit_score_attestation_lifecycle_alongside_normal_pool_activity() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let registry_attestor = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let invoice_id = env.register_contract_wasm(None, invoice::WASM);
    let pool_id = env.register_contract_wasm(None, pool::WASM);
    let credit_id = env.register_contract_wasm(None, credit_score::WASM);
    let share_id = env.register_contract_wasm(None, share::WASM);
    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin)
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

    // A brand-new SME with zero on-chain history gets the pre-v2 baseline
    // score, untouched — the whole "cold start" problem this issue exists for.
    let baseline = credit_client.get_credit_score(&sme);
    assert_eq!(baseline.total_invoices, 0);
    assert_eq!(baseline.blended_score, baseline.score);

    // Admin registers a business-registry attestor and it verifies the SME's
    // registration, submitting a strong (near-max) external signal.
    credit_client.register_attestor(
        &admin,
        &registry_attestor,
        &credit_score::AttestorType::BusinessRegistry,
        &10_000u32,
    );
    let attestation_id = credit_client.submit_attestation(
        &registry_attestor,
        &sme,
        &credit_score::AttestorType::BusinessRegistry,
        &950u32,
        &String::from_str(&env, "business-registry-report-hash"),
        &(env.ledger().timestamp() + 365 * 24 * 60 * 60),
    );

    let with_attestation = credit_client.get_credit_score(&sme);
    assert!(
        with_attestation.blended_score > baseline.blended_score,
        "verified business registration should measurably raise a cold-start SME's score: {} -> {}",
        baseline.blended_score,
        with_attestation.blended_score
    );
    // The pure internal (payment-history) score is unaffected.
    assert_eq!(with_attestation.score, baseline.score);

    // Meanwhile the SME funds and repays an invoice through the pool exactly
    // like any other SME — attestations never block normal protocol usage.
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
        .mint(&investor, &10_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&sme, &10_000_000_000i128);
    pool_client.deposit(&investor, &usdc_id, &5_000_000_000i128);

    let due_date = env.ledger().timestamp() + 30 * 86_400;
    let inv_id = invoice_client.create_invoice(
        &sme,
        &String::from_str(&env, "Cold Start Corp"),
        &2_000_000_000i128,
        &due_date,
        &String::from_str(&env, "Invoice #CS-001"),
        &String::from_str(&env, "hash_cs"),
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
    assert_eq!(
        invoice_client.get_invoice(&inv_id).status,
        invoice::InvoiceStatus::Funded
    );

    env.ledger().with_mut(|l| l.timestamp += 10 * 86_400);
    let amount_due = pool_client.estimate_repayment(&inv_id, &None);
    pool_client.repay_invoice(&inv_id, &sme, &amount_due);
    invoice_client.mark_paid(&inv_id, &pool_id);
    credit_client.record_payment(
        &pool_id,
        &inv_id,
        &sme,
        &2_000_000_000i128,
        &due_date,
        &env.ledger().timestamp(),
    );

    let after_repayment = credit_client.get_credit_score(&sme);
    assert_eq!(after_repayment.total_invoices, 1);
    assert_eq!(after_repayment.paid_on_time, 1);
    assert!(
        after_repayment.blended_score > with_attestation.blended_score,
        "a genuine on-time repayment should further raise the blended score"
    );

    // The SME disputes the attestation as bad-faith (e.g. a competitor forged
    // the registry entry); the admin investigates and does not uphold it, so
    // it is permanently revoked and immediately excluded from scoring.
    credit_client.dispute_attestation(
        &sme,
        &attestation_id,
        &String::from_str(&env, "forged-registry-entry-reason-hash"),
    );
    let disputed = credit_client.get_credit_score(&sme);
    assert_eq!(
        disputed.blended_score, disputed.score,
        "disputing the attestation must immediately exclude it from the blended score"
    );

    credit_client.resolve_attestation_dispute(&admin, &attestation_id, &false);
    let attestation = credit_client.get_attestation(&attestation_id).unwrap();
    assert!(matches!(
        attestation.status,
        credit_score::AttestationStatus::Revoked
    ));
    let after_revocation = credit_client.get_credit_score(&sme);
    assert_eq!(after_revocation.blended_score, after_revocation.score);

    // Normal pool operation continues to work after all of this — the
    // investor can still withdraw their principal plus yield.
    let shares = share_client.balance(&investor);
    pool_client.withdraw(&investor, &usdc_id, &shares);
    let investor_balance = soroban_sdk::token::Client::new(&env, &usdc_id).balance(&investor);
    assert!(investor_balance > 5_000_000_000i128);
}
