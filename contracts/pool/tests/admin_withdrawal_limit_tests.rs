#![cfg(test)]

//! #1647: the admin's own LP position must be subject to the same withdrawal
//! cooldown and single-withdrawal cap (#244) as every other LP. Emergency
//! exits go through the timelocked `propose_operation` flow instead.

use pool::{FundingPool, FundingPoolClient, PoolError};
use soroban_sdk::{
    contract, contractimpl, symbol_short,
    testutils::{Address as _, Ledger},
    token, Address, Env,
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
        let t = Self::total_supply(env.clone());
        let b = Self::balance(env.clone(), to.clone());
        env.storage()
            .instance()
            .set(&symbol_short!("tot"), &(t + amount));
        env.storage().persistent().set(&to, &(b + amount));
    }
    pub fn burn(env: Env, from: Address, amount: i128) {
        let t = Self::total_supply(env.clone());
        let b = Self::balance(env.clone(), from.clone());
        env.storage()
            .instance()
            .set(&symbol_short!("tot"), &(t - amount));
        env.storage().persistent().set(&from, &(b - amount));
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
}

const DEPOSIT: i128 = 1_000_000_000;

struct Fx {
    env: Env,
    pool: FundingPoolClient<'static>,
    admin: Address,
    lp: Address,
    usdc: Address,
}

/// The admin and one other LP each hold DEPOSIT, so pool_value = 2 * DEPOSIT.
fn setup() -> Fx {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 100_000);
    let admin = Address::generate(&env);
    let lp = Address::generate(&env);
    let usdc = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let share = env.register(DummyShare, ());
    let invoice = env.register(DummyInvoice, ());
    let pool_id = env.register(FundingPool, ());
    let pool = FundingPoolClient::new(&env, &pool_id);
    DummyInvoiceClient::new(&env, &invoice).set_pool(&pool_id);
    pool.initialize(&admin, &usdc, &share, &invoice);
    pool.set_max_investor_concentration(&admin, &10_000u32);
    for who in [&admin, &lp] {
        token::StellarAssetClient::new(&env, &usdc).mint(who, &DEPOSIT);
        pool.deposit(who, &usdc, &DEPOSIT, &None);
    }
    Fx {
        env,
        pool,
        admin,
        lp,
        usdc,
    }
}

fn usdc_balance(f: &Fx, who: &Address) -> i128 {
    token::Client::new(&f.env, &f.usdc).balance(who)
}

#[test]
fn admin_position_is_subject_to_single_withdrawal_cap() {
    let f = setup();
    // 25% cap of a 2 * DEPOSIT pool = DEPOSIT / 2.
    f.pool.set_withdrawal_limits(&f.admin, &2_500u32, &0u64);

    let over_cap = DEPOSIT / 2 + 1;
    let r = f.pool.try_withdraw(&f.admin, &f.usdc, &over_cap);
    assert_eq!(r.unwrap_err().unwrap(), PoolError::WithdrawalExceedsLimit);
    assert_eq!(
        usdc_balance(&f, &f.admin),
        0,
        "rejected withdrawal pays nothing"
    );
    assert_eq!(f.pool.get_token_totals(&f.usdc).pool_value, 2 * DEPOSIT);

    // Exactly at the cap is still allowed.
    f.pool.withdraw(&f.admin, &f.usdc, &(DEPOSIT / 2));
    assert_eq!(usdc_balance(&f, &f.admin), DEPOSIT / 2);
    assert_eq!(
        f.pool.get_token_totals(&f.usdc).pool_value,
        2 * DEPOSIT - DEPOSIT / 2
    );
}

#[test]
fn admin_position_is_subject_to_withdrawal_cooldown() {
    let f = setup();
    f.pool
        .set_withdrawal_limits(&f.admin, &10_000u32, &3_600u64);

    f.pool.withdraw(&f.admin, &f.usdc, &100_000_000);
    let r = f.pool.try_withdraw(&f.admin, &f.usdc, &100_000_000);
    assert_eq!(r.unwrap_err().unwrap(), PoolError::WithdrawalCooldownActive);
    assert_eq!(usdc_balance(&f, &f.admin), 100_000_000);

    f.env.ledger().with_mut(|l| l.timestamp += 3_601);
    f.pool.withdraw(&f.admin, &f.usdc, &100_000_000);
    assert_eq!(usdc_balance(&f, &f.admin), 200_000_000);
}

#[test]
fn admin_and_regular_lp_are_treated_identically() {
    let f = setup();
    f.pool.set_withdrawal_limits(&f.admin, &2_500u32, &0u64);
    // 25% of 2 * DEPOSIT.
    let over_cap = DEPOSIT / 2 + 1;
    for who in [&f.admin, &f.lp] {
        let r = f.pool.try_withdraw(who, &f.usdc, &over_cap);
        assert_eq!(r.unwrap_err().unwrap(), PoolError::WithdrawalExceedsLimit);
    }
    for who in [&f.admin, &f.lp] {
        f.pool.withdraw(who, &f.usdc, &300_000_000);
        assert_eq!(usdc_balance(&f, who), 300_000_000);
    }
}
