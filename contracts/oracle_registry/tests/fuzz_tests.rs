#![cfg(test)]

//! Property tests for the stake-weighted quorum math in `submit_vote`.
//!
//! Rather than fixed equal-weight cases (covered in consensus_tests.rs), these
//! generate uneven stake distributions across a variable number of oracles and
//! check the contract's finalization decision against an independent
//! reference model computed in the test itself.

use oracle_registry::{OracleRegistryContract, OracleRegistryContractClient};
use proptest::prelude::*;
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    token, Address, Env, String, Symbol, Vec,
};

#[contract]
pub struct DummyInvoice;

#[contractimpl]
impl DummyInvoice {
    pub fn consensus_verify(
        env: Env,
        id: u64,
        registry: Address,
        approved: bool,
        _reason: String,
        _oracle_hash: String,
    ) {
        registry.require_auth();
        let mut calls: Vec<(u64, bool)> = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "calls"))
            .unwrap_or_else(|| Vec::new(&env));
        calls.push_back((id, approved));
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "calls"), &calls);
    }

    pub fn calls(env: Env) -> Vec<(u64, bool)> {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "calls"))
            .unwrap_or_else(|| Vec::new(&env))
    }
}

const QUORUM_BPS: i128 = 6_600;
// Matches DEFAULT_REQUIRED_VOTES in src/lib.rs — the registry is never
// re-configured in this test, so a fresh `initialize` always defaults to 3.
const REQUIRED_VOTES: u32 = 3;

fn ceil_threshold(total_stake: i128) -> i128 {
    (total_stake * QUORUM_BPS + 9_999) / 10_000
}

/// Reference model: replays the same votes in the same order, stopping as
/// soon as one side crosses the ceiling-divided quorum threshold *and* at
/// least `REQUIRED_VOTES` distinct oracles have voted — exactly the two-gate
/// finalization behavior in `submit_vote` (stake weight alone isn't enough;
/// see test_whale_stake_alone_insufficient_without_minimum_vote_count in
/// consensus_tests.rs).
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Outcome {
    Approved,
    Rejected,
    StillOpen,
}

fn reference_model(stakes: &[i128], votes: &[bool]) -> Outcome {
    let total: i128 = stakes.iter().sum();
    let threshold = ceil_threshold(total);
    let mut weight_for: i128 = 0;
    let mut weight_against: i128 = 0;
    for (i, (stake, approved)) in stakes.iter().zip(votes.iter()).enumerate() {
        if *approved {
            weight_for += stake;
        } else {
            weight_against += stake;
        }
        let has_min_votes = (i + 1) as u32 >= REQUIRED_VOTES;
        if has_min_votes && weight_for >= threshold {
            return Outcome::Approved;
        }
        if has_min_votes && weight_against >= threshold {
            return Outcome::Rejected;
        }
    }
    Outcome::StillOpen
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_quorum_matches_reference_model(
        stakes in prop::collection::vec(100i128..1_000_000i128, 2..8),
        votes in prop::collection::vec(any::<bool>(), 2..8),
    ) {
        // Zip truncates to the shorter of the two, so both are the same
        // effective length; re-derive it up front for clarity.
        let n = stakes.len().min(votes.len());
        let stakes = &stakes[..n];
        let votes = &votes[..n];

        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let registry_id = env.register(OracleRegistryContract, ());
        let client = OracleRegistryContractClient::new(&env, &registry_id);
        let admin = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let stake_token = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        client.initialize(&admin, &stake_token, &100i128);

        let invoice_id = env.register(DummyInvoice, ());
        client.set_invoice_contract(&admin, &invoice_id);
        let invoice_client = DummyInvoiceClient::new(&env, &invoice_id);

        let mut oracles = Vec::new(&env);
        for stake in stakes.iter() {
            let op = Address::generate(&env);
            token::StellarAssetClient::new(&env, &stake_token).mint(&op, stake);
            client.register_oracle(&op, stake);
            oracles.push_back(op);
        }

        let caller = Address::generate(&env);
        let hash = String::from_str(&env, "h");
        client.open_verification_round(&caller, &1u64, &hash);

        let expected = reference_model(stakes, votes);

        for (i, approved) in votes.iter().enumerate() {
            let oracle = oracles.get(i as u32).unwrap();
            // Once finalized, further votes are rejected — ignore that error,
            // it's expected once `expected != StillOpen` and we've already
            // passed the finalizing vote.
            let _ = client.try_submit_vote(&oracle, &1u64, approved, &String::from_str(&env, "e"));
        }

        let round = client.get_verification_round(&1u64).unwrap();
        match expected {
            Outcome::Approved => {
                prop_assert_eq!(round.status, oracle_registry::RoundStatus::ConsensusApproved);
                let calls = invoice_client.calls();
                prop_assert_eq!(calls.len(), 1);
                prop_assert!(calls.get(0).unwrap().1);
            }
            Outcome::Rejected => {
                prop_assert_eq!(round.status, oracle_registry::RoundStatus::ConsensusRejected);
                let calls = invoice_client.calls();
                prop_assert_eq!(calls.len(), 1);
                prop_assert!(!calls.get(0).unwrap().1);
            }
            Outcome::StillOpen => {
                prop_assert_eq!(round.status, oracle_registry::RoundStatus::Open);
                prop_assert_eq!(invoice_client.calls().len(), 0);
            }
        }
    }

    #[test]
    fn prop_quorum_threshold_never_floors_to_zero_with_positive_stake_and_bps(
        total_stake in 1i128..1_000_000_000i128,
        bps in 1u32..=10_000u32,
    ) {
        let threshold = (total_stake * bps as i128 + 9_999) / 10_000;
        prop_assert!(threshold >= 1);
    }
}
