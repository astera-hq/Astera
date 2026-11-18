#![cfg(test)]

use arbitration::{
    ArbitrationContract, ArbitrationContractClient, ArbitrationError, CaseStatus,
    DisputeResolution, PartyRole,
};
use soroban_sdk::{
    contract, contractimpl, symbol_short,
    testutils::{Address as _, Ledger},
    token, Address, Bytes, BytesN, Env, String, Symbol,
};

/// Records the last resolution it was asked to apply so tests can assert the
/// cross-contract callback actually fired with the expected arguments.
#[contract]
pub struct DummyInvoice;

const LAST_ID: Symbol = symbol_short!("last_id");
const LAST_OUT: Symbol = symbol_short!("last_out");

#[contractimpl]
impl DummyInvoice {
    pub fn arbitration_resolve_dispute(
        env: Env,
        arbitration: Address,
        id: u64,
        outcome: DisputeResolution,
    ) {
        arbitration.require_auth();
        env.storage().instance().set(&LAST_ID, &id);
        env.storage().instance().set(&LAST_OUT, &outcome);
    }

    pub fn get_last(env: Env) -> (u64, DisputeResolution) {
        (
            env.storage().instance().get(&LAST_ID).unwrap_or(0),
            env.storage()
                .instance()
                .get(&LAST_OUT)
                .unwrap_or(DisputeResolution::Pending),
        )
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

fn salt_for(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

/// Registers `n` jurors (each staking `min_stake`) and returns their
/// addresses in registration order.
fn register_jurors(
    env: &Env,
    client: &ArbitrationContractClient,
    stake_token: &Address,
    min_stake: i128,
    n: u32,
) -> soroban_sdk::Vec<Address> {
    let mut jurors = soroban_sdk::Vec::new(env);
    for _ in 0..n {
        let operator = Address::generate(env);
        mint(env, stake_token, &operator, min_stake);
        client.register_juror(&operator, &min_stake);
        jurors.push_back(operator);
    }
    jurors
}

#[test]
fn test_full_dispute_lifecycle_resolves_and_settles_jurors() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token, invoice_id, min_stake) = setup(&env);
    let invoice_client = DummyInvoiceClient::new(&env, &invoice_id);

    let jurors = register_jurors(&env, &client, &stake_token, min_stake, 5);
    let total_stake_before: i128 = jurors
        .iter()
        .map(|j| client.get_juror(&j).unwrap().stake_amount)
        .sum();
    assert_eq!(total_stake_before, min_stake * 5);

    let claimant = Address::generate(&env);
    let respondent = Address::generate(&env);
    let case_id = client.open_case(&invoice_id, &7u64, &claimant, &respondent, &50_000i128);

    let case = client.get_case(&case_id).unwrap();
    assert_eq!(case.status, CaseStatus::EvidenceWindow);

    // Both parties submit evidence.
    client.submit_evidence(
        &case_id,
        &claimant,
        &String::from_str(&env, "invoice-scan.pdf#hash"),
    );
    client.submit_evidence(
        &case_id,
        &respondent,
        &String::from_str(&env, "delivery-receipt.pdf#hash"),
    );
    let evidence = client.get_evidence(&case_id);
    assert_eq!(evidence.len(), 2);
    assert_eq!(evidence.get(0).unwrap().party, PartyRole::Claimant);
    assert_eq!(evidence.get(1).unwrap().party, PartyRole::Respondent);

    // A third party can't submit evidence.
    let stranger = Address::generate(&env);
    let result = client.try_submit_evidence(&case_id, &stranger, &String::from_str(&env, "x"));
    assert_eq!(result, Err(Ok(ArbitrationError::InvalidParty)));

    // Close the evidence window and draw the committee.
    env.ledger()
        .with_mut(|l| l.timestamp += 3 * 24 * 60 * 60 + 1);
    client.select_jurors(&case_id);
    let case = client.get_case(&case_id).unwrap();
    assert_eq!(case.status, CaseStatus::CommitReveal);
    assert_eq!(case.jurors.len(), 5);

    // 4-of-5 vote in favor of the debtor (true), 1 votes for the SME (false)
    // — a lopsided (80% > 66% confidence) outcome, so the minority juror
    // should be slashed and the majority rewarded from that slash.
    let votes = [true, true, true, true, false];
    let mut salts: soroban_sdk::Vec<BytesN<32>> = soroban_sdk::Vec::new(&env);
    for i in 0u8..5 {
        salts.push_back(salt_for(&env, i + 1));
    }

    for i in 0..5u32 {
        let juror = case.jurors.get(i).unwrap();
        let salt = salts.get(i).unwrap();
        let hash = commit_hash(&env, votes[i as usize], &salt);
        client.commit_vote(&case_id, &juror, &hash);
    }

    // Reveal is rejected before the commit window closes.
    let first_juror = case.jurors.get(0).unwrap();
    let first_salt = salts.get(0).unwrap();
    let too_early = client.try_reveal_vote(&case_id, &first_juror, &votes[0], &first_salt);
    assert_eq!(too_early, Err(Ok(ArbitrationError::WindowNotClosed)));

    // Move past commit_deadline into the reveal window.
    env.ledger()
        .with_mut(|l| l.timestamp += 2 * 24 * 60 * 60 + 1);

    // Revealing with the wrong vote/salt combination is rejected.
    let bad_salt = salt_for(&env, 99);
    let mismatch = client.try_reveal_vote(&case_id, &first_juror, &votes[0], &bad_salt);
    assert_eq!(mismatch, Err(Ok(ArbitrationError::RevealMismatch)));

    for i in 0..5u32 {
        let juror = case.jurors.get(i).unwrap();
        let salt = salts.get(i).unwrap();
        client.reveal_vote(&case_id, &juror, &votes[i as usize], &salt);
    }

    // Finalizing before the reveal deadline is rejected.
    let too_soon = client.try_finalize_case(&case_id);
    assert_eq!(too_soon, Err(Ok(ArbitrationError::WindowNotClosed)));

    env.ledger()
        .with_mut(|l| l.timestamp += 2 * 24 * 60 * 60 + 1);
    client.finalize_case(&case_id);

    let case = client.get_case(&case_id).unwrap();
    assert_eq!(case.status, CaseStatus::Resolved);
    assert_eq!(case.resolution, DisputeResolution::InFavorOfDebtor);

    let (last_id, last_out) = invoice_client.get_last();
    assert_eq!(last_id, 7u64);
    assert_eq!(last_out, DisputeResolution::InFavorOfDebtor);

    // Minority juror (index 4, voted false) was slashed 10%.
    let minority = case.jurors.get(4).unwrap();
    let minority_info = client.get_juror(&minority).unwrap();
    assert_eq!(minority_info.stake_amount, min_stake - (min_stake / 10));
    assert_eq!(minority_info.times_slashed, 1);

    // Majority jurors' combined stake grew by (approximately, modulo pro-rata
    // rounding dust) the amount slashed from the minority juror — total
    // stake in the system is conserved, not created or destroyed.
    let total_stake_after: i128 = jurors
        .iter()
        .map(|j| client.get_juror(&j).unwrap().stake_amount)
        .sum();
    assert_eq!(total_stake_after, total_stake_before);

    let majority_juror = case.jurors.get(0).unwrap();
    let majority_info = client.get_juror(&majority_juror).unwrap();
    assert!(majority_info.stake_amount > min_stake);
    assert_eq!(majority_info.cases_served, 1);
}
