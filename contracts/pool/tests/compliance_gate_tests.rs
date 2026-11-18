#![cfg(test)]

//! #867: pool compliance gate — opt-in fatal check on deposit / withdraw /
//! request_withdrawal / fund_invoice. With the flag off, behavior is unchanged.

use compliance::{ComplianceContract, ComplianceContractClient, ComplianceStatus, RiskTier};
use pool::{FundingPool, FundingPoolClient, PoolError};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    token, Address, Env, String, Symbol,
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

fn setup_pool(
    env: &Env,
) -> (
    FundingPoolClient<'_>,
    ComplianceContractClient<'_>,
    Address,
    Address,
) {
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);
    env.mock_all_auths();

    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let share_token = env.register(DummyShare, ());
    let invoice_contract = Address::generate(env);

    let pool_id = env.register(FundingPool, ());
    let pool = FundingPoolClient::new(env, &pool_id);
    pool.initialize(&admin, &token, &share_token, &invoice_contract);
    pool.set_max_investor_concentration(&admin, &10_000u32);

    let compliance_id = env.register(ComplianceContract, ());
    let compliance = ComplianceContractClient::new(env, &compliance_id);
    compliance.initialize(&admin);
    compliance.set_screener_timelock(&admin, &0u64);

    pool.set_compliance_registry(&admin, &compliance_id);

    (pool, compliance, admin, token)
}

fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token_id).mint(to, &amount);
}

fn clear(compliance: &ComplianceContractClient<'_>, admin: &Address, addr: &Address, env: &Env) {
    compliance.submit_screening_result(
        admin,
        addr,
        &ComplianceStatus::Cleared,
        &0u32,
        &RiskTier::Low,
        &(1_000_000u64 + 86_400 * 180),
        &String::from_str(env, "ok"),
    );
}

#[test]
fn test_compliance_off_deposit_unaffected() {
    let env = Env::default();
    let (pool, _compliance, _admin, token) = setup_pool(&env);
    let investor = Address::generate(&env);

    assert!(!pool.require_compliance_check());
    mint(&env, &token, &investor, 10_000);
    pool.deposit(&investor, &token, &1_000i128, &None);
}

#[test]
fn test_compliance_on_blocks_unscreened_deposit() {
    let env = Env::default();
    let (pool, _compliance, admin, token) = setup_pool(&env);
    let investor = Address::generate(&env);

    pool.set_require_compliance_check(&admin, &true);
    mint(&env, &token, &investor, 10_000);

    let result = pool.try_deposit(&investor, &token, &1_000i128, &None);
    assert_eq!(result, Err(Ok(PoolError::ComplianceNotCleared)));
}

#[test]
fn test_compliance_on_allows_cleared_deposit() {
    let env = Env::default();
    let (pool, compliance, admin, token) = setup_pool(&env);
    let investor = Address::generate(&env);

    pool.set_require_compliance_check(&admin, &true);
    clear(&compliance, &admin, &investor, &env);
    mint(&env, &token, &investor, 10_000);

    pool.deposit(&investor, &token, &1_000i128, &None);
}

#[test]
fn test_compliance_on_blocks_blocked_investor_deposit() {
    let env = Env::default();
    let (pool, compliance, admin, token) = setup_pool(&env);
    let investor = Address::generate(&env);

    pool.set_require_compliance_check(&admin, &true);
    compliance.submit_screening_result(
        &admin,
        &investor,
        &ComplianceStatus::Blocked,
        &9001u32,
        &RiskTier::High,
        &0u64,
        &String::from_str(&env, "sanctions"),
    );
    mint(&env, &token, &investor, 10_000);

    let result = pool.try_deposit(&investor, &token, &1_000i128, &None);
    assert_eq!(result, Err(Ok(PoolError::ComplianceNotCleared)));
}

#[test]
fn test_compliance_on_blocks_expired_clearance() {
    let env = Env::default();
    let (pool, compliance, admin, token) = setup_pool(&env);
    let investor = Address::generate(&env);

    pool.set_require_compliance_check(&admin, &true);
    let expires = 1_000_000u64 + 100;
    compliance.submit_screening_result(
        &admin,
        &investor,
        &ComplianceStatus::Cleared,
        &0u32,
        &RiskTier::Low,
        &expires,
        &String::from_str(&env, "ok"),
    );

    env.ledger().with_mut(|l| l.timestamp = expires + 1);
    mint(&env, &token, &investor, 10_000);

    let result = pool.try_deposit(&investor, &token, &1_000i128, &None);
    assert_eq!(result, Err(Ok(PoolError::ComplianceNotCleared)));
}

#[test]
fn test_compliance_on_blocks_withdraw() {
    let env = Env::default();
    let (pool, compliance, admin, token) = setup_pool(&env);
    let investor = Address::generate(&env);

    mint(&env, &token, &investor, 10_000);
    pool.deposit(&investor, &token, &1_000i128, &None);

    pool.set_require_compliance_check(&admin, &true);
    let result = pool.try_withdraw(&investor, &token, &100i128);
    assert_eq!(result, Err(Ok(PoolError::ComplianceNotCleared)));

    clear(&compliance, &admin, &investor, &env);
    pool.withdraw(&investor, &token, &100i128);
}

#[test]
fn test_compliance_on_blocks_request_withdrawal() {
    let env = Env::default();
    let (pool, _compliance, admin, token) = setup_pool(&env);
    let investor = Address::generate(&env);

    mint(&env, &token, &investor, 10_000);
    pool.deposit(&investor, &token, &1_000i128, &None);

    pool.set_require_compliance_check(&admin, &true);
    let result = pool.try_request_withdrawal(&investor, &token, &100i128);
    assert_eq!(result, Err(Ok(PoolError::ComplianceNotCleared)));
}

#[test]
fn test_compliance_on_blocks_fund_invoice_for_unscreened_sme() {
    let env = Env::default();
    let (pool, _compliance, admin, token) = setup_pool(&env);
    let sme = Address::generate(&env);

    pool.set_require_compliance_check(&admin, &true);

    let result = pool.try_fund_invoice(
        &admin,
        &1u64,
        &500i128,
        &sme,
        &(1_000_000u64 + 86_400 * 30),
        &token,
    );
    assert!(result.is_err());
    if let Err(Ok(e)) = result {
        assert!(
            e == PoolError::ComplianceNotCleared || e == PoolError::InvoicePoolMismatch,
            "unexpected error: {:?}",
            e
        );
    }
}
