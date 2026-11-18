#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

use invoice::{InvoiceContract, InvoiceContractClient, InvoiceError, InvoiceStatus};

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

// ── Test 1: Primary oracle can still verify ──────────────────────────────────

#[test]
fn test_primary_oracle_can_verify() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, _pool, sme, oracle) = setup_with_oracle(&env);
    let id = create_awaiting_verification_invoice(&env, &client, &sme);

    // Verify invoice is in AwaitingVerification status
    assert_eq!(
        client.get_invoice(&id).status,
        InvoiceStatus::AwaitingVerification
    );

    // Primary oracle verifies the invoice
    client.verify_invoice(
        &id,
        &oracle,
        &true,
        &String::from_str(&env, ""),
        &String::from_str(&env, "hash123"),
    );

    // Assert invoice is now Verified
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Verified);
}

// ── Test 2: Secondary oracle can verify when configured ──────────────────────

#[test]
fn test_secondary_oracle_can_verify_when_configured() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, admin, _pool, sme, _oracle) = setup_with_oracle(&env);

    // Set a secondary oracle
    let secondary_oracle = Address::generate(&env);
    client.set_secondary_oracle(&admin, &Some(secondary_oracle.clone()));

    let id = create_awaiting_verification_invoice(&env, &client, &sme);

    // Verify invoice is in AwaitingVerification status
    assert_eq!(
        client.get_invoice(&id).status,
        InvoiceStatus::AwaitingVerification
    );

    // Secondary oracle verifies the invoice
    client.verify_invoice(
        &id,
        &secondary_oracle,
        &true,
        &String::from_str(&env, ""),
        &String::from_str(&env, "hash123"),
    );

    // Assert invoice is now Verified
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Verified);
}

// ── Test 3: Unknown address cannot verify ────────────────────────────────────

#[test]
#[should_panic(expected = "unauthorized oracle")]
fn test_unknown_address_cannot_verify() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, _pool, sme, _oracle) = setup_with_oracle(&env);
    let id = create_awaiting_verification_invoice(&env, &client, &sme);

    // Try to verify with an unauthorized address
    let unauthorized = Address::generate(&env);
    client.verify_invoice(
        &id,
        &unauthorized,
        &true,
        &String::from_str(&env, ""),
        &String::from_str(&env, "hash123"),
    );
}

// ── Test 4: No secondary oracle configured — unknown address still fails ─────

#[test]
#[should_panic(expected = "unauthorized oracle")]
fn test_no_secondary_oracle_unknown_address_fails() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, _pool, sme, _oracle) = setup_with_oracle(&env);
    // Do NOT set a secondary oracle
    let id = create_awaiting_verification_invoice(&env, &client, &sme);

    // Try to verify with an address that is neither primary nor secondary
    let random = Address::generate(&env);
    client.verify_invoice(
        &id,
        &random,
        &true,
        &String::from_str(&env, ""),
        &String::from_str(&env, "hash123"),
    );
}

// ── Test 5: set_secondary_oracle restricted to admin ─────────────────────────

#[test]
fn test_set_secondary_oracle_restricted_to_admin() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, _pool, _sme, _oracle) = setup_with_oracle(&env);

    // Try to call set_secondary_oracle from a non-admin address
    let non_admin = Address::generate(&env);
    let secondary_oracle = Address::generate(&env);
    let result = client.try_set_secondary_oracle(&non_admin, &Some(secondary_oracle));

    // Assert error is Unauthorized
    assert_eq!(
        result.unwrap_err().unwrap(),
        InvoiceError::Unauthorized.into()
    );
}

// ── Test 6: set_secondary_oracle allows removal ──────────────────────────────

#[test]
fn test_set_secondary_oracle_allows_removal() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, admin, _pool, sme, _oracle) = setup_with_oracle(&env);

    // Set a secondary oracle
    let secondary_oracle = Address::generate(&env);
    client.set_secondary_oracle(&admin, &Some(secondary_oracle.clone()));

    // Verify secondary oracle can verify
    let id1 = create_awaiting_verification_invoice(&env, &client, &sme);
    client.verify_invoice(
        &id1,
        &secondary_oracle,
        &true,
        &String::from_str(&env, ""),
        &String::from_str(&env, "hash123"),
    );
    assert_eq!(client.get_invoice(&id1).status, InvoiceStatus::Verified);

    // Now remove the secondary oracle
    client.set_secondary_oracle(&admin, &None);

    // Create a new invoice
    let id2 = create_awaiting_verification_invoice(&env, &client, &sme);

    // Try to verify with the removed secondary oracle — should fail
    let result = client.try_verify_invoice(
        &id2,
        &secondary_oracle,
        &true,
        &String::from_str(&env, ""),
        &String::from_str(&env, "hash123"),
    );

    // Assert that verification fails (unauthorized oracle)
    assert!(result.is_err());

    // Verify invoice status is still AwaitingVerification
    assert_eq!(
        client.get_invoice(&id2).status,
        InvoiceStatus::AwaitingVerification
    );
}

// ── Test 7: Primary and secondary oracles can both verify different invoices ─

#[test]
fn test_primary_and_secondary_can_both_verify() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, admin, _pool, sme, primary_oracle) = setup_with_oracle(&env);

    // Set a secondary oracle
    let secondary_oracle = Address::generate(&env);
    client.set_secondary_oracle(&admin, &Some(secondary_oracle.clone()));

    // Create two invoices
    let id1 = create_awaiting_verification_invoice(&env, &client, &sme);
    let id2 = create_awaiting_verification_invoice(&env, &client, &sme);

    // Primary oracle verifies invoice 1
    client.verify_invoice(
        &id1,
        &primary_oracle,
        &true,
        &String::from_str(&env, ""),
        &String::from_str(&env, "hash123"),
    );
    assert_eq!(client.get_invoice(&id1).status, InvoiceStatus::Verified);

    // Secondary oracle verifies invoice 2
    client.verify_invoice(
        &id2,
        &secondary_oracle,
        &true,
        &String::from_str(&env, ""),
        &String::from_str(&env, "hash123"),
    );
    assert_eq!(client.get_invoice(&id2).status, InvoiceStatus::Verified);
}

// ── Test 8: set_secondary_oracle succeeds and can be retrieved ──────────────

#[test]
fn test_set_secondary_oracle_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, admin, _pool, _sme, _oracle) = setup_with_oracle(&env);

    // Set a secondary oracle
    let secondary_oracle = Address::generate(&env);
    client.set_secondary_oracle(&admin, &Some(secondary_oracle.clone()));

    // Verify the secondary oracle was set by attempting to verify an invoice with it
    let sme = Address::generate(&env);
    let id = create_awaiting_verification_invoice(&env, &client, &sme);

    // Secondary oracle can now verify
    client.verify_invoice(
        &id,
        &secondary_oracle,
        &true,
        &String::from_str(&env, ""),
        &String::from_str(&env, "hash123"),
    );

    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Verified);
}

// ── Test 9: Secondary oracle can dispute invoices ────────────────────────────

#[test]
fn test_secondary_oracle_can_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, admin, _pool, sme, _oracle) = setup_with_oracle(&env);

    // Set a secondary oracle
    let secondary_oracle = Address::generate(&env);
    client.set_secondary_oracle(&admin, &Some(secondary_oracle.clone()));

    let id = create_awaiting_verification_invoice(&env, &client, &sme);

    // Secondary oracle disputes the invoice
    client.verify_invoice(
        &id,
        &secondary_oracle,
        &false, // approved = false
        &String::from_str(&env, "Document mismatch"),
        &String::from_str(&env, "hash123"),
    );

    // Assert invoice is now Disputed
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Disputed);
}

// ── #861: N-of-M staked oracle consensus network ─────────────────────────────
//
// These tests exercise `consensus_verify` and the `require_consensus_verification`
// gate directly against the invoice contract, using a plain `Address::generate`
// as a stand-in "registry" (the real stake-weighted quorum math lives in the
// separate `oracle_registry` crate and is tested there — here we only care
// about the invoice contract's side of the contract: who is allowed to call
// `consensus_verify`, and that the legacy path can be locked out).

#[test]
fn test_consensus_verify_rejects_unregistered_caller() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, admin, _pool, sme, _oracle) = setup_with_oracle(&env);
    let registry = Address::generate(&env);
    client.set_oracle_registry(&admin, &registry);

    let id = create_awaiting_verification_invoice(&env, &client, &sme);
    let impostor = Address::generate(&env);
    let result = client.try_consensus_verify(
        &id,
        &impostor,
        &true,
        &String::from_str(&env, "consensus approved"),
        &String::from_str(&env, "hash123"),
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        InvoiceError::UnauthorizedRegistry.into()
    );
    assert_eq!(
        client.get_invoice(&id).status,
        InvoiceStatus::AwaitingVerification
    );
}

#[test]
fn test_consensus_verify_without_registry_configured_returns_typed_error() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, _pool, sme, _oracle) = setup_with_oracle(&env);
    let id = create_awaiting_verification_invoice(&env, &client, &sme);
    let some_caller = Address::generate(&env);
    let result = client.try_consensus_verify(
        &id,
        &some_caller,
        &true,
        &String::from_str(&env, "consensus approved"),
        &String::from_str(&env, "hash123"),
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        InvoiceError::OracleRegistryNotConfigured.into()
    );
}

#[test]
fn test_consensus_verify_approves_and_rejects_like_verify_invoice() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, admin, _pool, sme, _oracle) = setup_with_oracle(&env);
    let registry = Address::generate(&env);
    client.set_oracle_registry(&admin, &registry);

    let id1 = create_awaiting_verification_invoice(&env, &client, &sme);
    client.consensus_verify(
        &id1,
        &registry,
        &true,
        &String::from_str(&env, "consensus approved"),
        &String::from_str(&env, "hash123"),
    );
    assert_eq!(client.get_invoice(&id1).status, InvoiceStatus::Verified);

    let id2 = create_awaiting_verification_invoice(&env, &client, &sme);
    client.consensus_verify(
        &id2,
        &registry,
        &false,
        &String::from_str(&env, "consensus rejected"),
        &String::from_str(&env, "hash123"),
    );
    assert_eq!(client.get_invoice(&id2).status, InvoiceStatus::Disputed);
}

#[test]
fn test_consensus_verify_hash_mismatch_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, admin, _pool, sme, _oracle) = setup_with_oracle(&env);
    let registry = Address::generate(&env);
    client.set_oracle_registry(&admin, &registry);
    let id = create_awaiting_verification_invoice(&env, &client, &sme);

    let result = client.try_consensus_verify(
        &id,
        &registry,
        &true,
        &String::from_str(&env, "consensus approved"),
        &String::from_str(&env, "wrong-hash"),
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        InvoiceError::HashMismatch.into()
    );
}

#[test]
fn test_require_consensus_verification_blocks_legacy_verify_invoice() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, admin, _pool, sme, oracle) = setup_with_oracle(&env);
    let registry = Address::generate(&env);
    client.set_oracle_registry(&admin, &registry);

    assert!(!client.require_consensus_verification());
    client.set_consensus_required(&admin, &true);
    assert!(client.require_consensus_verification());

    let id = create_awaiting_verification_invoice(&env, &client, &sme);
    let result = client.try_verify_invoice(
        &id,
        &oracle,
        &true,
        &String::from_str(&env, ""),
        &String::from_str(&env, "hash123"),
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        InvoiceError::ConsensusVerificationRequired.into()
    );

    // The registry path still works once the legacy path is locked out.
    client.consensus_verify(
        &id,
        &registry,
        &true,
        &String::from_str(&env, "consensus approved"),
        &String::from_str(&env, "hash123"),
    );
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Verified);
}

#[test]
fn test_set_oracle_registry_restricted_to_admin() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, _pool, _sme, _oracle) = setup_with_oracle(&env);
    let non_admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let result = client.try_set_oracle_registry(&non_admin, &registry);
    assert_eq!(
        result.unwrap_err().unwrap(),
        InvoiceError::Unauthorized.into()
    );
}

#[test]
fn test_set_consensus_required_restricted_to_admin() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, _pool, _sme, _oracle) = setup_with_oracle(&env);
    let non_admin = Address::generate(&env);
    let result = client.try_set_consensus_required(&non_admin, &true);
    assert_eq!(
        result.unwrap_err().unwrap(),
        InvoiceError::Unauthorized.into()
    );
}
