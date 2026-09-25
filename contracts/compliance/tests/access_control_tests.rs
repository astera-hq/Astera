#![cfg(test)]

//! #1042: verifies the additive role-based multisig access-control path on
//! `compliance` — (a) the legacy single-admin path is completely untouched,
//! (b) an access-control-approved call executes the same effect the legacy
//! path would, (c) calls without a configured/matching access-control
//! address are rejected, and (d) a real `access_control` contract (not a
//! mock) driving a 2-of-3 `ComplianceOfficer` proposal through to execution
//! actually registers a screener on the real contract.

use access_control::{AccessControlContract, AccessControlContractClient, ActionPayload, Role};
use compliance::{ComplianceContract, ComplianceContractClient, ComplianceError};
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

struct Fixture {
    env: Env,
    client: ComplianceContractClient<'static>,
    compliance_id: Address,
    admin: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let compliance_id = env.register(ComplianceContract, ());
    let client = ComplianceContractClient::new(&env, &compliance_id);
    client.initialize(&admin);

    Fixture {
        env,
        client,
        compliance_id,
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

// ── (c) rejected without a configured/matching access-control address ──────

#[test]
fn test_via_ac_entrypoint_rejected_when_access_control_not_configured() {
    let f = setup();
    let someone = Address::generate(&f.env);
    let result = f.client.try_set_paused_via_ac(&someone, &true);
    assert_eq!(
        result.unwrap_err().unwrap(),
        ComplianceError::AccessControlNotConfigured
    );
}

#[test]
fn test_via_ac_entrypoint_rejected_from_a_non_matching_address() {
    let f = setup();
    let access_control = Address::generate(&f.env);
    f.client.set_access_control(&f.admin, &access_control);

    let impostor = Address::generate(&f.env);
    let screener = Address::generate(&f.env);
    let result = f.client.try_register_screener_via_ac(&impostor, &screener);
    assert_eq!(result.unwrap_err().unwrap(), ComplianceError::Unauthorized);
    assert!(!f.client.is_screener(&screener));
}

// ── (b) an access-control-approved call executes the same effect ───────────

#[test]
fn test_via_ac_entrypoints_apply_the_same_effects_as_their_legacy_admin_counterparts() {
    let f = setup();
    let access_control = Address::generate(&f.env);
    f.client.set_access_control(&f.admin, &access_control);

    // Registration only activates immediately when the timelock is zero —
    // the default is 24h (DEFAULT_SCREENER_TIMELOCK_SECS) — so zero it out
    // via the admin path first.
    f.client
        .set_screener_timelock_via_ac(&access_control, &0u64);

    let screener = Address::generate(&f.env);
    f.client
        .register_screener_via_ac(&access_control, &screener);
    assert!(f.client.is_screener(&screener));

    f.client
        .deregister_screener_via_ac(&access_control, &screener);
    assert!(!f.client.is_screener(&screener));

    f.client
        .set_screener_timelock_via_ac(&access_control, &1_000u64);
    assert_eq!(f.client.get_screener_timelock(), 1_000);

    let screener2 = Address::generate(&f.env);
    f.client
        .register_screener_via_ac(&access_control, &screener2);
    // Timelock is now non-zero, so screener2 is only pending.
    assert!(!f.client.is_screener(&screener2));

    f.client
        .set_rescreening_interval_via_ac(&access_control, &500u64);
    assert_eq!(f.client.get_rescreening_interval(), 500);

    // Rotating the trust anchor itself must also go through the currently
    // configured access_control, not the legacy admin key.
    let new_ac = Address::generate(&f.env);
    f.client.set_access_control_via_ac(&access_control, &new_ac);
    assert_eq!(f.client.get_access_control(), Some(new_ac));
}

// ── (d) real access_control contract driving a genuine 2-of-3 proposal ─────

#[test]
fn test_real_access_control_contract_2_of_3_compliance_officer_registers_real_screener() {
    let f = setup();

    let ac_id = f.env.register(AccessControlContract, ());
    let ac_client = AccessControlContractClient::new(&f.env, &ac_id);

    let super1 = Address::generate(&f.env);
    let super2 = Address::generate(&f.env);
    let _ = ac_client.initialize(&vec![&f.env, super1.clone(), super2.clone()], &2, &604_800, &0);

    f.client.set_access_control(&f.admin, &ac_id);
    // Zero the screener timelock (default is 24h) so a registration
    // proposal's effect is directly observable via is_screener() below,
    // without a second confirm_screener_via_ac step.
    f.client.set_screener_timelock(&f.admin, &0u64);

    let c1 = Address::generate(&f.env);
    let c2 = Address::generate(&f.env);
    let c3 = Address::generate(&f.env);
    for signer in [&c1, &c2, &c3] {
        let add = ac_client.propose_action(
            &Role::SuperAdmin,
            &super1,
            &ac_id,
            &ActionPayload::AddSigner(Role::ComplianceOfficer, signer.clone()),
        );
        let _ = ac_client.approve_action(&super2, &add);
        let _ = ac_client.execute_action(&super1, &add);
    }
    let set_threshold = ac_client.propose_action(
        &Role::SuperAdmin,
        &super1,
        &ac_id,
        &ActionPayload::SetThreshold(Role::ComplianceOfficer, 2),
    );
    let _ = ac_client.approve_action(&super2, &set_threshold);
    let _ = ac_client.execute_action(&super1, &set_threshold);

    let screener = Address::generate(&f.env);
    let proposal_id = ac_client.propose_action(
        &Role::ComplianceOfficer,
        &c1,
        &f.compliance_id,
        &ActionPayload::RegisterScreener(screener.clone()),
    );

    let premature = ac_client.try_execute_action(&c1, &proposal_id);
    assert!(premature.is_err());
    assert!(!f.client.is_screener(&screener));

    let _ = ac_client.approve_action(&c2, &proposal_id);
    let _ = ac_client.execute_action(&c1, &proposal_id);

    assert!(f.client.is_screener(&screener));
}
