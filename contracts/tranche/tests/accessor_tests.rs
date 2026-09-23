use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};
use tranche::{errors::TrancheError, state::TrancheClass, TrancheContract};

#[test]
fn get_admin_returns_not_initialized_error_before_initialize() {
    let env = Env::default();
    let contract_id = env.register_contract(None, TrancheContract);

    let result = env.as_contract(&contract_id, || TrancheContract::get_admin(env.clone()));

    assert!(matches!(result, Err(TrancheError::NotInitialized)));
}

#[test]
fn get_position_returns_none_for_unknown_investor() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let token = Address::generate(&env);
    let contract_id = env.register_contract(None, TrancheContract);

    let position = env.as_contract(&contract_id, || {
        TrancheContract::get_position(env.clone(), investor, token, TrancheClass::Senior)
    });

    assert!(position.is_none());
}