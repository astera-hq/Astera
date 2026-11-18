#![cfg(test)]

//! #864: verifies the additive role-based multisig access-control path on
//! `credit_score` — (a) the legacy single-admin path is untouched, (b) an
//! access-control-approved call executes the same effect the legacy path
//! would, (c) calls without a configured/matching access-control address
//! are rejected.

use access_control::{AccessControlContract, AccessControlContractClient, ActionPayload, Role};
use credit_score::{CreditScoreContract, CreditScoreContractClient, CreditScoreError};
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

struct Fixture {
    env: Env,
    client: CreditScoreContractClient<'static>,
    contract_id: Address,
    admin: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let invoice_contract = Address::generate(&env);
    let pool_contract = Address::generate(&env);

    let contract_id = env.register(CreditScoreContract, ());
    let client = CreditScoreContractClient::new(&env, &contract_id);
    client.initialize(&admin, &invoice_contract, &pool_contract);

    Fixture {
        env,
        client,
        contract_id,
        admin,
    }
}

// ── (a) legacy admin path is untouched ──────────────────────────────────────

#[test]
fn test_legacy_admin_pause_unpause_still_work_unmodified() {
    let f = setup();
    f.client.pause(&f.admin);
    assert!(f.client.is_paused());
    f.client.unpause(&f.admin);
    assert!(!f.client.is_paused());
}

#[test]
fn test_legacy_admin_set_late_threshold_still_works_unmodified() {
    let f = setup();
    f.client.set_late_threshold(&f.admin, &45);
    assert_eq!(f.client.get_late_threshold(), 45);
}

// ── (c) rejected without a configured/matching access-control address ──────

#[test]
fn test_via_ac_entrypoint_rejected_when_access_control_not_configured() {
    let f = setup();
    let someone = Address::generate(&f.env);
    let result = f.client.try_set_late_threshold_via_ac(&someone, &45);
    assert_eq!(
        result.unwrap_err().unwrap(),
        CreditScoreError::AccessControlNotConfigured.into()
    );
}

#[test]
fn test_via_ac_entrypoint_rejected_from_a_non_matching_address() {
    let f = setup();
    let access_control = Address::generate(&f.env);
    f.client.set_access_control(&f.admin, &access_control);

    let impostor = Address::generate(&f.env);
    let result = f.client.try_set_late_threshold_via_ac(&impostor, &45);
    assert_eq!(
        result.unwrap_err().unwrap(),
        CreditScoreError::Unauthorized.into()
    );
    assert_eq!(f.client.get_late_threshold(), 30); // unchanged default
}

// ── (b) an access-control-approved call executes the same effect ───────────

#[test]
fn test_via_ac_entrypoints_apply_the_same_effects_as_their_legacy_admin_counterparts() {
    let f = setup();
    let access_control = Address::generate(&f.env);
    f.client.set_access_control(&f.admin, &access_control);

    f.client.set_paused_via_ac(&access_control, &true);
    assert!(f.client.is_paused());
    f.client.set_paused_via_ac(&access_control, &false);
    assert!(!f.client.is_paused());

    f.client.set_late_threshold_via_ac(&access_control, &45);
    assert_eq!(f.client.get_late_threshold(), 45);

    f.client
        .set_score_thresholds_via_ac(&access_control, &810, &750, &680, &600);
    // get_score_thresholds() takes `&Env` rather than `Env`, so it isn't part
    // of the deployed contract interface (no generated client method) —
    // call the plain Rust associated function directly instead, scoped to
    // this specific contract instance's storage.
    let thresholds = f.env.as_contract(&f.contract_id, || {
        CreditScoreContract::get_score_thresholds(&f.env)
    });
    assert_eq!(thresholds.excellent, 810);
    assert_eq!(thresholds.fair, 600);

    let attestor = Address::generate(&f.env);
    f.client
        .register_attestor_via_ac(&access_control, &attestor, &1, &5_000);
    let info = f.client.get_attestor_info(&attestor).unwrap();
    assert!(info.is_active);
    assert_eq!(info.weight_bps, 5_000);
}

#[test]
fn test_via_ac_register_attestor_rejects_invalid_type_discriminant() {
    let f = setup();
    let access_control = Address::generate(&f.env);
    f.client.set_access_control(&f.admin, &access_control);

    let attestor = Address::generate(&f.env);
    let result = f
        .client
        .try_register_attestor_via_ac(&access_control, &attestor, &99, &5_000);
    assert_eq!(
        result.unwrap_err().unwrap(),
        CreditScoreError::InvalidAttestorType.into()
    );
}

// ── #1042: admin-key rotation itself requires access_control, not admin ────

#[test]
fn test_set_access_control_via_ac_rotates_the_trust_anchor() {
    let f = setup();
    let access_control = Address::generate(&f.env);
    f.client.set_access_control(&f.admin, &access_control);

    let new_access_control = Address::generate(&f.env);
    f.client
        .set_access_control_via_ac(&access_control, &new_access_control);
    assert_eq!(
        f.client.get_access_control(),
        Some(new_access_control.clone())
    );

    let result = f
        .client
        .try_set_access_control_via_ac(&access_control, &access_control);
    assert_eq!(
        result.unwrap_err().unwrap(),
        CreditScoreError::Unauthorized.into()
    );

    f.client.set_paused_via_ac(&new_access_control, &true);
    assert!(f.client.is_paused());
}

// ── real access_control contract driving a genuine 2-of-3 proposal ─────────

#[test]
fn test_real_access_control_contract_drives_late_threshold_change() {
    let f = setup();

    let ac_id = f.env.register(AccessControlContract, ());
    let ac_client = AccessControlContractClient::new(&f.env, &ac_id);

    let super1 = Address::generate(&f.env);
    let super2 = Address::generate(&f.env);
    let _ = ac_client.initialize(&vec![&f.env, super1.clone(), super2.clone()], &2, &604_800, &0);

    f.client.set_access_control(&f.admin, &ac_id);

    let r1 = Address::generate(&f.env);
    let r2 = Address::generate(&f.env);
    for signer in [&r1, &r2] {
        let add = ac_client.propose_action(
            &Role::SuperAdmin,
            &super1,
            &ac_id,
            &ActionPayload::AddSigner(Role::RiskManager, signer.clone()),
        );
        let _ = ac_client.approve_action(&super2, &add);
        let _ = ac_client.execute_action(&super1, &add);
    }
    let set_threshold = ac_client.propose_action(
        &Role::SuperAdmin,
        &super1,
        &ac_id,
        &ActionPayload::SetThreshold(Role::RiskManager, 2),
    );
    let _ = ac_client.approve_action(&super2, &set_threshold);
    let _ = ac_client.execute_action(&super1, &set_threshold);

    let proposal_id = ac_client.propose_action(
        &Role::RiskManager,
        &r1,
        &f.contract_id,
        &ActionPayload::SetLateThreshold(60),
    );

    let premature = ac_client.try_execute_action(&r1, &proposal_id);
    assert!(premature.is_err());
    assert_eq!(f.client.get_late_threshold(), 30);

    let _ = ac_client.approve_action(&r2, &proposal_id);
    let _ = ac_client.execute_action(&r1, &proposal_id);

    assert_eq!(f.client.get_late_threshold(), 60);
}
