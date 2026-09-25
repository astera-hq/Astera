#![cfg(test)]

//! #1042: verifies the additive role-based multisig access-control path on
//! `governance` — (a) the legacy single-admin path is completely untouched
//! (and the token-weighted proposal/vote/execute DAO flow is unrelated and
//! also untouched), (b) an access-control-approved call executes the same
//! effect the legacy path would, (c) calls without a configured/matching
//! access-control address are rejected, and (d) a real `access_control`
//! contract (not a mock) driving a 2-of-3 `TreasuryManager` proposal
//! through to execution actually changes the real contract's config.

use access_control::{AccessControlContract, AccessControlContractClient, ActionPayload, Role};
use governance::{Governance, GovernanceClient, GovernanceError};
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

struct Fixture {
    env: Env,
    client: GovernanceClient<'static>,
    governance_id: Address,
    admin: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let share_token = Address::generate(&env);

    let governance_id = env.register(Governance, ());
    let client = GovernanceClient::new(&env, &governance_id);
    client.initialize(
        &admin,
        &share_token,
        &0u64,
        &2_000u32,
        &6_000u32,
        &0u64,
        &1i128,
    );

    Fixture {
        env,
        client,
        governance_id,
        admin,
    }
}

// ── (a) legacy admin path is untouched ──────────────────────────────────────

#[test]
fn test_legacy_admin_update_config_still_works_unmodified() {
    let f = setup();
    f.client.update_config(&f.admin, &3_000, &7_000);
    let cfg = f.client.get_config();
    assert_eq!(cfg.quorum_bps, 3_000);
    assert_eq!(cfg.pass_bps, 7_000);
}

// ── (c) rejected without a configured/matching access-control address ──────

#[test]
fn test_via_ac_entrypoint_rejected_when_access_control_not_configured() {
    let f = setup();
    let someone = Address::generate(&f.env);
    let result = f.client.try_update_config_via_ac(&someone, &3_000, &7_000);
    assert_eq!(
        result.unwrap_err().unwrap(),
        GovernanceError::AccessControlNotConfigured
    );
}

#[test]
fn test_via_ac_entrypoint_rejected_from_a_non_matching_address() {
    let f = setup();
    let access_control = Address::generate(&f.env);
    f.client.set_access_control(&f.admin, &access_control);

    let impostor = Address::generate(&f.env);
    let result = f.client.try_update_config_via_ac(&impostor, &3_000, &7_000);
    assert_eq!(result.unwrap_err().unwrap(), GovernanceError::Unauthorized);
    assert_eq!(f.client.get_config().quorum_bps, 2_000);
}

// ── (b) an access-control-approved call executes the same effect ───────────

#[test]
fn test_via_ac_entrypoints_apply_the_same_effects_as_their_legacy_admin_counterparts() {
    let f = setup();
    let access_control = Address::generate(&f.env);
    f.client.set_access_control(&f.admin, &access_control);

    f.client
        .update_config_via_ac(&access_control, &4_000, &8_000);
    let cfg = f.client.get_config();
    assert_eq!(cfg.quorum_bps, 4_000);
    assert_eq!(cfg.pass_bps, 8_000);

    f.client.set_category_quorum_via_ac(
        &access_control,
        &governance::ProposalCategory::Treasury,
        &5_500u32,
    );
    assert_eq!(f.client.get_config().treasury_quorum_bps, 5_500);

    // Rotating the trust anchor itself must also go through the currently
    // configured access_control, not the legacy admin key.
    let new_ac = Address::generate(&f.env);
    f.client.set_access_control_via_ac(&access_control, &new_ac);
    assert_eq!(f.client.get_access_control(), Some(new_ac));
}

// ── (d) real access_control contract driving a genuine 2-of-3 proposal ─────

#[test]
fn test_real_access_control_contract_2_of_3_treasury_manager_changes_real_config() {
    let f = setup();

    let ac_id = f.env.register(AccessControlContract, ());
    let ac_client = AccessControlContractClient::new(&f.env, &ac_id);

    let super1 = Address::generate(&f.env);
    let super2 = Address::generate(&f.env);
    let _ = ac_client.initialize(&vec![&f.env, super1.clone(), super2.clone()], &2, &604_800);

    f.client.set_access_control(&f.admin, &ac_id);

    let t1 = Address::generate(&f.env);
    let t2 = Address::generate(&f.env);
    let t3 = Address::generate(&f.env);
    for signer in [&t1, &t2, &t3] {
        let add = ac_client.propose_action(
            &Role::SuperAdmin,
            &super1,
            &ac_id,
            &ActionPayload::AddSigner(Role::TreasuryManager, signer.clone()),
        );
        let _ = ac_client.approve_action(&super2, &add);
        let _ = ac_client.execute_action(&super1, &add);
    }
    let set_threshold = ac_client.propose_action(
        &Role::SuperAdmin,
        &super1,
        &ac_id,
        &ActionPayload::SetThreshold(Role::TreasuryManager, 2),
    );
    let _ = ac_client.approve_action(&super2, &set_threshold);
    let _ = ac_client.execute_action(&super1, &set_threshold);

    let proposal_id = ac_client.propose_action(
        &Role::TreasuryManager,
        &t1,
        &f.governance_id,
        &ActionPayload::UpdateGovernanceConfig(4_500, 7_500),
    );

    let premature = ac_client.try_execute_action(&t1, &proposal_id);
    assert!(premature.is_err());
    assert_eq!(f.client.get_config().quorum_bps, 2_000);

    let _ = ac_client.approve_action(&t2, &proposal_id);
    let _ = ac_client.execute_action(&t1, &proposal_id);

    let cfg = f.client.get_config();
    assert_eq!(cfg.quorum_bps, 4_500);
    assert_eq!(cfg.pass_bps, 7_500);
}
