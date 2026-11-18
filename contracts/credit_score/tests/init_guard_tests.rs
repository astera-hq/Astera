#![cfg(test)]

use credit_score::{CreditScoreContract, CreditScoreContractClient, CreditScoreError};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_credit_score_init_can_only_be_called_once() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(CreditScoreContract, ());
    let client = CreditScoreContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let invoice_contract = Address::generate(&env);
    let pool_contract = Address::generate(&env);

    // First initialization should succeed
    client.initialize(&admin, &invoice_contract, &pool_contract);

    // Second initialization should fail with AlreadyInitialized
    let result = client.try_initialize(&admin, &invoice_contract, &pool_contract);
    assert_eq!(
        result.unwrap_err().unwrap(),
        CreditScoreError::AlreadyInitialized.into()
    );
}

#[test]
fn test_credit_score_reinit_by_different_admin_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(CreditScoreContract, ());
    let client = CreditScoreContractClient::new(&env, &contract_id);
    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let invoice_contract = Address::generate(&env);
    let pool_contract = Address::generate(&env);

    // First initialization with admin1
    client.initialize(&admin1, &invoice_contract, &pool_contract);

    // Attempt to reinitialize with admin2 should fail
    let result = client.try_initialize(&admin2, &invoice_contract, &pool_contract);
    assert_eq!(
        result.unwrap_err().unwrap(),
        CreditScoreError::AlreadyInitialized.into()
    );
}
