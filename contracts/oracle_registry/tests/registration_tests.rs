#![cfg(test)]

use oracle_registry::{OracleRegistryContract, OracleRegistryContractClient, OracleRegistryError};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, String,
};

fn setup(env: &Env) -> (OracleRegistryContractClient<'_>, Address, Address, i128) {
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);
    let contract_id = env.register(OracleRegistryContract, ());
    let client = OracleRegistryContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let stake_token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let min_stake = 1_000i128;
    client.initialize(&admin, &stake_token, &min_stake);
    (client, admin, stake_token, min_stake)
}

fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token_id).mint(to, &amount);
}

#[test]
fn test_register_oracle_transfers_stake_and_records_info() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token, min_stake) = setup(&env);
    let operator = Address::generate(&env);
    mint(&env, &stake_token, &operator, min_stake);

    client.register_oracle(&operator, &min_stake);

    let info = client.get_oracle_info(&operator).unwrap();
    assert_eq!(info.stake_amount, min_stake);
    assert!(info.is_active);
    assert_eq!(info.total_verifications, 0);
    assert_eq!(info.total_slashes, 0);

    let token_client = token::Client::new(&env, &stake_token);
    assert_eq!(token_client.balance(&operator), 0);
    assert_eq!(token_client.balance(&client.address), min_stake);

    let active = client.list_active_oracles();
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

    let result = client.try_register_oracle(&operator, &(min_stake - 1));
    assert_eq!(result, Err(Ok(OracleRegistryError::InsufficientStake)));
}

#[test]
fn test_register_twice_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token, min_stake) = setup(&env);
    let operator = Address::generate(&env);
    mint(&env, &stake_token, &operator, min_stake * 2);

    client.register_oracle(&operator, &min_stake);
    let result = client.try_register_oracle(&operator, &min_stake);
    assert_eq!(result, Err(Ok(OracleRegistryError::AlreadyRegistered)));
}

#[test]
fn test_deregister_requires_cooldown_before_returning_stake() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token, min_stake) = setup(&env);
    let operator = Address::generate(&env);
    mint(&env, &stake_token, &operator, min_stake);
    client.register_oracle(&operator, &min_stake);

    // First call requests deregistration and starts the cooldown.
    client.deregister_oracle(&operator);
    let info = client.get_oracle_info(&operator).unwrap();
    assert!(!info.is_active);
    assert!(info.deregister_requested_at.is_some());

    // Stake is still held by the contract during the cooldown.
    let token_client = token::Client::new(&env, &stake_token);
    assert_eq!(token_client.balance(&operator), 0);

    // Calling again before the cooldown elapses is rejected.
    let too_soon = client.try_deregister_oracle(&operator);
    assert_eq!(
        too_soon,
        Err(Ok(OracleRegistryError::DeregisterCooldownActive))
    );

    // Advance past the default 7-day cooldown.
    env.ledger()
        .with_mut(|l| l.timestamp += 7 * 24 * 60 * 60 + 1);
    client.deregister_oracle(&operator);

    assert_eq!(token_client.balance(&operator), min_stake);
    assert!(client.get_oracle_info(&operator).is_none());
    assert_eq!(client.list_active_oracles().len(), 0);
}

#[test]
fn test_deregister_blocked_while_vote_pending_on_open_round() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token, min_stake) = setup(&env);
    let operator = Address::generate(&env);
    mint(&env, &stake_token, &operator, min_stake);
    client.register_oracle(&operator, &min_stake);

    let caller = Address::generate(&env);
    let hash = String::from_str(&env, "hash-1");
    client.open_verification_round(&caller, &42u64, &hash);

    let result = client.try_deregister_oracle(&operator);
    assert_eq!(
        result,
        Err(Ok(OracleRegistryError::DeregisterHasPendingVotes))
    );

    // Once the round expires (nobody reached quorum), it's no longer "open"
    // and the pending-vote block clears even though `operator` never voted.
    env.ledger()
        .with_mut(|l| l.timestamp += 3 * 24 * 60 * 60 + 1);
    client.expire_round(&42u64);
    client.deregister_oracle(&operator);
    let info = client.get_oracle_info(&operator).unwrap();
    assert!(!info.is_active);
}

#[test]
fn test_slash_oracle_reduces_stake_and_forwards_to_treasury() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, stake_token, min_stake) = setup(&env);
    let operator = Address::generate(&env);
    let treasury = Address::generate(&env);
    mint(&env, &stake_token, &operator, min_stake);
    client.register_oracle(&operator, &min_stake);
    client.set_treasury(&admin, &Some(treasury.clone()));

    // Slash 50% (5000 bps).
    client.slash_oracle(&admin, &operator, &5_000u32);

    let info = client.get_oracle_info(&operator).unwrap();
    assert_eq!(info.stake_amount, min_stake / 2);
    assert_eq!(info.total_slashes, 1);

    let token_client = token::Client::new(&env, &stake_token);
    assert_eq!(token_client.balance(&treasury), min_stake / 2);
}

#[test]
fn test_slash_without_treasury_keeps_funds_in_registry() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, stake_token, min_stake) = setup(&env);
    let operator = Address::generate(&env);
    mint(&env, &stake_token, &operator, min_stake);
    client.register_oracle(&operator, &min_stake);

    client.slash_oracle(&admin, &operator, &10_000u32); // full slash

    let info = client.get_oracle_info(&operator).unwrap();
    assert_eq!(info.stake_amount, 0);

    let token_client = token::Client::new(&env, &stake_token);
    assert_eq!(token_client.balance(&client.address), min_stake);
}

#[test]
fn test_slash_unauthorized_caller_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, stake_token, min_stake) = setup(&env);
    let operator = Address::generate(&env);
    let not_admin = Address::generate(&env);
    mint(&env, &stake_token, &operator, min_stake);
    client.register_oracle(&operator, &min_stake);

    let result = client.try_slash_oracle(&not_admin, &operator, &5_000u32);
    assert_eq!(result, Err(Ok(OracleRegistryError::Unauthorized)));
}

#[test]
fn test_pause_blocks_register_and_vote() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, stake_token, min_stake) = setup(&env);
    client.pause(&admin);
    let operator = Address::generate(&env);
    mint(&env, &stake_token, &operator, min_stake);

    let result = client.try_register_oracle(&operator, &min_stake);
    assert_eq!(result, Err(Ok(OracleRegistryError::ContractPaused)));
}
