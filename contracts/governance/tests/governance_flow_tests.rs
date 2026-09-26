#![cfg(test)]

//! Tests for the governance proposal → vote → timelock → execute cycle,
//! including that `execute_proposal` actually invokes the target contract's
//! `*_via_governance` setter (issue #1119) rather than merely emitting an
//! event for an off-chain relayer to act on.
//!
//! Uses `common::MockTarget` rather than the real pool/invoice/
//! oracle_registry/compliance contracts — those are independently broken on
//! `main` for reasons unrelated to governance (see tests/common/mod.rs for
//! details), so depending on them here would make this suite hostage to
//! bugs in other crates.

mod common;

use common::MockTargetClient;
use governance::{
    Governance, GovernanceAction, GovernanceClient, GovernanceError, OracleRegistryAction,
    PoolAction, ProposalCategory, ProposalStatus,
};
use share::{ShareToken, ShareTokenClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

#[allow(dead_code)]
struct Fixture {
    env: Env,
    governance_client: GovernanceClient<'static>,
    governance_id: Address,
    target_id: Address,
    target_client: MockTargetClient<'static>,
    admin: Address,
    voter: Address,
}

const VOTING_PERIOD: u64 = 86_400;
const EXEC_DELAY: u64 = 86_400;

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let voter = Address::generate(&env);

    let share_id = env.register(ShareToken, ());
    let share_client = ShareTokenClient::new(&env, &share_id);
    share_client.initialize(
        &admin,
        &7u32,
        &String::from_str(&env, "Pool Shares"),
        &String::from_str(&env, "POOL"),
    );
    // Admin needs a stake to create proposals; voter holds a supermajority
    // so every proposal below clears quorum/pass.
    share_client.mint(&admin, &1_000i128);
    share_client.mint(&voter, &1_000_000i128);

    let governance_id = env.register(Governance, ());
    let governance_client = GovernanceClient::new(&env, &governance_id);
    governance_client.initialize(
        &admin,
        &share_id,
        &VOTING_PERIOD,
        &1_000u32, // 10% quorum
        &6_000u32, // 60% pass
        &EXEC_DELAY,
        &1i128,
    );

    let target_id = env.register(common::MockTarget, ());
    let target_client = MockTargetClient::new(&env, &target_id);

    Fixture {
        env,
        governance_client,
        governance_id,
        target_id,
        target_client,
        admin,
        voter,
    }
}

fn pass_and_advance(f: &Fixture, proposal_id: u64) {
    f.governance_client.vote(&proposal_id, &f.voter, &true);
    f.env
        .ledger()
        .with_mut(|l| l.timestamp += VOTING_PERIOD + EXEC_DELAY + 2);
}

// ── Basic Governance Flow Tests ─────────────────────────────────────────────

#[test]
fn test_create_proposal() {
    let f = setup();

    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &String::from_str(&f.env, "Test proposal"),
        &f.target_id,
        &GovernanceAction::Pool(PoolAction::SetPoolYield(1500u32)),
        &ProposalCategory::ParameterChange,
    );

    let proposal = f.governance_client.get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.proposer, f.admin);
    assert_eq!(proposal.target_contract, f.target_id);
}

#[test]
fn test_vote_on_proposal() {
    let f = setup();

    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &String::from_str(&f.env, "Test proposal"),
        &f.target_id,
        &GovernanceAction::Pool(PoolAction::SetPoolYield(1500u32)),
        &ProposalCategory::ParameterChange,
    );

    f.governance_client.vote(&proposal_id, &f.voter, &true);

    let proposal = f.governance_client.get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.votes_for, 1_000_000i128);
    assert_eq!(proposal.votes_against, 0i128);
}

#[test]
fn test_execute_proposal_after_timelock() {
    let f = setup();

    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &String::from_str(&f.env, "Test proposal"),
        &f.target_id,
        &GovernanceAction::Pool(PoolAction::SetPoolYield(1500u32)),
        &ProposalCategory::ParameterChange,
    );

    pass_and_advance(&f, proposal_id);
    f.governance_client.execute_proposal(&proposal_id);

    let proposal = f.governance_client.get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Executed);
}

// ── execute_proposal actually invokes the target contract (#1119) ──────────

#[test]
fn test_execute_proposal_invokes_pool_style_setter() {
    let f = setup();

    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &String::from_str(&f.env, "Update yield"),
        &f.target_id,
        &GovernanceAction::Pool(PoolAction::SetPoolYield(1500u32)),
        &ProposalCategory::ParameterChange,
    );

    pass_and_advance(&f, proposal_id);
    f.governance_client.execute_proposal(&proposal_id);

    // The target's own state changed — proof the contract was actually
    // called, not just that governance recorded an "execute" event.
    assert_eq!(f.target_client.get_yield(), 1500u32);
}

#[test]
fn test_execute_proposal_invokes_oracle_registry_style_setter() {
    let f = setup();
    let new_treasury = Address::generate(&f.env);

    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &String::from_str(&f.env, "Update treasury"),
        &f.target_id,
        &GovernanceAction::OracleRegistry(OracleRegistryAction::SetOracleRegistryTreasury(Some(
            new_treasury.clone(),
        ))),
        &ProposalCategory::ParameterChange,
    );

    pass_and_advance(&f, proposal_id);
    f.governance_client.execute_proposal(&proposal_id);

    assert_eq!(f.target_client.get_treasury(), Some(new_treasury));
}

// ── Governance Gating Tests ─────────────────────────────────────────────────

#[test]
fn test_execute_proposal_rejects_before_quorum_finalizes() {
    let f = setup();

    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &String::from_str(&f.env, "Test proposal"),
        &f.target_id,
        &GovernanceAction::Pool(PoolAction::SetPoolYield(1500u32)),
        &ProposalCategory::ParameterChange,
    );

    // Nobody votes — quorum can never be met.
    f.env
        .ledger()
        .with_mut(|l| l.timestamp += VOTING_PERIOD + EXEC_DELAY + 2);

    let result = f.governance_client.try_execute_proposal(&proposal_id);
    assert_eq!(result.unwrap_err().unwrap(), GovernanceError::QuorumNotMet);
    // The target must be untouched — execution never reached invoke_contract.
    assert_eq!(f.target_client.get_yield(), 0u32);
}

// ── Quorum and Pass Threshold Tests ─────────────────────────────────────────

#[test]
fn test_proposal_rejected_when_quorum_not_met() {
    let f = setup();

    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &String::from_str(&f.env, "Test proposal"),
        &f.target_id,
        &GovernanceAction::Pool(PoolAction::SetPoolYield(1500u32)),
        &ProposalCategory::ParameterChange,
    );

    // A tiny holder votes — nowhere near the 10% quorum of the 1,000,000 supply.
    let small_voter = Address::generate(&f.env);
    let share_client = ShareTokenClient::new(&f.env, &f.governance_client.get_config().share_token);
    share_client.mint(&small_voter, &100i128);
    f.governance_client.vote(&proposal_id, &small_voter, &true);

    f.env
        .ledger()
        .with_mut(|l| l.timestamp += VOTING_PERIOD + EXEC_DELAY + 2);

    let result = f.governance_client.try_execute_proposal(&proposal_id);
    assert_eq!(result.unwrap_err().unwrap(), GovernanceError::QuorumNotMet);
}

#[test]
fn test_proposal_cannot_execute_before_timelock() {
    let f = setup();

    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &String::from_str(&f.env, "Test proposal"),
        &f.target_id,
        &GovernanceAction::Pool(PoolAction::SetPoolYield(1500u32)),
        &ProposalCategory::ParameterChange,
    );

    f.governance_client.vote(&proposal_id, &f.voter, &true);

    // Advance past the voting period, but not past the execution delay.
    f.env
        .ledger()
        .with_mut(|l| l.timestamp += VOTING_PERIOD + 1);
    let result = f.governance_client.try_execute_proposal(&proposal_id);
    assert_eq!(
        result.unwrap_err().unwrap(),
        GovernanceError::TimelockActive
    );
}
