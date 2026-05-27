use pool::{FundingPool, FundingPoolClient, PoolError};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    token, Address, Env, Symbol,
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

fn setup(env: &Env) -> (FundingPoolClient<'_>, Address, Address) {
    env.ledger().with_mut(|l| l.timestamp = 100_000);
    let contract_id = env.register(FundingPool, ());
    let client = FundingPoolClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let invoice_contract = Address::generate(env);
    let share_token = env.register(DummyShare, ());

    client.initialize(&admin, &usdc_id, &share_token, &invoice_contract);
    client.set_max_investor_concentration(&admin, &10_000u32);
    (client, admin, usdc_id)
}

fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token_id).mint(to, &amount);
}

#[test]
fn test_kyc_blocks_deposit_when_required() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);
    let investor = Address::generate(&env);

    client.set_kyc_required(&admin, &true);
    mint(&env, &usdc_id, &investor, 1_000);

    let result = client.try_deposit(&investor, &usdc_id, &1_000);
    assert_eq!(result, Err(Ok(PoolError::KycNotApproved)));
}

#[test]
fn test_kyc_allows_deposit_after_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);
    let investor = Address::generate(&env);

    client.set_kyc_required(&admin, &true);
    client.set_investor_kyc(&admin, &investor, &true);
    mint(&env, &usdc_id, &investor, 1_500);

    client.deposit(&investor, &usdc_id, &1_500);

    let totals = client.get_token_totals(&usdc_id);
    assert_eq!(totals.pool_value, 1_500);
}

#[test]
fn test_kyc_not_required_by_default() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, usdc_id) = setup(&env);
    let investor = Address::generate(&env);

    assert!(!client.kyc_required());
    mint(&env, &usdc_id, &investor, 750);
    client.deposit(&investor, &usdc_id, &750);

    let totals = client.get_token_totals(&usdc_id);
    assert_eq!(totals.pool_value, 750);
}

#[test]
fn test_kyc_required_flag_toggle() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);
    let investor = Address::generate(&env);

    mint(&env, &usdc_id, &investor, 3_000);

    client.set_kyc_required(&admin, &true);
    let blocked = client.try_deposit(&investor, &usdc_id, &1_000);
    assert_eq!(blocked, Err(Ok(PoolError::KycNotApproved)));

    client.set_kyc_required(&admin, &false);
    client.deposit(&investor, &usdc_id, &1_000);

    client.set_kyc_required(&admin, &true);
    let blocked_again = client.try_deposit(&investor, &usdc_id, &1_000);
    assert_eq!(blocked_again, Err(Ok(PoolError::KycNotApproved)));
}

#[test]
fn test_non_admin_cannot_approve_investor_kyc() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _usdc_id) = setup(&env);
    let attacker = Address::generate(&env);
    let investor = Address::generate(&env);

    let result = client.try_set_investor_kyc(&attacker, &investor, &true);
    assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
}

#[test]
fn test_non_admin_cannot_set_kyc_required() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _usdc_id) = setup(&env);
    let attacker = Address::generate(&env);

    let result = client.try_set_kyc_required(&attacker, &true);
    assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
}
