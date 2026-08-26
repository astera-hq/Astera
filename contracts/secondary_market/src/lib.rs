#![no_std]

// A `pool` satellite contract, split out because pool.wasm was over
// Soroban's 200KB deploy limit. Covers three originally-pool-native
// features that don't need pool's own trusted execution context:
//   - the #1025 secondary market for pool positions and co-funding shares
//     (listing lifecycle/storage lives here; the actual balance movement
//     between buyer and seller happens on `pool` via the trusted
//     `market_settle_listing` entrypoint, called after validating a listing
//     is open)
//   - the #1035 limit order book for the same pool positions/co-funding
//     shares: resting bid/ask orders with partial fills and price-time
//     priority, matched and settled (via the same `market_settle_listing`
//     entrypoint, once per fill) directly inside `place_order`. Sits
//     alongside the #1025 fixed-price listing flow above rather than
//     replacing it — `list_position`/`buy_listing` are simpler
//     take-it-or-leave-it listings some callers may still prefer, and nothing
//     about the order book requires deprecating them.
//   - the #865 withdrawal-wait/liquidity-forecast read-only analytics views,
//     recomputed here from pool's public getters (`get_withdrawal_queue`,
//     `get_token_totals`, `get_open_invoices_for_token`,
//     `get_trailing_inflow_rate`, `get_share_token`)
//
// Deliberately calls into `pool` via raw `env.invoke_contract`/
// `try_invoke_contract` rather than `pool`'s generated `FundingPoolClient` —
// `pool` is only a dev-dependency here (used by tests to spin up a real
// pool instance). Depending on it as a normal dependency and using its
// Client in real (non-test) code pulls the whole pool crate's compiled
// object code into this crate's wasm build (codegen-units=1 means pool
// compiles to a single object file, and any reference to it drags the
// entire file in), which then collides at link time: pool's own
// `#[contractimpl]` entrypoints (e.g. `pause`, `unpause`, `initialize`) get
// exported into this contract's wasm alongside this crate's own
// same-named entrypoints, producing duplicate wasm export symbols.
// `#[contracttype]` structs encode as a field-name-keyed map (sorted by
// field name, not declaration order — see soroban-sdk-macros
// `derive_struct.rs`), so the local mirror structs below decode
// cross-contract calls into pool correctly as long as field names/types
// match pool's, with no dependency on pool's crate needed.
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    Env, IntoVal, Map, Symbol, Vec,
};

/// Mirrors `pool::FundedInvoice`.
#[contracttype]
#[derive(Clone)]
struct FundedInvoiceView {
    pub invoice_id: u64,
    pub sme: Address,
    pub token: Address,
    pub principal: i128,
    pub funded_at: u64,
    pub factoring_fee: i128,
    pub due_date: u64,
    pub repaid_amount: i128,
    pub co_funding_round_id: Option<u64>,
    pub locked_yield_bps: u32,
}

/// Mirrors `pool::PoolTokenTotals`.
#[contracttype]
#[derive(Clone)]
struct TokenTotalsView {
    pub pool_value: i128,
    pub total_deployed: i128,
    pub total_paid_out: i128,
    pub total_fee_revenue: i128,
    pub reward_per_share: i128,
    pub protocol_revenue: i128,
}

/// Mirrors `pool::WithdrawalRequest`.
#[contracttype]
#[derive(Clone)]
struct WithdrawalRequestView {
    pub investor: Address,
    pub token: Address,
    pub shares: i128,
    pub requested_at: u64,
    pub request_id: u64,
}

/// Mirrors `pool::ListingSettlement`, sent as an arg to `market_settle_listing`.
#[contracttype]
#[derive(Clone)]
struct PoolListingSettlement {
    pub buyer: Address,
    pub seller: Address,
    pub invoice_id: u64,
    pub is_co_funding: bool,
    pub amount_or_bps: u64,
    pub price: i128,
}

const SECS_PER_DAY: u64 = 86_400;
// #865: clamp bounds for the predictive wait estimate so a sparse/empty inflow history
// or a huge queue never produces a nonsensical (zero or unbounded) estimate.
const MIN_WAIT_ESTIMATE_SECS: u64 = 3_600; // 1 hour
const MAX_WAIT_ESTIMATE_SECS: u64 = 31_536_000; // 365 days
                                                // #865: liquidity forecast is capped to this many days to bound loop iteration/gas cost.
const MAX_FORECAST_HORIZON_DAYS: u32 = 365;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum MarketError {
    AlreadyInitialized = 0,
    NotInitialized = 1,
    Unauthorized = 2,
    ContractPaused = 3,
    ZeroAmount = 4,
    InvalidAmount = 5,
    ListingNotFound = 6,
    ListingNotOpen = 7,
    ListingNotSeller = 8,
    TooManyListings = 9,
    /// The cross-contract call into pool's `market_settle_listing` failed.
    /// Pool's own `PoolError` isn't re-exposed here since it's a different
    /// contract's error domain (KYC, compliance, concentration cap, etc.) —
    /// callers that need the specific reason should simulate the transaction.
    SettlementFailed = 10,
    OrderNotFound = 11,
    OrderNotOpen = 12,
    OrderNotOwner = 13,
    TooManyOrders = 14,
    InvalidExpiry = 15,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ListingStatus {
    Open,
    Filled,
    Cancelled,
}

/// Whether the listing covers a co-funded share (bps of a CoFundingRound)
/// or a single-funded position slice (raw token amount of deployed principal).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ListingKind {
    CoFunding,
    SingleFunded,
}

/// A secondary-market listing created by `list_position`.
/// `amount_or_bps` is:
///   - for `CoFunding`: the bps of the seller's CoFundShare being offered
///   - for `SingleFunded`: the raw token amount of deployed principal being offered
/// `price` is the flat token amount the buyer must pay.
#[contracttype]
#[derive(Clone)]
pub struct Listing {
    pub listing_id: u64,
    pub invoice_id: u64,
    pub seller: Address,
    pub token: Address,
    pub kind: ListingKind,
    pub amount_or_bps: u64,
    pub price: i128,
    pub created_at: u64,
    pub status: ListingStatus,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum OrderSide {
    Bid,
    Ask,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum OrderStatus {
    Open,
    PartiallyFilled,
    Filled,
    Cancelled,
    Expired,
}

/// A resting or partially-filled limit order placed by `place_order`.
/// `price` is per-unit, scaled by `PRICE_SCALE` — e.g. a `price` of
/// `PRICE_SCALE / 2` means half a token (or half a bps-unit's worth of
/// token) per unit of `amount_or_bps`. Fills always execute at the resting
/// (maker) order's price, never the incoming (taker) order's price, and are
/// capped at `MAX_MATCHES_PER_CALL` per `place_order` call to bound gas —
/// any unmatched remainder rests on the book rather than blocking on a full
/// match. `expires_at` of `0` means the order never expires.
#[contracttype]
#[derive(Clone)]
pub struct Order {
    pub order_id: u64,
    pub invoice_id: u64,
    pub owner: Address,
    pub token: Address,
    pub kind: ListingKind,
    pub side: OrderSide,
    pub price: i128,
    pub amount_or_bps: u64,
    pub remaining: u64,
    pub created_at: u64,
    pub expires_at: u64,
    pub status: OrderStatus,
}

/// One resting order in `get_order_book`'s depth view (#1133): its id,
/// per-unit price (scaled by `PRICE_SCALE`, same units as `Order.price`),
/// and remaining quantity (bps of CoFundShare for `CoFunding` books, raw
/// token amount for `SingleFunded` books). Lets callers render a real book
/// without a follow-up `get_order` call per id.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OrderBookLevel {
    pub order_id: u64,
    pub price: i128,
    pub quantity: u64,
}

// Bundles `place_order`'s params (matches `OpenCoFundingRequest`'s existing
// role of keeping multi-field contract entrypoints under clippy's
// too-many-arguments threshold). `owner` stays a separate top-level param
// since it's the one `require_auth()` is called on.
#[contracttype]
#[derive(Clone)]
pub struct PlaceOrderRequest {
    pub invoice_id: u64,
    pub kind: ListingKind,
    pub side: OrderSide,
    pub amount_or_bps: u64,
    pub price: i128,
    pub expires_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct WaitEstimate {
    pub queue_position: u32,
    pub capital_ahead: i128,
    pub nearest_invoice_due_date: u64,
    /// #865: predicted seconds until this request is likely to clear, projected from
    /// `capital_ahead` divided by the trailing deposit-inflow rate, combined with
    /// `nearest_invoice_due_date` and clamped to
    /// `[MIN_WAIT_ESTIMATE_SECS, MAX_WAIT_ESTIMATE_SECS]`. This is an estimate, not a
    /// guarantee — actual settlement depends on future deposits/repayments.
    pub estimated_wait_secs: u64,
}

/// #865: a single projected point in `get_liquidity_forecast`'s horizon.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct LiquidityForecastPoint {
    /// Days from now (1-indexed; day 1 is the first point after "now").
    pub day: u32,
    /// Projected `available_liquidity` at this point: current liquidity, plus principal
    /// from open invoices whose `due_date` falls within the window, plus the trailing
    /// deposit-inflow rate extrapolated over the elapsed days.
    pub projected_available: i128,
}

#[contracttype]
pub enum DataKey {
    Admin,
    PoolContract,
    Initialized,
    Paused,
}

const LISTING_DATA: Symbol = symbol_short!("lst_data");
const LISTING_IDS_INV: Symbol = symbol_short!("lst_inv"); // invoice_id -> Vec<u64>
const LISTING_IDS_SELLER: Symbol = symbol_short!("lst_sel"); // seller -> Vec<u64>
const LISTING_COUNTER: Symbol = symbol_short!("lst_cnt");
const EVT: Symbol = symbol_short!("market");
// Maximum open listings per invoice to bound iteration gas cost.
const MAX_LISTINGS_PER_INVOICE: u32 = 50;

// #1035: order-book storage. Orders are keyed by their own id/counter,
// separate from the #1025 listing id space above.
const ORDER_DATA: Symbol = symbol_short!("ord_data"); // Map<u64 order_id, Order>
const ORDER_COUNTER: Symbol = symbol_short!("ord_cnt");
const ORDER_IDS_OWNER: Symbol = symbol_short!("ord_own"); // Map<Address owner, Vec<u64>>
const BOOK_BIDS: Symbol = symbol_short!("bk_bids"); // Map<u64 book_key, Vec<u64> order_ids>
const BOOK_ASKS: Symbol = symbol_short!("bk_asks"); // Map<u64 book_key, Vec<u64> order_ids>
                                                    // Maximum resting orders per side of a single (invoice_id, kind) book, to
                                                    // bound both book-depth reads and the matching-loop scan below.
const MAX_ORDERS_PER_BOOK_SIDE: u32 = 30;
// Maximum fills a single `place_order` call will attempt, to bound the
// number of cross-contract settlement calls (and therefore gas) per call.
const MAX_MATCHES_PER_CALL: u32 = 10;
// `Order.price` is per-unit, scaled by this factor (matches Stellar's own
// 7-decimal stroop convention) so fill prices for partial quantities don't
// need fractional arithmetic.
const PRICE_SCALE: i128 = 10_000_000;

fn require_not_paused(env: &Env) {
    if env
        .storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Paused)
        .unwrap_or(false)
    {
        panic_with_error!(env, MarketError::ContractPaused);
    }
}

/// Combines an invoice id and listing kind into a single order-book key —
/// `CoFunding` and `SingleFunded` positions on the same invoice trade on
/// separate books, since they're denominated differently (bps vs. raw
/// token amount).
fn book_key(invoice_id: u64, kind: &ListingKind) -> u64 {
    let kind_bit = match kind {
        ListingKind::CoFunding => 0u64,
        ListingKind::SingleFunded => 1u64,
    };
    invoice_id.saturating_mul(2).saturating_add(kind_bit)
}

fn book_side_symbol(side: &OrderSide) -> Symbol {
    match side {
        OrderSide::Bid => BOOK_BIDS,
        OrderSide::Ask => BOOK_ASKS,
    }
}

fn load_order(env: &Env, order_id: u64) -> Option<Order> {
    let all: Map<u64, Order> = env
        .storage()
        .persistent()
        .get(&ORDER_DATA)
        .unwrap_or_else(|| Map::new(env));
    all.get(order_id)
}

fn load_listing(env: &Env, listing_id: u64) -> Option<Listing> {
    let all: Map<u64, Listing> = env
        .storage()
        .persistent()
        .get(&LISTING_DATA)
        .unwrap_or_else(|| Map::new(env));
    all.get(listing_id)
}

fn save_order(env: &Env, order: &Order) {
    let mut all: Map<u64, Order> = env
        .storage()
        .persistent()
        .get(&ORDER_DATA)
        .unwrap_or_else(|| Map::new(env));
    all.set(order.order_id, order.clone());
    env.storage().persistent().set(&ORDER_DATA, &all);
}

fn available_commitment_capacity(
    env: &Env,
    invoice_id: u64,
    owner: &Address,
    kind: &ListingKind,
    owned_amount_or_bps: u64,
) -> Option<u64> {
    let mut committed = 0u64;

    let seller_map: Map<Address, Vec<u64>> = env
        .storage()
        .instance()
        .get(&LISTING_IDS_SELLER)
        .unwrap_or_else(|| Map::new(env));
    let listing_ids = seller_map
        .get(owner.clone())
        .unwrap_or_else(|| Vec::new(env));
    for listing_id in listing_ids.iter() {
        let Some(listing) = load_listing(env, listing_id) else {
            continue;
        };
        if listing.invoice_id != invoice_id
            || listing.seller != *owner
            || listing.kind != *kind
            || listing.status != ListingStatus::Open
        {
            continue;
        }
        committed = committed.checked_add(listing.amount_or_bps)?;
    }

    let owner_map: Map<Address, Vec<u64>> = env
        .storage()
        .instance()
        .get(&ORDER_IDS_OWNER)
        .unwrap_or_else(|| Map::new(env));
    let order_ids = owner_map
        .get(owner.clone())
        .unwrap_or_else(|| Vec::new(env));
    for order_id in order_ids.iter() {
        let Some(order) = load_order(env, order_id) else {
            continue;
        };
        if order.invoice_id != invoice_id
            || order.owner != *owner
            || order.kind != *kind
            || order.side != OrderSide::Ask
            || (order.status != OrderStatus::Open && order.status != OrderStatus::PartiallyFilled)
        {
            continue;
        }
        committed = committed.checked_add(order.remaining)?;
    }

    owned_amount_or_bps.checked_sub(committed)
}

fn fill_total_price(price_per_unit: i128, fill_qty: u64) -> Option<i128> {
    let numerator = price_per_unit.checked_mul(fill_qty as i128)?;
    numerator
        .checked_add(PRICE_SCALE - 1)
        .and_then(|value| value.checked_div(PRICE_SCALE))
}

fn index_order_for_owner(env: &Env, owner: &Address, order_id: u64) {
    let mut owner_map: Map<Address, Vec<u64>> = env
        .storage()
        .instance()
        .get(&ORDER_IDS_OWNER)
        .unwrap_or_else(|| Map::new(env));
    let mut ids: Vec<u64> = owner_map
        .get(owner.clone())
        .unwrap_or_else(|| Vec::new(env));
    ids.push_back(order_id);
    owner_map.set(owner.clone(), ids);
    env.storage().instance().set(&ORDER_IDS_OWNER, &owner_map);
}

fn book_side_len(env: &Env, invoice_id: u64, kind: &ListingKind, side: &OrderSide) -> u32 {
    let key = book_key(invoice_id, kind);
    let book: Map<u64, Vec<u64>> = env
        .storage()
        .instance()
        .get(&book_side_symbol(side))
        .unwrap_or_else(|| Map::new(env));
    book.get(key).map(|ids| ids.len()).unwrap_or(0)
}

fn insert_into_book(env: &Env, order: &Order) {
    let key = book_key(order.invoice_id, &order.kind);
    let symbol = book_side_symbol(&order.side);
    let mut book: Map<u64, Vec<u64>> = env
        .storage()
        .instance()
        .get(&symbol)
        .unwrap_or_else(|| Map::new(env));
    let mut ids: Vec<u64> = book.get(key).unwrap_or_else(|| Vec::new(env));
    ids.push_back(order.order_id);
    book.set(key, ids);
    env.storage().instance().set(&symbol, &book);
}

fn remove_from_book(env: &Env, order: &Order) {
    let key = book_key(order.invoice_id, &order.kind);
    let symbol = book_side_symbol(&order.side);
    let mut book: Map<u64, Vec<u64>> = env
        .storage()
        .instance()
        .get(&symbol)
        .unwrap_or_else(|| Map::new(env));
    if let Some(ids) = book.get(key) {
        let mut updated = Vec::new(env);
        for id in ids.iter() {
            if id != order.order_id {
                updated.push_back(id);
            }
        }
        book.set(key, updated);
        env.storage().instance().set(&symbol, &book);
    }
}

/// Calls pool's trusted `market_settle_listing` entrypoint for a single
/// fill. Returns `false` (rather than propagating an error) if settlement
/// fails, so one unfillable counterparty (blocked by KYC, compliance, or
/// the concentration cap) doesn't abort the whole matching pass — the
/// caller just stops matching and leaves the resting order on the book for
/// a future attempt.
#[allow(clippy::too_many_arguments)]
fn settle_fill(
    env: &Env,
    pool_id: &Address,
    buyer: &Address,
    seller: &Address,
    invoice_id: u64,
    is_co_funding: bool,
    amount_or_bps: u64,
    price: i128,
) -> bool {
    let settlement = PoolListingSettlement {
        buyer: buyer.clone(),
        seller: seller.clone(),
        invoice_id,
        is_co_funding,
        amount_or_bps,
        price,
    };
    let args = Vec::from_array(
        env,
        [
            env.current_contract_address().into_val(env),
            settlement.into_val(env),
        ],
    );
    let result = env.try_invoke_contract::<(), soroban_sdk::Error>(
        pool_id,
        &Symbol::new(env, "market_settle_listing"),
        args,
    );
    matches!(result, Ok(Ok(())))
}

/// Bounded matching: scans the opposite side's resting orders for the same
/// (invoice_id, kind) book — capped at `MAX_ORDERS_PER_BOOK_SIDE` candidates
/// — and repeatedly settles against the best-priced crossing order (ties
/// broken by lowest order id, i.e. time priority) until `taker` is fully
/// filled, no crossing order remains, or `MAX_MATCHES_PER_CALL` fills have
/// been made. Expired resting orders found along the way are pruned from
/// the book as a side effect. Never returns an error: a Soroban transaction
/// either commits or rolls back in full, so an error here would undo any
/// fills already settled earlier in this same call — any unmatchable or
/// unrepresentable candidate is skipped instead, same as a failed
/// settlement.
fn match_order(env: &Env, pool_id: &Address, taker: &mut Order, now: u64) {
    let key = book_key(taker.invoice_id, &taker.kind);
    let opposite_symbol = match taker.side {
        OrderSide::Bid => BOOK_ASKS,
        OrderSide::Ask => BOOK_BIDS,
    };

    let mut matches_made = 0u32;
    let mut excluded_ids: Vec<u64> = Vec::new(env);
    while taker.remaining > 0 && matches_made < MAX_MATCHES_PER_CALL {
        let ids: Vec<u64> = {
            let book: Map<u64, Vec<u64>> = env
                .storage()
                .instance()
                .get(&opposite_symbol)
                .unwrap_or_else(|| Map::new(env));
            book.get(key).unwrap_or_else(|| Vec::new(env))
        };

        let mut best: Option<Order> = None;
        let mut expired: Vec<u64> = Vec::new(env);
        for id in ids.iter() {
            let mut is_excluded = false;
            for excluded_id in excluded_ids.iter() {
                if excluded_id == id {
                    is_excluded = true;
                    break;
                }
            }
            if is_excluded {
                continue;
            }
            let Some(candidate) = load_order(env, id) else {
                continue;
            };
            if candidate.status != OrderStatus::Open
                && candidate.status != OrderStatus::PartiallyFilled
            {
                continue;
            }
            if candidate.expires_at != 0 && candidate.expires_at <= now {
                expired.push_back(id);
                continue;
            }
            let crosses = match taker.side {
                OrderSide::Bid => candidate.price <= taker.price,
                OrderSide::Ask => candidate.price >= taker.price,
            };
            if !crosses {
                continue;
            }
            let is_better = match &best {
                None => true,
                Some(current) => match taker.side {
                    OrderSide::Bid => {
                        candidate.price < current.price
                            || (candidate.price == current.price
                                && candidate.order_id < current.order_id)
                    }
                    OrderSide::Ask => {
                        candidate.price > current.price
                            || (candidate.price == current.price
                                && candidate.order_id < current.order_id)
                    }
                },
            };
            if is_better {
                best = Some(candidate);
            }
        }

        for id in expired.iter() {
            if let Some(mut stale) = load_order(env, id) {
                stale.status = OrderStatus::Expired;
                save_order(env, &stale);
                remove_from_book(env, &stale);
                env.events().publish(
                    (EVT, symbol_short!("ord_exp")),
                    (stale.order_id, stale.invoice_id, stale.owner.clone()),
                );
            }
        }

        let Some(mut maker) = best else {
            break;
        };

        let (buyer, seller) = match taker.side {
            OrderSide::Bid => (taker.owner.clone(), maker.owner.clone()),
            OrderSide::Ask => (maker.owner.clone(), taker.owner.clone()),
        };
        if buyer == seller {
            // Can't self-match (e.g. a taker crossing their own resting
            // order) — exclude it from the rest of this pass so it can't be
            // re-selected as the best candidate, and do not count it as a
            // fill against the per-call match budget.
            excluded_ids.push_back(maker.order_id);
            continue;
        }

        matches_made = matches_made.saturating_add(1);
        let fill_qty = taker.remaining.min(maker.remaining);
        let Some(total_price) = fill_total_price(maker.price, fill_qty) else {
            // Price*qty doesn't fit an i128 — can't represent this fill.
            // Stop matching rather than erroring out the whole call.
            break;
        };

        let settled = settle_fill(
            env,
            pool_id,
            &buyer,
            &seller,
            taker.invoice_id,
            taker.kind == ListingKind::CoFunding,
            fill_qty,
            total_price,
        );
        if !settled {
            break;
        }

        maker.remaining = maker.remaining.saturating_sub(fill_qty);
        maker.status = if maker.remaining == 0 {
            OrderStatus::Filled
        } else {
            OrderStatus::PartiallyFilled
        };
        save_order(env, &maker);
        if maker.remaining == 0 {
            remove_from_book(env, &maker);
        }
        taker.remaining = taker.remaining.saturating_sub(fill_qty);

        env.events().publish(
            (EVT, symbol_short!("ord_fill")),
            (
                taker.order_id,
                maker.order_id,
                taker.invoice_id,
                buyer,
                seller,
                fill_qty,
                total_price,
            ),
        );
    }
}

#[contract]
pub struct SecondaryMarket;

#[contractimpl]
impl SecondaryMarket {
    pub fn initialize(env: Env, admin: Address, pool_contract: Address) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic_with_error!(&env, MarketError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::PoolContract, &pool_contract);
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage().instance().set(&DataKey::Paused, &false);
    }

    pub fn pause(env: Env, admin: Address) -> Result<(), MarketError> {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(MarketError::NotInitialized)?;
        if admin != stored_admin {
            return Err(MarketError::Unauthorized);
        }
        env.storage().instance().set(&DataKey::Paused, &true);
        Ok(())
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), MarketError> {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(MarketError::NotInitialized)?;
        if admin != stored_admin {
            return Err(MarketError::Unauthorized);
        }
        env.storage().instance().set(&DataKey::Paused, &false);
        Ok(())
    }

    pub fn get_pool_contract(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::PoolContract)
    }

    /// List part or all of a position for sale on the secondary market.
    ///
    /// For `CoFunding` kind: `amount_or_bps` is the bps of the seller's
    /// `CoFundShare` to offer. For `SingleFunded` kind: `amount_or_bps` is
    /// the raw token amount of deployed principal to offer. Ownership is
    /// best-effort validated here against pool's current state — settlement
    /// re-validates independently at buy time regardless, so a listing that
    /// outlives the seller's actual holding (e.g. after a withdrawal) simply
    /// fails to fill rather than being exploitable.
    pub fn list_position(
        env: Env,
        seller: Address,
        invoice_id: u64,
        kind: ListingKind,
        amount_or_bps: u64,
        price: i128,
    ) -> Result<u64, MarketError> {
        seller.require_auth();
        require_not_paused(&env);

        if amount_or_bps == 0 {
            return Err(MarketError::ZeroAmount);
        }
        if price <= 0 {
            return Err(MarketError::InvalidAmount);
        }

        let pool_id: Address = env
            .storage()
            .instance()
            .get(&DataKey::PoolContract)
            .ok_or(MarketError::NotInitialized)?;

        let record: Option<FundedInvoiceView> = env.invoke_contract(
            &pool_id,
            &Symbol::new(&env, "get_funded_invoice"),
            Vec::from_array(&env, [invoice_id.into_val(&env)]),
        );
        let record = record.ok_or(MarketError::InvalidAmount)?;
        let token = record.token.clone();

        match kind {
            ListingKind::CoFunding => {
                let seller_bps: u32 = env.invoke_contract(
                    &pool_id,
                    &Symbol::new(&env, "get_co_fund_share"),
                    Vec::from_array(&env, [invoice_id.into_val(&env), seller.into_val(&env)]),
                );
                let Some(available_bps) = available_commitment_capacity(
                    &env,
                    invoice_id,
                    &seller,
                    &kind,
                    seller_bps as u64,
                ) else {
                    return Err(MarketError::InvalidAmount);
                };
                if amount_or_bps as u32 > seller_bps || available_bps < amount_or_bps {
                    return Err(MarketError::InvalidAmount);
                }
            }
            ListingKind::SingleFunded => {
                if record.co_funding_round_id.is_some() {
                    return Err(MarketError::InvalidAmount);
                }
                let (_available, deployed): (i128, i128) = env.invoke_contract(
                    &pool_id,
                    &Symbol::new(&env, "get_investor_position"),
                    Vec::from_array(&env, [seller.into_val(&env), token.into_val(&env)]),
                );
                if (amount_or_bps as i128) > deployed || deployed == 0 {
                    return Err(MarketError::InvalidAmount);
                }
                let Some(available_amount) = available_commitment_capacity(
                    &env,
                    invoice_id,
                    &seller,
                    &kind,
                    deployed as u64,
                ) else {
                    return Err(MarketError::InvalidAmount);
                };
                if available_amount < amount_or_bps {
                    return Err(MarketError::InvalidAmount);
                }
            }
        }

        // Enforce per-invoice listing cap to bound iteration gas.
        let mut inv_map: Map<u64, Vec<u64>> = env
            .storage()
            .instance()
            .get(&LISTING_IDS_INV)
            .unwrap_or_else(|| Map::new(&env));
        let existing: Vec<u64> = inv_map.get(invoice_id).unwrap_or_else(|| Vec::new(&env));
        if existing.len() >= MAX_LISTINGS_PER_INVOICE {
            return Err(MarketError::TooManyListings);
        }

        let listing_id: u64 = env
            .storage()
            .instance()
            .get(&LISTING_COUNTER)
            .unwrap_or(0u64)
            .checked_add(1)
            .ok_or(MarketError::InvalidAmount)?;
        env.storage().instance().set(&LISTING_COUNTER, &listing_id);

        let listing = Listing {
            listing_id,
            invoice_id,
            seller: seller.clone(),
            token,
            kind,
            amount_or_bps,
            price,
            created_at: env.ledger().timestamp(),
            status: ListingStatus::Open,
        };

        let mut all_listings: Map<u64, Listing> = env
            .storage()
            .persistent()
            .get(&LISTING_DATA)
            .unwrap_or_else(|| Map::new(&env));
        all_listings.set(listing_id, listing);
        env.storage().persistent().set(&LISTING_DATA, &all_listings);

        let mut updated_inv = existing;
        updated_inv.push_back(listing_id);
        inv_map.set(invoice_id, updated_inv);
        env.storage().instance().set(&LISTING_IDS_INV, &inv_map);

        let mut sel_map: Map<Address, Vec<u64>> = env
            .storage()
            .instance()
            .get(&LISTING_IDS_SELLER)
            .unwrap_or_else(|| Map::new(&env));
        let mut seller_vec: Vec<u64> = sel_map
            .get(seller.clone())
            .unwrap_or_else(|| Vec::new(&env));
        seller_vec.push_back(listing_id);
        sel_map.set(seller.clone(), seller_vec);
        env.storage().instance().set(&LISTING_IDS_SELLER, &sel_map);

        env.events().publish(
            (EVT, symbol_short!("lst_open")),
            (listing_id, invoice_id, seller, amount_or_bps, price),
        );
        Ok(listing_id)
    }

    /// Cancel an open listing. Only the original seller may cancel.
    pub fn cancel_listing(env: Env, seller: Address, listing_id: u64) -> Result<(), MarketError> {
        seller.require_auth();
        require_not_paused(&env);

        let mut all_listings: Map<u64, Listing> = env
            .storage()
            .persistent()
            .get(&LISTING_DATA)
            .ok_or(MarketError::ListingNotFound)?;
        let mut listing: Listing = all_listings
            .get(listing_id)
            .ok_or(MarketError::ListingNotFound)?;

        if listing.seller != seller {
            return Err(MarketError::ListingNotSeller);
        }
        if listing.status != ListingStatus::Open {
            return Err(MarketError::ListingNotOpen);
        }

        listing.status = ListingStatus::Cancelled;
        all_listings.set(listing_id, listing.clone());
        env.storage().persistent().set(&LISTING_DATA, &all_listings);

        env.events().publish(
            (EVT, symbol_short!("lst_cncl")),
            (listing_id, listing.invoice_id, seller),
        );
        Ok(())
    }

    /// Buy an open listing. Delegates the actual balance movement to pool's
    /// `market_settle_listing`, which re-validates the underlying balances
    /// independently of what was checked at list time.
    pub fn buy_listing(env: Env, buyer: Address, listing_id: u64) -> Result<(), MarketError> {
        buyer.require_auth();
        require_not_paused(&env);

        let mut all_listings: Map<u64, Listing> = env
            .storage()
            .persistent()
            .get(&LISTING_DATA)
            .ok_or(MarketError::ListingNotFound)?;
        let mut listing: Listing = all_listings
            .get(listing_id)
            .ok_or(MarketError::ListingNotFound)?;

        if listing.status != ListingStatus::Open {
            return Err(MarketError::ListingNotOpen);
        }
        if listing.seller == buyer {
            return Err(MarketError::Unauthorized);
        }

        let pool_id: Address = env
            .storage()
            .instance()
            .get(&DataKey::PoolContract)
            .ok_or(MarketError::NotInitialized)?;

        let settlement = PoolListingSettlement {
            buyer: buyer.clone(),
            seller: listing.seller.clone(),
            invoice_id: listing.invoice_id,
            is_co_funding: listing.kind == ListingKind::CoFunding,
            amount_or_bps: listing.amount_or_bps,
            price: listing.price,
        };
        let args = Vec::from_array(
            &env,
            [
                env.current_contract_address().into_val(&env),
                settlement.into_val(&env),
            ],
        );
        let result = env.try_invoke_contract::<(), soroban_sdk::Error>(
            &pool_id,
            &Symbol::new(&env, "market_settle_listing"),
            args,
        );
        match result {
            Ok(Ok(())) => {}
            _ => return Err(MarketError::SettlementFailed),
        }

        listing.status = ListingStatus::Filled;
        all_listings.set(listing_id, listing.clone());
        env.storage().persistent().set(&LISTING_DATA, &all_listings);

        env.events().publish(
            (EVT, symbol_short!("lst_buy")),
            (
                listing_id,
                listing.invoice_id,
                listing.seller,
                buyer,
                listing.price,
            ),
        );
        Ok(())
    }

    /// Read a single listing by ID. Returns `None` if not found.
    pub fn get_listing(env: Env, listing_id: u64) -> Option<Listing> {
        let all_listings: Map<u64, Listing> = env
            .storage()
            .persistent()
            .get(&LISTING_DATA)
            .unwrap_or_else(|| Map::new(&env));
        all_listings.get(listing_id)
    }

    /// List all listing IDs for a given invoice (open and closed).
    pub fn list_listings_for_invoice(env: Env, invoice_id: u64) -> Vec<u64> {
        let inv_listings: Map<u64, Vec<u64>> = env
            .storage()
            .instance()
            .get(&LISTING_IDS_INV)
            .unwrap_or_else(|| Map::new(&env));
        inv_listings
            .get(invoice_id)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// List all listing IDs created by a given seller (open and closed).
    pub fn list_listings_for_investor(env: Env, seller: Address) -> Vec<u64> {
        let sel_listings: Map<Address, Vec<u64>> = env
            .storage()
            .instance()
            .get(&LISTING_IDS_SELLER)
            .unwrap_or_else(|| Map::new(&env));
        sel_listings.get(seller).unwrap_or_else(|| Vec::new(&env))
    }

    /// Place a resting limit order (bid or ask) on the order book for an
    /// invoice's pool position or co-funding share. Matches immediately
    /// against crossing resting orders on the opposite side, best price
    /// first with ties broken by time priority (lowest order id), up to
    /// `MAX_MATCHES_PER_CALL` fills per call; any unfilled remainder rests
    /// on the book. `price` is per-unit, scaled by `PRICE_SCALE`.
    /// `expires_at` is a ledger timestamp after which the order can no
    /// longer match (0 means no expiry). A match that fails settlement
    /// (e.g. the counterparty no longer clears KYC/compliance, or the
    /// concentration cap) is skipped rather than aborting the whole call.
    pub fn place_order(
        env: Env,
        owner: Address,
        request: PlaceOrderRequest,
    ) -> Result<u64, MarketError> {
        let PlaceOrderRequest {
            invoice_id,
            kind,
            side,
            amount_or_bps,
            price,
            expires_at,
        } = request;
        owner.require_auth();
        require_not_paused(&env);

        if amount_or_bps == 0 {
            return Err(MarketError::ZeroAmount);
        }
        if price <= 0 {
            return Err(MarketError::InvalidAmount);
        }
        let now = env.ledger().timestamp();
        if expires_at != 0 && expires_at <= now {
            return Err(MarketError::InvalidExpiry);
        }
        // Bound the book upfront — if it's already full on this side, an
        // order that doesn't fully match would have nowhere to rest. A
        // fully-matching order is rejected too in that case; this mirrors
        // `list_position`'s simple upfront gas-bound cap rather than trying
        // to predict whether matching will free up room.
        if book_side_len(&env, invoice_id, &kind, &side) >= MAX_ORDERS_PER_BOOK_SIDE {
            return Err(MarketError::TooManyOrders);
        }

        let pool_id: Address = env
            .storage()
            .instance()
            .get(&DataKey::PoolContract)
            .ok_or(MarketError::NotInitialized)?;

        let record: Option<FundedInvoiceView> = env.invoke_contract(
            &pool_id,
            &Symbol::new(&env, "get_funded_invoice"),
            Vec::from_array(&env, [invoice_id.into_val(&env)]),
        );
        let record = record.ok_or(MarketError::InvalidAmount)?;
        let token = record.token.clone();

        // Best-effort holding check, ask side only — a bid only needs
        // available cash, checked at settlement. Mirrors `list_position`'s
        // ownership pre-check; settlement re-validates independently
        // regardless.
        if side == OrderSide::Ask {
            match kind {
                ListingKind::CoFunding => {
                    let owner_bps: u32 = env.invoke_contract(
                        &pool_id,
                        &Symbol::new(&env, "get_co_fund_share"),
                        Vec::from_array(&env, [invoice_id.into_val(&env), owner.into_val(&env)]),
                    );
                    let Some(available_bps) = available_commitment_capacity(
                        &env,
                        invoice_id,
                        &owner,
                        &kind,
                        owner_bps as u64,
                    ) else {
                        return Err(MarketError::InvalidAmount);
                    };
                    if amount_or_bps as u32 > owner_bps || available_bps < amount_or_bps {
                        return Err(MarketError::InvalidAmount);
                    }
                }
                ListingKind::SingleFunded => {
                    if record.co_funding_round_id.is_some() {
                        return Err(MarketError::InvalidAmount);
                    }
                    let (_available, deployed): (i128, i128) = env.invoke_contract(
                        &pool_id,
                        &Symbol::new(&env, "get_investor_position"),
                        Vec::from_array(&env, [owner.into_val(&env), token.into_val(&env)]),
                    );
                    if (amount_or_bps as i128) > deployed || deployed == 0 {
                        return Err(MarketError::InvalidAmount);
                    }
                    let Some(available_amount) = available_commitment_capacity(
                        &env,
                        invoice_id,
                        &owner,
                        &kind,
                        deployed as u64,
                    ) else {
                        return Err(MarketError::InvalidAmount);
                    };
                    if available_amount < amount_or_bps {
                        return Err(MarketError::InvalidAmount);
                    }
                }
            }
        }

        let order_id: u64 = env
            .storage()
            .instance()
            .get(&ORDER_COUNTER)
            .unwrap_or(0u64)
            .checked_add(1)
            .ok_or(MarketError::InvalidAmount)?;
        env.storage().instance().set(&ORDER_COUNTER, &order_id);

        let mut order = Order {
            order_id,
            invoice_id,
            owner: owner.clone(),
            token,
            kind,
            side,
            price,
            amount_or_bps,
            remaining: amount_or_bps,
            created_at: now,
            expires_at,
            status: OrderStatus::Open,
        };

        env.events().publish(
            (EVT, symbol_short!("ord_open")),
            (
                order.order_id,
                order.invoice_id,
                order.owner.clone(),
                order.side.clone(),
                order.amount_or_bps,
                order.price,
            ),
        );

        match_order(&env, &pool_id, &mut order, now);

        order.status = if order.remaining == 0 {
            OrderStatus::Filled
        } else if order.remaining == order.amount_or_bps {
            OrderStatus::Open
        } else {
            OrderStatus::PartiallyFilled
        };
        if order.remaining > 0 {
            insert_into_book(&env, &order);
        }
        save_order(&env, &order);
        index_order_for_owner(&env, &owner, order_id);

        Ok(order_id)
    }

    /// Cancel a resting or partially-filled order. Only the original owner
    /// may cancel.
    pub fn cancel_order(env: Env, owner: Address, order_id: u64) -> Result<(), MarketError> {
        owner.require_auth();
        require_not_paused(&env);

        let mut order = load_order(&env, order_id).ok_or(MarketError::OrderNotFound)?;
        if order.owner != owner {
            return Err(MarketError::OrderNotOwner);
        }
        if order.status != OrderStatus::Open && order.status != OrderStatus::PartiallyFilled {
            return Err(MarketError::OrderNotOpen);
        }

        order.status = OrderStatus::Cancelled;
        save_order(&env, &order);
        remove_from_book(&env, &order);

        env.events().publish(
            (EVT, symbol_short!("ord_cncl")),
            (order_id, order.invoice_id, owner),
        );
        Ok(())
    }

    /// Permissionlessly expire a resting order past its `expires_at`.
    /// Anyone may call this — it only prunes stale book state, no funds
    /// move. Mirrors this codebase's permissionless-trigger convention for
    /// time-based state transitions nobody else is incentivized to trigger.
    pub fn expire_order(env: Env, order_id: u64) -> Result<(), MarketError> {
        require_not_paused(&env);

        let mut order = load_order(&env, order_id).ok_or(MarketError::OrderNotFound)?;
        if order.status != OrderStatus::Open && order.status != OrderStatus::PartiallyFilled {
            return Err(MarketError::OrderNotOpen);
        }
        let now = env.ledger().timestamp();
        if order.expires_at == 0 || order.expires_at > now {
            return Err(MarketError::InvalidExpiry);
        }

        order.status = OrderStatus::Expired;
        save_order(&env, &order);
        remove_from_book(&env, &order);

        env.events().publish(
            (EVT, symbol_short!("ord_exp")),
            (order_id, order.invoice_id, order.owner),
        );
        Ok(())
    }

    /// Read a single order by ID. Returns `None` if not found.
    pub fn get_order(env: Env, order_id: u64) -> Option<Order> {
        load_order(&env, order_id)
    }

    /// Current resting depth for an invoice's order book, as
    /// `(bid_levels, ask_levels)` in insertion (time-priority) order. Each
    /// level carries the order's id, price, and remaining quantity so callers
    /// can render a real book without a follow-up `get_order` per id (#1133).
    /// Bounded to `MAX_ORDERS_PER_BOOK_SIDE` per side.
    pub fn get_order_book(
        env: Env,
        invoice_id: u64,
        kind: ListingKind,
    ) -> (Vec<OrderBookLevel>, Vec<OrderBookLevel>) {
        let key = book_key(invoice_id, &kind);
        let bids: Map<u64, Vec<u64>> = env
            .storage()
            .instance()
            .get(&BOOK_BIDS)
            .unwrap_or_else(|| Map::new(&env));
        let asks: Map<u64, Vec<u64>> = env
            .storage()
            .instance()
            .get(&BOOK_ASKS)
            .unwrap_or_else(|| Map::new(&env));
        let to_levels = |ids: Vec<u64>| -> Vec<OrderBookLevel> {
            let mut levels = Vec::new(&env);
            for id in ids.iter() {
                if let Some(order) = load_order(&env, id) {
                    levels.push_back(OrderBookLevel {
                        order_id: order.order_id,
                        price: order.price,
                        quantity: order.remaining,
                    });
                }
            }
            levels
        };
        (
            to_levels(bids.get(key).unwrap_or_else(|| Vec::new(&env))),
            to_levels(asks.get(key).unwrap_or_else(|| Vec::new(&env))),
        )
    }

    /// All order IDs ever placed by `owner` (any status).
    pub fn list_orders_for_owner(env: Env, owner: Address) -> Vec<u64> {
        let owner_map: Map<Address, Vec<u64>> = env
            .storage()
            .instance()
            .get(&ORDER_IDS_OWNER)
            .unwrap_or_else(|| Map::new(&env));
        owner_map.get(owner).unwrap_or_else(|| Vec::new(&env))
    }

    /// #865: predict how long `investor`'s (already-queued) withdrawal
    /// request will take to clear, based on the pool's current withdrawal
    /// queue, its trailing deposit-inflow rate, and the nearest due date
    /// among its open invoices for `token`.
    pub fn estimate_withdrawal_wait(
        env: Env,
        investor: Address,
        token: Address,
    ) -> Result<WaitEstimate, MarketError> {
        let pool_id: Address = env
            .storage()
            .instance()
            .get(&DataKey::PoolContract)
            .ok_or(MarketError::NotInitialized)?;

        let queue: Vec<WithdrawalRequestView> = env.invoke_contract(
            &pool_id,
            &Symbol::new(&env, "get_withdrawal_queue"),
            Vec::from_array(&env, [token.into_val(&env)]),
        );
        let tt: TokenTotalsView = env.invoke_contract(
            &pool_id,
            &Symbol::new(&env, "get_token_totals"),
            Vec::from_array(&env, [token.into_val(&env)]),
        );
        let share_token: Option<Address> = env.invoke_contract(
            &pool_id,
            &Symbol::new(&env, "get_share_token"),
            Vec::from_array(&env, [token.into_val(&env)]),
        );
        let total_shares: i128 = match share_token {
            Some(share_token) => env.invoke_contract(
                &share_token,
                &Symbol::new(&env, "total_supply"),
                Vec::new(&env),
            ),
            None => 0,
        };

        let mut queue_position = 0u32;
        let mut capital_ahead = 0i128;
        let mut position = 1u32;
        for request in queue.iter() {
            if request.investor == investor {
                queue_position = position;
                break;
            }
            if total_shares > 0 {
                if let Some(amount) = request
                    .shares
                    .checked_mul(tt.pool_value)
                    .and_then(|value| value.checked_div(total_shares))
                {
                    capital_ahead = capital_ahead.saturating_add(amount);
                }
            }
            position = position.saturating_add(1);
        }

        let now = env.ledger().timestamp();
        let open_invoices: Vec<FundedInvoiceView> = env.invoke_contract(
            &pool_id,
            &Symbol::new(&env, "get_open_invoices_for_token"),
            Vec::from_array(&env, [token.into_val(&env)]),
        );
        let mut nearest_invoice_due_date = 0u64;
        for record in open_invoices.iter() {
            if nearest_invoice_due_date == 0 || record.due_date < nearest_invoice_due_date {
                nearest_invoice_due_date = record.due_date;
            }
        }

        let rate: i128 = env.invoke_contract(
            &pool_id,
            &Symbol::new(&env, "get_trailing_inflow_rate"),
            Vec::from_array(&env, [token.into_val(&env)]),
        );
        let via_rate = if rate > 0 && capital_ahead > 0 {
            Some((capital_ahead / rate) as u64)
        } else if capital_ahead <= 0 {
            Some(0)
        } else {
            None
        };
        let via_due_date = if nearest_invoice_due_date > now {
            Some(nearest_invoice_due_date - now)
        } else {
            None
        };
        let estimated_wait_secs = match (via_rate, via_due_date) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => MAX_WAIT_ESTIMATE_SECS,
        }
        .clamp(MIN_WAIT_ESTIMATE_SECS, MAX_WAIT_ESTIMATE_SECS);

        Ok(WaitEstimate {
            queue_position,
            capital_ahead,
            nearest_invoice_due_date,
            estimated_wait_secs,
        })
    }

    /// #865: project available liquidity at up to `horizon_days` daily points, based on
    /// principal from open invoices' known due dates plus the trailing deposit-inflow
    /// rate extrapolated forward. `horizon_days` is clamped to
    /// `[1, MAX_FORECAST_HORIZON_DAYS]` to bound loop iteration cost.
    pub fn get_liquidity_forecast(
        env: Env,
        token: Address,
        horizon_days: u32,
    ) -> Result<Vec<LiquidityForecastPoint>, MarketError> {
        let pool_id: Address = env
            .storage()
            .instance()
            .get(&DataKey::PoolContract)
            .ok_or(MarketError::NotInitialized)?;

        let horizon = horizon_days.clamp(1, MAX_FORECAST_HORIZON_DAYS);
        let tt: TokenTotalsView = env.invoke_contract(
            &pool_id,
            &Symbol::new(&env, "get_token_totals"),
            Vec::from_array(&env, [token.into_val(&env)]),
        );
        let current_liquidity = tt.pool_value.checked_sub(tt.total_deployed).unwrap_or(0);
        let now = env.ledger().timestamp();
        let open_invoices: Vec<FundedInvoiceView> = env.invoke_contract(
            &pool_id,
            &Symbol::new(&env, "get_open_invoices_for_token"),
            Vec::from_array(&env, [token.into_val(&env)]),
        );
        let rate: i128 = env.invoke_contract(
            &pool_id,
            &Symbol::new(&env, "get_trailing_inflow_rate"),
            Vec::from_array(&env, [token.into_val(&env)]),
        );

        let mut points = Vec::new(&env);
        for day in 1..=horizon {
            let horizon_ts = now.saturating_add((day as u64) * SECS_PER_DAY);
            let mut expected_repayments: i128 = 0;
            for invoice in open_invoices.iter() {
                if invoice.due_date <= horizon_ts {
                    let outstanding = invoice.principal.saturating_sub(invoice.repaid_amount);
                    expected_repayments = expected_repayments.saturating_add(outstanding);
                }
            }
            let extrapolated_inflow = rate.saturating_mul((day as i128) * SECS_PER_DAY as i128);
            let projected_available = current_liquidity
                .saturating_add(expected_repayments)
                .saturating_add(extrapolated_inflow);
            points.push_back(LiquidityForecastPoint {
                day,
                projected_available,
            });
        }
        Ok(points)
    }
}
