#![cfg(test)]

//! #1042: verifies the additive role-based multisig access-control path on
//! `referral` — (a) the legacy single-admin path is completely untouched,
//! (b) an access-control-approved call executes the same effect the legacy
//! path would, (c) calls without a configured/matching access-control
//! address are rejected, and (d) a real `access_control` contract (not a
//! mock) driving a 2-of-3 `TreasuryManager` proposal through to execution
//! actually changes the real contract's reward bps.

use access_control::{AccessControlContract, AccessControlContractClient, ActionPayload, Role};
use referral::{ReferralContract, ReferralContractClient, ReferralError};
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

struct Fixture {
    env: Env,
    client: ReferralContractClient<'static>,
    referral_id: Address,
    admin: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let pool = Address::generate(&env);

    let referral_id = env.register(ReferralContract, ());
    let client = ReferralContractClient::new(&env, &referral_id);
    client.initialize(&admin, &pool);

    Fixture {
        env,
        client,
        referral_id,
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
        result,
        Err(Ok::<soroban_sdk::Error, _>(
            ReferralError::AccessControlNotConfigured.into()
        ))
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
        result,
        Err(Ok::<soroban_sdk::Error, _>(
            ReferralError::Unauthorized.into()
        ))
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

    let new_pool = Address::generate(&f.env);
    f.client.set_pool_via_ac(&access_control, &new_pool);
    assert_eq!(f.client.get_pool(), new_pool);

    f.client
        .set_borrow_reward_bps_via_ac(&access_control, &750u32);
    assert_eq!(f.client.get_borrow_reward_bps(), 750);

    f.client
        .set_deposit_reward_bps_via_ac(&access_control, &1_500u32);
    assert_eq!(f.client.get_deposit_reward_bps(), 1_500);

    // Rotating the trust anchor itself must also go through the currently
    // configured access_control, not the legacy admin key.
    let new_ac = Address::generate(&f.env);
    f.client.set_access_control_via_ac(&access_control, &new_ac);
    assert_eq!(f.client.get_access_control(), Some(new_ac));
}

// ── (d) real access_control contract driving a genuine 2-of-3 proposal ─────

#[test]
fn test_real_access_control_contract_2_of_3_treasury_manager_changes_real_referral_bps() {
    let f = setup();

    let ac_id = f.env.register(AccessControlContract, ());
    let ac_client = AccessControlContractClient::new(&f.env, &ac_id);

    let super1 = Address::generate(&f.env);
    let super2 = Address::generate(&f.env);
    let _ = ac_client.initialize(&vec![&f.env, super1.clone(), super2.clone()], &2, &604_800, &0);

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
        &f.referral_id,
        &ActionPayload::SetBorrowRewardBps(900),
    );

    let premature = ac_client.try_execute_action(&t1, &proposal_id);
    assert!(premature.is_err());
    assert_eq!(f.client.get_borrow_reward_bps(), 500);

    let _ = ac_client.approve_action(&t2, &proposal_id);
    let _ = ac_client.execute_action(&t1, &proposal_id);

    assert_eq!(f.client.get_borrow_reward_bps(), 900);
}
