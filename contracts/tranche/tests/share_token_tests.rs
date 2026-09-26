//! #1303: nothing stopped a pool from being configured with the *same* address
//! as both the senior and the junior share token. `deposit` then mints and
//! `withdraw` burns against one token for both classes, so senior and junior
//! holders share a single fungible claim and the waterfall has no seniority
//! left to enforce. Both pool-creation paths must reject that configuration.

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};
use tranche::state::TrancheConfig;
use soroban_sdk::Error as SdkError;
use tranche::{errors::TrancheError, TrancheContract, TrancheContractClient};

fn config() -> TrancheConfig {
    TrancheConfig {
        senior_target_yield_bps: 1_000,
        senior_advance_rate_bps: 8_000,
        junior_first_loss_bps: 10_000,
    }
}

fn register(env: &Env) -> (TrancheContractClient<'_>, Address) {
    let contract_id = env.register(TrancheContract, ());
    (TrancheContractClient::new(env, &contract_id), contract_id)
}

#[test]
fn initialize_rejects_the_same_token_for_both_classes() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, contract_id) = register(&env);
    let admin = Address::generate(&env);
    let token = Address::generate(&env);
    let share_token = Address::generate(&env);

    assert_eq!(
        client
            .try_initialize(&admin, &token, &share_token, &share_token, &config())
            .unwrap_err()
            .unwrap(),
        SdkError::from_contract_error(TrancheError::InvalidShareTokens as u32)
    );

    // The rejected call must not have left a half-initialized pool behind.
    env.as_contract(&contract_id, || {
        assert!(TrancheContract::get_admin(env.clone()).is_err());
    });
}

#[test]
fn initialize_accepts_distinct_share_tokens() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _id) = register(&env);
    let admin = Address::generate(&env);
    let token = Address::generate(&env);

    client.initialize(
        &admin,
        &token,
        &Address::generate(&env),
        &Address::generate(&env),
        &config(),
    );

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_pool(&token).token, token);
}

#[test]
fn open_tranche_for_token_rejects_the_same_token_for_both_classes() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _id) = register(&env);
    let admin = Address::generate(&env);
    let initialized_token = Address::generate(&env);

    client.initialize(
        &admin,
        &initialized_token,
        &Address::generate(&env),
        &Address::generate(&env),
        &config(),
    );

    let token = Address::generate(&env);
    let share_token = Address::generate(&env);

    assert_eq!(
        client.try_open_tranche_for_token(&admin, &token, &share_token, &share_token, &config()),
        Err(Ok(TrancheError::InvalidShareTokens))
    );

    // No pool and no enabled flag for the rejected token.
    assert!(client.try_get_pool(&token).is_err());
    assert!(!client.is_tranche_enabled(&token));

    // Distinct addresses still open a pool, and the classes stay separable.
    let senior = Address::generate(&env);
    let junior = Address::generate(&env);
    client.open_tranche_for_token(&admin, &token, &senior, &junior, &config());

    let pool = client.get_pool(&token);
    assert_eq!(pool.senior_share_token, senior);
    assert_eq!(pool.junior_share_token, junior);
    assert_ne!(pool.senior_share_token, pool.junior_share_token);
    assert!(client.is_tranche_enabled(&token));
}

/// A rejected `initialize` must leave no state behind, so a corrected
/// configuration can still be applied afterwards.
#[test]
fn rejected_initialize_leaves_no_state_behind() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _id) = register(&env);
    let admin = Address::generate(&env);
    let token = Address::generate(&env);
    let share_token = Address::generate(&env);

    assert_eq!(
        client
            .try_initialize(&admin, &token, &share_token, &share_token, &config())
            .unwrap_err()
            .unwrap(),
        SdkError::from_contract_error(TrancheError::InvalidShareTokens as u32)
    );

    let senior = Address::generate(&env);
    let junior = Address::generate(&env);
    client.initialize(&admin, &token, &senior, &junior, &config());

    let pool = client.get_pool(&token);
    assert_eq!(pool.senior_share_token, senior);
    assert_eq!(pool.junior_share_token, junior);
    assert_eq!(pool.senior.deposited, 0);
    assert_eq!(pool.junior.deposited, 0);
    // The two classes are tracked in separate accounting slots.
    assert_eq!(pool.senior.available, 0);
    assert_eq!(pool.junior.available, 0);
}
