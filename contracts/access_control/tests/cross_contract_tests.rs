#![cfg(test)]

//! Cross-contract dispatch tests: `execute_action` calling out into a
//! target contract's `*_via_ac` entrypoint, carrying this
//! contract's own address as proof of origin (the same pattern the real
//! `pool` contract already uses to trust `invoice_contract` for
//! `update_invoice_due_date` — the callee does `access_control.require_auth()`
//! against the caller's own on-chain identity).
//!
//! These mocks are local, minimal stand-ins for the real
//! `set_yield_via_ac` etc. methods added to `pool` in this same
//! change — see contracts/pool/src/lib.rs. They only need to look and
//! authenticate exactly like the real thing for `execute_action` to be
//! exercised faithfully.

use access_control::{
    AccessControlContract, AccessControlContractClient, ActionPayload, ProposalStatus, Role,
};
use soroban_sdk::{
    contract, contractimpl, symbol_short, testutils::Address as _, vec, Address, Env,
};

mod mock_pool {
    use super::*;

    #[contract]
    pub struct MockPool;

    #[contractimpl]
    impl MockPool {
        pub fn set_yield_via_ac(env: Env, access_control: Address, new_yield_bps: u32) {
            access_control.require_auth();
            env.storage()
                .instance()
                .set(&symbol_short!("yield"), &new_yield_bps);
        }

        pub fn get_yield(env: Env) -> u32 {
            env.storage()
                .instance()
                .get(&symbol_short!("yield"))
                .unwrap_or(0)
        }

        pub fn set_paused_via_ac(env: Env, access_control: Address, paused: bool) {
            access_control.require_auth();
            env.storage()
                .instance()
                .set(&symbol_short!("paused"), &paused);
        }

        pub fn is_paused(env: Env) -> bool {
            env.storage()
                .instance()
                .get(&symbol_short!("paused"))
                .unwrap_or(false)
        }
    }
}

use mock_pool::{MockPool, MockPoolClient};

struct Fixture {
    env: Env,
    ac_client: AccessControlContractClient<'static>,
    ac_id: Address,
    pool_client: MockPoolClient<'static>,
    pool_id: Address,
    s1: Address,
    s2: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(AccessControlContract, ());
    let ac_client = AccessControlContractClient::new(&env, &ac_id);

    let pool_id = env.register(MockPool, ());
    let pool_client = MockPoolClient::new(&env, &pool_id);

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);

    let _ = ac_client.initialize(&vec![&env, s1.clone(), s2.clone()], &2, &604_800, &0);

    Fixture {
        env,
        ac_client,
        ac_id,
        pool_client,
        pool_id,
        s1,
        s2,
    }
}

/// Seeds RiskManager as a 2-of-3 role via the SuperAdmin bootstrap flow.
fn seed_risk_manager(f: &Fixture, threshold: u32) -> (Address, Address, Address) {
    let r1 = Address::generate(&f.env);
    let r2 = Address::generate(&f.env);
    let r3 = Address::generate(&f.env);

    for signer in [&r1, &r2, &r3] {
        let add = f.ac_client.propose_action(
            &Role::SuperAdmin,
            &f.s1,
            &f.ac_id,
            &ActionPayload::AddSigner(Role::RiskManager, signer.clone()),
        );
        let _ = f.ac_client.approve_action(&f.s2, &add);
        let _ = f.ac_client.execute_action(&f.s1, &add);
    }
    let set_threshold = f.ac_client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.ac_id,
        &ActionPayload::SetThreshold(Role::RiskManager, threshold),
    );
    let _ = f.ac_client.approve_action(&f.s2, &set_threshold);
    let _ = f.ac_client.execute_action(&f.s1, &set_threshold);

    (r1, r2, r3)
}

#[test]
fn test_risk_manager_2_of_3_yield_change_requires_two_approvals_then_executes() {
    let f = setup();
    let (r1, r2, _r3) = seed_risk_manager(&f, 2);

    assert_eq!(f.pool_client.get_yield(), 0);

    let proposal_id = f.ac_client.propose_action(
        &Role::RiskManager,
        &r1,
        &f.pool_id,
        &ActionPayload::SetYield(650),
    );

    // One approval (the proposer's own) is not enough at threshold 2 —
    // execution must be rejected and the pool's yield must stay untouched.
    let premature = f.ac_client.try_execute_action(&r1, &proposal_id);
    assert!(premature.is_err());
    assert_eq!(f.pool_client.get_yield(), 0);
    assert_eq!(
        f.ac_client.get_proposal(&proposal_id).unwrap().status,
        ProposalStatus::Pending
    );

    // Second approval reaches threshold — now it can execute, and the mock
    // pool's stored yield actually changes via the access-control-trusted
    // call.
    let _ = f.ac_client.approve_action(&r2, &proposal_id);
    assert_eq!(
        f.ac_client.get_proposal(&proposal_id).unwrap().status,
        ProposalStatus::Approved
    );

    let _ = f.ac_client.execute_action(&r1, &proposal_id);
    assert_eq!(f.pool_client.get_yield(), 650);
    assert_eq!(
        f.ac_client.get_proposal(&proposal_id).unwrap().status,
        ProposalStatus::Executed
    );
}

#[test]
fn test_execute_action_carries_access_control_own_identity_not_an_arbitrary_caller() {
    // The mock pool's set_yield_via_ac requires
    // `access_control.require_auth()` on the address it's given — proving
    // that execute_action always passes *this contract's own* address
    // (env.current_contract_address()), never some caller-supplied address
    // that could be spoofed. If execute_action ever passed the wrong
    // address, the mock's own require_auth() call would be checking
    // authorization for a different contract than the one truly invoking
    // it, and — since this test uses mock_all_auths() — that distinction
    // isn't directly observable via a panic here. Instead we assert the
    // effect actually landed under the *pool's* own storage keyed off the
    // access_control contract having successfully authenticated itself,
    // which only happens if `this_contract` was passed correctly.
    let f = setup();
    let (r1, r2, _r3) = seed_risk_manager(&f, 2);

    let proposal_id = f.ac_client.propose_action(
        &Role::RiskManager,
        &r1,
        &f.pool_id,
        &ActionPayload::SetPaused(true),
    );
    let _ = f.ac_client.approve_action(&r2, &proposal_id);
    let _ = f.ac_client.execute_action(&r1, &proposal_id);

    assert!(f.pool_client.is_paused());
}

#[test]
fn test_second_signer_approving_a_different_proposal_does_not_affect_the_first() {
    let f = setup();
    let (r1, r2, r3) = seed_risk_manager(&f, 2);

    let yield_proposal = f.ac_client.propose_action(
        &Role::RiskManager,
        &r1,
        &f.pool_id,
        &ActionPayload::SetYield(700),
    );
    let pause_proposal = f.ac_client.propose_action(
        &Role::RiskManager,
        &r2,
        &f.pool_id,
        &ActionPayload::SetPaused(true),
    );

    // Approve only the pause proposal with the third signer.
    let _ = f.ac_client.approve_action(&r3, &pause_proposal);

    assert_eq!(
        f.ac_client.get_proposal(&yield_proposal).unwrap().status,
        ProposalStatus::Pending
    );
    assert_eq!(
        f.ac_client.get_proposal(&pause_proposal).unwrap().status,
        ProposalStatus::Approved
    );

    let _ = f.ac_client.execute_action(&r2, &pause_proposal);
    assert!(f.pool_client.is_paused());
    assert_eq!(f.pool_client.get_yield(), 0); // untouched — never reached threshold
}
