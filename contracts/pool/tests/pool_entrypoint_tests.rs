#![cfg(test)]

//! #1648: coverage for the pool entrypoints that move money or rewrite
//! storage but had no tests: `cancel_funded_invoice`, `market_settle_listing`,
//! `risk_liquidate_collateral` and `run_migration`. Each gets a happy path,
//! authorization failures and accounting assertions.

use pool::{
    CoFundingRound, CoFundingStatus, CollateralDeposit, DataKey, FundingPool, FundingPoolClient,
    InvestorPosition, InvoiceRecord, InvoiceStatus, ListingSettlement, OpenCoFundingRequest,
    PoolError,
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
    pub fn is_invoice_defaulted(env: Env, _id: u64) -> bool {
        env.storage()
            .instance()
            .get(&symbol_short!("dflt"))
            .unwrap_or(false)
    }
    pub fn set_defaulted(env: Env, defaulted: bool) {
        env.storage()
            .instance()
            .set(&symbol_short!("dflt"), &defaulted);
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
            status: InvoiceStatus::Verified,
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
    fn fund_cofunded(&self) {
        self.pool.open_co_funding(
            &self.admin,
            &OpenCoFundingRequest {
                invoice_id: INVOICE_ID,
                token: self.usdc.clone(),
                target_principal: PRINCIPAL,
                sme: self.sme.clone(),
                due_date: self.due_date,
                funding_deadline: self.env.ledger().timestamp() + 86_400,
                min_commitment: 0,
                max_investor_bps: 10_000,
            },
        );
        self.pool.commit_to_invoice(&self.lp, &INVOICE_ID, &PRINCIPAL);
        self.pool.finalize_co_funding(&self.lp, &INVOICE_ID);
    }
    fn balance(&self, who: &Address) -> i128 {
        token::Client::new(&self.env, &self.usdc).balance(who)
    }
}

// ───────────────────────── cancel_funded_invoice ─────────────────────────

#[test]
fn cancel_funded_invoice_writes_off_unreturned_principal_and_releases_collateral() {
    let f = setup();
    f.fund();

    let collateral = 25_000i128;
    token::StellarAssetClient::new(&f.env, &f.usdc).mint(&f.sme, &collateral);
    f.pool
        .deposit_collateral(&INVOICE_ID, &f.sme, &f.usdc, &collateral);

    let before = f.pool.get_token_totals(&f.usdc);
    assert_eq!(before.total_deployed, PRINCIPAL);
    let sme_before_cancel = f.balance(&f.sme);
    let stats_before = f.pool.get_storage_stats();

    f.pool.cancel_funded_invoice(&f.admin, &INVOICE_ID);

    let after = f.pool.get_token_totals(&f.usdc);
    assert_eq!(after.total_deployed, 0, "principal no longer deployed");
    assert_eq!(
        after.pool_value,
        before.pool_value - PRINCIPAL,
        "unreturned principal is written off from NAV"
    );
    assert_eq!(
        f.balance(&f.sme),
        sme_before_cancel + collateral,
        "collateral is returned to its depositor"
    );
    let col = f.pool.get_collateral_deposit(&INVOICE_ID).unwrap();
    assert!(col.settled);
    assert_eq!(col.released_at, f.env.ledger().timestamp());
    assert!(f.pool.get_funded_invoice(&INVOICE_ID).is_none());
    assert_eq!(
        f.pool.get_storage_stats().active_funded_invoices,
        stats_before.active_funded_invoices - 1
    );
}

#[test]
fn cancel_funded_invoice_recovers_principal_when_sme_has_approved_pool() {
    let f = setup();
    f.fund();
    let pool_balance_before_cancel = f.balance(&f.pool_id);
    let before = f.pool.get_token_totals(&f.usdc);

    let expiration = f.env.ledger().sequence() + 100;
    token::Client::new(&f.env, &f.usdc).approve(
        &f.sme,
        &f.pool_id,
        &PRINCIPAL,
        &expiration,
    );
    f.pool.cancel_funded_invoice(&f.admin, &INVOICE_ID);

    let after = f.pool.get_token_totals(&f.usdc);
    assert_eq!(after.total_deployed, 0);
    assert_eq!(after.pool_value, before.pool_value, "returned cash replaces the receivable");
    assert_eq!(f.balance(&f.pool_id), pool_balance_before_cancel + PRINCIPAL);
    assert!(f.pool.get_funded_invoice(&INVOICE_ID).is_none());
}

#[test]
fn cancel_cofunded_invoice_clears_claims_and_keeps_the_already_written_down_nav() {
    let f = setup();
    f.fund_cofunded();
    let before = f.pool.get_token_totals(&f.usdc);
    let shares_before_cancel = f.share.balance(&f.lp);
    assert_eq!(f.pool.get_co_fund_share(&INVOICE_ID, &f.lp), 10_000);

    f.pool.cancel_funded_invoice(&f.admin, &INVOICE_ID);

    let after = f.pool.get_token_totals(&f.usdc);
    assert_eq!(after.pool_value, before.pool_value, "co-funded principal was already excluded from NAV");
    assert_eq!(f.pool.get_co_fund_share(&INVOICE_ID, &f.lp), 0);
    assert_eq!(f.share.balance(&f.lp), shares_before_cancel);
    let (round, commitment_exists) = f.env.as_contract(&f.pool_id, || {
        let round: CoFundingRound = f
            .env
            .storage()
            .persistent()
            .get(&DataKey::CoFundingRound(INVOICE_ID))
            .unwrap();
        let committed = f
            .env
            .storage()
            .persistent()
            .has(&DataKey::CoFundCommitted(INVOICE_ID, f.lp.clone()));
        (round, committed)
    });
    assert_eq!(round.status, CoFundingStatus::Cancelled);
    assert_eq!(round.committed_principal, 0);
    assert!(round.participants.is_empty());
    assert!(!commitment_exists);
    assert!(f.pool.get_funded_invoice(&INVOICE_ID).is_none());
}

#[test]
fn cancel_cofunded_invoice_distributes_returned_principal_to_participants() {
    let f = setup();
    f.fund_cofunded();
    let before = f.pool.get_token_totals(&f.usdc);
    let pool_balance_before_cancel = f.balance(&f.pool_id);
    let shares_before_cancel = f.share.balance(&f.lp);

    let expiration = f.env.ledger().sequence() + 100;
    token::Client::new(&f.env, &f.usdc).approve(
        &f.sme,
        &f.pool_id,
        &PRINCIPAL,
        &expiration,
    );
    f.pool.cancel_funded_invoice(&f.admin, &INVOICE_ID);

    let after = f.pool.get_token_totals(&f.usdc);
    assert_eq!(after.pool_value, before.pool_value + PRINCIPAL);
    assert_eq!(f.balance(&f.pool_id), pool_balance_before_cancel + PRINCIPAL);
    assert!(f.share.balance(&f.lp) > shares_before_cancel);
    assert_eq!(f.pool.get_co_fund_share(&INVOICE_ID, &f.lp), 0);
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

    // #1629: the collateral was sent away, so it must not be credited to NAV. The
    // unrecovered principal is written down exactly as a default seizure does, which
    // leaves NAV equal to the real token balance.
    let totals = f.pool.get_token_totals(&f.usdc);
    assert_eq!(totals.total_deployed, 0, "unrecovered principal written down");
    assert_eq!(
        totals.pool_value,
        f.balance(&f.pool_id),
        "NAV equals the pool's real balance"
    );
}

#[test]
fn risk_liquidate_collateral_does_not_credit_collateral_to_nav() {
    let f = setup();
    let risk = risk_setup(&f);
    let nav_before = f.pool.get_token_totals(&f.usdc).pool_value;
    assert_eq!(nav_before, DEPOSIT, "posted collateral is not counted in NAV");

    f.env.ledger().with_mut(|l| l.timestamp = f.due_date + 1);
    f.pool.risk_liquidate_collateral(&risk, &INVOICE_ID);

    // The pool's NAV drops by the written-off principal only, not by the
    // principal less a collateral credit that never should have been added.
    let nav_after = f.pool.get_token_totals(&f.usdc).pool_value;
    assert_eq!(nav_after, nav_before - PRINCIPAL);
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

// ─────────────────────── collateral posting and withdrawal ───────────────────

const WITHDRAW_TIMEOUT_SECS: u64 = 30 * 86_400;

fn post_collateral(f: &Fx) {
    token::StellarAssetClient::new(&f.env, &f.usdc).mint(&f.sme, &COLLATERAL);
    f.pool
        .deposit_collateral(&INVOICE_ID, &f.sme, &f.usdc, &COLLATERAL);
}

fn set_invoice_status(f: &Fx, status: InvoiceStatus) {
    f.invoice.set_invoice(
        &INVOICE_ID,
        &InvoiceRecord {
            owner: f.sme.clone(),
            amount: PRINCIPAL * 2,
            due_date: f.due_date,
            funded_amount: 0,
            status,
        },
    );
}

#[test]
fn deposit_collateral_rejects_a_depositor_who_is_not_the_invoice_owner() {
    // #1627: a stranger must not be able to post collateral against someone
    // else's invoice and block its funding.
    let f = setup();
    let attacker = Address::generate(&f.env);
    token::StellarAssetClient::new(&f.env, &f.usdc).mint(&attacker, &1);

    let r = f.pool.try_deposit_collateral(&INVOICE_ID, &attacker, &f.usdc, &1);
    assert_eq!(r.unwrap_err().unwrap(), PoolError::CollateralNotInvoiceOwner);
    assert!(f.pool.get_collateral_deposit(&INVOICE_ID).is_none());
    assert_eq!(f.balance(&attacker), 1, "the attacker's tokens are not taken");

    // The real owner can still post afterwards.
    post_collateral(&f);
    assert_eq!(
        f.pool.get_collateral_deposit(&INVOICE_ID).unwrap().amount,
        COLLATERAL
    );
}

#[test]
fn deposit_collateral_requires_the_invoice_to_exist() {
    // #1627: collateral for an invoice that doesn't exist is rejected.
    let f = setup();
    token::StellarAssetClient::new(&f.env, &f.usdc).mint(&f.sme, &COLLATERAL);
    let r = f
        .pool
        .try_deposit_collateral(&777u64, &f.sme, &f.usdc, &COLLATERAL);
    assert_eq!(r.unwrap_err().unwrap(), PoolError::InvoiceNotFound);
}

#[test]
fn withdraw_collateral_returns_it_once_the_invoice_is_cancelled() {
    // #1628: collateral for a cancelled invoice is no longer locked.
    let f = setup();
    post_collateral(&f);
    set_invoice_status(&f, InvoiceStatus::Cancelled);

    f.pool.withdraw_collateral(&INVOICE_ID, &f.sme);

    assert_eq!(f.balance(&f.sme), COLLATERAL, "the depositor gets it back");
    let deposit = f.pool.get_collateral_deposit(&INVOICE_ID).unwrap();
    assert!(deposit.settled);
    assert_eq!(deposit.released_at, f.env.ledger().timestamp());
    let r = f.pool.try_withdraw_collateral(&INVOICE_ID, &f.sme);
    assert_eq!(r.unwrap_err().unwrap(), PoolError::CollateralAlreadySettled);
}

#[test]
fn withdraw_collateral_is_locked_while_fundable_until_the_timeout() {
    // #1628: a verified invoice can still be funded, so its collateral stays
    // put until the timeout elapses.
    let f = setup();
    post_collateral(&f);

    let r = f.pool.try_withdraw_collateral(&INVOICE_ID, &f.sme);
    assert_eq!(r.unwrap_err().unwrap(), PoolError::CollateralWithdrawalLocked);
    assert!(!f.pool.get_collateral_deposit(&INVOICE_ID).unwrap().settled);

    f.env
        .ledger()
        .with_mut(|l| l.timestamp += WITHDRAW_TIMEOUT_SECS);
    f.pool.withdraw_collateral(&INVOICE_ID, &f.sme);
    assert_eq!(f.balance(&f.sme), COLLATERAL);
}

#[test]
fn withdraw_collateral_is_rejected_while_the_invoice_is_funded() {
    // #1628: once funded, the deposit secures the loan and can't be pulled out,
    // even if the invoice record later looks terminal.
    let f = setup();
    post_collateral(&f);
    f.fund();
    set_invoice_status(&f, InvoiceStatus::Cancelled);

    let r = f.pool.try_withdraw_collateral(&INVOICE_ID, &f.sme);
    assert_eq!(r.unwrap_err().unwrap(), PoolError::CollateralInUse);
    assert!(!f.pool.get_collateral_deposit(&INVOICE_ID).unwrap().settled);
    assert_eq!(
        f.balance(&f.sme),
        PRINCIPAL,
        "the SME holds only the funded principal, not the collateral"
    );
}

#[test]
fn withdraw_collateral_rejects_anyone_but_the_depositor() {
    let f = setup();
    post_collateral(&f);
    set_invoice_status(&f, InvoiceStatus::Cancelled);
    let stranger = Address::generate(&f.env);

    let r = f.pool.try_withdraw_collateral(&INVOICE_ID, &stranger);
    assert_eq!(r.unwrap_err().unwrap(), PoolError::Unauthorized);
    assert!(!f.pool.get_collateral_deposit(&INVOICE_ID).unwrap().settled);
}

#[test]
fn owner_can_post_collateral_again_after_a_withdrawal() {
    let f = setup();
    post_collateral(&f);
    set_invoice_status(&f, InvoiceStatus::Cancelled);
    f.pool.withdraw_collateral(&INVOICE_ID, &f.sme);

    // Withdrawn collateral secures nothing, so the owner may post again.
    set_invoice_status(&f, InvoiceStatus::Verified);
    post_collateral(&f);
    let deposit = f.pool.get_collateral_deposit(&INVOICE_ID).unwrap();
    assert!(!deposit.settled);
    assert_eq!(deposit.amount, COLLATERAL);
}

// ─────────────────────── partial repayment accounting ────────────────────────

#[test]
fn partial_repayment_survives_a_default_with_the_recovered_half_in_nav() {
    // #1630: 50% repaid, then the invoice defaults. NAV keeps the recovered
    // half and writes off only the unpaid remainder.
    let f = setup();
    post_collateral(&f);
    f.fund();
    // Repaid in the funding ledger, so no interest or fee has accrued yet.
    f.pool.repay_invoice(&INVOICE_ID, &f.sme, &(PRINCIPAL / 2));

    let after_partial = f.pool.get_token_totals(&f.usdc);
    assert_eq!(
        after_partial.total_deployed,
        PRINCIPAL / 2,
        "the recovered principal is no longer deployed"
    );
    assert_eq!(
        after_partial.pool_value, DEPOSIT,
        "partial recovery is not a loss"
    );

    f.invoice.set_defaulted(&true);
    f.pool.liquidate_invoice(&f.admin, &INVOICE_ID);

    let after_default = f.pool.get_token_totals(&f.usdc);
    assert_eq!(after_default.total_deployed, 0);
    assert_eq!(
        after_default.pool_value,
        DEPOSIT - PRINCIPAL / 2 + COLLATERAL,
        "only the unrecovered half is written off, and the collateral is credited"
    );
    assert_eq!(
        after_default.pool_value,
        f.balance(&f.pool_id),
        "NAV equals the pool's real balance"
    );
}

#[test]
fn repayment_after_a_default_write_off_is_recovered_into_nav() {
    // #1630: once the remainder has been written off, a later payment is a
    // recovery of a loss NAV already took, so it goes back into pool_value
    // rather than being paid out as yield.
    let f = setup();
    post_collateral(&f);
    f.fund();
    f.pool.repay_invoice(&INVOICE_ID, &f.sme, &(PRINCIPAL / 2));
    f.invoice.set_defaulted(&true);
    f.pool.liquidate_invoice(&f.admin, &INVOICE_ID);

    f.pool.repay_invoice(&INVOICE_ID, &f.sme, &(PRINCIPAL / 2));

    let totals = f.pool.get_token_totals(&f.usdc);
    assert_eq!(totals.total_deployed, 0, "nothing is written off twice");
    assert_eq!(
        totals.pool_value,
        f.balance(&f.pool_id),
        "the recovery is reflected in NAV"
    );
}

#[test]
fn partial_then_full_repayment_books_principal_exactly_once() {
    // #1630: an over-sized final payment is capped at what is owed, and the
    // books end where a single full repayment would have left them.
    let f = setup();
    f.fund();
    f.pool.repay_invoice(&INVOICE_ID, &f.sme, &(PRINCIPAL / 2));
    f.pool.repay_invoice(&INVOICE_ID, &f.sme, &PRINCIPAL);

    let totals = f.pool.get_token_totals(&f.usdc);
    assert_eq!(totals.total_deployed, 0, "principal is released exactly once");
    assert_eq!(totals.pool_value, DEPOSIT, "NAV is unchanged by repayment");
    assert_eq!(totals.total_paid_out, PRINCIPAL, "the SME paid what it owed");
    assert_eq!(f.balance(&f.sme), 0, "only the amount owed was taken");
    assert_eq!(f.balance(&f.pool_id), DEPOSIT, "the pool holds every token again");
    assert_eq!(
        f.pool.get_funded_invoice(&INVOICE_ID).unwrap().repaid_amount,
        PRINCIPAL
    );
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
