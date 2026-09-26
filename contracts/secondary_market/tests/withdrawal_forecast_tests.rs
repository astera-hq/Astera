#![cfg(test)]

// #865: withdrawal-wait/liquidity-forecast analytics, now served from the
// `secondary_market` satellite via cross-contract reads of `pool`'s public
// getters. Moved here from `contracts/pool/tests/withdrawal_queue_tests.rs`
// during the pool-split (pool.wasm exceeded Soroban's 200KB limit).

use pool::FundingPoolClient;
use secondary_market::{MarketError, SecondaryMarket, SecondaryMarketClient};
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
        let t = Self::total_supply(env.clone());
        let b = Self::balance(env.clone(), to.clone());
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "tot"), &(t + amount));
        env.storage().persistent().set(&to, &(b + amount));
    }
    pub fn burn(env: Env, from: Address, amount: i128) {
        let t = Self::total_supply(env.clone());
        let b = Self::balance(env.clone(), from.clone());
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "tot"), &(t - amount));
        env.storage().persistent().set(&from, &(b - amount));
    }
    pub fn decimals(_env: Env) -> u32 {
        7
    }
}

#[contract]
pub struct DummyInvoice;

#[contractimpl]
impl DummyInvoice {
    pub fn get_authorized_pool(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "pool"))
            .expect("not initialized")
    }
    pub fn set_pool(env: Env, pool: Address) {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "pool"), &pool);
    }
    pub fn is_invoice_defaulted(_env: Env, _id: u64) -> bool {
        false
    }
}

fn setup(
    env: &Env,
) -> (
    FundingPoolClient<'_>,
    SecondaryMarketClient<'_>,
    Address,
    Address,
) {
    env.ledger().with_mut(|l| l.timestamp = 100_000);
    let pool_id = env.register(pool::FundingPool, ());
    let pool_client = FundingPoolClient::new(env, &pool_id);
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let invoice_contract = env.register(DummyInvoice, ());
    DummyInvoiceClient::new(env, &invoice_contract).set_pool(&pool_id);
    let share_token = env.register(DummyShare, ());
    pool_client.initialize(&admin, &usdc_id, &share_token, &invoice_contract);
    pool_client.set_max_investor_concentration(&admin, &10_000u32);

    let market_id = env.register(secondary_market::SecondaryMarket, ());
    let market_client = SecondaryMarketClient::new(env, &market_id);
    market_client.initialize(&admin, &pool_id);
    pool_client.set_secondary_market_contract(&admin, &market_id);

    (pool_client, market_client, admin, usdc_id)
}

fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token_id).mint(to, &amount);
}

// Mirrors the secondary_market contract's private MIN_WAIT_ESTIMATE_SECS constant (1 hour).
const MIN_WAIT_ESTIMATE_SECS: u64 = 3_600;

#[test]
fn test_estimate_withdrawal_wait_front_of_queue_returns_minimum_estimate() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc_id) = setup(&env);
    let investor = Address::generate(&env);
    let sme = Address::generate(&env);

    mint(&env, &usdc_id, &investor, 10_000);
    mint(&env, &usdc_id, &sme, 10_000);
    pool_client.deposit(&investor, &usdc_id, &10_000, &None);
    pool_client.fund_invoice(
        &admin,
        &1u64,
        &10_000,
        &sme,
        &(env.ledger().timestamp() + 86_400),
        &usdc_id,
    );
    pool_client.request_withdrawal(&investor, &usdc_id, &10_000);

    // Alone at the front of the queue: capital_ahead == 0, so the predictive estimate
    // clamps down to the minimum rather than reporting a nonsensical zero wait.
    let estimate = market_client.estimate_withdrawal_wait(&investor, &usdc_id);
    assert_eq!(estimate.queue_position, 1);
    assert_eq!(estimate.capital_ahead, 0);
    assert_eq!(estimate.estimated_wait_secs, MIN_WAIT_ESTIMATE_SECS);
}

#[test]
fn test_estimate_withdrawal_wait_returns_position_and_due_date() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc_id) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let sme = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 172800;

    mint(&env, &usdc_id, &alice, 5_000);
    mint(&env, &usdc_id, &bob, 5_000);
    mint(&env, &usdc_id, &sme, 10_000);
    pool_client.deposit(&alice, &usdc_id, &5_000, &None);
    pool_client.deposit(&bob, &usdc_id, &5_000, &None);
    pool_client.fund_invoice(&admin, &1u64, &10_000, &sme, &due_date, &usdc_id);

    pool_client.request_withdrawal(&alice, &usdc_id, &2_000);
    pool_client.request_withdrawal(&bob, &usdc_id, &3_000);

    let estimate = market_client.estimate_withdrawal_wait(&bob, &usdc_id);
    assert_eq!(estimate.queue_position, 2);
    assert_eq!(estimate.capital_ahead, 2_000);
    assert_eq!(estimate.nearest_invoice_due_date, due_date);
}

#[test]
fn test_liquidity_forecast_reflects_known_invoice_due_dates() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc_id) = setup(&env);
    let investor = Address::generate(&env);
    let sme = Address::generate(&env);
    let now = env.ledger().timestamp();

    mint(&env, &usdc_id, &investor, 1_000_000);
    mint(&env, &usdc_id, &sme, 1_000_000);
    pool_client.deposit(&investor, &usdc_id, &1_000_000, &None);

    let principal_1: i128 = 100_000;
    let principal_2: i128 = 200_000;
    pool_client.fund_invoice(
        &admin,
        &1u64,
        &principal_1,
        &sme,
        &(now + 10 * 86_400),
        &usdc_id,
    );
    pool_client.fund_invoice(
        &admin,
        &2u64,
        &principal_2,
        &sme,
        &(now + 20 * 86_400),
        &usdc_id,
    );

    let points = market_client.get_liquidity_forecast(&usdc_id, &30u32);
    assert_eq!(points.len(), 30);
    assert_eq!(points.get(0).unwrap().day, 1);
    assert_eq!(points.get(29).unwrap().day, 30);

    // Liquidity is monotonically non-decreasing over the horizon (repayments only
    // add liquidity; the trailing inflow rate is never negative).
    for i in 1..points.len() {
        assert!(
            points.get(i).unwrap().projected_available
                >= points.get(i - 1).unwrap().projected_available
        );
    }

    // Isolate the due-date contribution from the (unknown, constant-per-call) trailing
    // inflow-rate term by differencing consecutive daily deltas: on the day an
    // invoice's due_date is crossed, the delta jumps by exactly that invoice's
    // principal relative to a non-crossing day.
    let delta = |day_idx: usize| -> i128 {
        points.get(day_idx as u32).unwrap().projected_available
            - points
                .get((day_idx - 1) as u32)
                .unwrap()
                .projected_available
    };
    // day index 9 = day 10 (0-indexed `points`), day index 10 = day 11 (non-crossing).
    assert_eq!(delta(9) - delta(10), principal_1);
    // day index 19 = day 20, day index 20 = day 21 (non-crossing).
    assert_eq!(delta(19) - delta(20), principal_2);
}

#[test]
fn test_liquidity_forecast_clamps_horizon() {
    let env = Env::default();
    env.mock_all_auths();
    let (_pool_client, market_client, _admin, usdc_id) = setup(&env);

    assert_eq!(
        market_client.get_liquidity_forecast(&usdc_id, &0u32).len(),
        1
    );
    // MAX_FORECAST_HORIZON_DAYS = 365.
    assert_eq!(
        market_client
            .get_liquidity_forecast(&usdc_id, &100_000u32)
            .len(),
        365
    );
}

#[test]
fn test_estimate_withdrawal_wait_returns_typed_not_initialized_error() {
    let env = Env::default();
    env.mock_all_auths();
    let market_id = env.register(SecondaryMarket, ());
    let market_client = SecondaryMarketClient::new(&env, &market_id);
    let investor = Address::generate(&env);
    let token = Address::generate(&env);

    let result = market_client.try_estimate_withdrawal_wait(&investor, &token);
    assert_eq!(result.unwrap_err().unwrap(), MarketError::NotInitialized);
}

#[test]
fn test_get_liquidity_forecast_returns_typed_not_initialized_error() {
    let env = Env::default();
    env.mock_all_auths();
    let market_id = env.register(SecondaryMarket, ());
    let market_client = SecondaryMarketClient::new(&env, &market_id);
    let token = Address::generate(&env);

    let result = market_client.try_get_liquidity_forecast(&token, &30u32);
    assert_eq!(result.unwrap_err().unwrap(), MarketError::NotInitialized);
}
