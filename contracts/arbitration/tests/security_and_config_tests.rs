#![cfg(test)]

// Tests for #1677 (case parties cannot be selected as jurors), #1678
// (deterministic draw via evidence-deadline ledger sequence), and #1679
// (config getters, pause/unpause, get_case_by_invoice).

use arbitration::{
    ArbitrationConfig, ArbitrationContract, ArbitrationContractClient, ArbitrationError,
    CaseStatus, DisputeResolution,
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
        _arbitration: Address,
        _id: u64,
        _outcome: DisputeResolution,
    ) {
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

#[test]
fn test_claimant_cannot_be_selected_as_juror_1677() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token, invoice_id, min_stake) = setup(&env);

    let claimant = Address::generate(&env);
    let respondent = Address::generate(&env);

    // Register the claimant as a juror
    mint(&env, &stake_token, &claimant, min_stake);
    client.register_juror(&claimant, &min_stake);

    // Register enough other jurors to form a committee
    for _ in 0..5 {
        let juror = Address::generate(&env);
        mint(&env, &stake_token, &juror, min_stake);
        client.register_juror(&juror, &min_stake);
    }

    // Open a case with the claimant
    let case_id = client.open_case(&invoice_id, &1u64, &claimant, &respondent, &10_000i128);

    // Close evidence window and select jurors
    env.ledger()
        .with_mut(|l| l.timestamp += 3 * 24 * 60 * 60 + 1);
    client.select_jurors(&case_id);

    // Verify claimant is not in the committee
    let case = client.get_case(&case_id).unwrap();
    assert!(!case.jurors.contains(&claimant));
    assert_eq!(case.jurors.len(), 5);
}

#[test]
fn test_respondent_cannot_be_selected_as_juror_1677() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token, invoice_id, min_stake) = setup(&env);

    let claimant = Address::generate(&env);
    let respondent = Address::generate(&env);

    // Register the respondent as a juror
    mint(&env, &stake_token, &respondent, min_stake);
    client.register_juror(&respondent, &min_stake);

    // Register enough other jurors to form a committee
    for _ in 0..5 {
        let juror = Address::generate(&env);
        mint(&env, &stake_token, &juror, min_stake);
        client.register_juror(&juror, &min_stake);
    }

    // Open a case with the respondent
    let case_id = client.open_case(&invoice_id, &2u64, &claimant, &respondent, &10_000i128);

    // Close evidence window and select jurors
    env.ledger()
        .with_mut(|l| l.timestamp += 3 * 24 * 60 * 60 + 1);
    client.select_jurors(&case_id);

    // Verify respondent is not in the committee
    let case = client.get_case(&case_id).unwrap();
    assert!(!case.jurors.contains(&respondent));
    assert_eq!(case.jurors.len(), 5);
}

#[test]
fn test_deterministic_draw_via_evidence_deadline_1678() {
    // The draw should be deterministic based on the evidence deadline,
    // not the current ledger. If we simulate two different timestamps but
    // the same evidence deadline, the committee should be identical.
    let env1 = Env::default();
    env1.mock_all_auths();
    let (client1, _admin, stake_token, invoice_id, min_stake) = setup(&env1);

    // Register jurors
    let mut juror_addrs = soroban_sdk::Vec::new(&env1);
    for _ in 0..7 {
        let juror = Address::generate(&env1);
        mint(&env1, &stake_token, &juror, min_stake);
        client1.register_juror(&juror, &min_stake);
        juror_addrs.push_back(juror);
    }

    let claimant = Address::generate(&env1);
    let respondent = Address::generate(&env1);
    let case_id = client1.open_case(&invoice_id, &10u64, &claimant, &respondent, &10_000i128);

    // Set specific evidence deadline
    let deadline = 1_000_000 + 3 * 24 * 60 * 60;
    env1.ledger()
        .with_mut(|l| l.timestamp = deadline + 1);
    client1.select_jurors(&case_id);

    let committee1 = client1.get_case(&case_id).unwrap().jurors;

    // Now create a second environment and repeat with a different "current time"
    // but the same case setup
    let env2 = Env::default();
    env2.mock_all_auths();
    let (client2, _admin2, stake_token2, invoice_id2, _min_stake2) = setup(&env2);

    // Register the same jurors in the same order
    for juror in juror_addrs.iter() {
        mint(&env2, &stake_token2, &juror, min_stake);
        client2.register_juror(&juror, &min_stake);
    }

    let case_id2 = client2.open_case(&invoice_id2, &10u64, &claimant, &respondent, &10_000i128);

    // Set the same evidence deadline but advance to a different "current time"
    env2.ledger()
        .with_mut(|l| l.timestamp = deadline + 100); // different from env1
    client2.select_jurors(&case_id2);

    let committee2 = client2.get_case(&case_id2).unwrap().jurors;

    // Since both use the same evidence deadline for seeding, the committees should match
    assert_eq!(committee1.len(), committee2.len());
    for i in 0..committee1.len() {
        assert_eq!(
            committee1.get(i).unwrap(),
            committee2.get(i).unwrap(),
            "Committee members at position {} differ", i
        );
    }
}

#[test]
fn test_get_config_entrypoint_1679() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _stake_token, _invoice_id, _min_stake) = setup(&env);

    let config = client.get_config();

    // Verify all fields are present and have expected defaults
    assert_eq!(config.committee_size, 5);
    assert_eq!(config.quorum_floor, 3);
    assert!(config.evidence_window_secs > 0);
    assert!(config.commit_window_secs > 0);
    assert!(config.reveal_window_secs > 0);
    assert!(config.slash_bps > 0);
    assert!(config.non_reveal_slash_bps > 0);
}

#[test]
fn test_pause_and_unpause_entrypoints_1679() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _stake_token, _invoice_id, _min_stake) = setup(&env);

    // Initially not paused
    assert!(!client.is_paused());

    // Admin can pause
    client.pause(&admin);
    assert!(client.is_paused());

    // Non-admin cannot pause or unpause
    let impostor = Address::generate(&env);
    let result = client.try_pause(&impostor);
    assert_eq!(result, Err(Ok(ArbitrationError::Unauthorized)));

    // Admin can unpause
    client.unpause(&admin);
    assert!(!client.is_paused());
}

#[test]
fn test_get_case_by_invoice_entrypoint_1679() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token, invoice_id, min_stake) = setup(&env);

    // Register a juror for case finalization
    let juror = Address::generate(&env);
    mint(&env, &stake_token, &juror, min_stake);
    client.register_juror(&juror, &min_stake);

    let claimant = Address::generate(&env);
    let respondent = Address::generate(&env);
    let invoice_id_1 = 42u64;

    // Open a case
    let case_id = client.open_case(&invoice_id, &invoice_id_1, &claimant, &respondent, &10_000i128);

    // Lookup by invoice id should return the same case
    let looked_up = client.get_case_by_invoice(&invoice_id_1).unwrap();
    assert_eq!(looked_up.id, case_id);
    assert_eq!(looked_up.invoice_id, invoice_id_1);
    assert_eq!(looked_up.claimant, claimant);
    assert_eq!(looked_up.respondent, respondent);

    // Non-existent invoice should return None
    let not_found = client.get_case_by_invoice(&999u64);
    assert_eq!(not_found, None);
}

#[test]
fn test_set_config_entrypoint_1679() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _stake_token, _invoice_id, _min_stake) = setup(&env);

    let old_config = client.get_config();

    let new_config = ArbitrationConfig {
        min_stake: old_config.min_stake,
        stake_token: old_config.stake_token.clone(),
        committee_size: 7,
        quorum_floor: 4,
        evidence_window_secs: old_config.evidence_window_secs,
        commit_window_secs: old_config.commit_window_secs,
        reveal_window_secs: old_config.reveal_window_secs,
        deregister_cooldown_secs: old_config.deregister_cooldown_secs,
        slash_bps: old_config.slash_bps,
        non_reveal_slash_bps: old_config.non_reveal_slash_bps,
        lopsided_confidence_bps: old_config.lopsided_confidence_bps,
        max_retries: old_config.max_retries,
    };

    // Admin can set config
    client.set_config(&admin, &new_config);

    let updated = client.get_config();
    assert_eq!(updated.committee_size, 7);
    assert_eq!(updated.quorum_floor, 4);

    // Non-admin cannot set config
    let impostor = Address::generate(&env);
    let result = client.try_set_config(&impostor, &new_config);
    assert_eq!(result, Err(Ok(ArbitrationError::Unauthorized)));
}
