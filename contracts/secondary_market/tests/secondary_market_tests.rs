#![cfg(test)]

// #1025 / pool-split: secondary market for pool positions and co-funding
// shares, now a satellite contract calling into `pool`'s trusted
// `market_settle_listing` for the actual balance movement.

use pool::{DataKey, FundingPool, FundingPoolClient, InvestorPosition, OpenCoFundingRequest};
use secondary_market::{
    ListingKind, ListingStatus, MarketError, SecondaryMarket, SecondaryMarketClient,
};
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

/// Helper: open + fill a co-funding round and return (round_id, investor, bps).
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

// ── list_position ────────────────────────────────────────────────────────────

#[test]
fn test_list_cofund_position_creates_listing() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, investor) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);

    let bps = pool_client.get_co_fund_share(&invoice_id, &investor);
    assert!(bps > 0);

    let listing_id = market_client.list_position(
        &investor,
        &invoice_id,
        &ListingKind::CoFunding,
        &(bps as u64),
        &1_000i128,
    );

    let listing = market_client.get_listing(&listing_id).unwrap();
    assert_eq!(listing.invoice_id, invoice_id);
    assert_eq!(listing.seller, investor);
    assert_eq!(listing.status, ListingStatus::Open);
    assert_eq!(listing.amount_or_bps, bps as u64);
    assert_eq!(listing.price, 1_000);
}

#[test]
fn test_list_position_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, investor) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);

    let result = market_client.try_list_position(
        &investor,
        &invoice_id,
        &ListingKind::CoFunding,
        &0u64,
        &1_000i128,
    );
    assert_eq!(result.unwrap_err().unwrap(), MarketError::ZeroAmount);
}

#[test]
fn test_list_position_zero_price_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, investor) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);

    let bps = pool_client.get_co_fund_share(&invoice_id, &investor);
    let result = market_client.try_list_position(
        &investor,
        &invoice_id,
        &ListingKind::CoFunding,
        &(bps as u64),
        &0i128,
    );
    assert_eq!(result.unwrap_err().unwrap(), MarketError::InvalidAmount);
}

#[test]
fn test_list_position_exceeds_share_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, investor) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);

    let bps = pool_client.get_co_fund_share(&invoice_id, &investor);
    let result = market_client.try_list_position(
        &investor,
        &invoice_id,
        &ListingKind::CoFunding,
        &(bps as u64 + 1),
        &1_000i128,
    );
    assert_eq!(result.unwrap_err().unwrap(), MarketError::InvalidAmount);
}

#[test]
fn test_list_position_rejects_amount_reserved_in_other_open_listings() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, investor) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);

    let bps = pool_client.get_co_fund_share(&invoice_id, &investor) as u64;
    market_client.list_position(
        &investor,
        &invoice_id,
        &ListingKind::CoFunding,
        &6_000u64,
        &1_000i128,
    );

    let result = market_client.try_list_position(
        &investor,
        &invoice_id,
        &ListingKind::CoFunding,
        &5_000u64,
        &1_000i128,
    );
    assert_eq!(bps, 10_000);
    assert_eq!(result.unwrap_err().unwrap(), MarketError::InvalidAmount);
}

#[test]
fn test_list_position_rejects_amount_reserved_in_open_ask_orders() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, investor) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);

    market_client.place_order(
        &investor,
        &secondary_market::PlaceOrderRequest {
            invoice_id,
            kind: ListingKind::CoFunding,
            side: secondary_market::OrderSide::Ask,
            amount_or_bps: 6_000u64,
            price: 10_000_000i128,
            expires_at: 0u64,
        },
    );

    let result = market_client.try_list_position(
        &investor,
        &invoice_id,
        &ListingKind::CoFunding,
        &5_000u64,
        &1_000i128,
    );
    assert_eq!(result.unwrap_err().unwrap(), MarketError::InvalidAmount);
}

// ── cancel_listing ───────────────────────────────────────────────────────────

#[test]
fn test_cancel_listing_by_seller_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, investor) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);

    let bps = pool_client.get_co_fund_share(&invoice_id, &investor);
    let listing_id = market_client.list_position(
        &investor,
        &invoice_id,
        &ListingKind::CoFunding,
        &(bps as u64),
        &500i128,
    );

    market_client.cancel_listing(&investor, &listing_id);

    let listing = market_client.get_listing(&listing_id).unwrap();
    assert_eq!(listing.status, ListingStatus::Cancelled);
}

#[test]
fn test_cancel_listing_by_non_seller_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, investor) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);

    let bps = pool_client.get_co_fund_share(&invoice_id, &investor);
    let listing_id = market_client.list_position(
        &investor,
        &invoice_id,
        &ListingKind::CoFunding,
        &(bps as u64),
        &500i128,
    );

    let stranger = Address::generate(&env);
    let result = market_client.try_cancel_listing(&stranger, &listing_id);
    assert_eq!(result.unwrap_err().unwrap(), MarketError::ListingNotSeller);
}

#[test]
fn test_cancel_already_cancelled_listing_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, investor) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);

    let bps = pool_client.get_co_fund_share(&invoice_id, &investor);
    let listing_id = market_client.list_position(
        &investor,
        &invoice_id,
        &ListingKind::CoFunding,
        &(bps as u64),
        &500i128,
    );

    market_client.cancel_listing(&investor, &listing_id);
    let result = market_client.try_cancel_listing(&investor, &listing_id);
    assert_eq!(result.unwrap_err().unwrap(), MarketError::ListingNotOpen);
}

// ── buy_listing ──────────────────────────────────────────────────────────────

#[test]
fn test_buy_cofund_listing_transfers_share_and_price() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, seller) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);

    let seller_bps = pool_client.get_co_fund_share(&invoice_id, &seller);
    let price = 800i128;

    let listing_id = market_client.list_position(
        &seller,
        &invoice_id,
        &ListingKind::CoFunding,
        &(seller_bps as u64),
        &price,
    );

    // Buyer deposits funds so they have available balance.
    let buyer = Address::generate(&env);
    mint(&env, &usdc, &buyer, 2_000);
    pool_client.deposit(&buyer, &usdc, &2_000i128, &None);

    let buyer_available_before = available_of(&env, &pool_client.address, &buyer, &usdc);
    let seller_available_before = available_of(&env, &pool_client.address, &seller, &usdc);

    market_client.buy_listing(&buyer, &listing_id);

    // Listing is now Filled.
    let listing = market_client.get_listing(&listing_id).unwrap();
    assert_eq!(listing.status, ListingStatus::Filled);

    // Buyer's CoFundShare increased; seller's decreased.
    let buyer_bps_after = pool_client.get_co_fund_share(&invoice_id, &buyer);
    let seller_bps_after = pool_client.get_co_fund_share(&invoice_id, &seller);
    assert_eq!(buyer_bps_after, seller_bps);
    assert_eq!(seller_bps_after, 0);

    // Buyer's available balance decreased by price; seller's increased.
    let buyer_available_after = available_of(&env, &pool_client.address, &buyer, &usdc);
    let seller_available_after = available_of(&env, &pool_client.address, &seller, &usdc);
    assert_eq!(buyer_available_after, buyer_available_before - price);
    assert_eq!(seller_available_after, seller_available_before + price);
}

#[test]
fn test_buy_cancelled_listing_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, seller) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);

    let bps = pool_client.get_co_fund_share(&invoice_id, &seller);
    let listing_id = market_client.list_position(
        &seller,
        &invoice_id,
        &ListingKind::CoFunding,
        &(bps as u64),
        &500i128,
    );
    market_client.cancel_listing(&seller, &listing_id);

    let buyer = Address::generate(&env);
    mint(&env, &usdc, &buyer, 1_000);
    pool_client.deposit(&buyer, &usdc, &1_000i128, &None);

    let result = market_client.try_buy_listing(&buyer, &listing_id);
    assert_eq!(result.unwrap_err().unwrap(), MarketError::ListingNotOpen);
}

#[test]
fn test_seller_cannot_buy_own_listing() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, seller) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);

    let bps = pool_client.get_co_fund_share(&invoice_id, &seller);
    let listing_id = market_client.list_position(
        &seller,
        &invoice_id,
        &ListingKind::CoFunding,
        &(bps as u64),
        &500i128,
    );

    let result = market_client.try_buy_listing(&seller, &listing_id);
    assert_eq!(result.unwrap_err().unwrap(), MarketError::Unauthorized);
}

#[test]
fn test_buy_listing_insufficient_available_balance_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, seller) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);

    let bps = pool_client.get_co_fund_share(&invoice_id, &seller);
    // Price is higher than buyer's available balance.
    let listing_id = market_client.list_position(
        &seller,
        &invoice_id,
        &ListingKind::CoFunding,
        &(bps as u64),
        &99_999i128,
    );

    let buyer = Address::generate(&env);
    mint(&env, &usdc, &buyer, 100);
    pool_client.deposit(&buyer, &usdc, &100i128, &None);

    // Pool's own InvalidAmount check inside market_settle_listing fails;
    // the market contract surfaces that as a generic settlement failure
    // since it isn't in the business of re-exposing pool's error domain.
    let result = market_client.try_buy_listing(&buyer, &listing_id);
    assert_eq!(result.unwrap_err().unwrap(), MarketError::SettlementFailed);
}

// ── index queries ────────────────────────────────────────────────────────────

#[test]
fn test_list_listings_for_invoice_returns_all_ids() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, investor) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);

    let bps = pool_client.get_co_fund_share(&invoice_id, &investor);
    let id1 = market_client.list_position(
        &investor,
        &invoice_id,
        &ListingKind::CoFunding,
        &(bps as u64 / 2),
        &100i128,
    );
    let id2 = market_client.list_position(
        &investor,
        &invoice_id,
        &ListingKind::CoFunding,
        &(bps as u64 / 2),
        &200i128,
    );

    let ids = market_client.list_listings_for_invoice(&invoice_id);
    assert!(ids.contains(id1));
    assert!(ids.contains(id2));
}

#[test]
fn test_list_listings_for_investor_returns_seller_ids() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, market_client, admin, usdc) = setup(&env);
    let (invoice_id, investor) = setup_filled_cofund_round(&env, &pool_client, &admin, &usdc);

    let bps = pool_client.get_co_fund_share(&invoice_id, &investor);
    let listing_id = market_client.list_position(
        &investor,
        &invoice_id,
        &ListingKind::CoFunding,
        &(bps as u64),
        &300i128,
    );

    let ids = market_client.list_listings_for_investor(&investor);
    assert!(ids.contains(listing_id));
}

#[test]
fn test_get_listing_nonexistent_returns_none() {
    let env = Env::default();
    env.mock_all_auths();
    let (_pool_client, market_client, _admin, _usdc) = setup(&env);
    assert!(market_client.get_listing(&9999u64).is_none());
}
