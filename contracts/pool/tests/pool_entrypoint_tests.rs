#![cfg(test)]

//! #1648: coverage for the pool entrypoints that move money or rewrite
//! storage but had no tests: `cancel_funded_invoice`, `market_settle_listing`,
//! `risk_liquidate_collateral` and `run_migration`. Each gets a happy path,
//! authorization failures and accounting assertions.

use pool::{
    CollateralDeposit, DataKey, FundingPool, FundingPoolClient, InvestorPosition, InvoiceRecord,
    ListingSettlement, PoolError,
};
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

/// Minimal invoice contract: every invoice is owned by the SME recorded with
/// `set_invoice`, and `mark_cancelled` can be made to fail.
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
    pub fn set_invoice(env: Env, id: u64, rec: InvoiceRecord) {
        env.storage().persistent().set(&id, &rec);
    }
    pub fn get_invoice(env: Env, id: u64) -> InvoiceRecord {
        env.storage().persistent().get(&id).expect("no invoice")
    }
    pub fn is_invoice_defaulted(_env: Env, _id: u64) -> bool {
        false
    }
    pub fn record_funding(_env: Env, _id: u64, _amount: i128, _pool: Address) {}
    pub fn set_cancel_fails(env: Env, fails: bool) {
        env.storage()
            .instance()
            .set(&symbol_short!("failc"), &fails);
    }
    pub fn mark_cancelled(env: Env, _id: u64, _pool: Address) {
        let fails: bool = env
            .storage()
            .instance()
            .get(&symbol_short!("failc"))
            .unwrap_or(false);
        if fails {
            panic!("cancel rejected");
        }
    }
}

const DEPOSIT: i128 = 1_000_000_000;
const PRINCIPAL: i128 = 100_000_000;
const INVOICE_ID: u64 = 1;

struct Fx {
    env: Env,
    pool: FundingPoolClient<'static>,
    pool_id: Address,
    admin: Address,
    usdc: Address,
    share: DummyShareClient<'static>,
    invoice: DummyInvoiceClient<'static>,
    sme: Address,
    lp: Address,
    due_date: u64,
}

fn setup() -> Fx {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 100_000);
    let admin = Address::generate(&env);
    let lp = Address::generate(&env);
    let sme = Address::generate(&env);
    let usdc = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let share_id = env.register(DummyShare, ());
    let invoice_id = env.register(DummyInvoice, ());
    let pool_id = env.register(FundingPool, ());
    let pool = FundingPoolClient::new(&env, &pool_id);
    let invoice = DummyInvoiceClient::new(&env, &invoice_id);
    invoice.set_pool(&pool_id);
    pool.initialize(&admin, &usdc, &share_id, &invoice_id);
    pool.set_max_investor_concentration(&admin, &10_000u32);

    token::StellarAssetClient::new(&env, &usdc).mint(&lp, &DEPOSIT);
    pool.deposit(&lp, &usdc, &DEPOSIT, &None);

    let due_date = env.ledger().timestamp() + 60 * 86_400;
    invoice.set_invoice(
        &INVOICE_ID,
        &InvoiceRecord {
            owner: sme.clone(),
            amount: PRINCIPAL * 2,
            due_date,
            funded_amount: 0,
        },
    );
    let share = DummyShareClient::new(&env, &share_id);
    Fx {
        env,
        pool,
        pool_id,
        admin,
        usdc,
        share,
        invoice,
        sme,
        lp,
        due_date,
    }
}

impl Fx {
    fn fund(&self) {
        self.pool.fund_invoice(
            &self.admin,
            &INVOICE_ID,
            &PRINCIPAL,
            &self.sme,
            &self.due_date,
            &self.usdc,
        );
    }
    fn balance(&self, who: &Address) -> i128 {
        token::Client::new(&self.env, &self.usdc).balance(who)
    }
}

// ───────────────────────── cancel_funded_invoice ─────────────────────────

#[test]
fn cancel_funded_invoice_releases_deployed_capital_and_removes_record() {
    let f = setup();
    f.fund();
    let before = f.pool.get_token_totals(&f.usdc);
    assert_eq!(before.total_deployed, PRINCIPAL);
    let stats_before = f.pool.get_storage_stats();

    f.pool.cancel_funded_invoice(&f.admin, &INVOICE_ID);

    let after = f.pool.get_token_totals(&f.usdc);
    assert_eq!(after.total_deployed, 0, "principal no longer deployed");
    assert_eq!(after.pool_value, before.pool_value, "NAV is not rewritten");
    assert!(f.pool.get_funded_invoice(&INVOICE_ID).is_none());
    assert_eq!(
        f.pool.get_storage_stats().active_funded_invoices,
        stats_before.active_funded_invoices - 1
    );
}

#[test]
fn cancel_funded_invoice_rejects_non_admin() {
    let f = setup();
    f.fund();
    let impostor = Address::generate(&f.env);
    let r = f.pool.try_cancel_funded_invoice(&impostor, &INVOICE_ID);
    assert_eq!(r.unwrap_err().unwrap(), PoolError::Unauthorized);
    // Nothing moved.
    assert_eq!(f.pool.get_token_totals(&f.usdc).total_deployed, PRINCIPAL);
    assert!(f.pool.get_funded_invoice(&INVOICE_ID).is_some());
}

#[test]
fn cancel_funded_invoice_rejects_unknown_invoice() {
    let f = setup();
    let r = f.pool.try_cancel_funded_invoice(&f.admin, &999u64);
    assert_eq!(r.unwrap_err().unwrap(), PoolError::InvoiceNotFound);
}

#[test]
fn cancel_funded_invoice_fails_cleanly_when_invoice_contract_rejects() {
    let f = setup();
    f.fund();
    f.invoice.set_cancel_fails(&true);
    let r = f.pool.try_cancel_funded_invoice(&f.admin, &INVOICE_ID);
    assert_eq!(r.unwrap_err().unwrap(), PoolError::InvoiceNotCancelled);
    assert_eq!(f.pool.get_token_totals(&f.usdc).total_deployed, PRINCIPAL);
    assert!(f.pool.get_funded_invoice(&INVOICE_ID).is_some());
}

// ─────────────────────────── market_settle_listing ───────────────────────

struct Market {
    market: Address,
    buyer: Address,
}

/// Registers an authorized secondary market, a second LP (the buyer) and
/// seeds the seller (`f.lp`) with `PRINCIPAL` of deployed claim.
fn market_setup(f: &Fx) -> Market {
    let market = Address::generate(&f.env);
    f.pool.set_secondary_market_contract(&f.admin, &market);
    let buyer = Address::generate(&f.env);
    token::StellarAssetClient::new(&f.env, &f.usdc).mint(&buyer, &DEPOSIT);
    f.pool.deposit(&buyer, &f.usdc, &DEPOSIT, &None);
    f.fund();
    f.env.as_contract(&f.pool_id, || {
        let key = DataKey::InvestorPosition(f.lp.clone(), f.usdc.clone());
        let mut pos: InvestorPosition = f.env.storage().persistent().get(&key).unwrap();
        pos.deployed = PRINCIPAL;
        f.env.storage().persistent().set(&key, &pos);
    });
    Market { market, buyer }
}

fn settlement(f: &Fx, m: &Market, price: i128) -> ListingSettlement {
    ListingSettlement {
        buyer: m.buyer.clone(),
        seller: f.lp.clone(),
        invoice_id: INVOICE_ID,
        is_co_funding: false,
        amount_or_bps: PRINCIPAL as u64,
        price,
    }
}

#[test]
fn market_settle_listing_moves_claim_and_shares_between_investors() {
    let f = setup();
    let m = market_setup(&f);
    let price: i128 = 50_000_000;
    let totals_before = f.pool.get_token_totals(&f.usdc);
    let supply_before = f.share.total_supply();
    let seller_shares = f.share.balance(&f.lp);
    let buyer_shares = f.share.balance(&m.buyer);
    let pool_balance = f.balance(&f.pool_id);

    f.pool
        .market_settle_listing(&m.market, &settlement(&f, &m, price));

    // Deployed claim moves from seller to buyer.
    assert_eq!(f.pool.get_investor_position(&f.lp, &f.usdc).1, 0);
    assert_eq!(f.pool.get_investor_position(&m.buyer, &f.usdc).1, PRINCIPAL);
    // Payment is made in pool shares, buyer -> seller (share price is 1:1).
    assert_eq!(f.share.balance(&m.buyer), buyer_shares - price);
    assert_eq!(f.share.balance(&f.lp), seller_shares + price);
    assert_eq!(f.share.total_supply(), supply_before, "shares conserved");
    // Pool-level accounting and real token balances are untouched.
    let totals_after = f.pool.get_token_totals(&f.usdc);
    assert_eq!(totals_after.pool_value, totals_before.pool_value);
    assert_eq!(totals_after.total_deployed, totals_before.total_deployed);
    assert_eq!(f.balance(&f.pool_id), pool_balance);
}

#[test]
fn market_settle_listing_rejects_unauthorized_market() {
    let f = setup();
    let m = market_setup(&f);
    let impostor = Address::generate(&f.env);
    let r = f
        .pool
        .try_market_settle_listing(&impostor, &settlement(&f, &m, 50_000_000));
    assert_eq!(r.unwrap_err().unwrap(), PoolError::Unauthorized);
    assert_eq!(f.pool.get_investor_position(&f.lp, &f.usdc).1, PRINCIPAL);
}

#[test]
fn market_settle_listing_rejects_when_no_market_configured() {
    let f = setup();
    let buyer = Address::generate(&f.env);
    let market = Address::generate(&f.env);
    let s = ListingSettlement {
        buyer,
        seller: f.lp.clone(),
        invoice_id: INVOICE_ID,
        is_co_funding: false,
        amount_or_bps: 1,
        price: 1,
    };
    let r = f.pool.try_market_settle_listing(&market, &s);
    assert_eq!(r.unwrap_err().unwrap(), PoolError::NotInitialized);
}

#[test]
fn market_settle_listing_rejects_self_trade_and_overselling() {
    let f = setup();
    let m = market_setup(&f);

    let mut self_trade = settlement(&f, &m, 50_000_000);
    self_trade.buyer = f.lp.clone();
    let r = f.pool.try_market_settle_listing(&m.market, &self_trade);
    assert_eq!(r.unwrap_err().unwrap(), PoolError::Unauthorized);

    let mut oversell = settlement(&f, &m, 50_000_000);
    oversell.amount_or_bps = (PRINCIPAL + 1) as u64;
    let r = f.pool.try_market_settle_listing(&m.market, &oversell);
    assert_eq!(
        r.unwrap_err().unwrap(),
        PoolError::InsufficientListingBalance
    );
    // Neither rejected attempt moved anything.
    assert_eq!(f.pool.get_investor_position(&f.lp, &f.usdc).1, PRINCIPAL);
}

#[test]
fn market_settle_listing_rejects_buyer_without_enough_shares() {
    let f = setup();
    let m = market_setup(&f);
    let r = f
        .pool
        .try_market_settle_listing(&m.market, &settlement(&f, &m, DEPOSIT + 1));
    assert_eq!(r.unwrap_err().unwrap(), PoolError::InvalidAmount);
}

#[test]
fn market_settle_listing_rejects_unknown_invoice() {
    let f = setup();
    let m = market_setup(&f);
    let mut s = settlement(&f, &m, 50_000_000);
    s.invoice_id = 404;
    let r = f.pool.try_market_settle_listing(&m.market, &s);
    assert_eq!(r.unwrap_err().unwrap(), PoolError::InvoiceNotFound);
}

// ───────────────────────── risk_liquidate_collateral ─────────────────────

const COLLATERAL: i128 = 20_000_000;

/// Funds the invoice, posts collateral for it and registers a risk contract
/// through the governance setter (the legacy `set_risk_contract` requires a
/// timelocked proposal).
fn risk_setup(f: &Fx) -> Address {
    let governance = Address::generate(&f.env);
    f.pool.set_governance_address(&f.admin, &governance);
    let risk = Address::generate(&f.env);
    f.pool.set_risk_contract_via_governance(&governance, &risk);

    // Collateral must be posted before the invoice is funded.
    token::StellarAssetClient::new(&f.env, &f.usdc).mint(&f.sme, &COLLATERAL);
    f.pool
        .deposit_collateral(&INVOICE_ID, &f.sme, &f.usdc, &COLLATERAL);
    f.fund();
    risk
}

#[test]
fn risk_liquidate_collateral_seizes_collateral_to_the_risk_contract() {
    let f = setup();
    let risk = risk_setup(&f);
    assert_eq!(f.balance(&risk), 0);
    let pool_balance_before = f.balance(&f.pool_id);
    let sme_balance_before = f.balance(&f.sme);

    // Past due and unpaid -> eligible.
    f.env.ledger().with_mut(|l| l.timestamp = f.due_date + 1);
    f.pool.risk_liquidate_collateral(&risk, &INVOICE_ID);

    // Real token balances: the collateral leaves the pool for the risk contract.
    assert_eq!(f.balance(&risk), COLLATERAL);
    assert_eq!(f.balance(&f.pool_id), pool_balance_before - COLLATERAL);
    assert_eq!(f.balance(&f.sme), sme_balance_before, "SME is not paid out");

    let deposit: CollateralDeposit = f.pool.get_collateral_deposit(&INVOICE_ID).unwrap();
    assert!(deposit.settled);
    assert_eq!(deposit.seized_at, f.due_date + 1);
    // Deployed capital is not touched by a seizure.
    assert_eq!(f.pool.get_token_totals(&f.usdc).total_deployed, PRINCIPAL);
}

#[test]
fn risk_liquidate_collateral_cannot_run_twice() {
    let f = setup();
    let risk = risk_setup(&f);
    f.env.ledger().with_mut(|l| l.timestamp = f.due_date + 1);
    f.pool.risk_liquidate_collateral(&risk, &INVOICE_ID);
    let r = f.pool.try_risk_liquidate_collateral(&risk, &INVOICE_ID);
    assert_eq!(r.unwrap_err().unwrap(), PoolError::CollateralAlreadySettled);
    assert_eq!(f.balance(&risk), COLLATERAL, "no second payout");
}

#[test]
fn risk_liquidate_collateral_rejects_callers_other_than_the_risk_contract() {
    let f = setup();
    let _risk = risk_setup(&f);
    f.env.ledger().with_mut(|l| l.timestamp = f.due_date + 1);
    for caller in [Address::generate(&f.env), f.admin.clone(), f.sme.clone()] {
        let r = f.pool.try_risk_liquidate_collateral(&caller, &INVOICE_ID);
        assert_eq!(r.unwrap_err().unwrap(), PoolError::Unauthorized);
    }
    assert!(!f.pool.get_collateral_deposit(&INVOICE_ID).unwrap().settled);
}

#[test]
fn risk_liquidate_collateral_requires_a_configured_risk_contract() {
    let f = setup();
    let caller = Address::generate(&f.env);
    let r = f.pool.try_risk_liquidate_collateral(&caller, &INVOICE_ID);
    assert_eq!(r.unwrap_err().unwrap(), PoolError::NotInitialized);
}

#[test]
fn risk_liquidate_collateral_requires_the_invoice_to_be_overdue() {
    let f = setup();
    let risk = risk_setup(&f);
    // Still before the due date.
    let r = f.pool.try_risk_liquidate_collateral(&risk, &INVOICE_ID);
    assert_eq!(r.unwrap_err().unwrap(), PoolError::InvoiceNotDue);
    assert!(!f.pool.get_collateral_deposit(&INVOICE_ID).unwrap().settled);
    assert_eq!(f.balance(&risk), 0);
}

#[test]
fn risk_liquidate_collateral_rejects_unknown_invoice() {
    let f = setup();
    let risk = risk_setup(&f);
    let r = f.pool.try_risk_liquidate_collateral(&risk, &777u64);
    assert_eq!(r.unwrap_err().unwrap(), PoolError::InvoiceNotFound);
}

// ───────────────────────────── run_migration ─────────────────────────────

fn migration_version(f: &Fx) -> u32 {
    f.env.as_contract(&f.pool_id, || {
        f.env
            .storage()
            .instance()
            .get(&DataKey::MigrationVersion)
            .unwrap_or(0u32)
    })
}

#[test]
fn run_migration_advances_the_version_and_is_idempotent() {
    let f = setup();
    assert_eq!(migration_version(&f), 0);
    let totals_before = f.pool.get_token_totals(&f.usdc);
    let config_before = f.pool.get_config();

    f.pool.run_migration(&f.admin);
    assert_eq!(migration_version(&f), 1);

    // Running again is a no-op.
    f.pool.run_migration(&f.admin);
    assert_eq!(migration_version(&f), 1);

    // The migration does not touch accounting or config.
    let totals_after = f.pool.get_token_totals(&f.usdc);
    assert_eq!(totals_after.pool_value, totals_before.pool_value);
    assert_eq!(totals_after.total_deployed, totals_before.total_deployed);
    assert_eq!(f.pool.get_config().yield_bps, config_before.yield_bps);
}

#[test]
fn run_migration_rejects_non_admin() {
    let f = setup();
    let impostor = Address::generate(&f.env);
    let r = f.pool.try_run_migration(&impostor);
    assert_eq!(r.unwrap_err().unwrap(), PoolError::Unauthorized);
    assert_eq!(migration_version(&f), 0);
}
