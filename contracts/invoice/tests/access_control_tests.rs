#![cfg(test)]

//! #864: verifies the additive role-based multisig access-control path on
//! `invoice` — (a) the legacy single-admin path is untouched, (b) an
//! access-control-approved call executes the same effect the legacy path
//! would, (c) calls without a configured/matching access-control address
//! are rejected.

use access_control::{AccessControlContract, AccessControlContractClient, ActionPayload, Role};
use invoice::{InvoiceContract, InvoiceContractClient, InvoiceError};
use soroban_sdk::{testutils::Address as _, vec, Address, Env, String};

struct Fixture {
    env: Env,
    client: InvoiceContractClient<'static>,
    contract_id: Address,
    admin: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let pool = Address::generate(&env);

    let contract_id = env.register(InvoiceContract, ());
    let client = InvoiceContractClient::new(&env, &contract_id);
    client.initialize(&admin, &pool, &1_000_000, &2_592_000, &7);

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
fn test_legacy_admin_register_debtor_still_works_unmodified() {
    let f = setup();
    let debtor_id = String::from_str(&f.env, "debtor-1");
    let debtor_name = String::from_str(&f.env, "Acme Co");
    f.client
        .register_debtor(&f.admin, &debtor_id, &debtor_name, &50_000);
    let record = f.client.get_debtor(&debtor_id);
    assert!(record.is_active);
    assert_eq!(record.max_exposure, 50_000);
}

// ── (c) rejected without a configured/matching access-control address ──────

#[test]
fn test_via_ac_entrypoint_rejected_when_access_control_not_configured() {
    let f = setup();
    let someone = Address::generate(&f.env);
    let result = f.client.try_set_paused_via_ac(&someone, &true);
    assert_eq!(
        result.unwrap_err().unwrap(),
        InvoiceError::AccessControlNotConfigured.into()
    );
}

#[test]
fn test_via_ac_entrypoint_rejected_from_a_non_matching_address() {
    let f = setup();
    let access_control = Address::generate(&f.env);
    f.client.set_access_control(&f.admin, &access_control);

    let impostor = Address::generate(&f.env);
    let result = f.client.try_set_paused_via_ac(&impostor, &true);
    assert_eq!(
        result.unwrap_err().unwrap(),
        InvoiceError::Unauthorized.into()
    );
    assert!(!f.client.is_paused());
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

    let oracle = Address::generate(&f.env);
    f.client.set_oracle_via_ac(&access_control, &oracle);
    assert_eq!(f.client.get_oracle(), Some(oracle));

    let debtor_id = String::from_str(&f.env, "debtor-2");
    let debtor_name = String::from_str(&f.env, "Beta Co");
    f.client
        .register_debtor_via_ac(&access_control, &debtor_id, &debtor_name, &75_000);
    let record = f.client.get_debtor(&debtor_id);
    assert!(record.is_active);
    assert_eq!(record.max_exposure, 75_000);

    f.client
        .deactivate_debtor_via_ac(&access_control, &debtor_id);
    assert!(!f.client.get_debtor(&debtor_id).is_active);

    let keeper = Address::generate(&f.env);
    f.client.add_keeper_via_ac(&access_control, &keeper);
    assert!(f.client.list_keepers().contains(&keeper));
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
        InvoiceError::Unauthorized.into()
    );

    f.client.set_paused_via_ac(&new_access_control, &true);
    assert!(f.client.is_paused());
}

// ── real access_control contract driving a genuine 2-of-2 proposal ─────────

#[test]
fn test_real_access_control_contract_drives_debtor_registration() {
    let f = setup();

    let ac_id = f.env.register(AccessControlContract, ());
    let ac_client = AccessControlContractClient::new(&f.env, &ac_id);

    let super1 = Address::generate(&f.env);
    let super2 = Address::generate(&f.env);
    let _ = ac_client.initialize(&vec![&f.env, super1.clone(), super2.clone()], &2, &604_800, &0);

    f.client.set_access_control(&f.admin, &ac_id);

    let c1 = Address::generate(&f.env);
    let c2 = Address::generate(&f.env);
    let add1 = ac_client.propose_action(
        &Role::SuperAdmin,
        &super1,
        &ac_id,
        &ActionPayload::AddSigner(Role::ComplianceOfficer, c1.clone()),
    );
    let _ = ac_client.approve_action(&super2, &add1);
    let _ = ac_client.execute_action(&super1, &add1);
    let add2 = ac_client.propose_action(
        &Role::SuperAdmin,
        &super1,
        &ac_id,
        &ActionPayload::AddSigner(Role::ComplianceOfficer, c2.clone()),
    );
    let _ = ac_client.approve_action(&super2, &add2);
    let _ = ac_client.execute_action(&super1, &add2);
    let set_threshold = ac_client.propose_action(
        &Role::SuperAdmin,
        &super1,
        &ac_id,
        &ActionPayload::SetThreshold(Role::ComplianceOfficer, 2),
    );
    let _ = ac_client.approve_action(&super2, &set_threshold);
    let _ = ac_client.execute_action(&super1, &set_threshold);

    let debtor_id = String::from_str(&f.env, "debtor-real");
    let debtor_name = String::from_str(&f.env, "Real Co");
    let proposal_id = ac_client.propose_action(
        &Role::ComplianceOfficer,
        &c1,
        &f.contract_id,
        &ActionPayload::RegisterDebtor(debtor_id.clone(), debtor_name, 20_000),
    );

    // Only one approval so far — must not have executed.
    let premature = ac_client.try_execute_action(&c1, &proposal_id);
    assert!(premature.is_err());

    let _ = ac_client.approve_action(&c2, &proposal_id);
    let _ = ac_client.execute_action(&c1, &proposal_id);

    let record = f.client.get_debtor(&debtor_id);
    assert!(record.is_active);
    assert_eq!(record.max_exposure, 20_000);
}
