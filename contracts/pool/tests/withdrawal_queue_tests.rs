#![cfg(test)]

// #865: coverage for the withdrawal-queue completion work: queue-depth caps, the
// deposit()/drain_withdrawal_queue() opportunistic-drain paths, age-based
// prioritization under insufficient liquidity, the predictive wait estimate, and the
// liquidity forecast view. The root-cause `InvestorPosition.available` fix and the
// pro-rata settlement math itself are already covered in-crate (see lib.rs `mod tests`)
// and are exercised incidentally here too.

use pool::{DataKey, FundingPool, FundingPoolClient, InvestorPosition, PoolError, PoolTokenTotals};
use proptest::prelude::*;
use soroban_sdk::{
    contract, contractimpl, symbol_short,
    testutils::{Address as _, Ledger},
    Address, Env, IntoVal,
};

#[contract]
pub struct DummyShare;

#[contractimpl]
impl DummyShare {
    pub fn total_supply(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&symbol_short!("tot"))
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
            .set(&symbol_short!("tot"), &(total + amount));
        env.storage().persistent().set(&to, &(balance + amount));
    }
    pub fn burn(env: Env, from: Address, amount: i128) {
        let total = Self::total_supply(env.clone());
        let balance = Self::balance(env.clone(), from.clone());
        env.storage()
            .instance()
            .set(&symbol_short!("tot"), &(total - amount));
        env.storage().persistent().set(&from, &(balance - amount));
    }
}

#[contract]
pub struct DummyInvoice;

#[contractimpl]
impl DummyInvoice {
    pub fn get_authorized_pool(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&symbol_short!("pool"))
            .expect("not initialized")
    }
    pub fn set_pool(env: Env, pool: Address) {
        env.storage().instance().set(&symbol_short!("pool"), &pool);
    }
    pub fn is_invoice_defaulted(_env: Env, _id: u64) -> bool {
        false
    }
}

fn create_token_contract<'a>(
    env: &Env,
    admin: &Address,
) -> soroban_sdk::token::StellarAssetClient<'a> {
    soroban_sdk::token::StellarAssetClient::new(
        env,
        &env.register_stellar_asset_contract_v2(admin.clone())
            .address(),
    )
}

fn setup(env: &Env) -> (FundingPoolClient<'_>, Address, Address, Address) {
    env.ledger().with_mut(|l| l.timestamp = 100_000);
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token = create_token_contract(env, &token_admin);
    let share_token = env.register(DummyShare, ());
    let invoice_contract = env.register(DummyInvoice, ());

    let pool_id = env.register(FundingPool, ());
    let client = FundingPoolClient::new(env, &pool_id);
    DummyInvoiceClient::new(env, &invoice_contract).set_pool(&pool_id);
    client.initialize(&admin, &token.address, &share_token, &invoice_contract);
    // Single/dual-investor scenarios throughout this file; disable the default 20%
    // concentration cap that would otherwise reject them.
    client.set_max_investor_concentration(&admin, &10_000u32);
    (client, admin, token.address, share_token)
}

fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    soroban_sdk::token::StellarAssetClient::new(env, token_id).mint(to, &amount);
}

fn share_balance(env: &Env, share_token: &Address, who: &Address) -> i128 {
    env.invoke_contract(
        share_token,
        &soroban_sdk::Symbol::new(env, "balance"),
        soroban_sdk::vec![env, who.clone().into_val(env)],
    )
}

fn available_of(env: &Env, pool_id: &Address, investor: &Address, token: &Address) -> i128 {
    env.as_contract(pool_id, || {
        env.storage()
            .persistent()
            .get::<DataKey, InvestorPosition>(&DataKey::InvestorPosition(
                investor.clone(),
                token.clone(),
            ))
            .map(|p| p.available)
            .unwrap_or(0)
    })
}

// Mirrors the contract's private MIN_WAIT_ESTIMATE_SECS constant (1 hour).
const MIN_WAIT_ESTIMATE_SECS: u64 = 3_600;

#[test]
fn test_deposit_drains_withdrawal_queue_opportunistically() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id, share_token) = setup(&env);
    let investor = Address::generate(&env);
    let new_capital = Address::generate(&env);
    let sme = Address::generate(&env);

    mint(&env, &usdc_id, &investor, 10_000);
    mint(&env, &usdc_id, &sme, 10_000);
    client.deposit(&investor, &usdc_id, &10_000);
    client.fund_invoice(
        &admin,
        &1u64,
        &10_000,
        &sme,
        &(env.ledger().timestamp() + 86_400),
        &usdc_id,
    );

    // No liquidity left (all 10,000 deployed) - this queues.
    let request_id = client.request_withdrawal(&investor, &usdc_id, &10_000);
    assert!(request_id > 0);
    assert_eq!(client.get_withdrawal_queue(&usdc_id).len(), 1);

    // A second investor's deposit brings in fresh liquidity. deposit() should now
    // opportunistically drain the queue without any repayment happening.
    mint(&env, &usdc_id, &new_capital, 10_000);
    client.deposit(&new_capital, &usdc_id, &10_000);

    assert_eq!(client.get_withdrawal_queue(&usdc_id).len(), 0);
    assert_eq!(share_balance(&env, &share_token, &investor), 0);
}

#[test]
fn test_drain_withdrawal_queue_permissionless_entrypoint() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id, share_token) = setup(&env);
    let investor = Address::generate(&env);
    let sme = Address::generate(&env);
    let keeper = Address::generate(&env);

    mint(&env, &usdc_id, &investor, 10_000);
    mint(&env, &usdc_id, &sme, 10_000);
    client.deposit(&investor, &usdc_id, &10_000);
    client.fund_invoice(
        &admin,
        &1u64,
        &10_000,
        &sme,
        &(env.ledger().timestamp() + 86_400),
        &usdc_id,
    );
    client.request_withdrawal(&investor, &usdc_id, &10_000);
    assert_eq!(client.get_withdrawal_queue(&usdc_id).len(), 1);

    // Simulate liquidity becoming available through a channel other than
    // deposit()/repay_invoice() (e.g. an off-chain settlement crediting the pool
    // directly) by minting real tokens into the pool's balance and updating
    // TokenTotals to match, then verify the new permissionless drain entrypoint
    // picks it up without waiting for either of those triggers.
    let pool_id = client.address.clone();
    mint(&env, &usdc_id, &pool_id, 10_000);
    env.as_contract(&pool_id, || {
        let key = DataKey::TokenTotals(usdc_id.clone());
        let mut tt: PoolTokenTotals = env.storage().instance().get(&key).unwrap_or_default();
        tt.total_deployed -= 10_000;
        env.storage().instance().set(&key, &tt);
    });

    client.drain_withdrawal_queue(&keeper, &usdc_id);

    assert_eq!(client.get_withdrawal_queue(&usdc_id).len(), 0);
    assert_eq!(share_balance(&env, &share_token, &investor), 0);
}

#[test]
fn test_request_withdrawal_rejects_when_queue_full() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id, _share_token) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let sme = Address::generate(&env);

    mint(&env, &usdc_id, &alice, 10_000);
    mint(&env, &usdc_id, &bob, 10_000);
    mint(&env, &usdc_id, &sme, 20_000);
    client.deposit(&alice, &usdc_id, &10_000);
    client.deposit(&bob, &usdc_id, &10_000);
    client.fund_invoice(
        &admin,
        &1u64,
        &20_000,
        &sme,
        &(env.ledger().timestamp() + 86_400),
        &usdc_id,
    );

    client.set_max_withdrawal_queue_depth(&admin, &1u32);

    // Alice fills the single queue slot.
    client.request_withdrawal(&alice, &usdc_id, &10_000);
    assert_eq!(client.get_withdrawal_queue(&usdc_id).len(), 1);

    // Bob's request must be rejected - the queue is full - not queued and not
    // charged against his `available` balance.
    let result = client.try_request_withdrawal(&bob, &usdc_id, &10_000);
    assert_eq!(
        result.unwrap_err().unwrap(),
        PoolError::WithdrawalQueueFull.into()
    );
    let pool_id = client.address.clone();
    assert_eq!(available_of(&env, &pool_id, &bob, &usdc_id), 10_000);
}

#[test]
fn test_process_withdrawal_queue_prioritizes_aged_requests() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id, share_token) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let sme = Address::generate(&env);

    mint(&env, &usdc_id, &alice, 10_000);
    mint(&env, &usdc_id, &bob, 10_000);
    mint(&env, &usdc_id, &sme, 20_500);
    client.deposit(&alice, &usdc_id, &10_000);
    client.deposit(&bob, &usdc_id, &10_000);
    client.fund_invoice(
        &admin,
        &1u64,
        &20_000,
        &sme,
        &(env.ledger().timestamp() + 60 * 86_400),
        &usdc_id,
    );

    // Alice queues first, with no liquidity available.
    client.request_withdrawal(&alice, &usdc_id, &10_000);

    // 31 days later - past the default 30-day max_withdrawal_queue_age_days - Bob
    // queues too. Alice's request is now "aged"; Bob's is not.
    env.ledger().with_mut(|l| l.timestamp += 31 * 86_400);
    client.request_withdrawal(&bob, &usdc_id, &10_000);

    // A full repayment brings in enough liquidity to cover Alice alone (principal +
    // ~31 days of interest), but process_withdrawal_queue should only settle the aged
    // tranche (Alice) and leave Bob's non-aged request untouched, even though there
    // may be leftover liquidity after paying Alice.
    let amount_due = client.estimate_repayment(&1u64, &None);
    client.repay_invoice(&1u64, &sme, &amount_due);

    let queue = client.get_withdrawal_queue(&usdc_id);
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.get(0).unwrap().investor, bob);
    assert_eq!(queue.get(0).unwrap().shares, 10_000);
    assert_eq!(share_balance(&env, &share_token, &alice), 0);
    assert_eq!(share_balance(&env, &share_token, &bob), 10_000);
}

#[test]
fn test_estimate_withdrawal_wait_front_of_queue_returns_minimum_estimate() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id, _share_token) = setup(&env);
    let investor = Address::generate(&env);
    let sme = Address::generate(&env);

    mint(&env, &usdc_id, &investor, 10_000);
    mint(&env, &usdc_id, &sme, 10_000);
    client.deposit(&investor, &usdc_id, &10_000);
    client.fund_invoice(
        &admin,
        &1u64,
        &10_000,
        &sme,
        &(env.ledger().timestamp() + 86_400),
        &usdc_id,
    );
    client.request_withdrawal(&investor, &usdc_id, &10_000);

    // Alone at the front of the queue: capital_ahead == 0, so the predictive estimate
    // clamps down to the minimum rather than reporting a nonsensical zero wait.
    let estimate = client.estimate_withdrawal_wait(&investor, &usdc_id);
    assert_eq!(estimate.queue_position, 1);
    assert_eq!(estimate.capital_ahead, 0);
    assert_eq!(estimate.estimated_wait_secs, MIN_WAIT_ESTIMATE_SECS);
}

#[test]
fn test_liquidity_forecast_reflects_known_invoice_due_dates() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id, _share_token) = setup(&env);
    let investor = Address::generate(&env);
    let sme = Address::generate(&env);
    let now = env.ledger().timestamp();

    mint(&env, &usdc_id, &investor, 1_000_000);
    mint(&env, &usdc_id, &sme, 1_000_000);
    client.deposit(&investor, &usdc_id, &1_000_000);

    let principal_1: i128 = 100_000;
    let principal_2: i128 = 200_000;
    client.fund_invoice(
        &admin,
        &1u64,
        &principal_1,
        &sme,
        &(now + 10 * 86_400),
        &usdc_id,
    );
    client.fund_invoice(
        &admin,
        &2u64,
        &principal_2,
        &sme,
        &(now + 20 * 86_400),
        &usdc_id,
    );

    let points = client.get_liquidity_forecast(&usdc_id, &30u32);
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
    let (client, _admin, usdc_id, _share_token) = setup(&env);

    assert_eq!(client.get_liquidity_forecast(&usdc_id, &0u32).len(), 1);
    // MAX_FORECAST_HORIZON_DAYS = 365.
    assert_eq!(
        client.get_liquidity_forecast(&usdc_id, &100_000u32).len(),
        365
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// #865 acceptance criterion: `available` never goes negative across a randomized
    /// sequence of deposit / request_withdrawal / cancel_withdrawal_request calls.
    #[test]
    fn prop_investor_available_never_negative(
        deposits in prop::collection::vec(1_000_000i128..20_000_000i128, 1..3),
        withdrawal_fracs in prop::collection::vec(0u32..100u32, 1..6),
        cancel_flags in prop::collection::vec(any::<bool>(), 1..6),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let pool_id = client.address.clone();

        for d in deposits.iter() {
            mint(&env, &usdc_id, &investor, *d);
            client.deposit(&investor, &usdc_id, d);
        }

        prop_assert!(available_of(&env, &pool_id, &investor, &usdc_id) >= 0);

        for (frac, cancel) in withdrawal_fracs.iter().zip(cancel_flags.iter()) {
            let available = available_of(&env, &pool_id, &investor, &usdc_id);
            prop_assert!(available >= 0, "available went negative: {}", available);

            let shares = (available * (*frac as i128)) / 100;
            if shares > 0 {
                let _ = client.try_request_withdrawal(&investor, &usdc_id, &shares);
            }
            if *cancel {
                let _ = client.try_cancel_withdrawal_request(&investor, &usdc_id);
            }

            let available_after = available_of(&env, &pool_id, &investor, &usdc_id);
            prop_assert!(
                available_after >= 0,
                "available went negative after op: {}",
                available_after
            );
        }
    }
}
