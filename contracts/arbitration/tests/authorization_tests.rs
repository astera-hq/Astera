#![cfg(test)]

// Maintainer-review follow-up: explicit coverage for unauthorized callers on
// the two entrypoints that pin against a trusted counterpart address
// (`open_case` pins the calling `invoice` contract, `admin_resolve_no_quorum`
// pins the admin), plus a double-finalize regression guard.

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

fn setup(
    env: &Env,
) -> (
    ArbitrationContractClient<'_>,
    Address,
    Address,
    Address,
    i128,
) {
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
    (client, admin, stake_token, invoice_id, min_stake)
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

#[test]
fn test_open_case_rejects_caller_that_is_not_the_configured_invoice_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _stake_token, _invoice_id, _min_stake) = setup(&env);

    let impostor = Address::generate(&env);
    let claimant = Address::generate(&env);
    let respondent = Address::generate(&env);
    let result = client.try_open_case(&impostor, &1u64, &claimant, &respondent, &10_000i128);
    assert_eq!(result, Err(Ok(ArbitrationError::Unauthorized)));
}

#[test]
fn test_admin_resolve_no_quorum_rejects_non_admin_caller() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token, invoice_id, min_stake) = setup(&env);

    // Get a case into NoQuorumEscalated with retries exhausted so the only
    // remaining question is *who* may call admin_resolve_no_quorum.
    for _ in 0..5 {
        let operator = Address::generate(&env);
        mint(&env, &stake_token, &operator, min_stake);
        client.register_juror(&operator, &min_stake);
    }
    let claimant = Address::generate(&env);
    let respondent = Address::generate(&env);
    let case_id = client.open_case(&invoice_id, &1u64, &claimant, &respondent, &10_000i128);
    env.ledger()
        .with_mut(|l| l.timestamp += 3 * 24 * 60 * 60 + 1);
    client.select_jurors(&case_id); // draw 1
    env.ledger()
        .with_mut(|l| l.timestamp += 2 * 24 * 60 * 60 + 1); // past commit
    env.ledger()
        .with_mut(|l| l.timestamp += 2 * 24 * 60 * 60 + 1); // past reveal, 0 revealed
    client.finalize_case(&case_id); // -> NoQuorumEscalated, retry_count 1
    client.select_jurors(&case_id); // draw 2 (the one allowed retry)
    env.ledger()
        .with_mut(|l| l.timestamp += 2 * 24 * 60 * 60 + 1);
    env.ledger()
        .with_mut(|l| l.timestamp += 2 * 24 * 60 * 60 + 1);
    client.finalize_case(&case_id); // -> NoQuorumEscalated, retries exhausted

    let impostor = Address::generate(&env);
    let result =
        client.try_admin_resolve_no_quorum(&impostor, &case_id, &DisputeResolution::InFavorOfSME);
    assert_eq!(result, Err(Ok(ArbitrationError::Unauthorized)));
}

#[test]
fn test_double_finalize_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token, invoice_id, min_stake) = setup(&env);

    let mut jurors = soroban_sdk::Vec::new(&env);
    for _ in 0..5 {
        let operator = Address::generate(&env);
        mint(&env, &stake_token, &operator, min_stake);
        client.register_juror(&operator, &min_stake);
        jurors.push_back(operator);
    }
    let claimant = Address::generate(&env);
    let respondent = Address::generate(&env);
    let case_id = client.open_case(&invoice_id, &1u64, &claimant, &respondent, &10_000i128);
    env.ledger()
        .with_mut(|l| l.timestamp += 3 * 24 * 60 * 60 + 1);
    client.select_jurors(&case_id);
    let case = client.get_case(&case_id).unwrap();
    for (i, juror) in case.jurors.iter().enumerate() {
        let salt = BytesN::from_array(&env, &[(i as u8) + 1; 32]);
        let hash = commit_hash(&env, true, &salt);
        client.commit_vote(&case_id, &juror, &hash);
    }
    env.ledger()
        .with_mut(|l| l.timestamp += 2 * 24 * 60 * 60 + 1);
    for (i, juror) in case.jurors.iter().enumerate() {
        let salt = BytesN::from_array(&env, &[(i as u8) + 1; 32]);
        client.reveal_vote(&case_id, &juror, &true, &salt);
    }
    env.ledger()
        .with_mut(|l| l.timestamp += 2 * 24 * 60 * 60 + 1);
    client.finalize_case(&case_id);
    assert_eq!(
        client.get_case(&case_id).unwrap().status,
        CaseStatus::Resolved
    );

    let result = client.try_finalize_case(&case_id);
    assert_eq!(result, Err(Ok(ArbitrationError::InvalidCaseStatus)));
}
