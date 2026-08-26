#![cfg(test)]

mod common;

use common::MockTarget;
use governance::{
    Governance, GovernanceAction, GovernanceClient, GovernanceError, PoolAction, ProposalCategory,
    ProposalStatus,
};
use share::{ShareToken, ShareTokenClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

const VOTING_PERIOD: u64 = 86_400; // 1 day minimum
const EXEC_DELAY: u64 = 100;
const QUORUM_BPS: u32 = 1_000; // 10 %
const PASS_BPS: u32 = 6_000; // 60 %
const MIN_SHARE_BALANCE: i128 = 1;

fn setup_share(env: &Env) -> (ShareTokenClient<'_>, Address, Address) {
    let share_admin = Address::generate(env);
    let contract_id = env.register(ShareToken, ());
    let client = ShareTokenClient::new(env, &contract_id);
    client.initialize(
        &share_admin,
        &7u32,
        &String::from_str(env, "Pool Shares"),
        &String::from_str(env, "POOL"),
    );
    (client, contract_id, share_admin)
}

/// Registers governance plus a real (minimal) target contract, so tests
/// exercising `execute_proposal` have something governance can actually
/// invoke — execution now performs a real cross-contract call rather than
/// only emitting an event (#1119), so a placeholder / non-contract address
/// as the proposal target would trap.
fn setup_governance<'a>(
    env: &'a Env,
    share_id: &Address,
) -> (GovernanceClient<'a>, Address, Address) {
    let gov_admin = Address::generate(env);
    let gov_id = env.register(Governance, ());
    let client = GovernanceClient::new(env, &gov_id);
    client.initialize(
        &gov_admin,
        share_id,
        &VOTING_PERIOD,
        &QUORUM_BPS,
        &PASS_BPS,
        &EXEC_DELAY,
        &MIN_SHARE_BALANCE,
    );

    let target_id = env.register(MockTarget, ());

    (client, gov_admin, target_id)
}

fn placeholder_action() -> GovernanceAction {
    GovernanceAction::Pool(PoolAction::SetPoolYield(1500))
}

fn make_proposal(env: &Env, gov: &GovernanceClient, proposer: &Address, target: &Address) -> u64 {
    gov.create_proposal(
        proposer,
        &String::from_str(env, "Test proposal"),
        target,
        &placeholder_action(),
        &ProposalCategory::ParameterChange,
    )
}

fn make_proposal_with_category(
    env: &Env,
    gov: &GovernanceClient,
    proposer: &Address,
    target: &Address,
    category: &ProposalCategory,
) -> u64 {
    gov.create_proposal(
        proposer,
        &String::from_str(env, "Test proposal"),
        target,
        &placeholder_action(),
        category,
    )
}

// ── snapshot captured at creation ────────────────────────────────────────────

#[test]
fn test_snapshot_supply_recorded_at_creation() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, _share_admin) = setup_share(&env);
    let (gov, _, target_id) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    share.mint(&proposer, &1_000_000i128);

    let id = make_proposal(&env, &gov, &proposer, &target_id);
    let proposal = gov.get_proposal(&id).unwrap();

    assert_eq!(proposal.snapshot_supply, 1_000_000i128);
}

#[test]
fn test_snapshot_supply_does_not_reflect_later_mints() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, _share_admin) = setup_share(&env);
    let (gov, _, target_id) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    share.mint(&proposer, &500_000i128);

    let id = make_proposal(&env, &gov, &proposer, &target_id);

    // Mint more shares after proposal creation
    share.mint(&proposer, &4_500_000i128);
    assert_eq!(share.total_supply(), 5_000_000i128);

    let proposal = gov.get_proposal(&id).unwrap();
    // Snapshot must reflect the supply at creation time, not the live supply
    assert_eq!(proposal.snapshot_supply, 500_000i128);
}

// ── core attack scenario ──────────────────────────────────────────────────────

/// Reproduces the attack described in issue #569:
///
/// 1. Supply at proposal creation = 1_000_000 → quorum threshold = 100_000
/// 2. Legitimate voters cast 105_000 YES votes (quorum met)
/// 3. Admin mints 2_000_000 shares *after* voting closes (supply → 3_000_000)
/// 4. With the old code the live-supply check raises quorum to 300_000, failing
///    a proposal that had legitimately passed. The fix locks quorum to the
///    creation-time snapshot so the proposal is correctly marked Passed.
#[test]
fn test_post_creation_minting_cannot_suppress_passing_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, share_admin) = setup_share(&env);
    let (gov, _, target_id) = setup_governance(&env, &share_id);

    // Mint initial supply and distribute voting power
    let proposer = Address::generate(&env);
    let voter_a = Address::generate(&env);
    let voter_b = Address::generate(&env);

    share.mint(&proposer, &1_000i128);
    share.mint(&voter_a, &80_000i128);
    share.mint(&voter_b, &25_000i128);
    // Total supply at creation = 1_000 + 80_000 + 25_000 = 106_000
    // Quorum (10%) = 10_600

    let id = make_proposal(&env, &gov, &proposer, &target_id);

    // Both voters vote YES (105_000 total > quorum of 10_600)
    gov.vote(&id, &voter_a, &true);
    gov.vote(&id, &voter_b, &true);

    // Advance past voting window
    env.ledger().with_mut(|l| l.timestamp += VOTING_PERIOD + 1);

    // ── Attack: admin mints 2_000_000 shares after voting closed ────────────
    share.mint(&share_admin, &2_000_000i128);
    // Live total supply is now 2_106_000; if quorum used live supply,
    // threshold = 210_600 which would make 105_000 votes fail.

    // Advance past execution delay
    env.ledger().with_mut(|l| l.timestamp += EXEC_DELAY + 1);

    // With the snapshot fix the proposal must pass
    gov.execute_proposal(&id);
    let proposal = gov.get_proposal(&id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Executed);
}

#[test]
fn test_post_creation_minting_cannot_manufacture_quorum() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, _share_admin) = setup_share(&env);
    let (gov, _, target_id) = setup_governance(&env, &share_id);

    // Large initial supply so the two voter balances are well below quorum
    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);

    share.mint(&proposer, &1_000_000i128);
    share.mint(&voter, &50_000i128);
    // Supply at creation = 1_050_000; quorum (10%) = 105_000
    // voter only has 50_000 — below quorum

    let id = make_proposal(&env, &gov, &proposer, &target_id);

    gov.vote(&id, &voter, &true);

    // Mint new shares to push live total supply *down* (can't — supply only grows)
    // Instead verify that even without additional minting the quorum is correctly
    // not met when actual votes < snapshot quorum threshold.

    env.ledger()
        .with_mut(|l| l.timestamp += VOTING_PERIOD + EXEC_DELAY + 2);

    let result = gov.try_execute_proposal(&id);
    assert!(result.is_err(), "proposal below quorum must be rejected");

    // execute_proposal rolls back storage on error; list_proposals commits finalization
    gov.list_proposals();
    let proposal = gov.get_proposal(&id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Rejected);
}

// ── normal voting flow still works ───────────────────────────────────────────

#[test]
fn test_proposal_passes_when_quorum_and_threshold_met() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, _share_admin) = setup_share(&env);
    let (gov, _, target_id) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);

    share.mint(&proposer, &1_000i128);
    share.mint(&voter, &200_000i128);
    // Supply = 201_000; quorum (10%) = 20_100; voter has 200_000 > 20_100

    let id = make_proposal(&env, &gov, &proposer, &target_id);
    gov.vote(&id, &voter, &true);

    env.ledger()
        .with_mut(|l| l.timestamp += VOTING_PERIOD + EXEC_DELAY + 2);

    gov.execute_proposal(&id);
    let proposal = gov.get_proposal(&id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Executed);
}

#[test]
fn test_proposal_rejected_when_quorum_not_met() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, _share_admin) = setup_share(&env);
    let (gov, _, target_id) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    let small_voter = Address::generate(&env);

    share.mint(&proposer, &900_000i128);
    share.mint(&small_voter, &1_000i128);
    // Supply = 901_000; quorum = 90_100; small_voter has 1_000 — way below

    let id = make_proposal(&env, &gov, &proposer, &target_id);
    gov.vote(&id, &small_voter, &true);

    env.ledger()
        .with_mut(|l| l.timestamp += VOTING_PERIOD + EXEC_DELAY + 2);

    let result = gov.try_execute_proposal(&id);
    assert!(result.is_err());

    gov.list_proposals();
    let proposal = gov.get_proposal(&id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Rejected);
}

#[test]
fn test_proposal_rejected_when_pass_threshold_not_met() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, _share_admin) = setup_share(&env);
    let (gov, _, target_id) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    let yes_voter = Address::generate(&env);
    let no_voter = Address::generate(&env);

    share.mint(&proposer, &1_000i128);
    share.mint(&yes_voter, &100_000i128);
    share.mint(&no_voter, &100_000i128);
    // Quorum (10%) = 20_100; total votes cast = 200_000 ✓
    // YES = 100_000, NO = 100_000 → 50% YES < 60% threshold → Rejected

    let id = make_proposal(&env, &gov, &proposer, &target_id);
    gov.vote(&id, &yes_voter, &true);
    gov.vote(&id, &no_voter, &false);

    env.ledger()
        .with_mut(|l| l.timestamp += VOTING_PERIOD + EXEC_DELAY + 2);

    let result = gov.try_execute_proposal(&id);
    assert!(result.is_err());

    gov.list_proposals();
    let proposal = gov.get_proposal(&id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Rejected);
}

// ── edge cases ────────────────────────────────────────────────────────────────

#[test]
fn test_zero_quorum_supply_of_one_still_passes() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share2, share_id2, _share_admin2) = setup_share(&env);
    let (gov2, _, target_id2) = setup_governance(&env, &share_id2);

    // Mint a share AFTER governance is initialised but BEFORE creating proposal — supply = 0 at
    // proposal creation time is impossible because proposer needs min_share_balance ≥ 1.
    // So min non-zero snapshot is 1. Verify quorum = 0 means any vote count passes quorum.
    let proposer2 = Address::generate(&env);
    share2.mint(&proposer2, &1i128); // supply = 1 at creation; quorum = 1*1000/10000 = 0

    let id = gov2.create_proposal(
        &proposer2,
        &String::from_str(&env, "zero-quorum proposal"),
        &target_id2,
        &placeholder_action(),
        &ProposalCategory::ParameterChange,
    );
    // No votes cast → total_votes = 0. quorum = 0 so 0 >= 0 passes quorum check.
    // Then pass threshold: YES=0, total=0. 0*10000 >= 0*6000 → 0 >= 0 → Passed.
    env.ledger()
        .with_mut(|l| l.timestamp += VOTING_PERIOD + EXEC_DELAY + 2);
    gov2.execute_proposal(&id);
    let p = gov2.get_proposal(&id).unwrap();
    assert_eq!(p.snapshot_supply, 1i128);
    assert_eq!(p.status, ProposalStatus::Executed);
}

#[test]
fn test_cannot_vote_after_voting_period_ends() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, _share_admin) = setup_share(&env);
    let (gov, _, target_id) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    share.mint(&proposer, &1_000i128);
    share.mint(&voter, &100_000i128);

    let id = make_proposal(&env, &gov, &proposer, &target_id);

    // Advance past voting window before voting
    env.ledger().with_mut(|l| l.timestamp += VOTING_PERIOD + 1);

    let result = gov.try_vote(&id, &voter, &true);
    assert!(result.is_err(), "voting after period must fail");
}

#[test]
fn test_cannot_vote_twice() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, _share_admin) = setup_share(&env);
    let (gov, _, target_id) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    share.mint(&proposer, &1_000i128);
    share.mint(&voter, &100_000i128);

    let id = make_proposal(&env, &gov, &proposer, &target_id);

    gov.vote(&id, &voter, &true);
    let result = gov.try_vote(&id, &voter, &false);
    assert!(result.is_err(), "double vote must fail");
}

// ── cancel_proposal (#1118: proposer must not be able to veto a Passed
//    proposal unilaterally — only Active proposals are proposer-cancellable) ─

#[test]
fn test_cancel_proposal_by_proposer_while_active() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, _share_admin) = setup_share(&env);
    let (gov, _, target_id) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    share.mint(&proposer, &10_000i128);

    let id = make_proposal(&env, &gov, &proposer, &target_id);
    gov.cancel_proposal(&id, &proposer);

    let proposal = gov.get_proposal(&id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Cancelled);
}

#[test]
fn test_proposer_cannot_cancel_passed_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, _share_admin) = setup_share(&env);
    let (gov, _, target_id) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    share.mint(&proposer, &1_000i128);
    share.mint(&voter, &200_000i128);

    let id = make_proposal(&env, &gov, &proposer, &target_id);
    gov.vote(&id, &voter, &true);

    // Advance past the voting period so the proposal finalizes to Passed
    // (finalization happens lazily on the next touch), then attempt an
    // early proposer cancellation before execution/timelock.
    env.ledger().with_mut(|l| l.timestamp += VOTING_PERIOD + 1);
    gov.list_proposals();
    let proposal = gov.get_proposal(&id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Passed);

    // The original proposer alone must not be able to veto a Passed proposal
    // — that would let a single voter unilaterally block an approved change
    // during the timelock window.
    let result = gov.try_cancel_proposal(&id, &proposer);
    assert_eq!(result, Err(Ok(GovernanceError::Unauthorized)));

    let proposal = gov.get_proposal(&id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Passed);
}

#[test]
fn test_admin_can_cancel_passed_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, _share_admin) = setup_share(&env);
    let (gov, gov_admin, target_id) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    share.mint(&proposer, &1_000i128);
    share.mint(&voter, &200_000i128);

    let id = make_proposal(&env, &gov, &proposer, &target_id);
    gov.vote(&id, &voter, &true);

    env.ledger().with_mut(|l| l.timestamp += VOTING_PERIOD + 1);
    gov.list_proposals();
    assert_eq!(
        gov.get_proposal(&id).unwrap().status,
        ProposalStatus::Passed
    );

    // The admin retains the ability to cancel a Passed proposal (e.g. to halt
    // an exploit discovered during the timelock window).
    gov.cancel_proposal(&id, &gov_admin);
    assert_eq!(
        gov.get_proposal(&id).unwrap().status,
        ProposalStatus::Cancelled
    );
}

// ── vote weight is snapshotted at proposal creation ──────────────────────────

#[test]
fn test_vote_weight_ignores_shares_acquired_after_proposal_creation() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, _share_admin) = setup_share(&env);
    let (gov, _, target_id) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    let latecomer = Address::generate(&env);
    share.mint(&proposer, &1_000_000i128);

    let id = make_proposal(&env, &gov, &proposer, &target_id);

    // latecomer acquires a large share balance only after the proposal was
    // created (in a later ledger) — this must not count toward voting weight.
    env.ledger().with_mut(|l| l.timestamp += 10);
    share.mint(&latecomer, &5_000_000i128);
    let result = gov.try_vote(&id, &latecomer, &true);
    assert!(
        result.is_err(),
        "voter with zero balance at snapshot time must not be able to vote"
    );
}

#[test]
fn test_vote_weight_uses_balance_at_creation_not_at_vote_time() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, _share_admin) = setup_share(&env);
    let (gov, _, target_id) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    share.mint(&proposer, &1_000i128);
    share.mint(&voter, &50_000i128);
    // Supply at creation = 51_000; quorum (10%) = 5_100

    let id = make_proposal(&env, &gov, &proposer, &target_id);

    // Voter transfers away most of their shares after the snapshot (in a
    // later ledger) but before voting — their voting weight must still
    // reflect the creation-time balance (50_000), not the post-transfer live
    // balance.
    env.ledger().with_mut(|l| l.timestamp += 10);
    let stranger = Address::generate(&env);
    share.transfer(&voter, &stranger, &49_000i128);
    assert_eq!(share.balance(&voter), 1_000);

    gov.vote(&id, &voter, &true);

    env.ledger()
        .with_mut(|l| l.timestamp += VOTING_PERIOD + EXEC_DELAY + 2);
    gov.execute_proposal(&id);

    let proposal = gov.get_proposal(&id).unwrap();
    assert_eq!(proposal.votes_for, 50_000i128);
    assert_eq!(proposal.status, ProposalStatus::Executed);
}

// ── governance-configurable quorum / pass threshold ──────────────────────────

#[test]
fn test_update_config_by_admin_changes_quorum_and_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (_share, share_id, _share_admin) = setup_share(&env);
    let (gov, gov_admin, _target_id) = setup_governance(&env, &share_id);

    gov.update_config(&gov_admin, &2_000u32, &5_500u32);

    let config = gov.get_config();
    assert_eq!(config.quorum_bps, 2_000);
    assert_eq!(config.pass_bps, 5_500);
}

#[test]
fn test_update_config_rejects_non_admin_caller() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (_share, share_id, _share_admin) = setup_share(&env);
    let (gov, _gov_admin, _target_id) = setup_governance(&env, &share_id);

    let impostor = Address::generate(&env);
    let result = gov.try_update_config(&impostor, &2_000u32, &5_500u32);
    assert!(result.is_err());
}

#[test]
fn test_update_config_rejects_invalid_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (_share, share_id, _share_admin) = setup_share(&env);
    let (gov, gov_admin, _target_id) = setup_governance(&env, &share_id);

    // pass_bps must be > 5_000
    let result = gov.try_update_config(&gov_admin, &2_000u32, &5_000u32);
    assert!(result.is_err());
}

// ── #1121: min_share_balance is updatable after initialize ──────────────────

#[test]
fn test_update_min_share_balance_by_admin() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, _share_admin) = setup_share(&env);
    let (gov, gov_admin, target_id) = setup_governance(&env, &share_id);

    gov.update_min_share_balance(&gov_admin, &10_000i128);
    assert_eq!(gov.get_config().min_share_balance, 10_000i128);

    // The new threshold is enforced immediately for subsequent proposals.
    let proposer = Address::generate(&env);
    share.mint(&proposer, &9_999i128);
    let result = gov.try_create_proposal(
        &proposer,
        &String::from_str(&env, "under new threshold"),
        &target_id,
        &placeholder_action(),
        &ProposalCategory::ParameterChange,
    );
    assert_eq!(result, Err(Ok(GovernanceError::InsufficientShareBalance)));
}

#[test]
fn test_update_min_share_balance_rejects_non_admin_caller() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (_share, share_id, _share_admin) = setup_share(&env);
    let (gov, _gov_admin, _target_id) = setup_governance(&env, &share_id);

    let impostor = Address::generate(&env);
    let result = gov.try_update_min_share_balance(&impostor, &10_000i128);
    assert_eq!(result, Err(Ok(GovernanceError::Unauthorized)));
}

#[test]
fn test_update_min_share_balance_rejects_non_positive_value() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (_share, share_id, _share_admin) = setup_share(&env);
    let (gov, gov_admin, _target_id) = setup_governance(&env, &share_id);

    let result = gov.try_update_min_share_balance(&gov_admin, &0i128);
    assert_eq!(result, Err(Ok(GovernanceError::InvalidConfig)));

    let result = gov.try_update_min_share_balance(&gov_admin, &-1i128);
    assert_eq!(result, Err(Ok(GovernanceError::InvalidConfig)));
}

// ── passed proposals expire if not executed in time ──────────────────────────

#[test]
fn test_passed_proposal_expires_if_not_executed_within_seven_days() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, _share_admin) = setup_share(&env);
    let (gov, _, target_id) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    share.mint(&proposer, &1_000i128);
    share.mint(&voter, &200_000i128);

    let id = make_proposal(&env, &gov, &proposer, &target_id);
    gov.vote(&id, &voter, &true);

    // Past voting period + execution delay, proposal is Passed but not yet executed.
    env.ledger()
        .with_mut(|l| l.timestamp += VOTING_PERIOD + EXEC_DELAY + 2);
    gov.list_proposals();
    let proposal = gov.get_proposal(&id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Passed);

    // Advance 7 days past passing without executing.
    env.ledger().with_mut(|l| l.timestamp += 7 * 86_400 + 1);

    let result = gov.try_execute_proposal(&id);
    assert!(result.is_err(), "expired proposal must not be executable");

    gov.list_proposals();
    let proposal = gov.get_proposal(&id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Expired);
}

#[test]
fn test_passed_proposal_executes_within_expiry_window() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, _share_admin) = setup_share(&env);
    let (gov, _, target_id) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    share.mint(&proposer, &1_000i128);
    share.mint(&voter, &200_000i128);

    let id = make_proposal(&env, &gov, &proposer, &target_id);
    gov.vote(&id, &voter, &true);

    env.ledger()
        .with_mut(|l| l.timestamp += VOTING_PERIOD + EXEC_DELAY + 2);

    // Executed comfortably within the 7-day expiry window.
    env.ledger().with_mut(|l| l.timestamp += 3 * 86_400);
    gov.execute_proposal(&id);

    let proposal = gov.get_proposal(&id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Executed);
}

// ── #932: voting period must fully elapse before execute ─────────────────────

#[test]
fn test_execute_rejected_at_exact_voting_ends_at() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, _) = setup_share(&env);
    let (gov, _, target_id) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    share.mint(&proposer, &1_000i128);
    share.mint(&voter, &200_000i128);

    let id = make_proposal(&env, &gov, &proposer, &target_id);
    gov.vote(&id, &voter, &true);

    let proposal = gov.get_proposal(&id).unwrap();
    // Land exactly on voting_ends_at — period has not fully elapsed past the end.
    env.ledger()
        .with_mut(|l| l.timestamp = proposal.voting_ends_at);

    let result = gov.try_execute_proposal(&id);
    assert_eq!(
        result,
        Err(Ok(GovernanceError::VotingPeriodActive)),
        "execute at voting_ends_at must fail even with unanimous support"
    );

    // Vote at equality must also fail (no overlap window).
    let late_voter = Address::generate(&env);
    share.mint(&late_voter, &10_000i128);
    let vote_result = gov.try_vote(&id, &late_voter, &true);
    assert!(vote_result.is_err(), "vote at voting_ends_at must fail");
}

#[test]
fn test_execute_requires_voting_period_elapsed_before_timelock() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, _) = setup_share(&env);
    let (gov, _, target_id) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    share.mint(&proposer, &1_000i128);
    share.mint(&voter, &200_000i128);

    let id = make_proposal(&env, &gov, &proposer, &target_id);
    gov.vote(&id, &voter, &true);

    // Still inside voting window.
    env.ledger().with_mut(|l| l.timestamp += VOTING_PERIOD / 2);
    let early = gov.try_execute_proposal(&id);
    assert_eq!(early, Err(Ok(GovernanceError::VotingPeriodActive)));

    // One second after voting ends — voting closed, but execution delay still active.
    let proposal = gov.get_proposal(&id).unwrap();
    env.ledger()
        .with_mut(|l| l.timestamp = proposal.voting_ends_at + 1);
    let delay = gov.try_execute_proposal(&id);
    assert_eq!(delay, Err(Ok(GovernanceError::TimelockActive)));
}

// ── #931: proposal threshold / create eligibility ────────────────────────────

#[test]
fn test_create_proposal_rejects_zero_balance() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, _) = setup_share(&env);
    let (gov, _, target_id) = setup_governance(&env, &share_id);

    // Ensure supply is non-zero via another holder, but proposer has nothing.
    let holder = Address::generate(&env);
    share.mint(&holder, &100_000i128);
    let poor = Address::generate(&env);

    let result = gov.try_create_proposal(
        &poor,
        &String::from_str(&env, "spam"),
        &target_id,
        &placeholder_action(),
        &ProposalCategory::ParameterChange,
    );
    assert_eq!(result, Err(Ok(GovernanceError::InsufficientShareBalance)));
}

#[test]
fn test_create_proposal_rejects_below_min_share_balance() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, _share_admin) = setup_share(&env);
    let gov_admin = Address::generate(&env);
    let gov_id = env.register(Governance, ());
    let gov = GovernanceClient::new(&env, &gov_id);
    // Threshold of 10_000 shares to create.
    gov.initialize(
        &gov_admin,
        &share_id,
        &VOTING_PERIOD,
        &QUORUM_BPS,
        &PASS_BPS,
        &EXEC_DELAY,
        &10_000i128,
    );

    let target_id = env.register(MockTarget, ());

    let proposer = Address::generate(&env);
    share.mint(&proposer, &9_999i128);

    let result = gov.try_create_proposal(
        &proposer,
        &String::from_str(&env, "under threshold"),
        &target_id,
        &placeholder_action(),
        &ProposalCategory::ParameterChange,
    );
    assert_eq!(result, Err(Ok(GovernanceError::InsufficientShareBalance)));

    share.mint(&proposer, &1i128); // now exactly 10_000
    let id = gov.create_proposal(
        &proposer,
        &String::from_str(&env, "at threshold"),
        &target_id,
        &placeholder_action(),
        &ProposalCategory::ParameterChange,
    );
    assert_eq!(id, 1u64);
}

// ── #933: proposal categories with per-category quorum ───────────────────────

#[test]
fn test_category_quorum_defaults_and_snapshot() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, _) = setup_share(&env);
    let (gov, gov_admin, target_id) = setup_governance(&env, &share_id);

    assert_eq!(
        gov.get_category_quorum(&ProposalCategory::ParameterChange),
        QUORUM_BPS
    );
    assert_eq!(gov.get_category_quorum(&ProposalCategory::Treasury), 2_000);
    assert_eq!(gov.get_category_quorum(&ProposalCategory::Critical), 5_000);

    let proposer = Address::generate(&env);
    share.mint(&proposer, &1_000_000i128);

    let param_id = make_proposal_with_category(
        &env,
        &gov,
        &proposer,
        &target_id,
        &ProposalCategory::ParameterChange,
    );
    let treasury_id = make_proposal_with_category(
        &env,
        &gov,
        &proposer,
        &target_id,
        &ProposalCategory::Treasury,
    );
    let critical_id = make_proposal_with_category(
        &env,
        &gov,
        &proposer,
        &target_id,
        &ProposalCategory::Critical,
    );

    assert_eq!(gov.get_proposal(&param_id).unwrap().quorum_bps, QUORUM_BPS);
    assert_eq!(gov.get_proposal(&treasury_id).unwrap().quorum_bps, 2_000);
    assert_eq!(gov.get_proposal(&critical_id).unwrap().quorum_bps, 5_000);
    assert_eq!(
        gov.get_proposal(&critical_id).unwrap().category,
        ProposalCategory::Critical
    );

    // Mid-flight config change must not rewrite snapshotted quorum.
    gov.set_category_quorum(&gov_admin, &ProposalCategory::Critical, &8_000u32);
    assert_eq!(gov.get_category_quorum(&ProposalCategory::Critical), 8_000);
    assert_eq!(
        gov.get_proposal(&critical_id).unwrap().quorum_bps,
        5_000,
        "in-flight proposal keeps creation-time quorum"
    );

    let new_critical = make_proposal_with_category(
        &env,
        &gov,
        &proposer,
        &target_id,
        &ProposalCategory::Critical,
    );
    assert_eq!(gov.get_proposal(&new_critical).unwrap().quorum_bps, 8_000);
}

#[test]
fn test_critical_category_requires_higher_quorum() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, _) = setup_share(&env);
    let (gov, _, target_id) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    // Supply = 1_000_000. Parameter quorum 10% = 100_000; Critical 50% = 500_000.
    share.mint(&proposer, &800_000i128);
    share.mint(&voter, &200_000i128);

    let param_id = make_proposal_with_category(
        &env,
        &gov,
        &proposer,
        &target_id,
        &ProposalCategory::ParameterChange,
    );
    let critical_id = make_proposal_with_category(
        &env,
        &gov,
        &proposer,
        &target_id,
        &ProposalCategory::Critical,
    );

    // 200k votes: enough for parameter (100k), not for critical (500k).
    gov.vote(&param_id, &voter, &true);
    gov.vote(&critical_id, &voter, &true);

    env.ledger()
        .with_mut(|l| l.timestamp += VOTING_PERIOD + EXEC_DELAY + 2);

    gov.execute_proposal(&param_id);
    assert_eq!(
        gov.get_proposal(&param_id).unwrap().status,
        ProposalStatus::Executed
    );

    let critical_result = gov.try_execute_proposal(&critical_id);
    assert!(critical_result.is_err());
    gov.list_proposals();
    assert_eq!(
        gov.get_proposal(&critical_id).unwrap().status,
        ProposalStatus::Rejected
    );
}

#[test]
fn test_set_category_quorum_rejects_non_admin_and_invalid() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (_share, share_id, _) = setup_share(&env);
    let (gov, gov_admin, _target_id) = setup_governance(&env, &share_id);

    let impostor = Address::generate(&env);
    assert!(gov
        .try_set_category_quorum(&impostor, &ProposalCategory::Treasury, &3_000u32)
        .is_err());
    assert!(gov
        .try_set_category_quorum(&gov_admin, &ProposalCategory::Treasury, &0u32)
        .is_err());
    assert!(gov
        .try_set_category_quorum(&gov_admin, &ProposalCategory::Treasury, &10_001u32)
        .is_err());
}
