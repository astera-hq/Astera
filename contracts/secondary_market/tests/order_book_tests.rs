#![cfg(test)]

// #1035: limit order book for pool positions and co-funding shares, sitting
// alongside the #1025 fixed-price listing flow tested in
// secondary_market_tests.rs. Orders match immediately on `place_order`
// against the opposite side's resting orders, best price first with ties
// broken by time priority, settling each fill through pool's trusted
// `market_settle_listing` entrypoint (same one `buy_listing` uses).

use pool::{DataKey, FundingPool, FundingPoolClient, InvestorPosition, OpenCoFundingRequest};
use secondary_market::{
    ListingKind, MarketError, Order, OrderSide, OrderStatus, PlaceOrderRequest, SecondaryMarket,
    SecondaryMarketClient,
};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    token, Address, Env, Symbol,
};

const PRICE_SCALE: i128 = 10_000_000;

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

fn setup(
    env: &Env,
) -> (
    FundingPoolClient<'_>,
    SecondaryMarketClient<'_>,
    Address,
    Address,
) {
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

    (pool_client, market_client, admin, usdc_id)
}

fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token_id).mint(to, &amount);
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

/// Open + fully commit a co-funding round with a single investor holding
/// 100% of the round's bps.
fn setup_filled_cofund_round(
    env: &Env,
    pool_client: &FundingPoolClient<'_>,
    admin: &Address,
    usdc: &Address,
) -> (u64, Address) {
    let investor = Address::generate(env);
    mint(env, usdc, &investor, 10_000);
    pool_client.deposit(&investor, usdc, &10_000i128, &None);

    let invoice_id: u64 = 42;
    let sme = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86_400;
    let deadline = env.ledger().timestamp() + 3_600;

    pool_client.open_co_funding(
        admin,
        &OpenCoFundingRequest {
            invoice_id,
            token: usdc.clone(),
            target_principal: 5_000,
            sme: sme.clone(),
            due_date,
            funding_deadline: deadline,
            min_commitment: 100,
            max_investor_bps: 0,
        },
    );

    pool_client.commit_to_invoice(&investor, &invoice_id, &5_000i128);
    pool_client.finalize_co_funding(&investor, &invoice_id);

    (invoice_id, investor)
}

/// Open + fully commit a co-funding round split 60/40 between two
/// investors, so both hold a resting-order-able CoFundShare on the same
/// invoice — used to exercise multi-maker matching.
fn setup_two_seller_cofund_round(
    env: &Env,
    pool_client: &FundingPoolClient<'_>,
    admin: &Address,
    usdc: &Address,
) -> (u64, Address, Address) {
    let seller1 = Address::generate(env);
    let seller2 = Address::generate(env);
    mint(env, usdc, &seller1, 10_000);
    mint(env, usdc, &seller2, 10_000);
    pool_client.deposit(&seller1, usdc, &10_000i128, &None);
    pool_client.deposit(&seller2, usdc, &10_000i128, &None);

    let invoice_id: u64 = 7;
    let sme = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86_400;
    let deadline = env.ledger().timestamp() + 3_600;

    pool_client.open_co_funding(
        admin,
        &OpenCoFundingRequest {
            invoice_id,
            token: usdc.clone(),
            target_principal: 10_000,
            sme: sme.clone(),
            due_date,
            funding_deadline: deadline,
            min_commitment: 100,
            max_investor_bps: 0,
        },
    );

    pool_client.commit_to_invoice(&seller1, &invoice_id, &6_000i128);
    pool_client.commit_to_invoice(&seller2, &invoice_id, &4_000i128);
    pool_client.finalize_co_funding(&seller1, &invoice_id);

    (invoice_id, seller1, seller2)
}

fn fund_buyer(
    env: &Env,
    pool_client: &FundingPoolClient<'_>,
    usdc: &Address,
    amount: i128,
) -> Address {
    let buyer = Address::generate(env);
    mint(env, usdc, &buyer, amount);
    pool_client.deposit(&buyer, usdc, &amount, &None);
    buyer
}

/// Thin wrapper so test bodies read as a flat arg list instead of building
/// a `PlaceOrderRequest` literal at every call site.
#[allow(clippy::too_many_arguments)]
fn place(
    market_client: &SecondaryMarketClient<'_>,
    owner: &Address,
    invoice_id: u64,
    kind: ListingKind,
    side: OrderSide,
    amount_or_bps: u64,
    price: i128,
    expires_at: u64,
) -> u64 {
    market_client.place_order(
        owner,
        &PlaceOrderRequest {
            invoice_id,
            kind,
            side,
            amount_or_bps,
            price,
            expires_at,
        },
    )
}

// ── place_order / matching ──────────────────────────────────────────────────

#[test]
fn test_marketable_bid_partially_fills_two_asks_price_time_priority() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, seller1, seller2) =
        setup_two_seller_cofund_round(&env, &pool_client, &admin, &usdc);

    // seller1 offers 3,000 of their 6,000 bps at the better (lower) price;
    // seller2 offers 2,000 of their 4,000 bps at a worse (higher) price.
    let ask1_id = place(
        &market_client,
        &seller1,
        invoice_id,
        ListingKind::CoFunding,
        OrderSide::Ask,
        3_000u64,
        PRICE_SCALE, // 1.0 token per bps unit
        0u64,
    );
    let ask2_id = place(
        &market_client,
        &seller2,
        invoice_id,
        ListingKind::CoFunding,
        OrderSide::Ask,
        2_000u64,
        PRICE_SCALE * 2, // 2.0 token per bps unit
        0u64,
    );

    // Buyer's marketable bid crosses both asks and asks for more than
    // seller1 alone can fill (4,000 > seller1's 3,000 resting).
    let buyer = fund_buyer(&env, &pool_client, &usdc, 10_000);
    let bid_id = place(
        &market_client,
        &buyer,
        invoice_id,
        ListingKind::CoFunding,
        OrderSide::Bid,
        4_000u64,
        PRICE_SCALE * 3, // willing to pay up to 3.0/unit
        0u64,
    );

    // Price-time priority: the cheaper ask (seller1) fills first and fully;
    // the remainder fills against seller2's ask, partially.
    let ask1 = market_client.get_order(&ask1_id).unwrap();
    assert_eq!(ask1.status, OrderStatus::Filled);
    assert_eq!(ask1.remaining, 0);

    let ask2 = market_client.get_order(&ask2_id).unwrap();
    assert_eq!(ask2.status, OrderStatus::PartiallyFilled);
    assert_eq!(ask2.remaining, 1_000);

    let bid = market_client.get_order(&bid_id).unwrap();
    assert_eq!(bid.status, OrderStatus::Filled);
    assert_eq!(bid.remaining, 0);

    // On-chain shares moved: buyer got all 3,000 from seller1 plus 1,000
    // from seller2; seller2 kept the 1,000 bps still resting in their ask.
    assert_eq!(pool_client.get_co_fund_share(&invoice_id, &buyer), 4_000);
    assert_eq!(pool_client.get_co_fund_share(&invoice_id, &seller1), 3_000);
    assert_eq!(pool_client.get_co_fund_share(&invoice_id, &seller2), 3_000);

    // Buyer paid seller1's price for the first fill (3,000) and seller2's
    // price for the second (1,000 * 2.0 = 2,000) — trades always execute
    // at the resting (maker) order's price, not the taker's.
    let buyer_available = available_of(&env, &pool_client.address, &buyer, &usdc);
    assert_eq!(buyer_available, 10_000 - 3_000 - 2_000);

    // The fully-filled ask is gone from the book; the partially-filled one
    // still rests there; the fully-filled bid was never inserted.
    let (bids, asks) = market_client.get_order_book(&invoice_id, &ListingKind::CoFunding);
    assert!(bids.is_empty());
    assert_eq!(asks.len(), 1);
    assert_eq!(asks.get(0).unwrap().order_id, ask2_id);
}

#[test]
fn test_match_skips_seller_without_kyc_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, seller) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);
    // Fund the buyer before turning KYC on — `deposit` itself requires KYC
    // once `kyc_required` is set, so an unapproved buyer couldn't deposit
    // at all otherwise.
    let buyer = fund_buyer(&env, &pool_client, &usdc, 20_000);

    pool_client.set_kyc_required(&admin, &true);
    pool_client.set_investor_kyc(&admin, &buyer, &true);
    // seller is deliberately left un-approved (NotRequested).

    let bps = pool_client.get_co_fund_share(&invoice_id, &seller);
    let ask_id = place(
        &market_client,
        &seller,
        invoice_id,
        ListingKind::CoFunding,
        OrderSide::Ask,
        bps as u64,
        PRICE_SCALE,
        0u64,
    );
    let bid_id = place(
        &market_client,
        &buyer,
        invoice_id,
        ListingKind::CoFunding,
        OrderSide::Bid,
        bps as u64,
        PRICE_SCALE,
        0u64,
    );

    // Settlement re-checks the seller's KYC status too (not just the
    // buyer's) — the match is skipped rather than reverting the whole call.
    let bid = market_client.get_order(&bid_id).unwrap();
    assert_eq!(bid.status, OrderStatus::Open);
    assert_eq!(bid.remaining, bps as u64);

    let ask = market_client.get_order(&ask_id).unwrap();
    assert_eq!(ask.status, OrderStatus::Open);

    assert_eq!(pool_client.get_co_fund_share(&invoice_id, &buyer), 0);
}

#[test]
fn test_match_skips_buyer_without_kyc_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, seller) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);
    // Fund the buyer before turning KYC on — see the comment in
    // test_match_skips_seller_without_kyc_approval above.
    let buyer = fund_buyer(&env, &pool_client, &usdc, 20_000);

    pool_client.set_kyc_required(&admin, &true);
    pool_client.set_investor_kyc(&admin, &seller, &true);
    // buyer is deliberately left un-approved.

    let bps = pool_client.get_co_fund_share(&invoice_id, &seller);
    place(
        &market_client,
        &seller,
        invoice_id,
        ListingKind::CoFunding,
        OrderSide::Ask,
        bps as u64,
        PRICE_SCALE,
        0u64,
    );
    let bid_id = place(
        &market_client,
        &buyer,
        invoice_id,
        ListingKind::CoFunding,
        OrderSide::Bid,
        bps as u64,
        PRICE_SCALE,
        0u64,
    );

    let bid = market_client.get_order(&bid_id).unwrap();
    assert_eq!(bid.status, OrderStatus::Open);
    assert_eq!(pool_client.get_co_fund_share(&invoice_id, &buyer), 0);
}

#[test]
fn test_place_order_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, seller) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);

    let result = market_client.try_place_order(
        &seller,
        &PlaceOrderRequest {
            invoice_id,
            kind: ListingKind::CoFunding,
            side: OrderSide::Ask,
            amount_or_bps: 0u64,
            price: PRICE_SCALE,
            expires_at: 0u64,
        },
    );
    assert_eq!(result.unwrap_err().unwrap(), MarketError::ZeroAmount);
}

#[test]
fn test_place_order_zero_price_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, seller) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);
    let bps = pool_client.get_co_fund_share(&invoice_id, &seller);

    let result = market_client.try_place_order(
        &seller,
        &PlaceOrderRequest {
            invoice_id,
            kind: ListingKind::CoFunding,
            side: OrderSide::Ask,
            amount_or_bps: bps as u64,
            price: 0i128,
            expires_at: 0u64,
        },
    );
    assert_eq!(result.unwrap_err().unwrap(), MarketError::InvalidAmount);
}

#[test]
fn test_place_order_past_expiry_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, seller) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);
    let bps = pool_client.get_co_fund_share(&invoice_id, &seller);
    let past = env.ledger().timestamp() - 1;

    let result = market_client.try_place_order(
        &seller,
        &PlaceOrderRequest {
            invoice_id,
            kind: ListingKind::CoFunding,
            side: OrderSide::Ask,
            amount_or_bps: bps as u64,
            price: PRICE_SCALE,
            expires_at: past,
        },
    );
    assert_eq!(result.unwrap_err().unwrap(), MarketError::InvalidExpiry);
}

// ── cancel_order ─────────────────────────────────────────────────────────────

#[test]
fn test_cancel_order_by_owner_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, seller) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);
    let bps = pool_client.get_co_fund_share(&invoice_id, &seller);

    let order_id = place(
        &market_client,
        &seller,
        invoice_id,
        ListingKind::CoFunding,
        OrderSide::Ask,
        bps as u64,
        PRICE_SCALE,
        0u64,
    );
    market_client.cancel_order(&seller, &order_id);

    let order: Order = market_client.get_order(&order_id).unwrap();
    assert_eq!(order.status, OrderStatus::Cancelled);

    let (_bids, asks) = market_client.get_order_book(&invoice_id, &ListingKind::CoFunding);
    assert!(asks.is_empty());
}

#[test]
fn test_cancel_order_by_non_owner_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, seller) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);
    let bps = pool_client.get_co_fund_share(&invoice_id, &seller);

    let order_id = place(
        &market_client,
        &seller,
        invoice_id,
        ListingKind::CoFunding,
        OrderSide::Ask,
        bps as u64,
        PRICE_SCALE,
        0u64,
    );

    let stranger = Address::generate(&env);
    let result = market_client.try_cancel_order(&stranger, &order_id);
    assert_eq!(result.unwrap_err().unwrap(), MarketError::OrderNotOwner);
}

// ── expire_order ─────────────────────────────────────────────────────────────

#[test]
fn test_expire_order_after_deadline_succeeds_permissionlessly() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, seller) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);
    let bps = pool_client.get_co_fund_share(&invoice_id, &seller);

    let expires_at = env.ledger().timestamp() + 100;
    let order_id = place(
        &market_client,
        &seller,
        invoice_id,
        ListingKind::CoFunding,
        OrderSide::Ask,
        bps as u64,
        PRICE_SCALE,
        expires_at,
    );

    env.ledger().with_mut(|l| l.timestamp = expires_at + 1);

    // Anyone (not just the owner) can trigger the expiry.
    let stranger = Address::generate(&env);
    market_client.expire_order(&order_id);
    let _ = stranger;

    let order = market_client.get_order(&order_id).unwrap();
    assert_eq!(order.status, OrderStatus::Expired);

    let (_bids, asks) = market_client.get_order_book(&invoice_id, &ListingKind::CoFunding);
    assert!(asks.is_empty());
}

#[test]
fn test_expire_order_before_deadline_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, seller) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);
    let bps = pool_client.get_co_fund_share(&invoice_id, &seller);

    let expires_at = env.ledger().timestamp() + 100;
    let order_id = place(
        &market_client,
        &seller,
        invoice_id,
        ListingKind::CoFunding,
        OrderSide::Ask,
        bps as u64,
        PRICE_SCALE,
        expires_at,
    );

    let result = market_client.try_expire_order(&order_id);
    assert_eq!(result.unwrap_err().unwrap(), MarketError::InvalidExpiry);
}

// ── read views ───────────────────────────────────────────────────────────────

#[test]
fn test_get_order_book_reflects_resting_orders_on_both_sides() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, seller) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);
    let bps = pool_client.get_co_fund_share(&invoice_id, &seller);

    let ask_id = place(
        &market_client,
        &seller,
        invoice_id,
        ListingKind::CoFunding,
        OrderSide::Ask,
        bps as u64,
        PRICE_SCALE * 5,
        0u64,
    );
    // A non-crossing bid (too low to match the ask) rests instead of filling.
    let buyer = fund_buyer(&env, &pool_client, &usdc, 20_000);
    let bid_id = place(
        &market_client,
        &buyer,
        invoice_id,
        ListingKind::CoFunding,
        OrderSide::Bid,
        bps as u64,
        PRICE_SCALE,
        0u64,
    );

    let (bids, asks) = market_client.get_order_book(&invoice_id, &ListingKind::CoFunding);
    assert_eq!(bids.len(), 1);
    assert_eq!(bids.get(0).unwrap().order_id, bid_id);
    assert_eq!(asks.len(), 1);
    assert_eq!(asks.get(0).unwrap().order_id, ask_id);

    // #1133: each level carries its price and remaining quantity directly.
    let bid_level = bids.get(0).unwrap();
    assert_eq!(bid_level.price, PRICE_SCALE);
    assert_eq!(bid_level.quantity, bps as u64);
    let ask_level = asks.get(0).unwrap();
    assert_eq!(ask_level.price, PRICE_SCALE * 5);
    assert_eq!(ask_level.quantity, bps as u64);
}

#[test]
fn test_list_orders_for_owner_returns_all_placed_orders() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, seller) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);
    let bps = pool_client.get_co_fund_share(&invoice_id, &seller);

    let id1 = place(
        &market_client,
        &seller,
        invoice_id,
        ListingKind::CoFunding,
        OrderSide::Ask,
        bps as u64 / 2,
        PRICE_SCALE,
        0u64,
    );
    let id2 = place(
        &market_client,
        &seller,
        invoice_id,
        ListingKind::CoFunding,
        OrderSide::Ask,
        bps as u64 / 2,
        PRICE_SCALE * 2,
        0u64,
    );

    let ids = market_client.list_orders_for_owner(&seller);
    assert!(ids.contains(id1));
    assert!(ids.contains(id2));
}

#[test]
fn test_get_order_nonexistent_returns_none() {
    let env = Env::default();
    env.mock_all_auths();
    let (_pool_client, market_client, _admin, _usdc) = setup(&env);
    assert!(market_client.get_order(&9999u64).is_none());
}
