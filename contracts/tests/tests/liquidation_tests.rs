#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, EnvTestConfig, Ledger},
    Address, Env, String,
};

mod invoice {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/invoice.wasm");
}

mod pool {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/pool.wasm");
}

mod credit_score {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/credit_score.wasm");
}

mod share {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/share.wasm");
}

mod auction {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/auction.wasm");
}

mod insurance {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/insurance.wasm");
}

fn test_env() -> Env {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.cost_estimate().budget().reset_unlimited();
    env
}

fn metadata_url(env: &Env) -> String {
    String::from_str(env, "https://example.com/meta")
}

#[test]
fn test_full_default_pipeline_liquidate_insurance_auction() {
    let env = test_env();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let admin = Address::generate(&env);
    let investor = Address::generate(&env);
    let sme = Address::generate(&env);
    let keeper = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let invoice_id = env.register_contract_wasm(None, invoice::WASM);
    let pool_id = env.register_contract_wasm(None, pool::WASM);
    let credit_id = env.register_contract_wasm(None, credit_score::WASM);
    let share_id = env.register_contract_wasm(None, share::WASM);
    let auction_id = env.register_contract_wasm(None, auction::WASM);
    let insurance_id = env.register_contract_wasm(None, insurance::WASM);
    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let invoice_client = invoice::Client::new(&env, &invoice_id);
    let pool_client = pool::Client::new(&env, &pool_id);
    let credit_client = credit_score::Client::new(&env, &credit_id);
    let share_client = share::Client::new(&env, &share_id);
    let auction_client = auction::Client::new(&env, &auction_id);
    let insurance_client = insurance::Client::new(&env, &insurance_id);

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
    pool_client.initialize(&admin, &usdc_id, &share_id, &invoice_id);
    pool_client.set_max_investor_concentration(&admin, &10_000u32);
    credit_client.initialize(&admin, &invoice_id, &pool_id);
    
    auction_client.initialize(&admin, &pool_id, &invoice_id, &7u32);
    insurance_client.initialize(&admin, &pool_id, &invoice_id);

    pool_client.set_insurance_contract(&admin, &insurance_id);
    pool_client.set_risk_contract(&admin, &auction_id);
    invoice_client.add_keeper(&admin, &keeper);

    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
        .mint(&investor, &10_000_000_000i128);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
        .mint(&sme, &10_000_000_000i128);

    pool_client.deposit(&investor, &usdc_id, &5_000_000_000i128, &None);

    let initial_totals = pool_client.get_token_totals(&usdc_id);
    let initial_nav = initial_totals.pool_value;
    assert_eq!(initial_nav, 5_000_000_000i128);

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

    invoice_client.approve_invoice(&keeper, &inv_id);

    let collateral_amount = 400_000_000i128;
    pool_client.fund_invoice(&sme, &inv_id, &usdc_id, &collateral_amount);

    let funded_totals = pool_client.get_token_totals(&usdc_id);
    assert_eq!(funded_totals.total_deployed, 2_000_000_000i128);

    let initial_score = credit_client.get_score(&String::from_str(&env, "ACME Corp"));

    env.ledger().with_mut(|l| l.timestamp = due_date + 86_400 + 1);

    let liquidation_status_before = pool_client.get_liquidation_status(&inv_id);
    assert!(!liquidation_status_before.settled);

    pool_client.liquidate_invoice(&keeper, &inv_id);

    let liquidation_status_after = pool_client.get_liquidation_status(&inv_id);
    assert!(liquidation_status_after.settled);

    let nav_after_liquidation = pool_client.get_token_totals(&usdc_id).pool_value;

    let claim_id = insurance_client.file_claim(&keeper, &inv_id, &usdc_id);

    let claim_status = insurance_client.get_claim(&claim_id);
    assert_eq!(claim_status.invoice_id, inv_id);

    let reserve_before = insurance_client.get_reserve_balance(&usdc_id);

    insurance_client.process_claim(&admin, &claim_id, &true);

    let reserve_after = insurance_client.get_reserve_balance(&usdc_id);
    assert!(reserve_after < reserve_before);

    let nav_after_insurance = pool_client.get_token_totals(&usdc_id).pool_value;

    let auction_params = auction::AuctionParams {
        min_bid: 100_000_000i128,
        duration_secs: 86_400u64,
        liquidation_bps: 8_000u32,
    };
    auction_client.create_auction(&keeper, &inv_id, &usdc_id, &auction_params);

    let bidder = Address::generate(&env);
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
        .mint(&bidder, &500_000_000i128);

    auction_client.place_bid(&bidder, &inv_id, &300_000_000i128);

    env.ledger().with_mut(|l| l.timestamp += 86_401);

    auction_client.finalize_auction(&keeper, &inv_id);

    let nav_after_auction = pool_client.get_token_totals(&usdc_id).pool_value;

    let final_score = credit_client.get_score(&String::from_str(&env, "ACME Corp"));

    assert!(nav_after_liquidation <= initial_nav);
    assert!(nav_after_insurance >= nav_after_liquidation || reserve_before == 0);
    assert!(nav_after_auction >= nav_after_insurance);
    assert!(final_score < initial_score);
}
