#![cfg(test)]

// Property-based (proptest) fuzz coverage for the #1035 limit order book in
// `secondary_market`. Each property is stated as a universally-quantified
// invariant that must hold for *any* randomised sequence of asks and bids
// placed by distinct participants.
//
// Constants that must stay in sync with src/lib.rs:
//   MAX_MATCHES_PER_CALL     = 10
//   MAX_ORDERS_PER_BOOK_SIDE = 30
//   PRICE_SCALE              = 10_000_000
//
// All tests use `CoFunding` orders because that kind requires only a
// pre-funded co-funding round, which the shared `setup_env` helper
// establishes once per test. `SingleFunded` shares the same matching
// code-path; covering it would just duplicate the setup complexity.

use proptest::prelude::*;
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    token, Address, Env, Symbol,
};

use pool::{DataKey, FundingPool, FundingPoolClient, InvestorPosition, OpenCoFundingRequest};
use secondary_market::{
    ListingKind, OrderSide, OrderStatus, PlaceOrderRequest, SecondaryMarket, SecondaryMarketClient,
};

// ── Constants (mirrors src/lib.rs) ───────────────────────────────────────────

const PRICE_SCALE: i128 = 10_000_000;
const MAX_MATCHES_PER_CALL: u32 = 10;

// ── Shared dummy contracts (same pattern as order_book_tests.rs) ─────────────

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
    pub fn record_funding(_env: Env, _id: u64, _amount: i128, _pool: Address) {}
}

// ── Test environment ─────────────────────────────────────────────────────────

/// Set up pool + secondary market on an already-created `Env`. Returns the
/// two clients, the admin address, a pre-funded invoice_id (invoice 1,
/// 100% committed by a bootstrap seller), and the USDC token address.
///
/// Every property test creates its own `Env` and calls `setup_env(&env)`.
/// This mirrors the `setup(&env)` pattern used by `order_book_tests.rs` and
/// avoids the self-referential tuple problem that would arise from trying to
/// return an `Env` alongside clients that borrow it.
fn setup_env<'a>(
    env: &'a Env,
) -> (
    FundingPoolClient<'a>,
    SecondaryMarketClient<'a>,
    Address, // admin — needed by open_co_funding, set_* calls
    u64,     // invoice_id (bootstrap invoice, fully committed)
    Address, // usdc
) {
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let pool_id = env.register(FundingPool, ());
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

    let market_id = env.register(SecondaryMarket, ());
    let market_client = SecondaryMarketClient::new(env, &market_id);
    market_client.initialize(&admin, &pool_id);
    pool_client.set_secondary_market_contract(&admin, &market_id);

    // Pre-fund invoice 1 with a bootstrap seller so it's available for tests
    // that want a pre-existing fully-committed round without extra setup.
    let invoice_id: u64 = 1;
    let sme = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86_400;
    let deadline = env.ledger().timestamp() + 3_600;
    let target: i128 = 100_000;

    pool_client.open_co_funding(
        &admin,
        &OpenCoFundingRequest {
            invoice_id,
            token: usdc_id.clone(),
            target_principal: target,
            sme: sme.clone(),
            due_date,
            funding_deadline: deadline,
            min_commitment: 1,
            max_investor_bps: 0,
        },
    );

    let bootstrap = Address::generate(env);
    token::StellarAssetClient::new(env, &usdc_id).mint(&bootstrap, &target);
    pool_client.deposit(&bootstrap, &usdc_id, &target, &None);
    pool_client.commit_to_invoice(&bootstrap, &invoice_id, &target);
    pool_client.finalize_co_funding(&bootstrap, &invoice_id);

    (pool_client, market_client, admin, invoice_id, usdc_id)
}

/// Mint USDC to a fresh address, deposit it into pool, and return the address.
fn fund_participant(
    env: &Env,
    pool: &FundingPoolClient<'_>,
    usdc: &Address,
    amount: i128,
) -> Address {
    let addr = Address::generate(env);
    token::StellarAssetClient::new(env, usdc).mint(&addr, &amount);
    pool.deposit(&addr, usdc, &amount, &None);
    addr
}

/// Read `available` balance from pool's persistent storage for an investor.
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

/// Open a fresh co-funding round on `invoice_id`, commit `seller` for
/// `bps_per` units, finalise, and return the seller address.
/// The seller is minted `bps_per * 100` USDC before depositing so they have
/// both enough to commit and a healthy available balance for future bids.
fn make_seller(
    env: &Env,
    pool: &FundingPoolClient<'_>,
    usdc: &Address,
    admin: &Address,
    invoice_id: u64,
    bps_per: u64,
) -> Address {
    let seller = Address::generate(env);
    let deposit_amount = bps_per as i128 * 100;
    token::StellarAssetClient::new(env, usdc).mint(&seller, &deposit_amount);
    pool.deposit(&seller, usdc, &deposit_amount, &None);

    let sme = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86_400;
    let deadline = env.ledger().timestamp() + 3_600;

    pool.open_co_funding(
        admin,
        &OpenCoFundingRequest {
            invoice_id,
            token: usdc.clone(),
            target_principal: bps_per as i128,
            sme: sme.clone(),
            due_date,
            funding_deadline: deadline,
            min_commitment: 1,
            max_investor_bps: 0,
        },
    );
    pool.commit_to_invoice(&seller, &invoice_id, &(bps_per as i128));
    pool.finalize_co_funding(&seller, &invoice_id);
    seller
}

/// Open a shared co-funding round on `invoice_id` for `n_sellers` equal
/// participants each contributing `bps_per` units.
fn make_shared_round(
    env: &Env,
    pool: &FundingPoolClient<'_>,
    usdc: &Address,
    admin: &Address,
    invoice_id: u64,
    bps_per: u64,
    n_sellers: usize,
) -> Vec<Address> {
    let target = bps_per as i128 * n_sellers as i128;
    let mut sellers: Vec<Address> = Vec::new();
    for _ in 0..n_sellers {
        let s = Address::generate(env);
        let deposit_amount = bps_per as i128 * 10;
        token::StellarAssetClient::new(env, usdc).mint(&s, &deposit_amount);
        pool.deposit(&s, usdc, &deposit_amount, &None);
        sellers.push(s);
    }

    let sme = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86_400;
    let deadline = env.ledger().timestamp() + 3_600;

    pool.open_co_funding(
        admin,
        &OpenCoFundingRequest {
            invoice_id,
            token: usdc.clone(),
            target_principal: target,
            sme: sme.clone(),
            due_date,
            funding_deadline: deadline,
            min_commitment: 1,
            max_investor_bps: 0,
        },
    );
    for s in &sellers {
        pool.commit_to_invoice(s, &invoice_id, &(bps_per as i128));
    }
    pool.finalize_co_funding(&sellers[0], &invoice_id);
    sellers
}

/// Place a CoFunding order; thin wrapper to keep test bodies readable.
fn place_order(
    market: &SecondaryMarketClient<'_>,
    owner: &Address,
    invoice_id: u64,
    side: OrderSide,
    amount_or_bps: u64,
    price: i128,
) -> u64 {
    market.place_order(
        owner,
        &PlaceOrderRequest {
            invoice_id,
            kind: ListingKind::CoFunding,
            side,
            amount_or_bps,
            price,
            expires_at: 0u64,
        },
    )
}

// ── Properties ───────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    // ─────────────────────────────────────────────────────────────────────────
    /// Price-time priority — asks filled cheapest first.
    ///
    /// Invariant: when multiple asks rest on the book at strictly different
    /// prices and a single crossing bid arrives with enough quantity to
    /// cross all of them, the cheapest ask fills first.
    ///
    /// Setup: place `n_asks` asks on a *shared* invoice at prices
    /// PRICE_SCALE * 1, PRICE_SCALE * 2, …, PRICE_SCALE * n_asks
    /// (ascending — ask[0] is cheapest). Send one bid large enough to
    /// cross all asks at up to MAX_MATCHES_PER_CALL fills, and assert
    /// ask[0] is Filled before any more-expensive ask is Filled.
    #[test]
    fn prop_price_time_priority_asks_filled_cheapest_first(
        n_asks in 2usize..=4usize,
    ) {
        let env = Env::default();
        let (pool, market, admin, _bootstrap_invoice, usdc) = setup_env(&env);

        // Use a distinct shared invoice so all asks are on the same book.
        let shared_invoice: u64 = 10;
        let bps_per: u64 = 500; // each seller contributes 500 bps
        prop_assume!(n_asks as u64 * bps_per <= 10_000);

        // Collect (seller, ask_price, ask_id) in ascending price order.
        // ask[0] is cheapest (price = PRICE_SCALE * 1).
        let mut asks: Vec<(Address, i128, u64)> = Vec::new();

        let sellers = make_shared_round(&env, &pool, &usdc, &admin, shared_invoice, bps_per, n_asks);

        for (i, seller) in sellers.iter().enumerate() {
            // Price grows with index: ask[0] cheapest, ask[n-1] most expensive.
            let ask_price = PRICE_SCALE * (i as i128 + 1);
            let ask_id = place_order(&market, seller, shared_invoice, OrderSide::Ask, bps_per, ask_price);
            asks.push((seller.clone(), ask_price, ask_id));
        }

        // Verify our test setup has strictly ascending prices.
        for w in asks.windows(2) {
            prop_assert!(w[0].1 < w[1].1, "test setup: prices not strictly ascending");
        }

        // A bid crossing all asks: price above the most expensive ask,
        // quantity covering all asks' combined bps.
        let max_price = PRICE_SCALE * (n_asks as i128 + 1);
        let total_bps = bps_per * n_asks as u64;
        let buyer_budget = max_price * total_bps as i128 + 1_000;
        let buyer = fund_participant(&env, &pool, &usdc, buyer_budget);

        place_order(&market, &buyer, shared_invoice, OrderSide::Bid, total_bps, max_price);

        // The cheapest ask (asks[0]) must have filled first.
        let ask0 = market.get_order(&asks[0].2).unwrap();
        prop_assert!(
            ask0.status == OrderStatus::Filled || ask0.status == OrderStatus::PartiallyFilled,
            "cheapest ask (price={}) should have been the first filled; got {:?}",
            asks[0].1,
            ask0.status
        );

        // No more-expensive ask may be Filled while a cheaper one is still Open.
        let mut cheapest_unfilled_price: Option<i128> = None;
        for (_, price, id) in &asks {
            let order = market.get_order(id).unwrap();
            if order.status == OrderStatus::Open {
                if cheapest_unfilled_price.is_none() {
                    cheapest_unfilled_price = Some(*price);
                }
            }
        }
        if let Some(unfilled_threshold) = cheapest_unfilled_price {
            for (_, price, id) in &asks {
                if *price > unfilled_threshold {
                    let order = market.get_order(id).unwrap();
                    prop_assert!(
                        order.status != OrderStatus::Filled,
                        "ask at price {} filled while cheaper ask at price {} is still Open",
                        price,
                        unfilled_threshold
                    );
                }
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    /// MAX_MATCHES_PER_CALL cap — shared book.
    ///
    /// Invariant: a single `place_order` call never applies more than
    /// MAX_MATCHES_PER_CALL = 10 fills on the same invoice book. When
    /// more than 10 asks are resting on the book before a large crossing
    /// bid arrives, at most 10 asks can transition away from Open and the
    /// bid must rest with a non-zero `remaining`.
    #[test]
    fn prop_max_matches_shared_book_cap_respected(
        n_asks in (MAX_MATCHES_PER_CALL as usize + 1)..=(MAX_MATCHES_PER_CALL as usize + 3),
    ) {
        let env = Env::default();
        let (pool, market, admin, _bootstrap_invoice, usdc) = setup_env(&env);

        let shared_invoice: u64 = 20;
        let bps_per: u64 = (10_000u64 / (n_asks as u64 + 2)).max(1);
        prop_assume!(n_asks as u64 * bps_per <= 10_000);

        let sellers = make_shared_round(&env, &pool, &usdc, &admin, shared_invoice, bps_per, n_asks);

        let ask_price = PRICE_SCALE;
        let mut ask_ids: Vec<u64> = Vec::new();
        for s in &sellers {
            let id = place_order(&market, s, shared_invoice, OrderSide::Ask, bps_per, ask_price);
            ask_ids.push(id);
        }

        // One large crossing bid.
        let total_bps = bps_per * n_asks as u64;
        let buyer_budget = ask_price * total_bps as i128 + 10_000;
        let buyer = fund_participant(&env, &pool, &usdc, buyer_budget);
        let bid_id = place_order(
            &market,
            &buyer,
            shared_invoice,
            OrderSide::Bid,
            total_bps,
            ask_price * 2, // willing to pay 2× for safety margin
        );

        // At most MAX_MATCHES_PER_CALL asks may have been touched (Filled or PartiallyFilled).
        let touched_count = ask_ids
            .iter()
            .filter(|&&id| {
                market
                    .get_order(&id)
                    .map(|o| o.status == OrderStatus::Filled || o.status == OrderStatus::PartiallyFilled)
                    .unwrap_or(false)
            })
            .count();

        prop_assert!(
            touched_count <= MAX_MATCHES_PER_CALL as usize,
            "touched {} asks; MAX_MATCHES_PER_CALL={}",
            touched_count,
            MAX_MATCHES_PER_CALL
        );

        // The bid must be PartiallyFilled (some qty remains after cap hit).
        let bid = market.get_order(&bid_id).unwrap();
        prop_assert_eq!(
            bid.status,
            OrderStatus::PartiallyFilled,
            "bid should be PartiallyFilled after hitting cap; got {:?}",
            bid.status
        );

        // The asks beyond the cap must still be Open.
        let extra = n_asks - MAX_MATCHES_PER_CALL as usize;
        let still_open_count = ask_ids
            .iter()
            .filter(|&&id| {
                market
                    .get_order(&id)
                    .map(|o| o.status == OrderStatus::Open)
                    .unwrap_or(false)
            })
            .count();
        prop_assert!(
            still_open_count >= extra,
            "expected at least {} Open (capped) asks, got {}",
            extra,
            still_open_count
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    /// Resting remainder after cap.
    ///
    /// Invariant: when a bid is capped by MAX_MATCHES_PER_CALL, its
    /// `remaining` field equals exactly the unfilled quantity, it is inserted
    /// into the bid side of the book, and at least `extra` asks remain Open.
    #[test]
    fn prop_remainder_rests_after_cap(
        extra in 1usize..=3usize,
    ) {
        let env = Env::default();
        let (pool, market, admin, _bootstrap_invoice, usdc) = setup_env(&env);

        let n_sellers = MAX_MATCHES_PER_CALL as usize + extra;
        let bps_per = 100u64;
        prop_assume!(n_sellers as u64 * bps_per <= 10_000);

        let shared_invoice: u64 = 30;
        let sellers = make_shared_round(&env, &pool, &usdc, &admin, shared_invoice, bps_per, n_sellers);

        let ask_price = PRICE_SCALE;
        let mut ask_ids: Vec<u64> = Vec::new();
        for s in &sellers {
            let id = place_order(&market, s, shared_invoice, OrderSide::Ask, bps_per, ask_price);
            ask_ids.push(id);
        }

        // Bid for the full amount — capped at MAX_MATCHES_PER_CALL.
        let total_bps = bps_per * n_sellers as u64;
        let buyer_budget = ask_price * total_bps as i128 + 10_000;
        let buyer = fund_participant(&env, &pool, &usdc, buyer_budget);
        let bid_id = place_order(
            &market,
            &buyer,
            shared_invoice,
            OrderSide::Bid,
            total_bps,
            ask_price * 2,
        );

        let bid = market.get_order(&bid_id).unwrap();

        // Bid must be PartiallyFilled, not Filled.
        prop_assert_eq!(
            bid.status,
            OrderStatus::PartiallyFilled,
            "bid should be PartiallyFilled after cap; got {:?}",
            bid.status
        );

        // Remaining quantity = the `extra` uncapped asks' worth of bps.
        let expected_remaining = bps_per * extra as u64;
        prop_assert_eq!(
            bid.remaining,
            expected_remaining,
            "bid.remaining should be {} (extra={} uncapped); got {}",
            expected_remaining,
            extra,
            bid.remaining
        );

        // At least `extra` asks still resting Open.
        let still_open_count = ask_ids
            .iter()
            .filter(|&&id| {
                market
                    .get_order(&id)
                    .map(|o| o.status == OrderStatus::Open)
                    .unwrap_or(false)
            })
            .count();
        prop_assert!(
            still_open_count >= extra,
            "expected at least {} open asks (capped), got {}",
            extra,
            still_open_count
        );

        // The resting bid must appear in the book.
        let (bids, _asks) = market.get_order_book(&shared_invoice, &ListingKind::CoFunding);
        let bid_in_book = bids.iter().any(|level| level.order_id == bid_id);
        prop_assert!(bid_in_book, "PartiallyFilled bid must appear in the order book");
    }

    // ─────────────────────────────────────────────────────────────────────────
    /// Book conservation — no bps created or destroyed.
    ///
    /// Invariant: the total bps received by the buyer must equal the total
    /// bps deducted from sellers across all fills. Nothing is created or
    /// destroyed by the matching process.
    #[test]
    fn prop_book_conservation_no_double_count(
        n_sellers in 2usize..=6usize,
        bps_per in 100u64..=500u64,
        bid_premium in 1u32..=5u32,
    ) {
        let env = Env::default();
        let (pool, market, admin, _bootstrap_invoice, usdc) = setup_env(&env);

        let shared_invoice: u64 = 40;
        let target = bps_per as i128 * n_sellers as i128;
        prop_assume!(target <= 10_000);

        let sellers = make_shared_round(&env, &pool, &usdc, &admin, shared_invoice, bps_per, n_sellers);

        // Record seller bps before placing orders.
        let bps_before: Vec<u32> = sellers
            .iter()
            .map(|s| pool.get_co_fund_share(&shared_invoice, s))
            .collect();

        let ask_price = PRICE_SCALE;
        for s in &sellers {
            place_order(&market, s, shared_invoice, OrderSide::Ask, bps_per, ask_price);
        }

        let bid_price = ask_price * bid_premium as i128;
        let buyer_budget = bid_price * target + 10_000;
        let buyer = fund_participant(&env, &pool, &usdc, buyer_budget);
        place_order(
            &market,
            &buyer,
            shared_invoice,
            OrderSide::Bid,
            bps_per * n_sellers as u64,
            bid_price,
        );

        let buyer_received = pool.get_co_fund_share(&shared_invoice, &buyer) as u64;
        let total_seller_sent: u64 = sellers
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let after = pool.get_co_fund_share(&shared_invoice, s) as u64;
                let before = bps_before[i] as u64;
                before.saturating_sub(after)
            })
            .sum();

        prop_assert_eq!(
            buyer_received,
            total_seller_sent,
            "book conservation violated: buyer received {} bps but sellers sent {} bps",
            buyer_received,
            total_seller_sent
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    /// Fill executes at maker (resting) price, not taker price.
    ///
    /// Invariant: the buyer's available balance decreases by exactly
    /// `ask_price * bps / PRICE_SCALE` regardless of how much higher the
    /// bid price is.
    #[test]
    fn prop_fill_executes_at_maker_price(
        bps in 100u64..=2_000u64,
        ask_price_units in 1i128..=5i128,
    ) {
        let env = Env::default();
        let (pool, market, admin, _bootstrap_invoice, usdc) = setup_env(&env);

        let shared_invoice: u64 = 50;
        prop_assume!(bps <= 10_000);

        let seller = make_seller(&env, &pool, &usdc, &admin, shared_invoice, bps);

        let ask_price = PRICE_SCALE * ask_price_units;
        place_order(&market, &seller, shared_invoice, OrderSide::Ask, bps, ask_price);

        // Buyer bids at 3× the ask price but must only pay the maker (ask) price.
        let bid_price = ask_price * 3;
        let buyer_budget = bid_price * bps as i128 + 1_000;
        let buyer = fund_participant(&env, &pool, &usdc, buyer_budget);
        let buyer_before = available_of(&env, &pool.address, &buyer, &usdc);

        place_order(&market, &buyer, shared_invoice, OrderSide::Bid, bps, bid_price);

        let buyer_after = available_of(&env, &pool.address, &buyer, &usdc);
        let spent = buyer_before - buyer_after;

        // Expected: ask_price * bps / PRICE_SCALE (integer, same as contract).
        let expected_spend = ask_price
            .checked_mul(bps as i128)
            .and_then(|v| v.checked_div(PRICE_SCALE))
            .unwrap_or(0);

        prop_assert_eq!(
            spent,
            expected_spend,
            "fill price wrong: spent {} but ask_price={} * bps={} / SCALE = {}",
            spent,
            ask_price,
            bps,
            expected_spend
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    /// No self-match.
    ///
    /// Invariant: if the same address places both an ask and a crossing bid
    /// on the same invoice, no fill occurs — both orders remain Open.
    #[test]
    fn prop_no_self_match(
        bps in 100u64..=2_000u64,
    ) {
        let env = Env::default();
        let (pool, market, admin, _bootstrap_invoice, usdc) = setup_env(&env);

        let shared_invoice: u64 = 60;
        prop_assume!(bps <= 10_000);

        let actor = make_seller(&env, &pool, &usdc, &admin, shared_invoice, bps);

        // Actor posts ask at 1× price.
        let ask_id = place_order(&market, &actor, shared_invoice, OrderSide::Ask, bps, PRICE_SCALE);

        // Same actor posts a crossing bid at 2× — must not self-fill.
        let bid_id = place_order(&market, &actor, shared_invoice, OrderSide::Bid, bps, PRICE_SCALE * 2);

        let ask = market.get_order(&ask_id).unwrap();
        let bid = market.get_order(&bid_id).unwrap();

        prop_assert_eq!(
            ask.status,
            OrderStatus::Open,
            "ask should remain Open after self-match attempt; got {:?}",
            ask.status
        );
        prop_assert_eq!(
            bid.status,
            OrderStatus::Open,
            "bid should remain Open after self-match attempt; got {:?}",
            bid.status
        );

        // No bps should have changed hands.
        prop_assert_eq!(
            pool.get_co_fund_share(&shared_invoice, &actor),
            bps as u32,
            "actor bps changed despite self-match block"
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    /// Time priority within the same price.
    ///
    /// Invariant: when two asks rest at an identical price, the one placed
    /// first (lower order_id — time priority) must fill first when a bid
    /// arrives that can only cross one of them.
    #[test]
    fn prop_time_priority_within_same_price(
        bps in 200u64..=1_000u64,
    ) {
        let env = Env::default();
        let (pool, market, admin, _bootstrap_invoice, usdc) = setup_env(&env);

        let shared_invoice: u64 = 70;
        prop_assume!(bps * 2 <= 10_000);

        // Two sellers at the same price on a shared round.
        let sellers = make_shared_round(&env, &pool, &usdc, &admin, shared_invoice, bps, 2);
        let seller1 = &sellers[0];
        let seller2 = &sellers[1];

        let price = PRICE_SCALE;
        // seller1 places first → lower order_id (older, higher priority).
        let ask1_id = place_order(&market, seller1, shared_invoice, OrderSide::Ask, bps, price);
        let ask2_id = place_order(&market, seller2, shared_invoice, OrderSide::Ask, bps, price);

        prop_assert!(ask1_id < ask2_id, "ask1_id should be older (smaller order_id)");

        // A bid that only crosses ONE ask's worth, so exactly one fills.
        let buyer = fund_participant(&env, &pool, &usdc, price * bps as i128 + 1_000);
        place_order(&market, &buyer, shared_invoice, OrderSide::Bid, bps, price);

        let ask1 = market.get_order(&ask1_id).unwrap();
        let ask2 = market.get_order(&ask2_id).unwrap();

        prop_assert_eq!(
            ask1.status,
            OrderStatus::Filled,
            "older ask1 (id={}) should fill first; got {:?}",
            ask1_id,
            ask1.status
        );
        prop_assert_eq!(
            ask2.status,
            OrderStatus::Open,
            "newer ask2 (id={}) should still be Open; got {:?}",
            ask2_id,
            ask2.status
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    /// Non-crossing orders never fill.
    ///
    /// Invariant: if the bid price is strictly below the ask price, neither
    /// order fills — both rest on their respective sides.
    #[test]
    fn prop_non_crossing_orders_never_fill(
        bps in 100u64..=2_000u64,
        ask_premium in 1i128..=5i128, // ask_price = PRICE_SCALE * (bid_price_units + ask_premium)
        bid_price_units in 1i128..=4i128,
    ) {
        let env = Env::default();
        let (pool, market, admin, _bootstrap_invoice, usdc) = setup_env(&env);

        let shared_invoice: u64 = 80;
        prop_assume!(bps <= 10_000);

        let seller = make_seller(&env, &pool, &usdc, &admin, shared_invoice, bps);

        let bid_price = PRICE_SCALE * bid_price_units;
        let ask_price = PRICE_SCALE * (bid_price_units + ask_premium);
        // Ask is strictly above bid — no cross.

        let ask_id = place_order(&market, &seller, shared_invoice, OrderSide::Ask, bps, ask_price);

        let buyer = fund_participant(&env, &pool, &usdc, bid_price * bps as i128 + 1_000);
        let bid_id = place_order(&market, &buyer, shared_invoice, OrderSide::Bid, bps, bid_price);

        let ask = market.get_order(&ask_id).unwrap();
        let bid = market.get_order(&bid_id).unwrap();

        prop_assert_eq!(
            ask.status,
            OrderStatus::Open,
            "ask should rest Open (no cross); got {:?}",
            ask.status
        );
        prop_assert_eq!(
            bid.status,
            OrderStatus::Open,
            "bid should rest Open (no cross); got {:?}",
            bid.status
        );

        // Both sides appear in the book.
        let (bids, asks) = market.get_order_book(&shared_invoice, &ListingKind::CoFunding);
        prop_assert!(
            bids.iter().any(|level| level.order_id == bid_id),
            "resting bid must be in book"
        );
        prop_assert!(
            asks.iter().any(|level| level.order_id == ask_id),
            "resting ask must be in book"
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    /// Expired orders are pruned lazily during matching.
    ///
    /// Invariant: if a resting ask is placed with an expiry and the clock
    /// advances past that expiry before a crossing bid arrives, the expired
    /// ask is not filled — it is marked Expired and the bid rests Open.
    #[test]
    fn prop_expired_ask_not_matched(
        bps in 100u64..=1_000u64,
        expiry_offset in 1u64..=500u64,
        clock_advance in 1u64..=200u64,
    ) {
        let env = Env::default();
        let (pool, market, admin, _bootstrap_invoice, usdc) = setup_env(&env);

        let shared_invoice: u64 = 90;
        prop_assume!(bps <= 10_000);

        let seller = make_seller(&env, &pool, &usdc, &admin, shared_invoice, bps);

        let now = env.ledger().timestamp();
        let expires_at = now + expiry_offset;
        let past_expiry = expires_at + clock_advance; // clock will be set to this

        // Place ask with a finite expiry.
        let ask_id = market.place_order(
            &seller,
            &PlaceOrderRequest {
                invoice_id: shared_invoice,
                kind: ListingKind::CoFunding,
                side: OrderSide::Ask,
                amount_or_bps: bps,
                price: PRICE_SCALE,
                expires_at,
            },
        );

        // Advance clock past expiry.
        env.ledger().with_mut(|l| l.timestamp = past_expiry);

        // Crossing bid arrives — the match loop should prune the expired ask
        // and leave the bid resting Open.
        let buyer = fund_participant(&env, &pool, &usdc, PRICE_SCALE * bps as i128 + 1_000);
        let bid_id = place_order(&market, &buyer, shared_invoice, OrderSide::Bid, bps, PRICE_SCALE * 2);

        let ask = market.get_order(&ask_id).unwrap();
        prop_assert_eq!(
            ask.status,
            OrderStatus::Expired,
            "ask should be Expired after clock passed expiry; got {:?}",
            ask.status
        );

        let bid = market.get_order(&bid_id).unwrap();
        prop_assert_eq!(
            bid.status,
            OrderStatus::Open,
            "bid should rest Open (no valid ask to cross); got {:?}",
            bid.status
        );
    }
}
