//! #1295: `open_tranche_for_token` stored both `DataKey::Pool(token)` and
//! `DataKey::TrancheEnabled(token)`, but `initialize` stored only the pool.
//! A pool created through `initialize` therefore accepted deposits and
//! withdrawals while `is_tranche_enabled(token)` reported `false`, so any
//! caller gating on that flag treated a live pool as disabled. Both
//! pool-creation paths must leave the pool enabled.

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};
use tranche::state::TrancheConfig;
use tranche::{TrancheContract, TrancheContractClient};

fn config() -> TrancheConfig {
    TrancheConfig {
        senior_target_yield_bps: 1_000,
        senior_advance_rate_bps: 8_000,
        junior_first_loss_bps: 10_000,
    }
}

fn register(env: &Env) -> TrancheContractClient<'_> {
    let contract_id = env.register(TrancheContract, ());
    TrancheContractClient::new(env, &contract_id)
}

#[test]
fn initialize_enables_the_pool_it_creates() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register(&env);
    let admin = Address::generate(&env);
    let token = Address::generate(&env);

    assert!(!client.is_tranche_enabled(&token));

    client.initialize(
        &admin,
        &token,
        &Address::generate(&env),
        &Address::generate(&env),
        &config(),
    );

    assert_eq!(client.get_pool(&token).token, token);
    assert!(client.is_tranche_enabled(&token));

    // Only the initialized token is enabled.
    assert!(!client.is_tranche_enabled(&Address::generate(&env)));
}

#[test]
fn initialize_and_open_tranche_for_token_agree_on_enabled_flag() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register(&env);
    let admin = Address::generate(&env);
    let initialized_token = Address::generate(&env);
    let opened_token = Address::generate(&env);

    client.initialize(
        &admin,
        &initialized_token,
        &Address::generate(&env),
        &Address::generate(&env),
        &config(),
    );
    client.open_tranche_for_token(
        &admin,
        &opened_token,
        &Address::generate(&env),
        &Address::generate(&env),
        &config(),
    );

    for token in [initialized_token, opened_token] {
        assert_eq!(client.get_pool(&token).token, token);
        assert!(client.is_tranche_enabled(&token));
    }
}

#[test]
fn reconfiguring_an_initialized_pool_keeps_it_enabled() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register(&env);
    let admin = Address::generate(&env);
    let token = Address::generate(&env);

    client.initialize(
        &admin,
        &token,
        &Address::generate(&env),
        &Address::generate(&env),
        &config(),
    );

    let senior = Address::generate(&env);
    let junior = Address::generate(&env);
    client.open_tranche_for_token(&admin, &token, &senior, &junior, &config());

    let pool = client.get_pool(&token);
    assert_eq!(pool.senior_share_token, senior);
    assert_eq!(pool.junior_share_token, junior);
    assert!(client.is_tranche_enabled(&token));
}

#[test]
fn rejected_initialize_does_not_enable_the_token() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register(&env);
    let admin = Address::generate(&env);
    let token = Address::generate(&env);
    let share_token = Address::generate(&env);

    assert!(client
        .try_initialize(&admin, &token, &share_token, &share_token, &config())
        .is_err());

    assert!(client.try_get_pool(&token).is_err());
    assert!(!client.is_tranche_enabled(&token));
}
