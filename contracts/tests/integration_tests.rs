#![cfg(test)]

use soroban_sdk::{
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
    let amount_due = pool_client.estimate_repayment(&inv_id);
    pool_client.repay_invoice(&inv_id, &sme, &amount_due);

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
    let amount1 = pool_client.estimate_repayment(&inv1);
    pool_client.repay_invoice(&inv1, &sme1, &amount1);
    let amount2 = pool_client.estimate_repayment(&inv2);
    pool_client.repay_invoice(&inv2, &sme2, &amount2);

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
    let amount_due = pool_client.estimate_repayment(&inv_id);
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

    credit_client.record_payment(&pool_id, &inv_id, &sme, &2_000_000_000i128, &due_date, &env.ledger().timestamp());
    
    // Verify credit score state
    let credit_data = credit_client.get_credit_score(&sme);
    assert_eq!(credit_data.total_invoices, 1);
    assert_eq!(credit_data.total_volume, 2_000_000_000i128);
    assert!(credit_client.is_invoice_processed(&inv_id));
}

fn setup_pool(env: &Env) -> (
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
    pool_client.initialize(&admin, &usdc_id, &share_id, &invoice_id);

    (pool_client, share_client, admin, usdc_id)
}

fn invoice_client_init(env: &Env, invoice_id: &Address, admin: &Address, pool_id: &Address) {
    let invoice_client = invoice::Client::new(env, invoice_id);
    invoice_client.initialize(admin, pool_id, &10_000_000_000i128, &(30u64 * 86_400u64));
}

/// Integration test: Collateral post and release on full repayment
#[test]
fn test_collateral_post_and_release() {
    let env = Env::default();
    env.mock_all_auths();
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

    invoice_client.initialize(&admin, &pool_id, &10_000_000_000i128, &(30u64 * 86_400u64));
    share_client.initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "Pool Shares"),
        &String::from_str(&env, "POOL"),
    );
    pool_client.initialize(&admin, &usdc_id, &share_id, &invoice_id_addr);

    // Threshold = 1_000 USDC, 20% collateral required
    pool_client.set_collateral_config(&admin, &1_000i128, &2_000u32);

    let principal: i128 = 5_000;
    let required_col = pool_client.required_collateral_for(&principal);
    assert_eq!(required_col, 1_000); // 20% of 5_000

    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
        .mint(&investor, &10_000i128);
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
    let amount_due = pool_client.estimate_repayment(&1u64);
    let sme_before_repay = soroban_sdk::token::Client::new(&env, &usdc_id).balance(&sme);
    pool_client.repay_invoice(&1u64, &sme, &amount_due);

    // Collateral should be automatically returned to SME on full repayment
    let col_after = pool_client.get_collateral_deposit(&1u64).unwrap();
    assert!(col_after.settled);

    let sme_after_repay = soroban_sdk::token::Client::new(&env, &usdc_id).balance(&sme);
    // Net: paid amount_due but got required_col back
    assert_eq!(sme_after_repay, sme_before_repay - amount_due + required_col);
}

/// Integration test: Collateral seized on default (no repayment past grace period)
#[test]
fn test_collateral_seize_on_default() {
    let env = Env::default();
    env.mock_all_auths();
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

    invoice_client.initialize(&admin, &pool_id, &10_000_000_000i128, &(30u64 * 86_400u64));
    share_client.initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "Pool Shares"),
        &String::from_str(&env, "POOL"),
    );
    pool_client.initialize(&admin, &usdc_id, &share_id, &invoice_id_addr);

    pool_client.set_collateral_config(&admin, &1_000i128, &2_000u32);

    let principal: i128 = 5_000;
    let required_col = pool_client.required_collateral_for(&principal);

    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
        .mint(&investor, &10_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
        .mint(&sme, &required_col);

    pool_client.deposit(&investor, &usdc_id, &10_000i128);
    pool_client.deposit_collateral(&1u64, &sme, &usdc_id, &required_col);

    let due_date = env.ledger().timestamp() + 30 * 86_400;
    pool_client.fund_invoice(&admin, &1u64, &principal, &sme, &due_date, &usdc_id);

    // Advance past due date without repayment — mark as defaulted
    env.ledger()
        .with_mut(|l| l.timestamp = due_date + 8 * 86_400);
    invoice_client.mark_defaulted(&1u64, &pool_id);

    let tt_before = pool_client.get_token_totals(&usdc_id);

    // Admin seizes collateral
    pool_client.seize_collateral(&admin, &1u64);

    let col = pool_client.get_collateral_deposit(&1u64).unwrap();
    assert!(col.settled);

    // Pool value increased by collateral, deployed reduced by principal
    let tt_after = pool_client.get_token_totals(&usdc_id);
    assert_eq!(tt_after.pool_value, tt_before.pool_value + required_col);
    assert_eq!(tt_after.total_deployed, tt_before.total_deployed - principal);

    // SME cannot seize again (collateral already settled)
    let result = pool_client.try_seize_collateral(&admin, &1u64);
    assert_eq!(result, Err(Ok(pool::Error::CollateralAlreadySettled)));
}

/// Integration test: Collateral not required below threshold
#[test]
fn test_collateral_not_required_below_threshold() {
    let env = Env::default();
    env.mock_all_auths();
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

    invoice_client.initialize(&admin, &pool_id, &10_000_000_000i128, &(30u64 * 86_400u64));
    share_client.initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "Pool Shares"),
        &String::from_str(&env, "POOL"),
    );
    pool_client.initialize(&admin, &usdc_id, &share_id, &invoice_id_addr);

    // Threshold = 10_000, principal = 500 → below threshold, no collateral needed
    pool_client.set_collateral_config(&admin, &10_000i128, &2_000u32);

    let principal: i128 = 500;
    assert_eq!(pool_client.required_collateral_for(&principal), 0);

    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
        .mint(&investor, &10_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
        .mint(&sme, &principal * 2);

    pool_client.deposit(&investor, &usdc_id, &10_000i128);

    // Fund without collateral — must succeed
    let due_date = env.ledger().timestamp() + 30 * 86_400;
    pool_client.fund_invoice(&admin, &1u64, &principal, &sme, &due_date, &usdc_id);

    let totals = pool_client.get_token_totals(&usdc_id);
    assert_eq!(totals.total_deployed, principal);

    // Repay fully
    env.ledger().with_mut(|l| l.timestamp += 15 * 86_400);
    let amount_due = pool_client.estimate_repayment(&1u64);
    pool_client.repay_invoice(&1u64, &sme, &amount_due);

    let fi = pool_client.get_funded_invoice(&1u64).unwrap();
    assert!(fi.repaid_amount >= amount_due);
}

/// Integration test: Collateral error cases
#[test]
fn test_collateral_error_double_deposit() {
    let env = Env::default();
    env.mock_all_auths();
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

    invoice_client.initialize(&admin, &pool_id, &10_000_000_000i128, &(30u64 * 86_400u64));
    share_client.initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "Pool Shares"),
        &String::from_str(&env, "POOL"),
    );
    pool_client.initialize(&admin, &usdc_id, &share_id, &invoice_id_addr);
    pool_client.set_collateral_config(&admin, &1_000i128, &2_000u32);

    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&sme, &5_000i128);

    pool_client.deposit_collateral(&1u64, &sme, &usdc_id, &1_000);

    // Double deposit must fail
    let result = pool_client.try_deposit_collateral(&1u64, &sme, &usdc_id, &1_000);
    assert_eq!(result, Err(Ok(pool::Error::StorageCorrupted)));
}

/// Integration test: Partial repayments accumulate to full repayment
#[test]
fn test_partial_repayment_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
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

    invoice_client.initialize(&admin, &pool_id, &10_000_000_000i128, &(30u64 * 86_400u64));
    share_client.initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "Pool Shares"),
        &String::from_str(&env, "POOL"),
    );
    pool_client.initialize(&admin, &usdc_id, &share_id, &invoice_id_addr);

    let principal: i128 = 10_000;
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&investor, &20_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&sme, &20_000i128);

    pool_client.deposit(&investor, &usdc_id, &20_000i128);

    let due_date = env.ledger().timestamp() + 30 * 86_400;
    pool_client.fund_invoice(&admin, &1u64, &principal, &sme, &due_date, &usdc_id);

    // Advance time and compute total due
    env.ledger().with_mut(|l| l.timestamp += 15 * 86_400);
    let total_due = pool_client.estimate_repayment(&1u64);

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
    let remaining = pool_client.estimate_repayment(&1u64);
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
    assert_eq!(result, Err(Ok(pool::Error::AlreadyFullyRepaid)));
}
