#![cfg(test)]

//! #1042: verifies the additive role-based multisig access-control path on
//! `oracle_registry` — (a) the legacy single-admin path is completely
//! untouched, (b) an access-control-approved call executes the same effect
//! the legacy path would, (c) calls without a configured/matching
//! access-control address are rejected, and (d) a real `access_control`
//! contract (not a mock) driving a 2-of-3 `OracleManager` proposal through
//! to execution actually changes the registry's live config.

use access_control::{AccessControlContract, AccessControlContractClient, ActionPayload, Role};
use oracle_registry::{OracleRegistryContract, OracleRegistryContractClient, OracleRegistryError};
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

struct Fixture {
    env: Env,
    client: OracleRegistryContractClient<'static>,
    registry_id: Address,
    admin: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let stake_token = Address::generate(&env);

    let registry_id = env.register(OracleRegistryContract, ());
    let client = OracleRegistryContractClient::new(&env, &registry_id);
    client.initialize(&admin, &stake_token, &1_000i128);

    Fixture {
        env,
        client,
        registry_id,
        admin,
    }
}

// ── (a) legacy admin path is untouched ──────────────────────────────────────

#[test]
fn test_legacy_admin_pause_unpause_still_work_unmodified() {
    let f = setup();
    let paused_before = f.client.get_registry_config().min_stake;
    f.client.pause(&f.admin);
    assert!(f.client.is_paused());
    f.client.unpause(&f.admin);
    assert!(!f.client.is_paused());
    // sanity: config untouched by pausing.
    assert_eq!(f.client.get_registry_config().min_stake, paused_before);
}

// ── (c) rejected without a configured/matching access-control address ──────

#[test]
fn test_via_ac_entrypoint_rejected_when_access_control_not_configured() {
    let f = setup();
    let someone = Address::generate(&f.env);
    let result = f.client.try_set_paused_via_ac(&someone, &true);
    assert_eq!(
        result.unwrap_err().unwrap(),
        OracleRegistryError::AccessControlNotConfigured
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
        OracleRegistryError::Unauthorized
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

    let invoice_contract = Address::generate(&f.env);
    f.client
        .set_invoice_contract_via_ac(&access_control, &invoice_contract);
    assert_eq!(f.client.get_invoice_contract(), Some(invoice_contract));

    let treasury = Address::generate(&f.env);
    f.client
        .set_treasury_via_ac(&access_control, &Some(treasury.clone()));
    assert_eq!(f.client.get_registry_config().treasury, Some(treasury));

    f.client.set_registry_config_via_ac(
        &access_control,
        &2_000i128,
        &5u32,
        &7_500u32,
        &(4 * 24 * 60 * 60u64),
        &(10 * 24 * 60 * 60u64),
    );
    let cfg = f.client.get_registry_config();
    assert_eq!(cfg.min_stake, 2_000);
    assert_eq!(cfg.required_votes, 5);
    assert_eq!(cfg.quorum_bps, 7_500);

    // Rotating the trust anchor itself must also go through the currently
    // configured access_control, not the legacy admin key.
    let new_ac = Address::generate(&f.env);
    f.client.set_access_control_via_ac(&access_control, &new_ac);
    assert_eq!(f.client.get_access_control(), Some(new_ac));
}

// ── (d) real access_control contract driving a genuine 2-of-3 proposal ─────

#[test]
fn test_real_access_control_contract_2_of_3_oracle_manager_changes_real_registry_config() {
    let f = setup();

    let ac_id = f.env.register(AccessControlContract, ());
    let ac_client = AccessControlContractClient::new(&f.env, &ac_id);

    let super1 = Address::generate(&f.env);
    let super2 = Address::generate(&f.env);
    let _ = ac_client.initialize(&vec![&f.env, super1.clone(), super2.clone()], &2, &604_800, &0);

    f.client.set_access_control(&f.admin, &ac_id);

    let o1 = Address::generate(&f.env);
    let o2 = Address::generate(&f.env);
    let o3 = Address::generate(&f.env);
    for signer in [&o1, &o2, &o3] {
        let add = ac_client.propose_action(
            &Role::SuperAdmin,
            &super1,
            &ac_id,
            &ActionPayload::AddSigner(Role::OracleManager, signer.clone()),
        );
        let _ = ac_client.approve_action(&super2, &add);
        let _ = ac_client.execute_action(&super1, &add);
    }
    let set_threshold = ac_client.propose_action(
        &Role::SuperAdmin,
        &super1,
        &ac_id,
        &ActionPayload::SetThreshold(Role::OracleManager, 2),
    );
    let _ = ac_client.approve_action(&super2, &set_threshold);
    let _ = ac_client.execute_action(&super1, &set_threshold);

    let proposal_id = ac_client.propose_action(
        &Role::OracleManager,
        &o1,
        &f.registry_id,
        &ActionPayload::SetOracleRegistryPaused(true),
    );

    let premature = ac_client.try_execute_action(&o1, &proposal_id);
    assert!(premature.is_err());
    assert!(!f.client.is_paused());

    let _ = ac_client.approve_action(&o2, &proposal_id);
    let _ = ac_client.execute_action(&o1, &proposal_id);

    assert!(f.client.is_paused());
}
