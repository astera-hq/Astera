#![cfg(test)]

// #1043 acceptance criterion: "Juror vote secrecy during the commit phase is
// verified (a test confirming a vote's content isn't derivable from on-chain
// state before reveal)". This is structurally guaranteed by `JurorVote` only
// ever storing `commit_hash: Option<BytesN<32>>` before reveal — there is no
// plaintext-vote field anywhere in persistent storage until `reveal_vote`
// writes `revealed_vote`. These tests are a regression guard on that
// invariant, not a test of the hash function's cryptographic properties.

use arbitration::{
    ArbitrationContract, ArbitrationContractClient, ArbitrationError, CaseStatus, DisputeResolution,
};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    token, Address, Bytes, BytesN, Env,
};

#[contract]
pub struct DummyInvoice;

#[contractimpl]
impl DummyInvoice {
    pub fn arbitration_resolve_dispute(
        _env: Env,
        arbitration: Address,
        _id: u64,
        _outcome: DisputeResolution,
    ) {
        arbitration.require_auth();
    }
}

fn setup(env: &Env) -> (ArbitrationContractClient<'_>, Address, i128) {
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);
    let contract_id = env.register(ArbitrationContract, ());
    let client = ArbitrationContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let stake_token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let invoice_id = env.register(DummyInvoice, ());
    let min_stake = 1_000i128;
    client.initialize(&admin, &invoice_id, &stake_token, &min_stake);
    (client, stake_token, min_stake)
}

fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token_id).mint(to, &amount);
}

fn commit_hash(env: &Env, vote: bool, salt: &BytesN<32>) -> BytesN<32> {
    let mut preimage = Bytes::new(env);
    preimage.push_back(if vote { 1u8 } else { 0u8 });
    preimage.append(&Bytes::from(salt.clone()));
    env.crypto().sha256(&preimage).to_bytes()
}

fn open_case_with_committee(
    env: &Env,
    client: &ArbitrationContractClient,
    stake_token: &Address,
    min_stake: i128,
) -> (u64, soroban_sdk::Vec<Address>) {
    let mut jurors = soroban_sdk::Vec::new(env);
    for _ in 0..5 {
        let operator = Address::generate(env);
        mint(env, stake_token, &operator, min_stake);
        client.register_juror(&operator, &min_stake);
        jurors.push_back(operator);
    }
    let invoice = client.get_invoice_contract().unwrap();
    let claimant = Address::generate(env);
    let respondent = Address::generate(env);
    let case_id = client.open_case(&invoice, &1u64, &claimant, &respondent, &10_000i128);
    env.ledger()
        .with_mut(|l| l.timestamp += 3 * 24 * 60 * 60 + 1);
    client.select_jurors(&case_id);
    let case = client.get_case(&case_id).unwrap();
    (case_id, case.jurors)
}

#[test]
fn test_committed_vote_exposes_only_hash_before_reveal() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, stake_token, min_stake) = setup(&env);
    let (case_id, jurors) = open_case_with_committee(&env, &client, &stake_token, min_stake);
    let juror = jurors.get(0).unwrap();

    let salt = BytesN::from_array(&env, &[7u8; 32]);
    let hash = commit_hash(&env, true, &salt);
    client.commit_vote(&case_id, &juror, &hash);

    let vote = client.get_vote(&case_id, &juror).unwrap();
    // The only thing readable pre-reveal is the hash the juror themselves
    // submitted — never the plaintext choice.
    assert_eq!(vote.commit_hash, Some(hash));
    assert_eq!(vote.revealed_vote, None);
}

#[test]
fn test_reveal_before_commit_window_closes_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, stake_token, min_stake) = setup(&env);
    let (case_id, jurors) = open_case_with_committee(&env, &client, &stake_token, min_stake);
    let juror = jurors.get(0).unwrap();

    let salt = BytesN::from_array(&env, &[1u8; 32]);
    let hash = commit_hash(&env, true, &salt);
    client.commit_vote(&case_id, &juror, &hash);

    // Still within the commit window — a premature reveal attempt (which
    // would otherwise let a later voter copy an early revealer's choice)
    // must be rejected rather than silently accepted.
    let result = client.try_reveal_vote(&case_id, &juror, &true, &salt);
    assert_eq!(result, Err(Ok(ArbitrationError::WindowNotClosed)));

    let vote = client.get_vote(&case_id, &juror).unwrap();
    assert_eq!(vote.revealed_vote, None);
}

#[test]
fn test_reveal_with_wrong_salt_does_not_expose_or_accept_vote() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, stake_token, min_stake) = setup(&env);
    let (case_id, jurors) = open_case_with_committee(&env, &client, &stake_token, min_stake);
    let juror = jurors.get(0).unwrap();

    let salt = BytesN::from_array(&env, &[3u8; 32]);
    let hash = commit_hash(&env, true, &salt);
    client.commit_vote(&case_id, &juror, &hash);

    env.ledger()
        .with_mut(|l| l.timestamp += 2 * 24 * 60 * 60 + 1);

    let wrong_salt = BytesN::from_array(&env, &[4u8; 32]);
    let result = client.try_reveal_vote(&case_id, &juror, &true, &wrong_salt);
    assert_eq!(result, Err(Ok(ArbitrationError::RevealMismatch)));

    let vote = client.get_vote(&case_id, &juror).unwrap();
    assert_eq!(vote.revealed_vote, None);
}

#[test]
fn test_same_vote_different_salts_produce_different_commit_hashes() {
    let env = Env::default();
    let salt_a = BytesN::from_array(&env, &[10u8; 32]);
    let salt_b = BytesN::from_array(&env, &[20u8; 32]);
    let hash_a = commit_hash(&env, true, &salt_a);
    let hash_b = commit_hash(&env, true, &salt_b);
    assert_ne!(hash_a, hash_b);

    let hash_true = commit_hash(&env, true, &salt_a);
    let hash_false = commit_hash(&env, false, &salt_a);
    assert_ne!(hash_true, hash_false);
}

#[test]
fn test_committee_member_only_may_commit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, stake_token, min_stake) = setup(&env);
    let (case_id, _jurors) = open_case_with_committee(&env, &client, &stake_token, min_stake);

    let outsider = Address::generate(&env);
    mint(&env, &stake_token, &outsider, min_stake);
    client.register_juror(&outsider, &min_stake);

    let hash = commit_hash(&env, true, &BytesN::from_array(&env, &[5u8; 32]));
    let result = client.try_commit_vote(&case_id, &outsider, &hash);
    assert_eq!(result, Err(Ok(ArbitrationError::NotSelectedJuror)));

    // Sanity: the case is genuinely in the commit/reveal phase, not just
    // rejecting for some unrelated status reason.
    assert_eq!(
        client.get_case(&case_id).unwrap().status,
        CaseStatus::CommitReveal
    );
}
