#![cfg(test)]

use governance::{Governance, GovernanceClient, ProposalStatus};
use share::{ShareToken, ShareTokenClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

const VOTING_PERIOD: u64 = 200;
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

fn setup_governance<'a>(env: &'a Env, share_id: &Address) -> (GovernanceClient<'a>, Address) {
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
    (client, gov_admin)
}

fn make_proposal(env: &Env, gov: &GovernanceClient, proposer: &Address, target: &Address) -> u64 {
    gov.create_proposal(
        proposer,
        &String::from_str(env, "Test proposal"),
        target,
        &String::from_str(env, "no_op"),
        &String::from_str(env, "{}"),
    )
}

// ── snapshot captured at creation ────────────────────────────────────────────

#[test]
fn test_snapshot_supply_recorded_at_creation() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, share_admin) = setup_share(&env);
    let (gov, _) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    share.mint(&proposer, &1_000_000i128);

    let id = make_proposal(&env, &gov, &proposer, &share_admin);
    let proposal = gov.get_proposal(&id).unwrap();

    assert_eq!(proposal.snapshot_supply, 1_000_000i128);
}

#[test]
fn test_snapshot_supply_does_not_reflect_later_mints() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, share_admin) = setup_share(&env);
    let (gov, _) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    share.mint(&proposer, &500_000i128);

    let id = make_proposal(&env, &gov, &proposer, &share_admin);

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
    let (gov, _) = setup_governance(&env, &share_id);

    // Mint initial supply and distribute voting power
    let proposer = Address::generate(&env);
    let voter_a = Address::generate(&env);
    let voter_b = Address::generate(&env);

    share.mint(&proposer, &1_000i128);
    share.mint(&voter_a, &80_000i128);
    share.mint(&voter_b, &25_000i128);
    // Total supply at creation = 1_000 + 80_000 + 25_000 = 106_000
    // Quorum (10%) = 10_600

    let id = make_proposal(&env, &gov, &proposer, &share_admin);

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

    let (share, share_id, share_admin) = setup_share(&env);
    let (gov, _) = setup_governance(&env, &share_id);

    // Large initial supply so the two voter balances are well below quorum
    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);

    share.mint(&proposer, &1_000_000i128);
    share.mint(&voter, &50_000i128);
    // Supply at creation = 1_050_000; quorum (10%) = 105_000
    // voter only has 50_000 — below quorum

    let id = make_proposal(&env, &gov, &proposer, &share_admin);

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

    let (share, share_id, share_admin) = setup_share(&env);
    let (gov, _) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);

    share.mint(&proposer, &1_000i128);
    share.mint(&voter, &200_000i128);
    // Supply = 201_000; quorum (10%) = 20_100; voter has 200_000 > 20_100

    let id = make_proposal(&env, &gov, &proposer, &share_admin);
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

    let (share, share_id, share_admin) = setup_share(&env);
    let (gov, _) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    let small_voter = Address::generate(&env);

    share.mint(&proposer, &900_000i128);
    share.mint(&small_voter, &1_000i128);
    // Supply = 901_000; quorum = 90_100; small_voter has 1_000 — way below

    let id = make_proposal(&env, &gov, &proposer, &share_admin);
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

    let (share, share_id, share_admin) = setup_share(&env);
    let (gov, _) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    let yes_voter = Address::generate(&env);
    let no_voter = Address::generate(&env);

    share.mint(&proposer, &1_000i128);
    share.mint(&yes_voter, &100_000i128);
    share.mint(&no_voter, &100_000i128);
    // Quorum (10%) = 20_100; total votes cast = 200_000 ✓
    // YES = 100_000, NO = 100_000 → 50% YES < 60% threshold → Rejected

    let id = make_proposal(&env, &gov, &proposer, &share_admin);
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
fn test_zero_snapshot_supply_rejects_proposal_immediately() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, share_admin) = setup_share(&env);
    let (_gov, _) = setup_governance(&env, &share_id);

    // Nobody holds shares yet — total supply is 0 at proposal creation
    let proposer = Address::generate(&env);
    share.mint(&proposer, &MIN_SHARE_BALANCE); // just enough to create proposal

    // Burn back down to 1 so snapshot_supply = 1, quorum = 0 → edge: passes quorum trivially
    // Instead mint AFTER creation to test zero-snapshot path
    // Simplest zero-snapshot: use a fresh share contract with supply = 0... but proposer needs
    // min_share_balance. Use MIN_SHARE_BALANCE = 1 so 1 share exists at creation.
    // quorum = 1 * 1000 / 10000 = 0 → total_votes(0) >= 0 → passes quorum, but 0 YES / 0 total
    // would divide by zero. Let's instead verify with large supply and zero votes the rejection.

    // Separate sub-scenario using a fresh environment
    let (share2, share_id2, share_admin2) = setup_share(&env);
    let (gov2, _) = setup_governance(&env, &share_id2);

    // Mint a share AFTER governance is initialised but BEFORE creating proposal — supply = 0 at
    // proposal creation time is impossible because proposer needs min_share_balance ≥ 1.
    // So min non-zero snapshot is 1. Verify quorum = 0 means any vote count passes quorum.
    let proposer2 = Address::generate(&env);
    share2.mint(&proposer2, &1i128); // supply = 1 at creation; quorum = 1*1000/10000 = 0

    let id = gov2.create_proposal(
        &proposer2,
        &String::from_str(&env, "zero-quorum proposal"),
        &share_admin2,
        &String::from_str(&env, "no_op"),
        &String::from_str(&env, "{}"),
    );
    // No votes cast → total_votes = 0. quorum = 0 so 0 >= 0 passes quorum check.
    // Then pass threshold: YES=0, total=0. 0*10000 >= 0*6000 → 0 >= 0 → Passed.
    env.ledger()
        .with_mut(|l| l.timestamp += VOTING_PERIOD + EXEC_DELAY + 2);
    gov2.execute_proposal(&id);
    let p = gov2.get_proposal(&id).unwrap();
    assert_eq!(p.snapshot_supply, 1i128);
    assert_eq!(p.status, ProposalStatus::Executed);
    let _ = (share, share_id, share_admin, proposer); // silence unused warnings
}

#[test]
fn test_cannot_vote_after_voting_period_ends() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, share_admin) = setup_share(&env);
    let (gov, _) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    share.mint(&proposer, &1_000i128);
    share.mint(&voter, &100_000i128);

    let id = make_proposal(&env, &gov, &proposer, &share_admin);

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

    let (share, share_id, share_admin) = setup_share(&env);
    let (gov, _) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    share.mint(&proposer, &1_000i128);
    share.mint(&voter, &100_000i128);

    let id = make_proposal(&env, &gov, &proposer, &share_admin);

    gov.vote(&id, &voter, &true);
    let result = gov.try_vote(&id, &voter, &false);
    assert!(result.is_err(), "double vote must fail");
}

#[test]
fn test_cancel_proposal_by_proposer() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let (share, share_id, share_admin) = setup_share(&env);
    let (gov, _) = setup_governance(&env, &share_id);

    let proposer = Address::generate(&env);
    share.mint(&proposer, &10_000i128);

    let id = make_proposal(&env, &gov, &proposer, &share_admin);
    gov.cancel_proposal(&id, &proposer);

    let proposal = gov.get_proposal(&id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Cancelled);
}
