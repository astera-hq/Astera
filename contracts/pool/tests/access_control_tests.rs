#![cfg(test)]

//! #864: verifies the additive role-based multisig access-control path on
//! `pool` — (a) the legacy single-admin path is completely untouched, (b)
//! an access-control-approved proposal executes the same effect the legacy
//! path would, (c) calls without a configured/matching access-control
//! address are rejected, and (d) a real `access_control` contract (not a
//! mock) driving a 2-of-3 `RiskManager` proposal through to execution
//! actually changes `PoolConfig.yield_bps`.

use access_control::{AccessControlContract, AccessControlContractClient, ActionPayload, Role};
use pool::{FundingPool, FundingPoolClient, PoolError};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    vec, Address, Env,
};

struct Fixture {
    env: Env,
    pool_client: FundingPoolClient<'static>,
    pool_id: Address,
    admin: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    // initialize() calls token::Client(initial_token).decimals(), so this
    // needs to be a real registered asset contract, not a bare address —
    // share_token/invoice_contract are never invoked by anything these
    // tests exercise, so plain generated addresses are fine for those.
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let share_token = Address::generate(&env);
    let invoice_contract = Address::generate(&env);

    let pool_id = env.register(FundingPool, ());
    let pool_client = FundingPoolClient::new(&env, &pool_id);
    pool_client.initialize(&admin, &token, &share_token, &invoice_contract);
    // set_yield()/set_yield_via_ac() both enforce a 24h cooldown from
    // last_yield_change_at, which initialize() sets to the current ledger
    // timestamp — advance past it so these tests can actually call it.
    env.ledger().with_mut(|li| li.timestamp += 86_400 + 1);

    Fixture {
        env,
        pool_client,
        pool_id,
        admin,
    }
}

// ── (a) legacy admin path is untouched ──────────────────────────────────────

#[test]
fn test_legacy_admin_set_yield_still_works_unmodified() {
    let f = setup();
    f.pool_client.set_yield(&f.admin, &600);
    assert_eq!(f.pool_client.get_config().yield_bps, 600);
}

#[test]
fn test_legacy_admin_pause_unpause_still_work_unmodified() {
    let f = setup();
    f.pool_client.pause(&f.admin);
    assert!(f.pool_client.is_paused());
    f.pool_client.unpause(&f.admin);
    assert!(!f.pool_client.is_paused());
}

// ── (c) rejected without a configured/matching access-control address ──────

#[test]
fn test_via_ac_entrypoint_rejected_when_access_control_not_configured() {
    let f = setup();
    let someone = Address::generate(&f.env);
    let result = f.pool_client.try_set_yield_via_ac(&someone, &600);
    assert_eq!(
        result.unwrap_err().unwrap(),
        PoolError::AccessControlNotConfigured
    );
}

#[test]
fn test_via_ac_entrypoint_rejected_from_a_non_matching_address() {
    let f = setup();
    let access_control = Address::generate(&f.env);
    f.pool_client.set_access_control(&f.admin, &access_control);

    let impostor = Address::generate(&f.env);
    let result = f.pool_client.try_set_yield_via_ac(&impostor, &600);
    assert_eq!(result.unwrap_err().unwrap(), PoolError::Unauthorized);
    // The legacy config is untouched by the rejected attempt.
    assert_eq!(f.pool_client.get_config().yield_bps, 800);
}

// ── (b) an access-control-approved call executes the same effect ───────────

#[test]
fn test_via_ac_entrypoints_apply_the_same_effects_as_their_legacy_admin_counterparts() {
    let f = setup();
    let access_control = Address::generate(&f.env);
    f.pool_client.set_access_control(&f.admin, &access_control);

    f.pool_client.set_yield_via_ac(&access_control, &650);
    assert_eq!(f.pool_client.get_config().yield_bps, 650);

    f.pool_client.set_paused_via_ac(&access_control, &true);
    assert!(f.pool_client.is_paused());
    f.pool_client.set_paused_via_ac(&access_control, &false);
    assert!(!f.pool_client.is_paused());

    let treasury = Address::generate(&f.env);
    f.pool_client
        .set_treasury_via_ac(&access_control, &treasury);
    assert_eq!(f.pool_client.get_treasury(), treasury);

    f.pool_client
        .set_kyc_required_via_ac(&access_control, &true);
    assert!(f.pool_client.kyc_required());

    f.pool_client
        .set_max_utilization_via_ac(&access_control, &9_000);
    assert_eq!(f.pool_client.get_config().max_utilization_bps, 9_000);

    let oracle = Address::generate(&f.env);
    f.pool_client
        .set_oracle_contract_via_ac(&access_control, &oracle);
    assert_eq!(f.pool_client.get_oracle_contract(), Some(oracle));
}

// #1042: pool has no set_access_control_via_ac (rotation) entrypoint —
// its wasm binary has no size budget left for a new public entrypoint.
// See the comment on pool's get_access_control in contracts/pool/src/lib.rs.

// ── (d) real access_control contract driving a genuine 2-of-3 proposal ─────

#[test]
fn test_real_access_control_contract_2_of_3_risk_manager_changes_real_pool_yield() {
    let f = setup();

    let ac_id = f.env.register(AccessControlContract, ());
    let ac_client = AccessControlContractClient::new(&f.env, &ac_id);

    let super1 = Address::generate(&f.env);
    let super2 = Address::generate(&f.env);
    let _ = ac_client.initialize(&vec![&f.env, super1.clone(), super2.clone()], &2, &604_800, &0);

    f.pool_client.set_access_control(&f.admin, &ac_id);

    // Seed RiskManager as 2-of-3 via the SuperAdmin bootstrap flow.
    let r1 = Address::generate(&f.env);
    let r2 = Address::generate(&f.env);
    let r3 = Address::generate(&f.env);
    for signer in [&r1, &r2, &r3] {
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

    assert_eq!(f.pool_client.get_config().yield_bps, 800); // default

    // Propose a real yield change against the real pool contract.
    let proposal_id = ac_client.propose_action(
        &Role::RiskManager,
        &r1,
        &f.pool_id,
        &ActionPayload::SetYield(650),
    );

    // One approval (the proposer's own) is short of the 2-of-3 threshold —
    // execution must be rejected and the pool's yield must stay untouched.
    let premature = ac_client.try_execute_action(&r1, &proposal_id);
    assert!(premature.is_err());
    assert_eq!(f.pool_client.get_config().yield_bps, 800);

    // Second approval reaches threshold.
    let _ = ac_client.approve_action(&r2, &proposal_id);
    let _ = ac_client.execute_action(&r1, &proposal_id);

    // The real pool's real PoolConfig.yield_bps actually changed.
    assert_eq!(f.pool_client.get_config().yield_bps, 650);
}
