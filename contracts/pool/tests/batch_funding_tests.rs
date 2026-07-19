#![cfg(test)]

use pool::{FundingPool, FundingPoolClient, FundingRequest, PoolError};
use soroban_sdk::{
    contract, contractimpl, testutils::Address as _, token, Address, Env, Symbol, Vec,
};

#[contract]
pub struct DummyShare;

#[contractimpl]
impl DummyShare {
    pub fn total_supply(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "tot"))
            .unwrap_or(0)
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage().persistent().get(&id).unwrap_or(0)
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        let total = Self::total_supply(env.clone());
        let balance = Self::balance(env.clone(), to.clone());
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "tot"), &(total + amount));
        env.storage().persistent().set(&to, &(balance + amount));
    }

    pub fn burn(env: Env, from: Address, amount: i128) {
        let total = Self::total_supply(env.clone());
        let balance = Self::balance(env.clone(), from.clone());
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "tot"), &(total - amount));
        env.storage().persistent().set(&from, &(balance - amount));
    }
}

fn create_token_contract<'a>(env: &Env, admin: &Address) -> token::StellarAssetClient<'a> {
    token::StellarAssetClient::new(
        env,
        &env.register_stellar_asset_contract_v2(admin.clone())
            .address(),
    )
}

fn setup(env: &Env) -> (FundingPoolClient, Address, Address) {
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token = create_token_contract(env, &token_admin);
    let share_token = env.register(DummyShare, ());
    let invoice_contract = Address::generate(env);

    let pool_id = env.register(FundingPool, ());
    let client = FundingPoolClient::new(env, &pool_id);

    client.initialize(&admin, &token.address, &share_token, &invoice_contract);
    // Tests here deposit from a single investor, so disable the default
    // 20% concentration cap that would otherwise reject a sole depositor.
    client.set_max_investor_concentration(&admin, &10_000u32);
    (client, admin, token.address)
}

#[test]
fn test_fund_multiple_invoices_rejects_duplicate_ids() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, token) = setup(&env);
    let sme = Address::generate(&env);

    // Create a batch with duplicate invoice IDs
    let mut requests: Vec<FundingRequest> = Vec::new(&env);
    requests.push_back(FundingRequest {
        invoice_id: 1,
        principal: 1_000_000,
        sme: sme.clone(),
        due_date: 1_000_000,
        token: token.clone(),
    });
    requests.push_back(FundingRequest {
        invoice_id: 1, // Duplicate ID
        principal: 2_000_000,
        sme: sme.clone(),
        due_date: 1_000_000,
        token: token.clone(),
    });

    // Should fail with DuplicateInvoiceId
    let result = client.try_fund_multiple_invoices(&admin, &requests);
    assert_eq!(
        result.unwrap_err().unwrap(),
        PoolError::DuplicateInvoiceId.into()
    );
}

#[test]
fn test_fund_multiple_invoices_accepts_unique_ids() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, token) = setup(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);

    // Deposit funds first
    let token_client = token::StellarAssetClient::new(&env, &token);
    token_client.mint(&investor, &10_000_000);
    client.deposit(&investor, &token, &10_000_000);

    // Create a batch with unique invoice IDs
    let mut requests: Vec<FundingRequest> = Vec::new(&env);
    requests.push_back(FundingRequest {
        invoice_id: 1,
        principal: 1_000_000,
        sme: sme.clone(),
        due_date: env.ledger().timestamp() + 86_400,
        token: token.clone(),
    });
    requests.push_back(FundingRequest {
        invoice_id: 2,
        principal: 2_000_000,
        sme: sme.clone(),
        due_date: env.ledger().timestamp() + 86_400,
        token: token.clone(),
    });
    requests.push_back(FundingRequest {
        invoice_id: 3,
        principal: 1_500_000,
        sme: sme.clone(),
        due_date: env.ledger().timestamp() + 86_400,
        token: token.clone(),
    });

    // Should succeed with unique IDs
    client.fund_multiple_invoices(&admin, &requests);

    // Verify all three invoices were funded
    assert!(client.get_funded_invoice(&1).is_some());
    assert!(client.get_funded_invoice(&2).is_some());
    assert!(client.get_funded_invoice(&3).is_some());
}

#[test]
fn test_fund_multiple_invoices_rejects_batch_too_large() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, token) = setup(&env);
    let sme = Address::generate(&env);

    // Create a batch larger than MAX_BATCH_SIZE (20)
    let mut requests: Vec<FundingRequest> = Vec::new(&env);
    for i in 1..=21 {
        requests.push_back(FundingRequest {
            invoice_id: i,
            principal: 1_000_000,
            sme: sme.clone(),
            due_date: 1_000_000,
            token: token.clone(),
        });
    }

    // Should fail with BatchTooLarge
    let result = client.try_fund_multiple_invoices(&admin, &requests);
    assert_eq!(
        result.unwrap_err().unwrap(),
        PoolError::BatchTooLarge.into()
    );
}

#[test]
fn test_fund_invoices_batch_max_size_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, token) = setup(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);

    // Deposit sufficient funds
    let token_client = token::StellarAssetClient::new(&env, &token);
    token_client.mint(&investor, &50_000_000);
    client.deposit(&investor, &token, &50_000_000);

    // Create a batch exactly at MAX_BATCH_SIZE (20)
    let mut requests: Vec<FundingRequest> = Vec::new(&env);
    for i in 1..=20 {
        requests.push_back(FundingRequest {
            invoice_id: i,
            principal: 1_000_000,
            sme: sme.clone(),
            due_date: env.ledger().timestamp() + 86_400,
            token: token.clone(),
        });
    }

    // Should succeed at max size
    client.fund_multiple_invoices(&admin, &requests);

    // Verify all 20 invoices were funded
    for i in 1..=20 {
        assert!(client.get_funded_invoice(&i).is_some());
    }
}
