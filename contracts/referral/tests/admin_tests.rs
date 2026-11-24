#![cfg(test)]

use referral::{ReferralContract, ReferralContractClient, ReferralError};
use soroban_sdk::{
    testutils::Address as _,
    Address, Env,
};

fn setup(env: &Env) -> (ReferralContractClient<'_>, Address, Address) {
    let contract_id = env.register(ReferralContract, ());
    let client = ReferralContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let pool = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin, &pool);
    (client, admin, pool)
}

fn setup_token(env: &Env) -> Address {
    let token_admin = Address::generate(env);
    env.register_stellar_asset_contract_v2(token_admin)
        .address()
}

#[test]
fn test_set_pool_requires_admin() {
    let env = Env::default();
    let (client, admin, _pool) = setup(&env);
    let new_pool = Address::generate(&env);
    let non_admin = Address::generate(&env);

    env.mock_all_auths_allowing_non_root_auth();
    let result = client.try_set_pool(&non_admin, &new_pool);
    assert_eq!(
        result,
        Err(Ok::<soroban_sdk::Error, _>(
            ReferralError::Unauthorized.into()
        ))
    );

    client.set_pool(&admin, &new_pool);
    assert_eq!(client.get_pool(), new_pool);
}

#[test]
fn test_set_deposit_reward_bps_requires_admin() {
    let env = Env::default();
    let (client, admin, _pool) = setup(&env);
    let non_admin = Address::generate(&env);

    env.mock_all_auths_allowing_non_root_auth();
    let result = client.try_set_deposit_reward_bps(&non_admin, &1_500u32);
    assert_eq!(
        result,
        Err(Ok::<soroban_sdk::Error, _>(
            ReferralError::Unauthorized.into()
        ))
    );

    client.set_deposit_reward_bps(&admin, &1_500u32);
    assert_eq!(client.get_deposit_reward_bps(), 1_500u32);
}

#[test]
fn test_set_deposit_reward_bps_above_max_rejected() {
    let env = Env::default();
    let (client, admin, _pool) = setup(&env);

    let result = client.try_set_deposit_reward_bps(&admin, &10_001u32);
    assert_eq!(
        result,
        Err(Ok::<soroban_sdk::Error, _>(
            ReferralError::InvalidBps.into()
        ))
    );
}

#[test]
fn test_is_activated_returns_false_before_first_activity() {
    let env = Env::default();
    let (client, _admin, _pool) = setup(&env);
    let referee = Address::generate(&env);
    let referrer = Address::generate(&env);

    client.register(&referee, &referrer);
    assert!(!client.is_activated(&referee));
}

#[test]
fn test_is_activated_returns_true_after_first_qualifying_activity() {
    let env = Env::default();
    let (client, admin, pool) = setup(&env);
    let referee = Address::generate(&env);
    let referrer = Address::generate(&env);
    let token = setup_token(&env);

    client.set_lifetime_reward_cap(&admin, &token, &100_0000000i128);
    client.register(&referee, &referrer);

    assert!(!client.is_activated(&referee));

    client.record_activity(
        &pool,
        &referee,
        &soroban_sdk::Symbol::new(&env, "borrow"),
        &1_000_0000000i128,
        &token,
    );

    assert!(client.is_activated(&referee));
}

#[test]
fn test_get_lifetime_reward_cap_returns_none_when_unconfigured() {
    let env = Env::default();
    let (client, _admin, _pool) = setup(&env);
    let token = setup_token(&env);

    assert_eq!(client.get_lifetime_reward_cap(&token), None);
}

#[test]
fn test_get_lifetime_reward_cap_returns_configured_value() {
    let env = Env::default();
    let (client, admin, _pool) = setup(&env);
    let token = setup_token(&env);

    client.set_lifetime_reward_cap(&admin, &token, &50_0000000i128);
    assert_eq!(client.get_lifetime_reward_cap(&token), Some(50_0000000i128));
}
