#![cfg(test)]

use arbitration::{
    ArbitrationContract, ArbitrationContractClient, ArbitrationError, CaseStatus, DisputeResolution,
};
use soroban_sdk::{
    contract, contractimpl, symbol_short,
    testutils::{Address as _, Ledger},
    token, Address, Bytes, BytesN, Env, Symbol,
};

const LAST_ID: Symbol = symbol_short!("last_id");
const LAST_OUT: Symbol = symbol_short!("last_out");

#[contract]
pub struct DummyInvoice;

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

fn setup(env: &Env) -> (ArbitrationContractClient<'_>, Address, Address, i128) {
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
    (client, admin, stake_token, min_stake)
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

/// #1043 acceptance criterion: "The no-quorum timeout/escalation path is
/// covered by a dedicated test." Registers exactly `committee_size` (5)
/// jurors so every committee draw uses the same fixed pool, commits all of
/// them each round but only reveals 2 (below the default `quorum_floor` of
/// 3) — first via the initial draw, then again on the one allowed retry —
/// and asserts the case ends up needing (and accepting) admin fallback
/// resolution, never leaving the invoice stuck.
#[test]
fn test_no_quorum_escalates_retries_then_falls_back_to_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, stake_token, min_stake) = setup(&env);
    let invoice_id = client.get_invoice_contract().unwrap();
    let invoice_client = DummyInvoiceClient::new(&env, &invoice_id);

    let mut jurors = soroban_sdk::Vec::new(&env);
    for _ in 0..5 {
        let operator = Address::generate(&env);
        mint(&env, &stake_token, &operator, min_stake);
        client.register_juror(&operator, &min_stake);
        jurors.push_back(operator);
    }

    let claimant = Address::generate(&env);
    let respondent = Address::generate(&env);
    let case_id = client.open_case(&invoice_id, &99u64, &claimant, &respondent, &10_000i128);

    // Round 1: evidence window closes, committee drawn.
    env.ledger()
        .with_mut(|l| l.timestamp += 3 * 24 * 60 * 60 + 1);
    client.select_jurors(&case_id);
    let case = client.get_case(&case_id).unwrap();
    assert_eq!(case.retry_count, 1);

    // select_jurors can't be called again mid-round.
    let too_soon = client.try_select_jurors(&case_id);
    assert_eq!(too_soon, Err(Ok(ArbitrationError::InvalidCaseStatus)));

    // Commit all 5, but only reveal 2 — below the quorum floor of 3.
    let mut salts = soroban_sdk::Vec::new(&env);
    for i in 0u8..5 {
        let salt = BytesN::from_array(&env, &[i + 1; 32]);
        salts.push_back(salt.clone());
        let juror = case.jurors.get(i as u32).unwrap();
        client.commit_vote(&case_id, &juror, &commit_hash(&env, true, &salt));
    }
    env.ledger()
        .with_mut(|l| l.timestamp += 2 * 24 * 60 * 60 + 1);
    for i in 0u32..2 {
        let juror = case.jurors.get(i).unwrap();
        let salt = salts.get(i).unwrap();
        client.reveal_vote(&case_id, &juror, &true, &salt);
    }

    // finalize_case can't run before the reveal deadline.
    let too_soon = client.try_finalize_case(&case_id);
    assert_eq!(too_soon, Err(Ok(ArbitrationError::WindowNotClosed)));

    env.ledger()
        .with_mut(|l| l.timestamp += 2 * 24 * 60 * 60 + 1);
    client.finalize_case(&case_id);

    let case = client.get_case(&case_id).unwrap();
    assert_eq!(case.status, CaseStatus::NoQuorumEscalated);
    assert_eq!(case.resolution, DisputeResolution::Pending);
    // Nothing was written back to invoice yet.
    assert_eq!(
        invoice_client.get_last(),
        (0u64, DisputeResolution::Pending)
    );

    // Admin can't shortcut yet — one committee re-draw is still available.
    let premature =
        client.try_admin_resolve_no_quorum(&admin, &case_id, &DisputeResolution::InFavorOfDebtor);
    assert_eq!(premature, Err(Ok(ArbitrationError::RetriesNotExhausted)));

    // Round 2 (the one allowed retry): same story, still below quorum.
    client.select_jurors(&case_id);
    let case = client.get_case(&case_id).unwrap();
    assert_eq!(case.retry_count, 2);
    assert_eq!(case.status, CaseStatus::CommitReveal);

    let mut salts2 = soroban_sdk::Vec::new(&env);
    for i in 0u8..5 {
        let salt = BytesN::from_array(&env, &[100 + i; 32]);
        salts2.push_back(salt.clone());
        let juror = case.jurors.get(i as u32).unwrap();
        client.commit_vote(&case_id, &juror, &commit_hash(&env, false, &salt));
    }
    env.ledger()
        .with_mut(|l| l.timestamp += 2 * 24 * 60 * 60 + 1);
    for i in 0u32..2 {
        let juror = case.jurors.get(i).unwrap();
        let salt = salts2.get(i).unwrap();
        client.reveal_vote(&case_id, &juror, &false, &salt);
    }
    env.ledger()
        .with_mut(|l| l.timestamp += 2 * 24 * 60 * 60 + 1);
    client.finalize_case(&case_id);

    let case = client.get_case(&case_id).unwrap();
    assert_eq!(case.status, CaseStatus::NoQuorumEscalated);
    assert_eq!(case.retry_count, 2);

    // Retries are now exhausted (max_retries defaults to 1, so 2 draws total
    // is the ceiling) — select_jurors must refuse a third draw...
    let exhausted = client.try_select_jurors(&case_id);
    assert_eq!(exhausted, Err(Ok(ArbitrationError::RetriesExhausted)));

    // ...and admin_resolve_no_quorum is now the only way forward, keeping
    // this dispute from being stuck indefinitely.
    client.admin_resolve_no_quorum(&admin, &case_id, &DisputeResolution::InFavorOfDebtor);

    let case = client.get_case(&case_id).unwrap();
    assert_eq!(case.status, CaseStatus::Resolved);
    assert_eq!(case.resolution, DisputeResolution::InFavorOfDebtor);
    assert_eq!(
        invoice_client.get_last(),
        (99u64, DisputeResolution::InFavorOfDebtor)
    );

    // A resolved case can't be admin-resolved again.
    let already_done =
        client.try_admin_resolve_no_quorum(&admin, &case_id, &DisputeResolution::InFavorOfDebtor);
    assert_eq!(already_done, Err(Ok(ArbitrationError::CaseNotEscalated)));
}
