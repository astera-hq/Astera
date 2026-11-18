#![cfg(test)]

use arbitration::{
    ArbitrationContract, ArbitrationContractClient, ArbitrationError, DisputeResolution,
};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

/// Minimal invoice-contract stand-in so `open_case`/`finalize_case`'s
/// cross-contract callback has something to call into. Always succeeds —
/// none of these registration-focused tests exercise the callback itself
/// (see lifecycle_tests.rs for that).
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

#[test]
fn test_register_juror_transfers_stake_and_records_info() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token, min_stake) = setup(&env);
    let operator = Address::generate(&env);
    mint(&env, &stake_token, &operator, min_stake);

    client.register_juror(&operator, &min_stake);

    let info = client.get_juror(&operator).unwrap();
    assert_eq!(info.stake_amount, min_stake);
    assert!(info.is_active);
    assert_eq!(info.cases_served, 0);
    assert_eq!(info.times_slashed, 0);

    let token_client = token::Client::new(&env, &stake_token);
    assert_eq!(token_client.balance(&operator), 0);
    assert_eq!(token_client.balance(&client.address), min_stake);

    let active = client.list_active_jurors();
    assert_eq!(active.len(), 1);
    assert_eq!(active.get(0).unwrap(), operator);
}

#[test]
fn test_register_below_min_stake_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token, min_stake) = setup(&env);
    let operator = Address::generate(&env);
    mint(&env, &stake_token, &operator, min_stake);

    let result = client.try_register_juror(&operator, &(min_stake - 1));
    assert_eq!(result, Err(Ok(ArbitrationError::InsufficientStake)));
}

#[test]
fn test_register_twice_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token, min_stake) = setup(&env);
    let operator = Address::generate(&env);
    mint(&env, &stake_token, &operator, min_stake * 2);

    client.register_juror(&operator, &min_stake);
    let result = client.try_register_juror(&operator, &min_stake);
    assert_eq!(result, Err(Ok(ArbitrationError::AlreadyRegistered)));
}

#[test]
fn test_deregister_requires_cooldown_before_returning_stake() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token, min_stake) = setup(&env);
    let operator = Address::generate(&env);
    mint(&env, &stake_token, &operator, min_stake);
    client.register_juror(&operator, &min_stake);

    // First call requests deregistration and starts the cooldown.
    client.deregister_juror(&operator);
    let info = client.get_juror(&operator).unwrap();
    assert!(!info.is_active);
    assert!(info.deregister_requested_at.is_some());

    // Stake is still held by the contract during the cooldown.
    let token_client = token::Client::new(&env, &stake_token);
    assert_eq!(token_client.balance(&operator), 0);

    // Calling again before the cooldown elapses is rejected.
    let too_soon = client.try_deregister_juror(&operator);
    assert_eq!(
        too_soon,
        Err(Ok(ArbitrationError::DeregisterCooldownActive))
    );

    // Advance past the default 7-day cooldown.
    env.ledger()
        .with_mut(|l| l.timestamp += 7 * 24 * 60 * 60 + 1);
    client.deregister_juror(&operator);

    assert_eq!(token_client.balance(&operator), min_stake);
    assert!(client.get_juror(&operator).is_none());
    assert_eq!(client.list_active_jurors().len(), 0);
}

#[test]
fn test_pause_blocks_register() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, stake_token, min_stake) = setup(&env);
    client.pause(&admin);
    let operator = Address::generate(&env);
    mint(&env, &stake_token, &operator, min_stake);

    let result = client.try_register_juror(&operator, &min_stake);
    assert_eq!(result, Err(Ok(ArbitrationError::ContractPaused)));
}

#[test]
fn test_deregister_blocked_while_committee_assignment_pending() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token, min_stake) = setup(&env);

    // Register 5 jurors (default committee size) so `select_jurors` has a
    // large enough active pool to draw from.
    let mut jurors = Vec::new();
    for _ in 0..5 {
        let operator = Address::generate(&env);
        mint(&env, &stake_token, &operator, min_stake);
        client.register_juror(&operator, &min_stake);
        jurors.push(operator);
    }

    let invoice = client.get_invoice_contract().unwrap();
    let claimant = Address::generate(&env);
    let respondent = Address::generate(&env);
    let case_id = client.open_case(&invoice, &42u64, &claimant, &respondent, &10_000i128);

    // Close the evidence window and draw the committee.
    env.ledger()
        .with_mut(|l| l.timestamp += 3 * 24 * 60 * 60 + 1);
    client.select_jurors(&case_id);

    let case = client.get_case(&case_id).unwrap();
    let selected = case.jurors.get(0).unwrap();

    let result = client.try_deregister_juror(&selected);
    assert_eq!(result, Err(Ok(ArbitrationError::DeregisterHasPendingCases)));
}
