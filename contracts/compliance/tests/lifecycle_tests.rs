#![cfg(test)]

use compliance::{
    ComplianceContract, ComplianceContractClient, ComplianceError, ComplianceStatus, RiskTier,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

fn setup(env: &Env) -> (ComplianceContractClient<'_>, Address) {
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);
    let contract_id = env.register(ComplianceContract, ());
    let client = ComplianceContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    // Zero timelock so tests can register screeners immediately.
    client.set_screener_timelock(&admin, &0u64);
    (client, admin)
}

#[test]
fn test_is_cleared_false_when_unscreened() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let addr = Address::generate(&env);
    assert!(!client.is_cleared(&addr));
    assert_eq!(
        client.get_effective_status(&addr),
        ComplianceStatus::Unscreened
    );
}

#[test]
fn test_cleared_address_is_cleared_until_expiry() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let target = Address::generate(&env);
    let expires = 1_000_000u64 + 86_400;

    client.submit_screening_result(
        &admin,
        &target,
        &ComplianceStatus::Cleared,
        &0u32,
        &RiskTier::Low,
        &expires,
        &String::from_str(&env, "notes"),
    );

    assert!(client.is_cleared(&target));
    assert_eq!(
        client.get_effective_status(&target),
        ComplianceStatus::Cleared
    );

    // Advance past expires_at — lazy expiry, no status-change tx required.
    env.ledger().with_mut(|l| l.timestamp = expires + 1);
    assert!(!client.is_cleared(&target));
    assert_eq!(
        client.get_effective_status(&target),
        ComplianceStatus::Unscreened
    );
}

#[test]
fn test_request_review_interrupts_cleared() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let target = Address::generate(&env);
    let monitor = Address::generate(&env);
    let expires = 1_000_000u64 + 86_400 * 180;

    client.submit_screening_result(
        &admin,
        &target,
        &ComplianceStatus::Cleared,
        &0u32,
        &RiskTier::Low,
        &expires,
        &String::from_str(&env, "ok"),
    );
    assert!(client.is_cleared(&target));

    client.request_review(
        &monitor,
        &target,
        &String::from_str(&env, "structuring_pattern"),
    );

    assert!(!client.is_cleared(&target));
    assert_eq!(
        client.get_effective_status(&target),
        ComplianceStatus::PendingReview
    );
    let pending = client.list_pending_review();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending.get(0).unwrap(), target);
}

#[test]
fn test_screening_history_append_only_multi_step() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let target = Address::generate(&env);
    let interval = client.get_rescreening_interval();

    // cleared → flagged → re-cleared. Each step is a distinct rescreening of
    // the same subject by the same caller, so time must advance past the
    // rescreening cooldown between submissions (see
    // `test_rescreening_interval_enforced` in screener_tests.rs).
    let now = env.ledger().timestamp();
    client.submit_screening_result(
        &admin,
        &target,
        &ComplianceStatus::Cleared,
        &0u32,
        &RiskTier::Low,
        &(now + 86_400 * 30),
        &String::from_str(&env, "a"),
    );

    env.ledger().with_mut(|l| l.timestamp += interval + 1);
    client.submit_screening_result(
        &admin,
        &target,
        &ComplianceStatus::Flagged,
        &1001u32,
        &RiskTier::High,
        &0u64,
        &String::from_str(&env, "b"),
    );

    env.ledger().with_mut(|l| l.timestamp += interval + 1);
    let now = env.ledger().timestamp();
    client.submit_screening_result(
        &admin,
        &target,
        &ComplianceStatus::Cleared,
        &0u32,
        &RiskTier::Medium,
        &(now + 86_400 * 30),
        &String::from_str(&env, "c"),
    );

    let history = client.get_screening_history(&target);
    assert_eq!(history.len(), 3);
    assert_eq!(history.get(0).unwrap().status, ComplianceStatus::Cleared);
    assert_eq!(history.get(1).unwrap().status, ComplianceStatus::Flagged);
    assert_eq!(history.get(1).unwrap().reason_code, 1001u32);
    assert_eq!(history.get(2).unwrap().status, ComplianceStatus::Cleared);
    assert!(client.is_cleared(&target));
}

#[test]
fn test_blocked_not_cleared() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let target = Address::generate(&env);

    client.submit_screening_result(
        &admin,
        &target,
        &ComplianceStatus::Blocked,
        &9001u32,
        &RiskTier::High,
        &0u64,
        &String::from_str(&env, "ofac"),
    );

    assert!(!client.is_cleared(&target));
    assert_eq!(
        client.get_effective_status(&target),
        ComplianceStatus::Blocked
    );
    let flagged = client.list_flagged();
    assert_eq!(flagged.len(), 1);
}

#[test]
fn test_submit_unscreened_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let target = Address::generate(&env);

    let result = client.try_submit_screening_result(
        &admin,
        &target,
        &ComplianceStatus::Unscreened,
        &0u32,
        &RiskTier::Low,
        &0u64,
        &String::from_str(&env, "x"),
    );
    assert_eq!(result, Err(Ok(ComplianceError::InvalidStatus)));
}

#[test]
fn test_default_expires_at_uses_rescreening_interval() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let target = Address::generate(&env);
    client.set_rescreening_interval(&admin, &86_400u64);

    client.submit_screening_result(
        &admin,
        &target,
        &ComplianceStatus::Cleared,
        &0u32,
        &RiskTier::Low,
        &0u64, // trigger default
        &String::from_str(&env, "auto"),
    );

    let record = client.get_compliance_record(&target).unwrap();
    assert_eq!(record.expires_at, 1_000_000 + 86_400);
    assert!(client.is_cleared(&target));
}
