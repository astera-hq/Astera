#![cfg(test)]

use pool::{FundingPool, FundingPoolClient, PoolError};
use soroban_sdk::{testutils::Address as _, token, Address, Env};

fn create_token_contract<'a>(env: &Env, admin: &Address) -> token::StellarAssetClient<'a> {
    token::StellarAssetClient::new(
        env,
        &env.register_stellar_asset_contract_v2(admin.clone())
            .address(),
    )
}

#[test]
fn test_pool_init_can_only_be_called_once() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = create_token_contract(&env, &token_admin);
    let share_token = create_token_contract(&env, &token_admin);
    let invoice_contract = Address::generate(&env);

    let pool_id = env.register(FundingPool, ());
    let client = FundingPoolClient::new(&env, &pool_id);

    // First initialization should succeed
    client.initialize(
        &admin,
        &token.address,
        &share_token.address,
        &invoice_contract,
    );

    // Second initialization should fail with AlreadyInitialized
    let result = client.try_initialize(
        &admin,
        &token.address,
        &share_token.address,
        &invoice_contract,
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        PoolError::AlreadyInitialized.into()
    );
}

#[test]
fn test_pool_reinit_by_different_admin_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = create_token_contract(&env, &token_admin);
    let share_token = create_token_contract(&env, &token_admin);
    let invoice_contract = Address::generate(&env);

    let pool_id = env.register(FundingPool, ());
    let client = FundingPoolClient::new(&env, &pool_id);

    // First initialization with admin1
    client.initialize(
        &admin1,
        &token.address,
        &share_token.address,
        &invoice_contract,
    );

    // Attempt to reinitialize with admin2 should fail
    let result = client.try_initialize(
        &admin2,
        &token.address,
        &share_token.address,
        &invoice_contract,
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        PoolError::AlreadyInitialized.into()
    );
}
