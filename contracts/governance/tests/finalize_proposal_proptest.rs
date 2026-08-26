#![cfg(test)]

//! Property-based coverage for the bps arithmetic in `finalize_proposal`
//! (contracts/governance/src/lib.rs) — the quorum and pass-threshold math
//! that decides whether a proposal executes. Mirrors the style of
//! `contracts/share/tests/fuzz_tests.rs`, which fuzzes share's own supply /
//! balance invariants; this file does the same for governance's finalize
//! decision, driven end-to-end through the public contract API (create →
//! vote → execute) since `finalize_proposal` itself is a private helper.

mod common;

use governance::{
    Governance, GovernanceAction, GovernanceClient, PoolAction, ProposalCategory, ProposalStatus,
};
use proptest::prelude::*;
use share::{ShareToken, ShareTokenClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

const VOTING_PERIOD: u64 = 86_400;
const EXEC_DELAY: u64 = 3_600;

/// Independent oracle mirroring the bps arithmetic in `finalize_proposal`:
/// quorum = snapshot_supply * quorum_bps / 10_000 (floor division), and a
/// proposal passes when votes_for * 10_000 >= total_votes * pass_bps.
fn expected_status(
    snapshot_supply: i128,
    votes_for: i128,
    votes_against: i128,
    quorum_bps: u32,
    pass_bps: u32,
) -> ProposalStatus {
    let total_votes = votes_for + votes_against;
    let quorum = (snapshot_supply * quorum_bps as i128) / 10_000i128;
    if total_votes < quorum {
        return ProposalStatus::Rejected;
    }
    if votes_for * 10_000i128 >= total_votes * pass_bps as i128 {
        ProposalStatus::Executed
    } else {
        ProposalStatus::Rejected
    }
}

/// Drives a single proposal through governance end-to-end (create → vote →
/// execute) and returns the final on-chain status plus the snapshotted
/// supply, so callers can compare against `expected_status`.
fn run_proposal(
    proposer_balance: i128,
    yes: i128,
    no: i128,
    other: i128,
    quorum_bps: u32,
    pass_bps: u32,
) -> (ProposalStatus, i128) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let share_admin = Address::generate(&env);
    let share_id = env.register(ShareToken, ());
    let share = ShareTokenClient::new(&env, &share_id);
    share.initialize(
        &share_admin,
        &7u32,
        &String::from_str(&env, "Shares"),
        &String::from_str(&env, "SHR"),
    );

    let gov_admin = Address::generate(&env);
    let gov_id = env.register(Governance, ());
    let gov = GovernanceClient::new(&env, &gov_id);
    gov.initialize(
        &gov_admin,
        &share_id,
        &VOTING_PERIOD,
        &quorum_bps,
        &pass_bps,
        &EXEC_DELAY,
        &1i128,
    );

    let target_id = env.register(common::MockTarget, ());

    let proposer = Address::generate(&env);
    share.mint(&proposer, &proposer_balance);

    let yes_voter = Address::generate(&env);
    let no_voter = Address::generate(&env);
    let other_holder = Address::generate(&env);
    if yes > 0 {
        share.mint(&yes_voter, &yes);
    }
    if no > 0 {
        share.mint(&no_voter, &no);
    }
    if other > 0 {
        share.mint(&other_holder, &other);
    }

    let id = gov.create_proposal(
        &proposer,
        &String::from_str(&env, "proptest proposal"),
        &target_id,
        &GovernanceAction::Pool(PoolAction::SetPoolYield(1_500u32)),
        &ProposalCategory::ParameterChange,
    );
    let snapshot_supply = gov.get_proposal(&id).unwrap().snapshot_supply;

    if yes > 0 {
        gov.vote(&id, &yes_voter, &true);
    }
    if no > 0 {
        gov.vote(&id, &no_voter, &false);
    }

    env.ledger()
        .with_mut(|l| l.timestamp += VOTING_PERIOD + EXEC_DELAY + 2);

    let _ = gov.try_execute_proposal(&id);
    // Belt-and-suspenders: commits lazy finalization even on paths where
    // execute_proposal's own early-return might not have persisted it.
    gov.list_proposals();

    (gov.get_proposal(&id).unwrap().status, snapshot_supply)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Invariant: the on-chain outcome always matches the bps formula for
    /// any snapshot supply / vote split / quorum & pass configuration
    /// allowed at `initialize` (quorum_bps in [1,10000], pass_bps in
    /// (5000,10000]).
    #[test]
    fn prop_finalize_matches_bps_formula(
        proposer_balance in 1i128..1_000i128,
        yes in 0i128..5_000_000i128,
        no in 0i128..5_000_000i128,
        other in 0i128..5_000_000i128,
        quorum_bps in 1u32..=10_000u32,
        pass_bps in 5_001u32..=10_000u32,
    ) {
        let (status, snapshot_supply) =
            run_proposal(proposer_balance, yes, no, other, quorum_bps, pass_bps);
        let expected = expected_status(snapshot_supply, yes, no, quorum_bps, pass_bps);
        prop_assert_eq!(status, expected);
    }

    /// Invariant: falling short of quorum always rejects, however lopsided
    /// (even unanimous) the vote — a large enough non-voting holder pool
    /// means `yes` alone cannot reach quorum.
    #[test]
    fn prop_under_quorum_never_executes(
        proposer_balance in 1i128..1_000i128,
        yes in 0i128..1_000i128,
        other in 500_000i128..5_000_000i128,
        quorum_bps in 5_000u32..=10_000u32,
    ) {
        let (status, snapshot_supply) =
            run_proposal(proposer_balance, yes, 0, other, quorum_bps, 6_000u32);
        let quorum = (snapshot_supply * quorum_bps as i128) / 10_000i128;
        prop_assume!(yes < quorum);
        prop_assert_eq!(status, ProposalStatus::Rejected);
    }

    /// Invariant: moving votes from No to Yes is monotonic for the pass
    /// decision — it never turns an Executed outcome into Rejected.
    #[test]
    fn prop_more_yes_votes_never_hurts_passage(
        proposer_balance in 1i128..1_000i128,
        base_yes in 0i128..2_000_000i128,
        extra_yes in 0i128..2_000_000i128,
        no in 0i128..2_000_000i128,
        quorum_bps in 1u32..=5_000u32,
        pass_bps in 5_001u32..=10_000u32,
    ) {
        let (status_low, supply_low) =
            run_proposal(proposer_balance, base_yes, no, 0, quorum_bps, pass_bps);
        let (status_high, supply_high) =
            run_proposal(proposer_balance, base_yes + extra_yes, no, 0, quorum_bps, pass_bps);
        // Both runs share proposer/no/other balances, so supply differs only
        // by extra_yes — sanity-check that before trusting monotonicity.
        prop_assert_eq!(supply_high, supply_low + extra_yes);

        if status_low == ProposalStatus::Executed {
            prop_assert_eq!(status_high, ProposalStatus::Executed);
        }
    }
}
