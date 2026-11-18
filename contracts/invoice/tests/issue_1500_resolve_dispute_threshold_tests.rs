#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    symbol_short, Address, Env, String, Symbol,
};

use invoice::{
    DisputeResolution, InvoiceContract, InvoiceContractClient, InvoiceError, InvoiceStatus,
};

const SECS_PER_DAY: u64 = 86400;
const DEFAULT_EXPIRATION_DURATION_SECS: u64 = SECS_PER_DAY * 30;
const DISPUTE_VALUE_THRESHOLD: Symbol = symbol_short!("disp_thr");

fn setup_with_arbitration(
    env: &Env,
) -> (
    InvoiceContractClient<'_>,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    let contract_id = env.register(InvoiceContract, ());
    let client = InvoiceContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let pool = Address::generate(env);
    let sme = Address::generate(env);
    let oracle = Address::generate(env);
    let arbitration = Address::generate(env);
    
    client.initialize(
        &admin,
        &pool,
        &i128::MAX,
        &DEFAULT_EXPIRATION_DURATION_SECS,
        &7u32,
    );
    client.set_oracle(&admin, &oracle);
    
    (client, admin, pool, sme, oracle, arbitration)
}

fn create_disputed_invoice(
    env: &Env,
    client: &InvoiceContractClient,
    sme: &Address,
    oracle: &Address,
    amount: i128,
) -> u64 {
    let due = env.ledger().timestamp() + SECS_PER_DAY * 30;
    let id = client.create_invoice(
        sme,
        &String::from_str(env, "Debtor Corp"),
        &amount,
        &due,
        &String::from_str(env, "Test invoice"),
        &String::from_str(env, "hash123"),
        &String::from_str(env, "https://example.com/meta"),
    );
    
    // Dispute the invoice via oracle rejection (approved = false)
    let _ = client.verify_invoice(
        &id,
        oracle,
        &false,
        &String::from_str(env, "Disputed by oracle"),
        &String::from_str(env, "hash123"),
    );
    
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Disputed);
    id
}

// ── Test 1: High-value dispute rejected by resolve_dispute (attack scenario) ──

#[test]
fn test_high_value_dispute_rejected_by_resolve_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, admin, pool, sme, oracle, arbitration) = setup_with_arbitration(&env);
    
    // Set threshold to 1M
    let threshold = 1_000_000i128;
    client.set_arbitration_contract(&admin, &arbitration);
    client.set_dispute_value_threshold(&admin, &threshold);
    
    // Create high-value dispute (>= threshold)
    let high_value_id = create_disputed_invoice(
        &env,
        &client,
        &sme,
        &oracle,
        threshold, // Exactly at threshold
    );
    
    // Attempt to resolve via oracle (should fail)
    let result = client.try_resolve_dispute(
        &high_value_id,
        &oracle,
        &DisputeResolution::InFavorOfSME,
    );
    
    // Assert ArbitrationRequired error
    assert_eq!(
        result.unwrap_err().unwrap(),
        InvoiceError::ArbitrationRequired.into()
    );
    
    // Assert invoice status unchanged
    assert_eq!(
        client.get_invoice(&high_value_id).status,
        InvoiceStatus::Disputed
    );
    
    // Vacuousness check: This test would fail if the threshold check is removed,
    // because resolve_dispute would succeed and change the invoice status.
}

// ── Test 2: Boundary - exactly at threshold is rejected ──

#[test]
fn test_exactly_at_threshold_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, admin, pool, sme, oracle, arbitration) = setup_with_arbitration(&env);
    
    let threshold = 500_000i128;
    client.set_arbitration_contract(&admin, &arbitration);
    client.set_dispute_value_threshold(&admin, &threshold);
    
    let id = create_disputed_invoice(
        &env,
        &client,
        &sme,
        &oracle,
        threshold, // Exactly at threshold
    );
    
    let result = client.try_resolve_dispute(&id, &oracle, &DisputeResolution::InFavorOfSME);
    
    assert_eq!(
        result.unwrap_err().unwrap(),
        InvoiceError::ArbitrationRequired.into()
    );
}

// ── Test 3: Boundary - one unit below threshold is accepted ──

#[test]
fn test_one_below_threshold_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, admin, pool, sme, oracle, arbitration) = setup_with_arbitration(&env);
    
    let threshold = 500_000i128;
    client.set_arbitration_contract(&admin, &arbitration);
    client.set_dispute_value_threshold(&admin, &threshold);
    
    let id = create_disputed_invoice(
        &env,
        &client,
        &sme,
        &oracle,
        threshold - 1, // One below threshold
    );
    
    // Should succeed
    client.resolve_dispute(&id, &oracle, &DisputeResolution::InFavorOfSME);
    
    // Verify resolution applied
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Verified);
}

// ── Test 4: Low-value oracle resolution still works ──

#[test]
fn test_low_value_oracle_resolution_works() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, admin, pool, sme, oracle, arbitration) = setup_with_arbitration(&env);
    
    let threshold = 1_000_000i128;
    client.set_arbitration_contract(&admin, &arbitration);
    client.set_dispute_value_threshold(&admin, &threshold);
    
    // Create low-value dispute
    let low_value_id = create_disputed_invoice(
        &env,
        &client,
        &sme,
        &oracle,
        100_000i128, // Well below threshold
    );
    
    // Oracle resolves
    client.resolve_dispute(
        &low_value_id,
        &oracle,
        &DisputeResolution::InFavorOfSME,
    );
    
    // Verify success
    assert_eq!(
        client.get_invoice(&low_value_id).status,
        InvoiceStatus::Verified
    );
}

// ── Test 5: Low-value admin resolution after window still works ──

#[test]
fn test_low_value_admin_resolution_after_window_works() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, admin, pool, sme, oracle, arbitration) = setup_with_arbitration(&env);
    
    let threshold = 1_000_000i128;
    client.set_arbitration_contract(&admin, &arbitration);
    client.set_dispute_value_threshold(&admin, &threshold);
    
    // Set dispute window to 30 days
    let window = SECS_PER_DAY * 30;
    client.set_dispute_window(&admin, &window);
    
    // Create low-value dispute
    let low_value_id = create_disputed_invoice(
        &env,
        &client,
        &sme,
        &oracle,
        100_000i128,
    );
    
    // Advance time past window
    env.ledger().with_mut(|l| {
        l.timestamp += window + 1;
    });
    
    // Admin resolves
    client.resolve_dispute(
        &low_value_id,
        &admin,
        &DisputeResolution::InFavorOfSME,
    );
    
    // Verify success
    assert_eq!(
        client.get_invoice(&low_value_id).status,
        InvoiceStatus::Verified
    );
}

// ── Test 6: Admin cannot bypass threshold even after window ──

#[test]
fn test_admin_cannot_bypass_threshold_after_window() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, admin, _pool, sme, oracle, arbitration) = setup_with_arbitration(&env);
    
    let threshold = 1_000_000i128;
    client.set_arbitration_contract(&admin, &arbitration);
    client.set_dispute_value_threshold(&admin, &threshold);
    
    let window = SECS_PER_DAY * 30;
    client.set_dispute_window(&admin, &window);
    
    // Create high-value dispute
    let high_value_id = create_disputed_invoice(
        &env,
        &client,
        &sme,
        &oracle,
        threshold,
    );
    
    // Advance time past window
    env.ledger().with_mut(|l| {
        l.timestamp += window + 1;
    });
    
    // Admin attempt should still fail due to threshold
    let result = client.try_resolve_dispute(
        &high_value_id,
        &admin,
        &DisputeResolution::InFavorOfSME,
    );
    
    assert_eq!(
        result.unwrap_err().unwrap(),
        InvoiceError::ArbitrationRequired.into()
    );
}

// ── Test 7: Threshold check only applies when arbitration configured ──

#[test]
fn test_threshold_check_only_when_arbitration_configured() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, admin, _pool, sme, oracle, _arbitration) = setup_with_arbitration(&env);
    
    // Set threshold but DON'T configure arbitration contract
    let threshold = 1_000_000i128;
    client.set_dispute_value_threshold(&admin, &threshold);
    
    // Create high-value dispute
    let high_value_id = create_disputed_invoice(
        &env,
        &client,
        &sme,
        &oracle,
        threshold,
    );
    
    // Should succeed because arbitration is not configured
    client.resolve_dispute(
        &high_value_id,
        &oracle,
        &DisputeResolution::InFavorOfSME,
    );
    
    assert_eq!(
        client.get_invoice(&high_value_id).status,
        InvoiceStatus::Verified
    );
}

// ── Test 8: Threshold check with arbitration configured and threshold unset ──

#[test]
fn test_unset_threshold_allows_all_disputes() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, admin, pool, sme, oracle, arbitration) = setup_with_arbitration(&env);
    
    // Configure arbitration but leave threshold unset (defaults to i128::MAX)
    client.set_arbitration_contract(&admin, &arbitration);
    
    // Create very high-value dispute
    let id = create_disputed_invoice(
        &env,
        &client,
        &sme,
        &oracle,
        1_000_000_000i128,
    );
    
    // Should succeed because threshold is i128::MAX (unset)
    client.resolve_dispute(&id, &oracle, &DisputeResolution::InFavorOfSME);
    
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Verified);
}

// ── Test 9: resolve_dispute rejects InFavorOfDebtor correctly for low-value ──

#[test]
fn test_low_value_resolve_in_favor_of_debtor() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, admin, pool, sme, oracle, arbitration) = setup_with_arbitration(&env);
    
    let threshold = 1_000_000i128;
    client.set_arbitration_contract(&admin, &arbitration);
    client.set_dispute_value_threshold(&admin, &threshold);
    
    // Create low-value dispute
    let id = create_disputed_invoice(
        &env,
        &client,
        &sme,
        &oracle,
        100_000i128,
    );
    
    // Resolve in favor of debtor
    client.resolve_dispute(&id, &oracle, &DisputeResolution::InFavorOfDebtor);
    
    // Should be Cancelled
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Cancelled);
}

// ── Test 10: High-value dispute must use resolve_default_dispute path ──

#[test]
fn test_high_value_uses_resolve_default_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, admin, pool, sme, oracle, arbitration) = setup_with_arbitration(&env);
    
    let threshold = 1_000_000i128;
    client.set_arbitration_contract(&admin, &arbitration);
    client.set_dispute_value_threshold(&admin, &threshold);
    
    // Create high-value dispute
    let id = create_disputed_invoice(
        &env,
        &client,
        &sme,
        &oracle,
        threshold,
    );
    
    // resolve_dispute should fail
    let result = client.try_resolve_dispute(&id, &oracle, &DisputeResolution::InFavorOfSME);
    assert!(result.is_err());
    
    // But resolve_default_dispute should work after stalled period
    env.ledger().with_mut(|l| {
        l.timestamp += SECS_PER_DAY * 91; // Past ARBITRATION_STALLED_OVERRIDE_SECS (90 days)
    });
    
    client.resolve_default_dispute(&id, &admin, &DisputeResolution::InFavorOfSME);
    
    // Should be Defaulted (InFavorOfSME in apply_dispute_resolution)
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Defaulted);
}
