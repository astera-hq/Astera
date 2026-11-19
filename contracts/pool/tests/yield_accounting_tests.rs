#![cfg(test)]

// Regression coverage for the LP yield accounting fixes:
// - #1619: repaid interest must be distributed once (reward_per_share only),
//   not also through pool_value.
// - #1620: a new depositor (or share-token transferee) must not be able to
//   claim yield realized before they held the shares.
// - #1621: yield accrued before a share-balance change (withdraw, co-funding
//   commit, top-up deposit) must still be claimable afterwards.

use pool::{FundingPool, FundingPoolClient, OpenCoFundingRequest};
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
    // Plain holder-to-holder transfer that bypasses the pool entirely.
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        let from_balance = Self::balance(env.clone(), from.clone());
        let to_balance = Self::balance(env.clone(), to.clone());
        env.storage()
            .persistent()
            .set(&from, &(from_balance - amount));
        env.storage().persistent().set(&to, &(to_balance + amount));
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

struct Ctx<'a> {
    env: Env,
    pool: FundingPoolClient<'a>,
    admin: Address,
    usdc: Address,
    share: DummyShareClient<'a>,
    sme: Address,
}

fn setup<'a>() -> Ctx<'a> {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 100_000);
    let admin = Address::generate(&env);
    let usdc = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let share_id = env.register(DummyShare, ());
    let invoice_contract = env.register(DummyInvoice, ());
    let pool_id = env.register(FundingPool, ());
    let pool = FundingPoolClient::new(&env, &pool_id);
    DummyInvoiceClient::new(&env, &invoice_contract).set_pool(&pool_id);
    pool.initialize(&admin, &usdc, &share_id, &invoice_contract);
    pool.set_max_investor_concentration(&admin, &10_000u32);
    let share = DummyShareClient::new(&env, &share_id);
    let sme = Address::generate(&env);
    token::StellarAssetClient::new(&env, &usdc).mint(&sme, &1_000_000_000);
    Ctx {
        env,
        pool,
        admin,
        usdc,
        share,
        sme,
    }
}

impl Ctx<'_> {
    fn balance(&self, who: &Address) -> i128 {
        token::Client::new(&self.env, &self.usdc).balance(who)
    }

    fn deposit(&self, who: &Address, amount: i128) {
        token::StellarAssetClient::new(&self.env, &self.usdc).mint(who, &amount);
        self.pool.deposit(who, &self.usdc, &amount, &None);
    }

    /// Funds `invoice_id` from the pool, lets interest accrue and repays it
    /// in full. Returns the interest realized for LPs.
    fn fund_and_repay(&self, invoice_id: u64, principal: i128) -> i128 {
        let due = self.env.ledger().timestamp() + 60 * 86_400;
        self.pool.fund_invoice(
            &self.admin,
            &invoice_id,
            &principal,
            &self.sme,
            &due,
            &self.usdc,
        );
        self.env.ledger().with_mut(|l| l.timestamp += 30 * 86_400);
        let fee_before = self.pool.get_token_totals(&self.usdc).total_fee_revenue;
        let due_amount = self.pool.estimate_repayment(&invoice_id, &None);
        self.pool.repay_invoice(&invoice_id, &self.sme, &due_amount);
        let fee = self.pool.get_token_totals(&self.usdc).total_fee_revenue - fee_before;
        let interest = due_amount - principal - fee;
        assert!(interest > 0, "test setup should realize some interest");
        interest
    }

    fn claim(&self, who: &Address) -> i128 {
        let before = self.balance(who);
        self.pool.claim_yield(who, &self.usdc);
        self.balance(who) - before
    }

    fn withdraw_all(&self, who: &Address) -> i128 {
        let shares = self.share.balance(who);
        let before = self.balance(who);
        self.pool.withdraw(who, &self.usdc, &shares);
        self.balance(who) - before
    }
}

// ── #1619 ────────────────────────────────────────────────────────────────────

#[test]
fn test_claim_then_full_withdraw_pays_principal_plus_interest_once() {
    let ctx = setup();
    let lp = Address::generate(&ctx.env);
    ctx.deposit(&lp, 10_000_000);
    let pool_value_before = ctx.pool.get_token_totals(&ctx.usdc).pool_value;

    let interest = ctx.fund_and_repay(1, 5_000_000);

    // Interest is not double-booked into the share price.
    let tt = ctx.pool.get_token_totals(&ctx.usdc);
    assert_eq!(tt.pool_value, pool_value_before);
    assert!(tt.reward_per_share > 0);

    let claimed = ctx.claim(&lp);
    let withdrawn = ctx.withdraw_all(&lp);
    assert_eq!(withdrawn, 10_000_000);
    assert!(claimed <= interest);
    assert!(claimed >= interest - 1); // reward_per_share rounding dust
    assert_eq!(ctx.claim(&lp), 0);
}

#[test]
fn test_every_lp_can_exit_after_claiming_yield() {
    let ctx = setup();
    let a = Address::generate(&ctx.env);
    let b = Address::generate(&ctx.env);
    ctx.deposit(&a, 6_000_000);
    ctx.deposit(&b, 4_000_000);

    let interest = ctx.fund_and_repay(1, 8_000_000);

    // A claims first then exits; B exits first then claims. Before #1619 the
    // last withdrawer hit a shortfall because interest was paid out twice.
    let a_total = ctx.claim(&a) + ctx.withdraw_all(&a);
    let b_total = ctx.withdraw_all(&b) + ctx.claim(&b);

    assert!(a_total <= 6_000_000 + interest * 6 / 10 + 1);
    assert!(a_total >= 6_000_000 + interest * 6 / 10 - 1);
    assert!(b_total <= 4_000_000 + interest * 4 / 10 + 1);
    assert!(b_total >= 4_000_000 + interest * 4 / 10 - 1);
    assert!(a_total + b_total <= 10_000_000 + interest);

    // What remains in the pool still covers the protocol's own revenue.
    let protocol_revenue = ctx.pool.get_protocol_revenue(&ctx.usdc);
    assert!(ctx.balance(&ctx.pool.address) >= protocol_revenue);
}

// ── #1620 ────────────────────────────────────────────────────────────────────

#[test]
fn test_depositor_after_repayment_cannot_claim_historical_yield() {
    let ctx = setup();
    let a = Address::generate(&ctx.env);
    let b = Address::generate(&ctx.env);
    ctx.deposit(&a, 10_000_000);
    let interest = ctx.fund_and_repay(1, 5_000_000);

    ctx.deposit(&b, 10_000_000);
    assert_eq!(ctx.claim(&b), 0);

    // The yield still belongs to A in full.
    let a_claimed = ctx.claim(&a);
    assert!(a_claimed >= interest - 1 && a_claimed <= interest);
}

#[test]
fn test_share_transferee_cannot_claim_historical_yield() {
    let ctx = setup();
    let a = Address::generate(&ctx.env);
    let c = Address::generate(&ctx.env);
    ctx.deposit(&a, 10_000_000);
    let interest = ctx.fund_and_repay(1, 5_000_000);

    // Shares moved outside the pool do not carry the old reward snapshot.
    ctx.share.transfer(&a, &c, &5_000_000);
    assert_eq!(ctx.claim(&c), 0);
    assert!(ctx.claim(&a) <= interest);

    // From now on C earns on the shares it holds.
    let interest2 = ctx.fund_and_repay(2, 5_000_000);
    let c_claimed = ctx.claim(&c);
    assert!(c_claimed > 0);
    assert!(c_claimed <= interest2 / 2 + 1);
}

// ── #1621 ────────────────────────────────────────────────────────────────────

#[test]
fn test_yield_survives_partial_withdraw() {
    let ctx = setup();
    let lp = Address::generate(&ctx.env);
    ctx.deposit(&lp, 10_000_000);
    let interest = ctx.fund_and_repay(1, 5_000_000);

    ctx.pool.withdraw(&lp, &ctx.usdc, &6_000_000);
    let claimed = ctx.claim(&lp);
    assert!(claimed >= interest - 1 && claimed <= interest);
}

#[test]
fn test_yield_survives_full_withdraw() {
    let ctx = setup();
    let lp = Address::generate(&ctx.env);
    ctx.deposit(&lp, 10_000_000);
    let interest = ctx.fund_and_repay(1, 5_000_000);

    assert_eq!(ctx.withdraw_all(&lp), 10_000_000);
    let claimed = ctx.claim(&lp);
    assert!(claimed >= interest - 1 && claimed <= interest);
}

#[test]
fn test_yield_survives_co_funding_commit() {
    let ctx = setup();
    let lp = Address::generate(&ctx.env);
    ctx.deposit(&lp, 10_000_000);
    let interest = ctx.fund_and_repay(1, 5_000_000);

    let now = ctx.env.ledger().timestamp();
    ctx.pool.open_co_funding(
        &ctx.admin,
        &OpenCoFundingRequest {
            invoice_id: 2,
            token: ctx.usdc.clone(),
            target_principal: 4_000_000,
            sme: ctx.sme.clone(),
            due_date: now + 60 * 86_400,
            funding_deadline: now + 3_600,
            min_commitment: 100,
            max_investor_bps: 0,
        },
    );
    let shares_before = ctx.share.balance(&lp);
    ctx.pool.commit_to_invoice(&lp, &2u64, &3_000_000i128);
    assert!(ctx.share.balance(&lp) < shares_before);

    let claimed = ctx.claim(&lp);
    assert!(claimed >= interest - 1 && claimed <= interest);
}

#[test]
fn test_top_up_deposit_keeps_accrued_yield_and_does_not_inflate_it() {
    let ctx = setup();
    let lp = Address::generate(&ctx.env);
    ctx.deposit(&lp, 10_000_000);
    let interest = ctx.fund_and_repay(1, 5_000_000);

    ctx.deposit(&lp, 30_000_000);
    let claimed = ctx.claim(&lp);
    assert!(claimed >= interest - 1 && claimed <= interest);
}
