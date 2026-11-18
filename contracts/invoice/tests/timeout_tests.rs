#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

use invoice::{
    InvoiceContract, InvoiceContractClient, InvoiceError, InvoiceStatus, VERIFICATION_TIMEOUT_SECS,
};

const SECS_PER_DAY: u64 = 86400;
const DEFAULT_EXPIRATION_DURATION_SECS: u64 = SECS_PER_DAY * 30;

fn setup_with_oracle(
    env: &Env,
) -> (
    InvoiceContractClient<'_>,
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
    client.initialize(
        &admin,
        &pool,
        &i128::MAX,
        &DEFAULT_EXPIRATION_DURATION_SECS,
        &90u32,
    );
    client.set_oracle(&admin, &oracle);
    (client, admin, pool, sme, oracle)
}

fn create_awaiting_verification_invoice(
    env: &Env,
    client: &InvoiceContractClient,
    sme: &Address,
) -> u64 {
    let due = env.ledger().timestamp() + SECS_PER_DAY * 30;
    client.create_invoice(
        sme,
        &String::from_str(env, "Debtor Corp"),
        &1_000_000i128,
        &due,
        &String::from_str(env, "Test invoice"),
        &String::from_str(env, "hash123"),
        &String::from_str(env, "https://example.com/meta"),
    )
}

// ── Test 1: timeout_verification succeeds after deadline ─────────────────────

#[test]
fn test_timeout_verification_succeeds_after_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, _pool, sme, _oracle) = setup_with_oracle(&env);
    let id = create_awaiting_verification_invoice(&env, &client, &sme);

    // Verify invoice is in AwaitingVerification status
    let invoice = client.get_invoice(&id);
    assert_eq!(invoice.status, InvoiceStatus::AwaitingVerification);
    assert_eq!(
        invoice.verification_deadline,
        1_000_000 + VERIFICATION_TIMEOUT_SECS
    );

    // Advance time past verification_deadline (72 hours + 1 second)
    env.ledger()
        .with_mut(|l| l.timestamp = 1_000_000 + VERIFICATION_TIMEOUT_SECS + 1);

    // Call timeout_verification as SME (invoice owner)
    client.timeout_verification(&sme, &id);

    // Assert invoice status is now Cancelled
    let invoice = client.get_invoice(&id);
    assert_eq!(invoice.status, InvoiceStatus::Cancelled);
}

// ── Test 2: timeout_verification fails before deadline ───────────────────────

#[test]
fn test_timeout_verification_fails_before_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, _pool, sme, _oracle) = setup_with_oracle(&env);
    let id = create_awaiting_verification_invoice(&env, &client, &sme);

    // Do not advance time — we're still before the deadline
    // Try to call timeout_verification before deadline
    let result = client.try_timeout_verification(&sme, &id);

    // Assert error is VerificationDeadlineNotPassed
    assert_eq!(
        result.unwrap_err().unwrap(),
        InvoiceError::VerificationDeadlineNotPassed.into()
    );

    // Assert invoice status is still AwaitingVerification
    let invoice = client.get_invoice(&id);
    assert_eq!(invoice.status, InvoiceStatus::AwaitingVerification);
}

// ── Test 3: timeout_verification fails on non-AwaitingVerification invoice ───

#[test]
fn test_timeout_verification_fails_on_verified_invoice() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, _pool, sme, oracle) = setup_with_oracle(&env);
    let id = create_awaiting_verification_invoice(&env, &client, &sme);

    // Verify the invoice (move it to Verified status)
    client.verify_invoice(
        &id,
        &oracle,
        &true,
        &String::from_str(&env, ""),
        &String::from_str(&env, "hash123"),
    );

    // Assert invoice is now Verified
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Verified);

    // Advance time past deadline
    env.ledger()
        .with_mut(|l| l.timestamp = 1_000_000 + VERIFICATION_TIMEOUT_SECS + 1);

    // Try to call timeout_verification on a Verified invoice
    let result = client.try_timeout_verification(&sme, &id);

    // Assert error is InvalidStatusTransition
    assert_eq!(
        result.unwrap_err().unwrap(),
        InvoiceError::InvalidStatusTransition.into()
    );

    // Assert invoice status is still Verified (unchanged)
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Verified);
}

// ── Test 4: timeout_verification by authorized SME ───────────────────────────

#[test]
fn test_timeout_verification_by_sme() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, _pool, sme, _oracle) = setup_with_oracle(&env);
    let id = create_awaiting_verification_invoice(&env, &client, &sme);

    // Advance time past deadline
    env.ledger()
        .with_mut(|l| l.timestamp = 1_000_000 + VERIFICATION_TIMEOUT_SECS + 1);

    // Call as SME (invoice owner)
    client.timeout_verification(&sme, &id);

    // Assert success
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Cancelled);
}

// ── Test 5: timeout_verification by admin ────────────────────────────────────

#[test]
fn test_timeout_verification_by_admin() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, admin, _pool, sme, _oracle) = setup_with_oracle(&env);
    let id = create_awaiting_verification_invoice(&env, &client, &sme);

    // Advance time past deadline
    env.ledger()
        .with_mut(|l| l.timestamp = 1_000_000 + VERIFICATION_TIMEOUT_SECS + 1);

    // Call as admin
    client.timeout_verification(&admin, &id);

    // Assert success
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Cancelled);
}

// ── Test 6: timeout_verification by unauthorized caller ──────────────────────

#[test]
fn test_timeout_verification_by_unauthorized_caller() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, _pool, sme, _oracle) = setup_with_oracle(&env);
    let id = create_awaiting_verification_invoice(&env, &client, &sme);

    // Advance time past deadline
    env.ledger()
        .with_mut(|l| l.timestamp = 1_000_000 + VERIFICATION_TIMEOUT_SECS + 1);

    // Try to call from an unauthorized address (not owner, not admin)
    let unauthorized = Address::generate(&env);
    let result = client.try_timeout_verification(&unauthorized, &id);

    // Assert error is Unauthorized
    assert_eq!(
        result.unwrap_err().unwrap(),
        InvoiceError::Unauthorized.into()
    );

    // Assert invoice status is still AwaitingVerification (unchanged)
    assert_eq!(
        client.get_invoice(&id).status,
        InvoiceStatus::AwaitingVerification
    );
}

// ── Test 7: verification_deadline is set correctly on creation ───────────────

#[test]
fn test_verification_deadline_set_on_creation() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, _pool, sme, _oracle) = setup_with_oracle(&env);
    let id = create_awaiting_verification_invoice(&env, &client, &sme);

    // Read the invoice
    let invoice = client.get_invoice(&id);

    // Assert verification_deadline is created_at + 72 hours
    let expected_deadline = invoice.created_at + VERIFICATION_TIMEOUT_SECS;
    assert_eq!(invoice.verification_deadline, expected_deadline);

    // Assert the deadline is greater than current time
    assert!(invoice.verification_deadline > env.ledger().timestamp());
}

// ── Test 8: timeout_verification does not affect SME outstanding ────────────

#[test]
fn test_timeout_verification_does_not_affect_sme_outstanding() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, _pool, sme, _oracle) = setup_with_oracle(&env);

    // Check initial SME outstanding (should be 0)
    let initial_outstanding = client.get_sme_outstanding(&sme);
    assert_eq!(initial_outstanding, 0);

    let _amount = 1_000_000i128;
    let id = create_awaiting_verification_invoice(&env, &client, &sme);

    // SME outstanding should still be 0 (not incremented until Funded)
    let outstanding_after_create = client.get_sme_outstanding(&sme);
    assert_eq!(outstanding_after_create, 0);

    // Advance time past deadline
    env.ledger()
        .with_mut(|l| l.timestamp = 1_000_000 + VERIFICATION_TIMEOUT_SECS + 1);

    // Call timeout_verification
    client.timeout_verification(&sme, &id);

    // SME outstanding should remain 0 (not decreased because it was never funded)
    let outstanding_after_timeout = client.get_sme_outstanding(&sme);
    assert_eq!(outstanding_after_timeout, 0);

    // Verify invoice is Cancelled
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Cancelled);
}

// ── Test 9: timeout_verification updates storage stats ───────────────────────

#[test]
fn test_timeout_verification_updates_storage_stats() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, _pool, sme, _oracle) = setup_with_oracle(&env);
    let id = create_awaiting_verification_invoice(&env, &client, &sme);

    // Check storage stats before timeout
    let stats_before = client.get_storage_stats();
    let active_before = stats_before.active_invoices;

    // Advance time past deadline
    env.ledger()
        .with_mut(|l| l.timestamp = 1_000_000 + VERIFICATION_TIMEOUT_SECS + 1);

    // Call timeout_verification
    client.timeout_verification(&sme, &id);

    // Check storage stats after timeout
    let stats_after = client.get_storage_stats();
    assert_eq!(stats_after.active_invoices, active_before - 1);
}
