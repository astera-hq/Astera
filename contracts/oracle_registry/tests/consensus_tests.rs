#![cfg(test)]

use oracle_registry::{OracleRegistryContract, OracleRegistryContractClient, OracleRegistryError};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    token, Address, Env, String, Symbol, Vec,
};

/// Records every `consensus_verify` call it receives so tests can assert on
/// exactly what the registry decided, without needing the real invoice wasm.
#[contract]
pub struct DummyInvoice;

#[contractimpl]
impl DummyInvoice {
    pub fn consensus_verify(
        env: Env,
        id: u64,
        registry: Address,
        approved: bool,
        reason: String,
        oracle_hash: String,
    ) {
        registry.require_auth();
        let mut calls: Vec<(u64, bool, String, String)> = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "calls"))
            .unwrap_or_else(|| Vec::new(&env));
        calls.push_back((id, approved, reason, oracle_hash));
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "calls"), &calls);
    }

    pub fn calls(env: Env) -> Vec<(u64, bool, String, String)> {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "calls"))
            .unwrap_or_else(|| Vec::new(&env))
    }
}

fn setup(
    env: &Env,
    min_stake: i128,
) -> (
    OracleRegistryContractClient<'_>,
    Address,
    Address,
    DummyInvoiceClient<'_>,
) {
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let contract_id = env.register(OracleRegistryContract, ());
    let client = OracleRegistryContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let stake_token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    client.initialize(&admin, &stake_token, &min_stake);

    let invoice_id = env.register(DummyInvoice, ());
    client.set_invoice_contract(&admin, &invoice_id);
    let invoice_client = DummyInvoiceClient::new(env, &invoice_id);

    (client, admin, stake_token, invoice_client)
}

fn register_n_equal(
    env: &Env,
    client: &OracleRegistryContractClient<'_>,
    stake_token: &Address,
    n: u32,
    stake: i128,
) -> Vec<Address> {
    let mut out = Vec::new(env);
    for _ in 0..n {
        let op = Address::generate(env);
        token::StellarAssetClient::new(env, stake_token).mint(&op, &stake);
        client.register_oracle(&op, &stake);
        out.push_back(op);
    }
    out
}

#[test]
fn test_open_round_snapshots_active_oracle_stake() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token, _invoice) = setup(&env, 1_000);
    let oracles = register_n_equal(&env, &client, &stake_token, 5, 1_000);
    let caller = Address::generate(&env);
    let hash = String::from_str(&env, "h1");
    client.open_verification_round(&caller, &7u64, &hash);

    let round = client.get_verification_round(&7u64).unwrap();
    assert_eq!(round.total_registered_oracles, 5);
    assert_eq!(round.total_stake_snapshot, 5_000);
    assert_eq!(oracles.len(), 5);
}

#[test]
fn test_single_rogue_oracle_cannot_reach_default_quorum() {
    // Default quorum is 6600 bps (two-thirds). With 5 equally-staked oracles,
    // a single vote (20% weight) must not be able to finalize a round.
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token, invoice) = setup(&env, 1_000);
    let oracles = register_n_equal(&env, &client, &stake_token, 5, 1_000);
    let caller = Address::generate(&env);
    let hash = String::from_str(&env, "h1");
    client.open_verification_round(&caller, &7u64, &hash);

    client.submit_vote(
        &oracles.get(0).unwrap(),
        &7u64,
        &true,
        &String::from_str(&env, "e"),
    );

    let round = client.get_verification_round(&7u64).unwrap();
    assert_eq!(round.status, oracle_registry::RoundStatus::Open);
    assert_eq!(invoice.calls().len(), 0);
}

#[test]
fn test_quorum_approval_at_exactly_two_thirds_calls_consensus_verify() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, stake_token, invoice) = setup(&env, 1_000);
    // required_votes lowered to 2 so this test can isolate the stake-quorum
    // threshold math from the separate N-of-M minimum-vote-count gate
    // (covered by test_whale_stake_alone_insufficient_without_minimum_vote_count).
    client.set_registry_config(
        &admin,
        &1_000i128,
        &2u32,
        &6_600u32,
        &(3 * 86_400u64),
        &(7 * 86_400u64),
    );
    // Total stake 3000, threshold = ceil(3000*6600/10000) = 1980.
    let oracles = register_n_equal(&env, &client, &stake_token, 3, 1_000);
    let caller = Address::generate(&env);
    let hash = String::from_str(&env, "h1");
    client.open_verification_round(&caller, &7u64, &hash);

    client.submit_vote(
        &oracles.get(0).unwrap(),
        &7u64,
        &true,
        &String::from_str(&env, "e"),
    );
    // 1000 < 1980, still open.
    assert_eq!(
        client.get_verification_round(&7u64).unwrap().status,
        oracle_registry::RoundStatus::Open
    );

    client.submit_vote(
        &oracles.get(1).unwrap(),
        &7u64,
        &true,
        &String::from_str(&env, "e"),
    );
    // 2000 >= 1980, consensus reached.
    let round = client.get_verification_round(&7u64).unwrap();
    assert_eq!(
        round.status,
        oracle_registry::RoundStatus::ConsensusApproved
    );

    let calls = invoice.calls();
    assert_eq!(calls.len(), 1);
    let (id, approved, _reason, oracle_hash) = calls.get(0).unwrap();
    assert_eq!(id, 7u64);
    assert!(approved);
    assert_eq!(oracle_hash, hash);
}

#[test]
fn test_quorum_rejection_calls_consensus_verify_with_approved_false() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, stake_token, invoice) = setup(&env, 1_000);
    // See the approval test above — isolates stake-quorum math from the
    // separate minimum-vote-count gate.
    client.set_registry_config(
        &admin,
        &1_000i128,
        &2u32,
        &6_600u32,
        &(3 * 86_400u64),
        &(7 * 86_400u64),
    );
    let oracles = register_n_equal(&env, &client, &stake_token, 3, 1_000);
    let caller = Address::generate(&env);
    let hash = String::from_str(&env, "h1");
    client.open_verification_round(&caller, &7u64, &hash);

    client.submit_vote(
        &oracles.get(0).unwrap(),
        &7u64,
        &false,
        &String::from_str(&env, "e"),
    );
    client.submit_vote(
        &oracles.get(1).unwrap(),
        &7u64,
        &false,
        &String::from_str(&env, "e"),
    );

    let round = client.get_verification_round(&7u64).unwrap();
    assert_eq!(
        round.status,
        oracle_registry::RoundStatus::ConsensusRejected
    );
    let calls = invoice.calls();
    assert_eq!(calls.len(), 1);
    let (_id, approved, _reason, _hash) = calls.get(0).unwrap();
    assert!(!approved);
}

#[test]
fn test_whale_stake_alone_insufficient_without_minimum_vote_count() {
    // A whale oracle holding 90% of stake must NOT be able to finalize a
    // round alone, even though its weight alone would clear the stake
    // quorum. `required_votes` (the N in N-of-M) is an independent floor on
    // the number of distinct participating oracles — a single high-stake
    // oracle unilaterally deciding a round is exactly the 1-of-2 problem
    // this contract replaces.
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token, _invoice) = setup(&env, 1_000);
    let whale = Address::generate(&env);
    token::StellarAssetClient::new(&env, &stake_token).mint(&whale, &9_000);
    client.register_oracle(&whale, &9_000);
    let others = register_n_equal(&env, &client, &stake_token, 2, 1_000);

    let caller = Address::generate(&env);
    let hash = String::from_str(&env, "h1");
    client.open_verification_round(&caller, &7u64, &hash);

    client.submit_vote(&whale, &7u64, &true, &String::from_str(&env, "e"));
    // Whale's weight alone (9000/11000 ≈ 82%) clears the default 66%
    // quorum, but only 1 of the default required_votes = 3 oracles has
    // voted — the round must stay open.
    assert_eq!(
        client.get_verification_round(&7u64).unwrap().status,
        oracle_registry::RoundStatus::Open
    );

    client.submit_vote(
        &others.get(0).unwrap(),
        &7u64,
        &true,
        &String::from_str(&env, "e"),
    );
    // 2 of 3 required votes in, weight is well past quorum — still open.
    assert_eq!(
        client.get_verification_round(&7u64).unwrap().status,
        oracle_registry::RoundStatus::Open
    );

    client.submit_vote(
        &others.get(1).unwrap(),
        &7u64,
        &true,
        &String::from_str(&env, "e"),
    );
    // Both gates satisfied: 3 distinct votes and weight past quorum.
    let round = client.get_verification_round(&7u64).unwrap();
    assert_eq!(
        round.status,
        oracle_registry::RoundStatus::ConsensusApproved
    );
}

#[test]
fn test_double_vote_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token, _invoice) = setup(&env, 1_000);
    let oracles = register_n_equal(&env, &client, &stake_token, 5, 1_000);
    let caller = Address::generate(&env);
    let hash = String::from_str(&env, "h1");
    client.open_verification_round(&caller, &7u64, &hash);
    client.submit_vote(
        &oracles.get(0).unwrap(),
        &7u64,
        &true,
        &String::from_str(&env, "e"),
    );

    let result = client.try_submit_vote(
        &oracles.get(0).unwrap(),
        &7u64,
        &true,
        &String::from_str(&env, "e"),
    );
    assert_eq!(result, Err(Ok(OracleRegistryError::AlreadyVoted)));
}

#[test]
fn test_vote_from_unregistered_oracle_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token, _invoice) = setup(&env, 1_000);
    register_n_equal(&env, &client, &stake_token, 3, 1_000);
    let caller = Address::generate(&env);
    let hash = String::from_str(&env, "h1");
    client.open_verification_round(&caller, &7u64, &hash);

    let stranger = Address::generate(&env);
    let result = client.try_submit_vote(&stranger, &7u64, &true, &String::from_str(&env, "e"));
    assert_eq!(result, Err(Ok(OracleRegistryError::NotRegistered)));
}

#[test]
fn test_round_expires_and_blocks_late_vote_then_admin_fallback_resolves() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, stake_token, invoice) = setup(&env, 1_000);
    let oracles = register_n_equal(&env, &client, &stake_token, 5, 1_000);
    let caller = Address::generate(&env);
    let hash = String::from_str(&env, "h1");
    client.open_verification_round(&caller, &7u64, &hash);

    // Only a minority votes before the 3-day default deadline passes.
    client.submit_vote(
        &oracles.get(0).unwrap(),
        &7u64,
        &true,
        &String::from_str(&env, "e"),
    );

    env.ledger()
        .with_mut(|l| l.timestamp += 3 * 24 * 60 * 60 + 1);

    let late_vote = client.try_submit_vote(
        &oracles.get(1).unwrap(),
        &7u64,
        &true,
        &String::from_str(&env, "e"),
    );
    assert_eq!(late_vote, Err(Ok(OracleRegistryError::RoundExpired)));

    // The stale-vote rejection alone doesn't flip the round to Expired (a
    // failed call rolls back its own writes) — `expire_round` commits that
    // transition explicitly.
    client.expire_round(&7u64);
    assert_eq!(
        client.get_verification_round(&7u64).unwrap().status,
        oracle_registry::RoundStatus::Expired
    );

    // Admin fallback resolves the invoice despite quorum never being reached.
    client.admin_resolve_round(
        &admin,
        &7u64,
        &true,
        &String::from_str(&env, "manual review"),
    );
    let calls = invoice.calls();
    assert_eq!(calls.len(), 1);
    let (_id, approved, _reason, _hash) = calls.get(0).unwrap();
    assert!(approved);
}

#[test]
fn test_admin_resolve_round_before_expiry_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, stake_token, _invoice) = setup(&env, 1_000);
    register_n_equal(&env, &client, &stake_token, 3, 1_000);
    let caller = Address::generate(&env);
    let hash = String::from_str(&env, "h1");
    client.open_verification_round(&caller, &7u64, &hash);

    let result =
        client.try_admin_resolve_round(&admin, &7u64, &true, &String::from_str(&env, "too early"));
    assert_eq!(result, Err(Ok(OracleRegistryError::RoundNotExpired)));
}

#[test]
fn test_reopening_round_rejected_while_still_open() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token, _invoice) = setup(&env, 1_000);
    register_n_equal(&env, &client, &stake_token, 3, 1_000);
    let caller = Address::generate(&env);
    let hash = String::from_str(&env, "h1");
    client.open_verification_round(&caller, &7u64, &hash);

    let result = client.try_open_verification_round(&caller, &7u64, &hash);
    assert_eq!(result, Err(Ok(OracleRegistryError::RoundAlreadyOpen)));
}

#[test]
fn test_no_active_oracles_blocks_round_open() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _stake_token, _invoice) = setup(&env, 1_000);
    let caller = Address::generate(&env);
    let hash = String::from_str(&env, "h1");
    let result = client.try_open_verification_round(&caller, &7u64, &hash);
    assert_eq!(result, Err(Ok(OracleRegistryError::NoActiveOracles)));
}
