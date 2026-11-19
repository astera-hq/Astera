#![cfg(test)]

//! #1676: coverage for view functions that had no tests:
//!   get_oracle_reputation, get_oracle_round_history,
//!   get_round_history_paginated, get_quorum_tiers, get_round_votes,
//!   get_governance_address.
//!
//! #1675: acceptance test proving that an oracle registered *after* a round
//! opens cannot vote — its stake was not part of total_stake_snapshot so
//! allowing it would make quorum easier to reach than quorum_bps intends.

use oracle_registry::{
    OracleRegistryContract, OracleRegistryContractClient, OracleRegistryError, QuorumTier,
};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    token, vec, Address, Env, String, Symbol, Vec,
};

// ── Minimal invoice stub ─────────────────────────────────────────────────────

#[contract]
pub struct DummyInvoice;

#[contractimpl]
impl DummyInvoice {
    pub fn consensus_verify(
        _env: Env,
        _id: u64,
        registry: Address,
        _approved: bool,
        _reason: String,
        _oracle_hash: String,
    ) {
        registry.require_auth();
    }

    pub fn get_invoice_verification_state(_env: Env, _id: u64) -> (bool, i128) {
        (true, 0)
    }
}

// ── Shared setup ─────────────────────────────────────────────────────────────

fn setup(env: &Env) -> (OracleRegistryContractClient<'_>, Address, Address) {
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);
    let contract_id = env.register(OracleRegistryContract, ());
    let client = OracleRegistryContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let stake_token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    client.initialize(&admin, &stake_token, &1_000i128);
    // Lower required_votes to 1 so single-oracle tests reach consensus easily.
    client.set_registry_config(
        &admin,
        &1_000i128,
        &1u32,
        &6_600u32,
        &(3 * 86_400u64),
        &(7 * 86_400u64),
    );
    let invoice_id = env.register(DummyInvoice, ());
    client.set_invoice_contract(&admin, &invoice_id);
    (client, admin, stake_token)
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token).mint(to, &amount);
}

fn register_oracle(
    env: &Env,
    client: &OracleRegistryContractClient<'_>,
    stake_token: &Address,
    stake: i128,
) -> Address {
    let op = Address::generate(env);
    mint(env, stake_token, &op, stake);
    client.register_oracle(&op, &stake);
    op
}

// ── #1676: get_governance_address ────────────────────────────────────────────

#[test]
fn test_get_governance_address_none_before_set() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _token) = setup(&env);
    assert!(client.get_governance_address().is_none());
}

#[test]
fn test_get_governance_address_returns_set_address() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _token) = setup(&env);
    let gov = Address::generate(&env);
    client.set_governance_address(&admin, &gov);
    assert_eq!(client.get_governance_address(), Some(gov));
}

// ── #1676: get_quorum_tiers ───────────────────────────────────────────────────

#[test]
fn test_get_quorum_tiers_empty_before_set() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _token) = setup(&env);
    assert_eq!(client.get_quorum_tiers().len(), 0);
}

#[test]
fn test_get_quorum_tiers_returns_configured_tiers() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _token) = setup(&env);

    let tiers = vec![
        &env,
        QuorumTier {
            min_invoice_amount: 1_000,
            quorum_bps: 5_000,
        },
        QuorumTier {
            min_invoice_amount: 10_000,
            quorum_bps: 8_000,
        },
    ];
    client.set_quorum_tiers(&admin, &tiers);

    let stored = client.get_quorum_tiers();
    assert_eq!(stored.len(), 2);
    assert_eq!(stored.get(0).unwrap().quorum_bps, 5_000);
    assert_eq!(stored.get(1).unwrap().min_invoice_amount, 10_000);
}

// ── #1676: get_round_votes ────────────────────────────────────────────────────

#[test]
fn test_get_round_votes_empty_for_unknown_round() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _token) = setup(&env);
    // No round opened — expect empty vec, not a panic.
    let votes = client.get_round_votes(&999u64);
    assert_eq!(votes.len(), 0);
}

#[test]
fn test_get_round_votes_reflects_submitted_votes() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token) = setup(&env);

    let oracle = register_oracle(&env, &client, &stake_token, 2_000);
    let caller = Address::generate(&env);
    client.open_verification_round(&caller, &1u64, &String::from_str(&env, "h1"));

    client.submit_vote(&oracle, &1u64, &true, &String::from_str(&env, "e1"));

    let votes = client.get_round_votes(&1u64);
    assert_eq!(votes.len(), 1);
    let (addr, approved) = votes.get(0).unwrap();
    assert_eq!(addr, oracle);
    assert!(approved);
}

#[test]
fn test_get_round_votes_multiple_oracles() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, stake_token) = setup(&env);
    // Need required_votes >= 2 to keep the round open after the first vote.
    client.set_registry_config(
        &admin,
        &1_000i128,
        &2u32,
        &6_600u32,
        &(3 * 86_400u64),
        &(7 * 86_400u64),
    );

    let o1 = register_oracle(&env, &client, &stake_token, 1_000);
    let o2 = register_oracle(&env, &client, &stake_token, 1_000);
    let caller = Address::generate(&env);
    client.open_verification_round(&caller, &2u64, &String::from_str(&env, "h2"));

    client.submit_vote(&o1, &2u64, &true, &String::from_str(&env, "e1"));
    client.submit_vote(&o2, &2u64, &false, &String::from_str(&env, "e2"));

    let votes = client.get_round_votes(&2u64);
    assert_eq!(votes.len(), 2);
}

// ── #1676: get_oracle_round_history ──────────────────────────────────────────

#[test]
fn test_get_oracle_round_history_empty_before_any_vote() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token) = setup(&env);
    let oracle = register_oracle(&env, &client, &stake_token, 1_000);
    assert_eq!(client.get_oracle_round_history(&oracle).len(), 0);
}

#[test]
fn test_get_oracle_round_history_records_vote() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token) = setup(&env);

    let oracle = register_oracle(&env, &client, &stake_token, 2_000);
    let caller = Address::generate(&env);
    client.open_verification_round(&caller, &5u64, &String::from_str(&env, "hashA"));

    client.submit_vote(&oracle, &5u64, &true, &String::from_str(&env, "ev"));

    let history = client.get_oracle_round_history(&oracle);
    assert_eq!(history.len(), 1);
    let (invoice_id, approved, hash) = history.get(0).unwrap();
    assert_eq!(invoice_id, 5u64);
    assert!(approved);
    assert_eq!(hash, String::from_str(&env, "hashA"));
}

// ── #1676: get_round_history_paginated ───────────────────────────────────────

#[test]
fn test_get_round_history_paginated_empty_history() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token) = setup(&env);
    let oracle = register_oracle(&env, &client, &stake_token, 1_000);

    // offset=0 on an oracle with no history should return empty, not panic.
    let page = client.get_round_history_paginated(&oracle, &0u32, &10u32);
    assert_eq!(page.unwrap().len(), 0);
}

#[test]
fn test_get_round_history_paginated_offset_beyond_end() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token) = setup(&env);

    let oracle = register_oracle(&env, &client, &stake_token, 2_000);
    let caller = Address::generate(&env);
    client.open_verification_round(&caller, &10u64, &String::from_str(&env, "h"));
    client.submit_vote(&oracle, &10u64, &true, &String::from_str(&env, "e"));

    // offset=100 is way past the single entry — should return empty.
    let page = client.get_round_history_paginated(&oracle, &100u32, &10u32);
    assert_eq!(page.unwrap().len(), 0);
}

#[test]
fn test_get_round_history_paginated_limit_zero_errors() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token) = setup(&env);
    let oracle = register_oracle(&env, &client, &stake_token, 1_000);

    let result = client.try_get_round_history_paginated(&oracle, &0u32, &0u32);
    assert!(result.is_err());
}

#[test]
fn test_get_round_history_paginated_first_page() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, stake_token) = setup(&env);
    // Need required_votes=1 so each vote finalises immediately.
    // Already set in setup, but each round needs a fresh invoice_id.
    let oracle = register_oracle(&env, &client, &stake_token, 5_000);
    let caller = Address::generate(&env);

    // Cast votes on 3 distinct rounds.
    for id in [20u64, 21u64, 22u64] {
        client.open_verification_round(&caller, &id, &String::from_str(&env, "h"));
        client.submit_vote(&oracle, &id, &true, &String::from_str(&env, "e"));
    }

    // Page of 2 starting at offset 0.
    let page = client
        .get_round_history_paginated(&oracle, &0u32, &2u32)
        .unwrap();
    assert_eq!(page.len(), 2);

    // Page of 2 starting at offset 2 — only 1 entry left.
    let page2 = client
        .get_round_history_paginated(&oracle, &2u32, &2u32)
        .unwrap();
    assert_eq!(page2.len(), 1);
}

// ── #1676: get_oracle_reputation ─────────────────────────────────────────────

#[test]
fn test_get_oracle_reputation_none_for_unregistered() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _token) = setup(&env);
    let stranger = Address::generate(&env);
    assert!(client.get_oracle_reputation(&stranger).is_none());
}

#[test]
fn test_get_oracle_reputation_initial_state() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token) = setup(&env);
    let oracle = register_oracle(&env, &client, &stake_token, 1_000);

    let rep = client.get_oracle_reputation(&oracle).unwrap();
    assert!(rep.is_active);
    assert_eq!(rep.total_verifications, 0);
    assert_eq!(rep.total_slashes, 0);
    assert_eq!(rep.rounds_participated, 0);
    assert_eq!(rep.registered_at, 1_000_000);
}

#[test]
fn test_get_oracle_reputation_after_approve_vote() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token) = setup(&env);

    let oracle = register_oracle(&env, &client, &stake_token, 2_000);
    let caller = Address::generate(&env);
    client.open_verification_round(&caller, &30u64, &String::from_str(&env, "h"));
    client.submit_vote(&oracle, &30u64, &true, &String::from_str(&env, "e"));

    let rep = client.get_oracle_reputation(&oracle).unwrap();
    assert_eq!(rep.total_verifications, 1);
    assert_eq!(rep.rounds_participated, 1);
    assert_eq!(rep.total_slashes, 0);
}

#[test]
fn test_get_oracle_reputation_after_reject_vote() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token) = setup(&env);

    let oracle = register_oracle(&env, &client, &stake_token, 2_000);
    let caller = Address::generate(&env);
    client.open_verification_round(&caller, &31u64, &String::from_str(&env, "h"));
    client.submit_vote(&oracle, &31u64, &false, &String::from_str(&env, "e"));

    let rep = client.get_oracle_reputation(&oracle).unwrap();
    assert_eq!(rep.total_verifications, 1);
    assert_eq!(rep.rounds_participated, 1);
}

#[test]
fn test_get_oracle_reputation_after_slash() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, stake_token) = setup(&env);

    let oracle = register_oracle(&env, &client, &stake_token, 2_000);
    let caller = Address::generate(&env);
    // Open and finalise a round to have a valid round_id for slash_oracle.
    client.open_verification_round(&caller, &32u64, &String::from_str(&env, "h"));
    client.submit_vote(&oracle, &32u64, &true, &String::from_str(&env, "e"));

    client.slash_oracle(
        &admin,
        &oracle,
        &1_000u32,
        &32u64,
        &String::from_str(&env, "bad verdict"),
    );

    let rep = client.get_oracle_reputation(&oracle).unwrap();
    assert_eq!(rep.total_slashes, 1);
}

// ── #1675: oracle registered after round opens cannot vote ───────────────────

#[test]
fn test_oracle_registered_after_round_opens_vote_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token) = setup(&env);

    // Register one oracle so a round can be opened (need >= 1 active oracle).
    let existing = register_oracle(&env, &client, &stake_token, 2_000);
    let caller = Address::generate(&env);
    client.open_verification_round(&caller, &40u64, &String::from_str(&env, "h"));

    // Now register a brand-new oracle AFTER the round is already open.
    let late_oracle = register_oracle(&env, &client, &stake_token, 5_000);

    // The late oracle's stake is NOT in total_stake_snapshot — its vote must
    // be rejected with OracleNotEligible.
    let result = client.try_submit_vote(
        &late_oracle,
        &40u64,
        &true,
        &String::from_str(&env, "evidence"),
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        OracleRegistryError::OracleNotEligible
    );

    // The existing oracle (registered before the round) can still vote normally.
    client.submit_vote(
        &existing,
        &40u64,
        &true,
        &String::from_str(&env, "evidence"),
    );
    let round = client.get_verification_round(&40u64).unwrap();
    // With required_votes=1 and existing oracle's 2000 stake clearing quorum,
    // round should have finalised as approved.
    assert_eq!(
        round.status,
        oracle_registry::RoundStatus::ConsensusApproved
    );
}

#[test]
fn test_oracle_registered_before_round_opens_can_vote() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token) = setup(&env);

    let oracle = register_oracle(&env, &client, &stake_token, 2_000);
    let caller = Address::generate(&env);
    client.open_verification_round(&caller, &41u64, &String::from_str(&env, "h"));

    // Oracle was registered before the round — vote must succeed.
    client.submit_vote(&oracle, &41u64, &true, &String::from_str(&env, "evidence"));
    let round = client.get_verification_round(&41u64).unwrap();
    assert_eq!(
        round.status,
        oracle_registry::RoundStatus::ConsensusApproved
    );
}
