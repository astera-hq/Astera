#![cfg(test)]

use invoice::{InvoiceContract, InvoiceContractClient, InvoiceError};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_init_can_only_be_called_once() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(InvoiceContract, ());
    let client = InvoiceContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let pool = Address::generate(&env);

    // First initialization should succeed
    client.initialize(&admin, &pool, &i128::MAX, &2_592_000u64, &7u32);

    // Second initialization should fail with AlreadyInitialized
    let result = client.try_initialize(&admin, &pool, &i128::MAX, &2_592_000u64, &7u32);
    assert_eq!(
        result.unwrap_err().unwrap(),
        InvoiceError::AlreadyInitialized.into()
    );
}

#[test]
fn test_reinit_by_different_admin_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(InvoiceContract, ());
    let client = InvoiceContractClient::new(&env, &contract_id);
    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let pool = Address::generate(&env);

    // First initialization with admin1
    client.initialize(&admin1, &pool, &i128::MAX, &2_592_000u64, &7u32);

    // Attempt to reinitialize with admin2 should fail
    let result = client.try_initialize(&admin2, &pool, &i128::MAX, &2_592_000u64, &7u32);
    assert_eq!(
        result.unwrap_err().unwrap(),
        InvoiceError::AlreadyInitialized.into()
    );
}
