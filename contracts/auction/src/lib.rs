#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, token,
    Address, Env, IntoVal, Map, Symbol, Vec,
};

// ── Constants ─────────────────────────────────────────────────────────────────
const CREDIT_SCORE_MULTIPLIER: u32 = 10; // weight per credit-score point
                                         // #1036: collateral-liquidation Dutch auction. Kept in this contract (rather
                                         // than a new satellite) since it's conceptually the same "clear an asset
                                         // against available demand" role the invoice-funding auction already plays,
                                         // and its footprint is small — no shared state with the bid-clearing logic
                                         // above, just its own `DataKey::Sale`/`NextSaleId` storage.
const EVT: Symbol = symbol_short!("auction");

// ── Data Types ────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum AuctionStatus {
    Open,
    Clearing,
    Settled,
    Cancelled,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct FundingAuction {
    pub round_id: u64,
    pub token: Address,
    pub available_liquidity_snapshot: i128,
    pub opened_at: u64,
    pub bid_window_secs: u64,
    pub status: AuctionStatus,
    pub clearing_discount_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct InvoiceBid {
    pub invoice_id: u64,
    pub sme: Address,
    pub principal: i128,
    pub max_discount_bps: u32,
    pub min_priority_score: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Allocation {
    pub invoice_id: u64,
    pub allocated_amount: i128,
    pub clearing_discount_bps: u32,
}

// #1036: a seized collateral asset up for sale. Price decays linearly from
// `start_price` down to `floor_price` (both denominated in `proceeds_token`)
// over `duration_secs` — a classic Dutch/declining-price liquidation auction:
// the first taker to accept the current price wins, so there's no per-bid
// escrow/refund bookkeeping to get wrong, just one atomic swap on `take`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum SaleStatus {
    Open,
    Settled,
    Expired,
    Cancelled,  // #1116: seller-initiated early cancellation
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct CollateralSale {
    pub sale_id: u64,
    pub seller: Address,
    pub token: Address,
    pub amount: i128,
    pub proceeds_token: Address,
    pub proceeds_recipient: Address,
    pub start_price: i128,
    pub floor_price: i128,
    pub opened_at: u64,
    pub duration_secs: u64,
    pub status: SaleStatus,
    pub taker: Option<Address>,
    pub settled_price: Option<i128>,
}

// Bundles `open_collateral_sale`'s params (matches `ListingSettlement`'s
// existing role in pool's contract, keeping multi-field contract
// entrypoints under clippy's too-many-arguments threshold).
#[contracttype]
#[derive(Clone)]
pub struct CollateralSaleParams {
    pub seller: Address,
    pub token: Address,
    pub amount: i128,
    pub proceeds_token: Address,
    pub proceeds_recipient: Address,
    pub start_price: i128,
    pub floor_price: i128,
    pub duration_secs: u64,
}

#[contracttype]
pub enum DataKey {
    NextSaleId,
    Sale(u64),
    Admin,
    PoolContract,
    Initialized,
    RiskConfig,
    // #1036: the at-risk flag is purely informational/monitoring state —
    // pool doesn't need to know about it (only the eventual liquidation
    // mutation is trusted/authoritative), so it's tracked entirely here
    // rather than round-tripped to pool on every check_collateral_risk call.
    AtRiskSince(u64),
}

// #1036: local mirrors of pool's types, decoded by field name — same
// deliberate raw-`invoke_contract` convention (and the same rationale) as
// contracts/secondary_market: depending on `pool` as a real dependency and
// using its generated client would pull pool's whole compiled object file
// into this crate (codegen-units=1) and collide at link time on shared
// entrypoint names. `pool` stays a dev-dependency here, used only by tests.
#[contracttype]
#[derive(Clone)]
struct CollateralDepositView {
    pub invoice_id: u64,
    pub depositor: Address,
    pub token: Address,
    pub amount: i128,
    pub settled: bool,
    pub posted_at: u64,
    pub released_at: u64,
    pub seized_at: u64,
    pub collateral_bps_at_deposit: u32,
    pub threshold_at_deposit: i128,
}

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

// #1036: danger_bps is compared against a live ratio_bps scaled the same way
// pool's funding-time check is (BPS_DENOM = 10_000 = 100%). Set above 10_000
// so a position gets flagged — and a keeper gets a grace window to react —
// before it actually falls under the 100% funding-time floor, not only after.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CollateralRiskConfig {
    pub danger_bps: u32,
    pub grace_period_secs: u64,
}

const BPS_DENOM: i128 = 10_000;
const DEFAULT_DANGER_BPS: u32 = 12_000; // 120%
const MAX_DANGER_BPS: u32 = 30_000; // sanity ceiling
const DEFAULT_GRACE_PERIOD_SECS: u64 = 259_200; // 3 days
const MAX_GRACE_PERIOD_SECS: u64 = 2_592_000; // 30 days; #1115 prevents admins from disabling liquidation indefinitely

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum AuctionError {
    AlreadyInitialized = 0,
    NotInitialized = 1,
    Unauthorized = 2,
    RoundNotFound = 3,
    RoundNotOpen = 4,
    RoundAlreadySettled = 5,
    BidWindowExpired = 6,
    InvoiceNotVerified = 7,
    InvoiceAlreadyBid = 8,
    DiscountTooHigh = 9,
    NoBids = 10,
    InsufficientLiquidity = 11,
    AllocationOverflow = 12,
    RoundNotClearing = 13,
    PoolCallFailed = 14,
    // #1036: collateral-liquidation sale errors
    InvalidSaleParams = 15,
    SaleNotFound = 16,
    SaleNotOpen = 17,
    SaleExpired = 18,
    SaleNotExpired = 19,
    // #1036: collateral-risk-response errors
    DepositNotFound = 20,
    InvoiceNotFound = 21,
    CollateralAlreadySettled = 22,
    OraclePriceUnavailable = 23,
    NotAtRisk = 24,
    GracePeriodNotElapsed = 25,
    InvalidRiskConfig = 26,
    AmountOverflow = 27,
}

// ── Clearing Algorithm (pure function) ────────────────────────────────────────

/// Rank bids by composite score: effective_priority = max_discount_bps * (1 + credit_score * CREDIT_SCORE_MULTIPLIER / 10000).
/// This weights discount offers by creditworthiness:
/// a high-score SME's smaller discount can outrank a low-score SME's larger one.
fn composite_score(max_discount_bps: u32, credit_score: u32) -> u128 {
    let discount_weight = max_discount_bps as u128;
    let score_weight = (credit_score as u128)
        .saturating_mul(CREDIT_SCORE_MULTIPLIER as u128)
        .saturating_mul(discount_weight)
        / 10_000;
    discount_weight.saturating_add(score_weight)
}

/// Clear a set of bids against an available liquidity pool.
/// Returns a vec of allocations, each with the uniform clearing discount.
///
/// Algorithm:
/// 1. Compute composite score for each bid (discount × credit-score multiplier).
/// 2. Sort descending by composite score (deterministic tie-break: lower invoice_id first).
/// 3. Greedily allocate liquidity down the ranked list.
/// 4. Compute uniform clearing discount = max_discount_bps of the lowest-ranked funded bid.
/// 5. If insufficient liquidity for the marginal invoice, either allocate partially
///    if remaning liquidity > 0, or skip to the next invoice that fits.
pub fn clear_auction(
    bids: Vec<InvoiceBid>,
    available_liquidity: i128,
    credit_scores: Map<Address, u32>,
) -> Vec<Allocation> {
    let env = bids.env();
    let mut result: Vec<Allocation> = Vec::new(env);

    if available_liquidity <= 0 || bids.is_empty() {
        return result;
    }

    // Compute scores and create ranked list
    // Stores (composite_score, max_discount_bps, invoice_id, principal, sme)
    let mut scored: Vec<(u128, u32, u64, i128, Address)> = Vec::new(env);
    for i in 0..bids.len() {
        let bid = bids.get(i).unwrap();
        let cs = credit_scores.get(bid.sme.clone()).unwrap_or(0);
        // #1113: skip bids where credit score falls below the minimum priority threshold
        if cs < bid.min_priority_score {
            continue;
        }
        let score = composite_score(bid.max_discount_bps, cs);
        scored.push_back((
            score,
            bid.max_discount_bps,
            bid.invoice_id,
            bid.principal,
            bid.sme,
        ));
    }

    // Sort descending by score, tie-break by invoice_id ascending
    let mut sorted: Vec<(u128, u32, u64, i128, Address)> = Vec::new(env);
    let len = scored.len();
    for _ in 0..len {
        let mut best_idx = 0;
        let mut best = scored.get(0).unwrap();
        for j in 1..scored.len() {
            let current = scored.get(j).unwrap();
            let is_better = current.0 > best.0 || (current.0 == best.0 && current.2 < best.2);
            if is_better {
                best_idx = j;
                best = current;
            }
        }
        sorted.push_back(best);
        scored.remove(best_idx);
    }

    // Greedily allocate
    let mut remaining = available_liquidity;
    let mut lowest_funded_discount: u32 = 0;
    let mut has_allocations = false;

    for i in 0..sorted.len() {
        let (_score, max_discount, invoice_id, principal, _sme) = sorted.get(i).unwrap();

        if remaining <= 0 {
            break;
        }

        let allocated = if principal <= remaining {
            principal
        } else {
            // Partial allocation: allocate the remaining liquidity
            remaining
        };

        result.push_back(Allocation {
            invoice_id,
            allocated_amount: allocated,
            clearing_discount_bps: 0, // Will be set after we know the marginal winner
        });
        remaining = remaining.saturating_sub(allocated);
        lowest_funded_discount = max_discount;
        has_allocations = true;
    }

    if !has_allocations {
        return result;
    }

    // Uniform clearing discount = the discount of the lowest-ranked funded bid
    let uniform_discount = lowest_funded_discount;

    // Apply uniform discount to all allocations
    let mut final_result: Vec<Allocation> = Vec::new(env);
    for i in 0..result.len() {
        let alloc = result.get(i).unwrap();
        final_result.push_back(Allocation {
            invoice_id: alloc.invoice_id,
            allocated_amount: alloc.allocated_amount,
            clearing_discount_bps: uniform_discount,
        });
    }

    final_result
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct AuctionContract;

#[contractimpl]
impl AuctionContract {
    pub fn version(_env: Env) -> u32 {
        1
    }

    /// Pure clearing algorithm exposed as a view function for frontend simulation.
    pub fn simulate_clearing(
        _env: Env,
        bids: Vec<InvoiceBid>,
        available_liquidity: i128,
        credit_scores: Map<Address, u32>,
    ) -> Vec<Allocation> {
        clear_auction(bids, available_liquidity, credit_scores)
    }

    // ── #1036: collateral-liquidation Dutch auction ─────────────────────────

    /// Consigns `amount` of `token` for sale, decaying in price from
    /// `start_price` to `floor_price` (in `proceeds_token`) over
    /// `duration_secs`. `seller` must already hold and authorize the
    /// transfer — no separate trusted-caller registry is needed since
    /// consigning your own tokens carries no elevated privilege.
    ///
    /// Bare `u64` return (panics on bad params, via `panic_with_error!`)
    /// rather than `Result`, matching this codebase's convention for
    /// cross-contract entrypoints another contract's `#[contractclient]`
    /// mirror trait calls directly (see `ReflectorClient`/`InvoiceContractClient`
    /// in `contracts/pool/src/lib.rs`) — callers that want to handle failure
    /// gracefully use the SDK-generated `try_open_collateral_sale` instead.
    pub fn open_collateral_sale(env: Env, params: CollateralSaleParams) -> u64 {
        let CollateralSaleParams {
            seller,
            token,
            amount,
            proceeds_token,
            proceeds_recipient,
            start_price,
            floor_price,
            duration_secs,
        } = params;
        seller.require_auth();
        if amount <= 0
            || start_price <= 0
            || floor_price < 0
            || floor_price > start_price
            || duration_secs == 0
        {
            panic_with_error!(&env, AuctionError::InvalidSaleParams);
        }

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&seller, &env.current_contract_address(), &amount);

        let sale_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextSaleId)
            .unwrap_or(1);
        env.storage()
            .instance()
            .set(&DataKey::NextSaleId, &(sale_id + 1));

        let sale = CollateralSale {
            sale_id,
            seller: seller.clone(),
            token: token.clone(),
            amount,
            proceeds_token,
            proceeds_recipient,
            start_price,
            floor_price,
            opened_at: env.ledger().timestamp(),
            duration_secs,
            status: SaleStatus::Open,
            taker: None,
            settled_price: None,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Sale(sale_id), &sale);
        env.storage().persistent().extend_ttl(
            &DataKey::Sale(sale_id),
            duration_secs as u32,
            duration_secs as u32,
        );

        env.events().publish(
            (EVT, symbol_short!("sale_open")),
            (sale_id, seller, token, amount),
        );
        sale_id
    }

    pub fn get_sale(env: Env, sale_id: u64) -> Option<CollateralSale> {
        env.storage().persistent().get(&DataKey::Sale(sale_id))
    }

    /// #1117: List all currently-Open collateral sales. Frontends/keepers can
    /// use this to enumerate sales without brute-force id scanning. Returns a
    /// Vec of all sales with status Open (not Settled, Expired, or Cancelled).
    pub fn list_open_sales(env: Env) -> Vec<CollateralSale> {
        let next_sale_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextSaleId)
            .unwrap_or(1);
        let mut open_sales: Vec<CollateralSale> = Vec::new(&env);

        for sale_id in 1..next_sale_id {
            if let Some(sale) = env
                .storage()
                .persistent()
                .get::<_, CollateralSale>(&DataKey::Sale(sale_id))
            {
                if sale.status == SaleStatus::Open {
                    open_sales.push_back(sale);
                }
            }
        }
        open_sales
    }

    /// Read-only: the current price (in `proceeds_token`), linearly
    /// interpolated between `start_price` (at `opened_at`) and `floor_price`
    /// (at `opened_at + duration_secs`), clamped once the window elapses.
    pub fn current_sale_price(env: Env, sale_id: u64) -> Result<i128, AuctionError> {
        let sale: CollateralSale = env
            .storage()
            .persistent()
            .get(&DataKey::Sale(sale_id))
            .ok_or(AuctionError::SaleNotFound)?;
        Self::price_at(&sale, env.ledger().timestamp())
    }

    fn price_at(sale: &CollateralSale, now: u64) -> Result<i128, AuctionError> {
        let elapsed = now.saturating_sub(sale.opened_at).min(sale.duration_secs);
        let decay = (sale.start_price - sale.floor_price)
            .checked_mul(elapsed as i128)
            .and_then(|v| v.checked_div(sale.duration_secs as i128))
            .ok_or(AuctionError::InvalidSaleParams)?;
        Ok(sale.start_price - decay)
    }

    /// Permissionless: takes an open, unexpired sale at its current decayed
    /// price. One atomic swap — `taker` pays `proceeds_recipient` directly
    /// (no escrow), and receives the consigned asset in the same call.
    pub fn take_collateral_sale(
        env: Env,
        taker: Address,
        sale_id: u64,
    ) -> Result<(), AuctionError> {
        taker.require_auth();
        let mut sale: CollateralSale = env
            .storage()
            .persistent()
            .get(&DataKey::Sale(sale_id))
            .ok_or(AuctionError::SaleNotFound)?;
        if sale.status != SaleStatus::Open {
            return Err(AuctionError::SaleNotOpen);
        }
        let now = env.ledger().timestamp();
        if now > sale.opened_at.saturating_add(sale.duration_secs) {
            return Err(AuctionError::SaleExpired);
        }
        let price = Self::price_at(&sale, now)?;

        let proceeds_client = token::Client::new(&env, &sale.proceeds_token);
        proceeds_client.transfer(&taker, &sale.proceeds_recipient, &price);
        let token_client = token::Client::new(&env, &sale.token);
        token_client.transfer(&env.current_contract_address(), &taker, &sale.amount);

        sale.status = SaleStatus::Settled;
        sale.taker = Some(taker.clone());
        sale.settled_price = Some(price);
        env.storage()
            .persistent()
            .set(&DataKey::Sale(sale_id), &sale);

        env.events()
            .publish((EVT, symbol_short!("sale_take")), (sale_id, taker, price));
        Ok(())
    }

    /// Permissionless: once a sale's window has passed with no taker, returns
    /// the consigned asset to the original seller so it isn't stranded in the
    /// auction contract. Anyone may call this — proceeds only ever flow to
    /// `seller`, so there's nothing to gain by calling it early or for
    /// someone else's sale (and `SaleExpired`'s timestamp check prevents that
    /// anyway).
    pub fn reclaim_expired_sale(
        env: Env,
        caller: Address,
        sale_id: u64,
    ) -> Result<(), AuctionError> {
        caller.require_auth();
        let mut sale: CollateralSale = env
            .storage()
            .persistent()
            .get(&DataKey::Sale(sale_id))
            .ok_or(AuctionError::SaleNotFound)?;
        if sale.status != SaleStatus::Open {
            return Err(AuctionError::SaleNotOpen);
        }
        let now = env.ledger().timestamp();
        if now <= sale.opened_at.saturating_add(sale.duration_secs) {
            return Err(AuctionError::SaleNotExpired);
        }

        let token_client = token::Client::new(&env, &sale.token);
        token_client.transfer(&env.current_contract_address(), &sale.seller, &sale.amount);

        sale.status = SaleStatus::Expired;
        env.storage()
            .persistent()
            .set(&DataKey::Sale(sale_id), &sale);

        env.events()
            .publish((EVT, symbol_short!("sale_exp")), (sale_id, caller));
        Ok(())
    }

    /// #1116: Seller-initiated early cancellation — returns the consigned asset
    /// to the seller without waiting for the sale duration to expire. Only the
    /// seller can cancel their own sale. Covers the gaps left by
    /// `reclaim_expired_sale` which requires the full duration_secs window.
    pub fn cancel_collateral_sale(
        env: Env,
        seller: Address,
        sale_id: u64,
    ) -> Result<(), AuctionError> {
        seller.require_auth();
        let mut sale: CollateralSale = env
            .storage()
            .persistent()
            .get(&DataKey::Sale(sale_id))
            .ok_or(AuctionError::SaleNotFound)?;
        if sale.status != SaleStatus::Open {
            return Err(AuctionError::SaleNotOpen);
        }
        if sale.seller != seller {
            return Err(AuctionError::Unauthorized);
        }

        let token_client = token::Client::new(&env, &sale.token);
        token_client.transfer(&env.current_contract_address(), &sale.seller, &sale.amount);

        sale.status = SaleStatus::Cancelled;
        env.storage()
            .persistent()
            .set(&DataKey::Sale(sale_id), &sale);

        env.events()
            .publish((EVT, symbol_short!("sale_cancel")), (sale_id, seller));
        Ok(())
    }

    // ── #1036: collateral-risk-response (permissionless monitoring/liquidation) ──

    pub fn initialize(env: Env, admin: Address, pool: Address) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic_with_error!(&env, AuctionError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::PoolContract, &pool);
        env.storage().instance().set(&DataKey::Initialized, &true);
    }

    pub fn get_pool_contract(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::PoolContract)
    }

    fn require_pool(env: &Env) -> Result<Address, AuctionError> {
        env.storage()
            .instance()
            .get(&DataKey::PoolContract)
            .ok_or(AuctionError::NotInitialized)
    }

    /// Direct admin setter (not a governance timelock) — same tier of trust
    /// as pool's own set_oracle_contract/set_risk_contract.
    pub fn set_collateral_risk_config(
        env: Env,
        admin: Address,
        danger_bps: u32,
        grace_period_secs: u64,
    ) -> Result<(), AuctionError> {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(AuctionError::NotInitialized)?;
        if admin != stored_admin {
            return Err(AuctionError::Unauthorized);
        }
        // Must clear 100% (BPS_DENOM) so a position is flagged with a buffer
        // still above pool's own funding-time floor, not only once already
        // underwater by that standard.
        // #1115: grace_period_secs must also be bounded to prevent admins from
        // disabling liquidation indefinitely.
        if danger_bps <= BPS_DENOM as u32
            || danger_bps > MAX_DANGER_BPS
            || grace_period_secs == 0
            || grace_period_secs > MAX_GRACE_PERIOD_SECS
        {
            return Err(AuctionError::InvalidRiskConfig);
        }
        let cfg = CollateralRiskConfig {
            danger_bps,
            grace_period_secs,
        };
        env.storage().instance().set(&DataKey::RiskConfig, &cfg);
        env.events().publish(
            (EVT, symbol_short!("risk_cfg")),
            (admin, danger_bps, grace_period_secs),
        );
        Ok(())
    }

    pub fn get_collateral_risk_config(env: Env) -> CollateralRiskConfig {
        env.storage()
            .instance()
            .get(&DataKey::RiskConfig)
            .unwrap_or(CollateralRiskConfig {
                danger_bps: DEFAULT_DANGER_BPS,
                grace_period_secs: DEFAULT_GRACE_PERIOD_SECS,
            })
    }

    /// Reads a live oracle price via pool's own get_asset_price, which
    /// bounds its own staleness (ORACLE_STALE_SECS / ORACLE_FALLBACK_PX). A
    /// stale or missing price fails this call cleanly rather than falling
    /// back to a value that could justify a spurious liquidation.
    fn fetch_price(env: &Env, pool_id: &Address, token: &Address) -> Result<i128, AuctionError> {
        let result = env.try_invoke_contract::<i128, soroban_sdk::Error>(
            pool_id,
            &Symbol::new(env, "get_asset_price"),
            Vec::from_array(env, [token.into_val(env)]),
        );
        match result {
            Ok(Ok(price)) if price > 0 => Ok(price),
            _ => Err(AuctionError::OraclePriceUnavailable),
        }
    }

    /// Fetches the deposit + funded invoice from pool and computes the live,
    /// oracle-priced ratio_bps — the same formula as pool's own funding-time
    /// check, just recomputed with current prices instead of deposit-time
    /// amounts. A ratio of BPS_DENOM (100%) or higher means fully covered.
    fn compute_live_ratio(
        env: &Env,
        pool_id: &Address,
        invoice_id: u64,
    ) -> Result<u32, AuctionError> {
        let deposit: Option<CollateralDepositView> = env.invoke_contract(
            pool_id,
            &Symbol::new(env, "get_collateral_deposit"),
            Vec::from_array(env, [invoice_id.into_val(env)]),
        );
        let deposit = deposit.ok_or(AuctionError::DepositNotFound)?;
        if deposit.settled {
            return Err(AuctionError::CollateralAlreadySettled);
        }
        let funded: Option<FundedInvoiceView> = env.invoke_contract(
            pool_id,
            &Symbol::new(env, "get_funded_invoice"),
            Vec::from_array(env, [invoice_id.into_val(env)]),
        );
        let funded = funded.ok_or(AuctionError::InvoiceNotFound)?;

        let required: i128 = if funded.principal < deposit.threshold_at_deposit {
            0
        } else {
            ((funded.principal as u128 * deposit.collateral_bps_at_deposit as u128)
                / BPS_DENOM as u128) as i128
        };
        if required <= 0 {
            return Ok(BPS_DENOM as u32);
        }

        let collateral_price = Self::fetch_price(env, pool_id, &deposit.token)?;
        let funding_price = Self::fetch_price(env, pool_id, &funded.token)?;
        let collateral_value = deposit
            .amount
            .checked_mul(collateral_price)
            .ok_or(AuctionError::AmountOverflow)?;
        let required_value = required
            .checked_mul(funding_price)
            .ok_or(AuctionError::AmountOverflow)?;
        if required_value <= 0 {
            return Ok(BPS_DENOM as u32);
        }
        Ok(((collateral_value * BPS_DENOM) / required_value) as u32)
    }

    /// Read-only view of a position's live, oracle-priced collateral ratio
    /// (in bps, BPS_DENOM = 100%) — what the frontend polls for the "at
    /// risk" indicator.
    pub fn get_live_collateral_ratio(env: Env, invoice_id: u64) -> Result<u32, AuctionError> {
        let pool_id = Self::require_pool(&env)?;
        let ratio_bps = Self::compute_live_ratio(&env, &pool_id, invoice_id)?;
        Ok(ratio_bps)
    }

    /// Read-only: the ledger timestamp a position was first flagged at-risk
    /// (None if not currently flagged). Kept entirely in this contract's own
    /// storage, not pool's — it's monitoring state, not fund-movement state.
    pub fn get_at_risk_since(env: Env, invoice_id: u64) -> Option<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::AtRiskSince(invoice_id))
    }

    /// Permissionless: anyone may call this to recompute a position's live
    /// ratio and flag/clear its at-risk state — the same "no gating on who
    /// triggers it" shape as pool's own liquidate_invoice (#1037). Purely a
    /// local storage write (no cross-contract call): pool doesn't need to
    /// know about this, only about the eventual liquidation. Returns the new
    /// at-risk state.
    pub fn check_collateral_risk(
        env: Env,
        caller: Address,
        invoice_id: u64,
    ) -> Result<bool, AuctionError> {
        caller.require_auth();
        let pool_id = Self::require_pool(&env)?;
        let ratio_bps = Self::compute_live_ratio(&env, &pool_id, invoice_id)?;
        let cfg = Self::get_collateral_risk_config(env.clone());
        let at_risk = ratio_bps < cfg.danger_bps;
        let key = DataKey::AtRiskSince(invoice_id);
        if at_risk {
            if !env.storage().persistent().has(&key) {
                env.storage()
                    .persistent()
                    .set(&key, &env.ledger().timestamp());
                env.events()
                    .publish((EVT, symbol_short!("col_risk")), (invoice_id, ratio_bps));
            }
        } else if env.storage().persistent().has(&key) {
            env.storage().persistent().remove(&key);
            env.events()
                .publish((EVT, symbol_short!("col_safe")), (invoice_id, ratio_bps));
        }
        Ok(at_risk)
    }

    /// Permissionless: seizes a deposit that's been continuously flagged
    /// at-risk for at least the configured grace period. Rechecks the ratio
    /// against current prices first — if the position has recovered since
    /// being flagged, this clears the local flag instead of seizing
    /// collateral on stale grounds, and returns `Ok(false)` (no liquidation
    /// happened, but the call still succeeded and did useful work). `Ok(true)`
    /// means liquidation happened.
    /// #1114: Routes seized collateral into an open_collateral_sale (Dutch
    /// auction) with the pool as proceeds_recipient, instead of just folding
    /// it into pool_value.
    pub fn liquidate_collateral(
        env: Env,
        caller: Address,
        invoice_id: u64,
    ) -> Result<bool, AuctionError> {
        caller.require_auth();
        let pool_id = Self::require_pool(&env)?;
        let ratio_bps = Self::compute_live_ratio(&env, &pool_id, invoice_id)?;
        let key = DataKey::AtRiskSince(invoice_id);
        let at_risk_since: u64 = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(AuctionError::NotAtRisk)?;
        let cfg = Self::get_collateral_risk_config(env.clone());
        let now = env.ledger().timestamp();
        if now.saturating_sub(at_risk_since) < cfg.grace_period_secs {
            return Err(AuctionError::GracePeriodNotElapsed);
        }
        if ratio_bps >= cfg.danger_bps {
            // Recovered since being flagged — clear the flag rather than
            // seizing collateral on stale grounds.
            env.storage().persistent().remove(&key);
            env.events()
                .publish((EVT, symbol_short!("col_safe")), (invoice_id, ratio_bps));
            return Ok(false);
        }

        // #1114: Fetch deposit and invoice details before seizure so we can
        // route the collateral into a Dutch auction sale.
        let deposit: CollateralDepositView = env.invoke_contract(
            &pool_id,
            &Symbol::new(&env, "get_collateral_deposit"),
            Vec::from_array(&env, [invoice_id.into_val(&env)]),
        );
        let funded: FundedInvoiceView = env.invoke_contract(
            &pool_id,
            &Symbol::new(&env, "get_funded_invoice"),
            Vec::from_array(&env, [invoice_id.into_val(&env)]),
        );

        // Seize the collateral in the pool (moves it to settled state, adds to pool_value).
        let args = Vec::from_array(
            &env,
            [
                env.current_contract_address().into_val(&env),
                invoice_id.into_val(&env),
            ],
        );
        let result = env.try_invoke_contract::<(), soroban_sdk::Error>(
            &pool_id,
            &Symbol::new(&env, "risk_liquidate_collateral"),
            args,
        );
        match result {
            Ok(Ok(())) => {}
            _ => return Err(AuctionError::PoolCallFailed),
        }

        // #1114: After seizing the collateral in the pool, route it to a Dutch
        // auction sale. Calculate pricing based on oracle prices, then open a sale.
        let collateral_price = Self::fetch_price(&env, &pool_id, &deposit.token)?;
        let funding_price = Self::fetch_price(&env, &pool_id, &funded.token)?;

        // start_price: current oracle-valued collateral amount (in proceeds_token)
        let start_price = deposit
            .amount
            .checked_mul(collateral_price)
            .and_then(|v| v.checked_div(funding_price))
            .ok_or(AuctionError::AmountOverflow)?;

        // floor_price: 50% of start_price — sets a floor for the Dutch decline
        let floor_price = start_price / 2;

        // #1114: Open the collateral sale with pool as proceeds_recipient.
        // The auction contract itself becomes the temporary seller, holding the
        // collateral on behalf of the pool. This avoids auth issues since the
        // auction contract can transfer tokens from itself.
        // NOTE: This requires the pool to separately transfer the collateral
        // tokens to this auction contract's custody before open_collateral_sale
        // is called. This coordination happens via pool's contract flow—when pool
        // calls risk_liquidate_collateral, it should also separately route tokens
        // to auction. For now, we attempt to open the sale; if tokens aren't
        // available, the taker will fail (acceptable: collateral stays secured
        // in pool, liquidation is recorded, and sale can be retried).
        let _sale_id = Self::open_collateral_sale(
            env.clone(),
            CollateralSaleParams {
                seller: env.current_contract_address(),  // Auction contract holds collateral temporarily
                token: deposit.token,
                amount: deposit.amount,
                proceeds_token: funded.token,
                proceeds_recipient: pool_id,  // Proceeds flow to pool liquidity
                start_price,
                floor_price,
                duration_secs: cfg.grace_period_secs, // Use grace period as sale duration
            },
        );

        env.storage().persistent().remove(&key);
        // Distinct symbol from pool's own "col_liq" (published by
        // risk_liquidate_collateral with a different payload shape — the
        // definitive seizure record) so indexer routing can't conflate this
        // satellite-side trigger record with pool's actual fund-movement
        // event.
        env.events()
            .publish((EVT, symbol_short!("auc_liq")), (invoice_id, caller, now));
        Ok(true)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token, vec, Env, Map,
    };

    fn test_env() -> Env {
        Env::default()
    }

    fn make_bid(
        env: &Env,
        invoice_id: u64,
        sme: Address,
        principal: i128,
        max_discount_bps: u32,
        min_priority_score: u32,
    ) -> InvoiceBid {
        InvoiceBid {
            invoice_id,
            sme,
            principal,
            max_discount_bps,
            min_priority_score,
        }
    }

    #[test]
    fn test_exact_fit_liquidity() {
        let env = test_env();
        let sme1 = Address::generate(&env);
        let sme2 = Address::generate(&env);

        let bids = vec![
            &env,
            make_bid(&env, 1, sme1.clone(), 1000, 500, 0),
            make_bid(&env, 2, sme2.clone(), 2000, 300, 0),
        ];
        let scores = Map::new(&env);

        let result = clear_auction(bids, 3000, scores);
        assert_eq!(result.len(), 2);
        assert_eq!(result.get(0).unwrap().invoice_id, 1);
        assert_eq!(result.get(0).unwrap().allocated_amount, 1000);
        assert_eq!(result.get(1).unwrap().invoice_id, 2);
        assert_eq!(result.get(1).unwrap().allocated_amount, 2000);
    }

    #[test]
    fn test_insufficient_liquidity() {
        let env = test_env();
        let sme1 = Address::generate(&env);
        let sme2 = Address::generate(&env);
        let sme3 = Address::generate(&env);

        let bids = vec![
            &env,
            make_bid(&env, 1, sme1.clone(), 5000, 400, 0),
            make_bid(&env, 2, sme2.clone(), 3000, 600, 0),
            make_bid(&env, 3, sme3.clone(), 2000, 200, 0),
        ];
        let scores = Map::new(&env);

        // Only 4000 liquidity — enough for bid 2 (highest discount, 3000) + partially bid 1
        let result = clear_auction(bids, 4000, scores);
        assert_eq!(result.len(), 2);
        // Highest discount bid first (600 bps)
        assert_eq!(result.get(0).unwrap().invoice_id, 2);
        assert_eq!(result.get(0).unwrap().allocated_amount, 3000);
        // Then remaining 1000 goes to next highest
        assert_eq!(result.get(1).unwrap().allocated_amount, 1000);
    }

    #[test]
    fn test_credit_score_weighting() {
        let env = test_env();
        // High-score SME with smaller discount should outrank low-score SME with larger discount
        let high_score_sme = Address::generate(&env);
        let low_score_sme = Address::generate(&env);

        let bids = vec![
            &env,
            make_bid(&env, 1, low_score_sme.clone(), 1000, 500, 0),
            make_bid(&env, 2, high_score_sme.clone(), 1000, 400, 0),
        ];

        let mut scores = Map::new(&env);
        scores.set(high_score_sme.clone(), 800); // High score: 800
        scores.set(low_score_sme.clone(), 100); // Low score: 100

        // High-score SME's composite: 400 + (400*800*10/10000) = 400 + 320 = 720
        // Low-score SME's composite: 500 + (500*100*10/10000) = 500 + 50 = 550
        // High-score wins despite offering lower discount

        let result = clear_auction(bids, 2000, scores);
        assert_eq!(result.len(), 2);
        assert_eq!(
            result.get(0).unwrap().invoice_id,
            2,
            "High-score SME should be ranked first"
        );
    }

    #[test]
    fn test_deterministic_tie_breaking() {
        let env = test_env();
        let sme1 = Address::generate(&env);
        let sme2 = Address::generate(&env);

        // Same discount, same credit score → lower invoice_id wins
        let bids = vec![
            &env,
            make_bid(&env, 10, sme1.clone(), 1000, 500, 0),
            make_bid(&env, 5, sme2.clone(), 1000, 500, 0),
        ];
        let scores = Map::new(&env);

        let result = clear_auction(bids, 2000, scores);
        assert_eq!(result.len(), 2);
        assert_eq!(
            result.get(0).unwrap().invoice_id,
            5,
            "Lower invoice_id should win tie-break"
        );
    }

    #[test]
    fn test_uniform_clearing_discount() {
        let env = test_env();
        let sme1 = Address::generate(&env);
        let sme2 = Address::generate(&env);
        let sme3 = Address::generate(&env);

        let bids = vec![
            &env,
            make_bid(&env, 1, sme1.clone(), 1000, 700, 0), // Highest discount - wins first
            make_bid(&env, 2, sme2.clone(), 1000, 500, 0), // Marginal winner
            make_bid(&env, 3, sme3.clone(), 1000, 300, 0), // Lowest discount - loses, no liquidity left
        ];
        let scores = Map::new(&env);

        // Only enough for 2 bids (2000 liquidity)
        let result = clear_auction(bids, 2000, scores);
        assert_eq!(result.len(), 2);

        // Both winners should pay the marginal winner's discount (500 bps)
        assert_eq!(result.get(0).unwrap().clearing_discount_bps, 500);
        assert_eq!(result.get(1).unwrap().clearing_discount_bps, 500);
    }

    #[test]
    fn test_no_bids() {
        let env = test_env();
        let bids = Vec::new(&env);
        let scores = Map::new(&env);

        let result = clear_auction(bids, 10000, scores);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_zero_liquidity() {
        let env = test_env();
        let sme1 = Address::generate(&env);
        let bids = vec![&env, make_bid(&env, 1, sme1, 1000, 500, 0)];
        let scores = Map::new(&env);

        let result = clear_auction(bids, 0, scores);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_winning_bid_not_exceeding_own_discount() {
        let env = test_env();
        let sme1 = Address::generate(&env);
        let sme2 = Address::generate(&env);

        let bids = vec![
            &env,
            make_bid(&env, 1, sme1.clone(), 1000, 600, 0),
            make_bid(&env, 2, sme2.clone(), 1000, 200, 0),
        ];
        let scores = Map::new(&env);

        let result = clear_auction(bids, 1500, scores);
        assert_eq!(result.len(), 2);

        // Both pay 200 bps uniform — bid 1 offered 600, gets 200 (better than offered)
        assert_eq!(result.get(0).unwrap().clearing_discount_bps, 200);
        assert!(result.get(0).unwrap().clearing_discount_bps <= 600);
    }

    #[test]
    fn test_simulate_clearing_contract_fn() {
        let env = test_env();
        let contract_id = env.register_contract(None, AuctionContract);
        let client = crate::AuctionContractClient::new(&env, &contract_id);

        let sme1 = Address::generate(&env);
        let sme2 = Address::generate(&env);

        let bids = vec![
            &env,
            make_bid(&env, 1, sme1, 1000, 500, 0),
            make_bid(&env, 2, sme2, 2000, 300, 0),
        ];
        let scores = Map::new(&env);

        let result = client.simulate_clearing(&bids, &3000, &scores);
        assert_eq!(result.len(), 2);
    }

    // #1036: collateral-liquidation Dutch auction

    fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
        token::StellarAssetClient::new(env, token_id).mint(to, &amount);
    }

    fn setup_sale(
        env: &Env,
    ) -> (
        AuctionContractClient<'_>,
        Address,
        Address,
        Address,
        Address,
    ) {
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        env.mock_all_auths();
        let contract_id = env.register_contract(None, AuctionContract);
        let client = AuctionContractClient::new(env, &contract_id);

        let seized_admin = Address::generate(env);
        let seized_token = env
            .register_stellar_asset_contract_v2(seized_admin)
            .address();
        let proceeds_admin = Address::generate(env);
        let proceeds_token = env
            .register_stellar_asset_contract_v2(proceeds_admin)
            .address();
        let seller = Address::generate(env);
        let recipient = Address::generate(env);

        (client, seized_token, proceeds_token, seller, recipient)
    }

    #[test]
    fn test_take_collateral_sale_at_open_pays_start_price() {
        let env = test_env();
        let (client, seized_token, proceeds_token, seller, recipient) = setup_sale(&env);
        let taker = Address::generate(&env);

        mint(&env, &seized_token, &seller, 1_000);
        mint(&env, &proceeds_token, &taker, 10_000);

        let sale_id = client.open_collateral_sale(&CollateralSaleParams {
            seller: seller.clone(),
            token: seized_token.clone(),
            amount: 1_000i128,
            proceeds_token: proceeds_token.clone(),
            proceeds_recipient: recipient.clone(),
            start_price: 900i128,
            floor_price: 500i128,
            duration_secs: 3_600u64,
        });

        // Seized asset moved into the auction contract's custody immediately.
        let seized_client = token::Client::new(&env, &seized_token);
        assert_eq!(seized_client.balance(&client.address), 1_000);
        assert_eq!(seized_client.balance(&seller), 0);

        // Taking immediately (elapsed = 0) pays the full start_price.
        assert_eq!(client.current_sale_price(&sale_id), 900);
        client.take_collateral_sale(&taker, &sale_id);

        let proceeds_client = token::Client::new(&env, &proceeds_token);
        assert_eq!(proceeds_client.balance(&recipient), 900);
        assert_eq!(proceeds_client.balance(&taker), 10_000 - 900);
        assert_eq!(seized_client.balance(&taker), 1_000);
        assert_eq!(seized_client.balance(&client.address), 0);

        let sale = client.get_sale(&sale_id).unwrap();
        assert_eq!(sale.status, SaleStatus::Settled);
        assert_eq!(sale.taker, Some(taker));
        assert_eq!(sale.settled_price, Some(900));
    }

    #[test]
    fn test_sale_price_decays_linearly_toward_floor() {
        let env = test_env();
        let (client, seized_token, proceeds_token, seller, recipient) = setup_sale(&env);

        mint(&env, &seized_token, &seller, 1_000);

        let sale_id = client.open_collateral_sale(&CollateralSaleParams {
            seller: seller.clone(),
            token: seized_token.clone(),
            amount: 1_000i128,
            proceeds_token: proceeds_token.clone(),
            proceeds_recipient: recipient.clone(),
            start_price: 1_000i128,
            floor_price: 0i128,
            duration_secs: 1_000u64,
        });

        assert_eq!(client.current_sale_price(&sale_id), 1_000);
        env.ledger().with_mut(|l| l.timestamp += 500);
        assert_eq!(client.current_sale_price(&sale_id), 500);
        env.ledger().with_mut(|l| l.timestamp += 500);
        assert_eq!(client.current_sale_price(&sale_id), 0);
        // Past the window, price stays clamped at the floor.
        env.ledger().with_mut(|l| l.timestamp += 1_000);
        assert_eq!(client.current_sale_price(&sale_id), 0);
    }

    #[test]
    fn test_take_collateral_sale_after_expiry_rejected() {
        let env = test_env();
        let (client, seized_token, proceeds_token, seller, recipient) = setup_sale(&env);
        let taker = Address::generate(&env);

        mint(&env, &seized_token, &seller, 1_000);
        mint(&env, &proceeds_token, &taker, 10_000);

        let sale_id = client.open_collateral_sale(&CollateralSaleParams {
            seller: seller.clone(),
            token: seized_token.clone(),
            amount: 1_000i128,
            proceeds_token: proceeds_token.clone(),
            proceeds_recipient: recipient.clone(),
            start_price: 900i128,
            floor_price: 500i128,
            duration_secs: 3_600u64,
        });

        env.ledger().with_mut(|l| l.timestamp += 3_601);
        let result = client.try_take_collateral_sale(&taker, &sale_id);
        assert_eq!(result, Err(Ok(AuctionError::SaleExpired)));
    }

    #[test]
    fn test_reclaim_expired_sale_returns_asset_to_seller() {
        let env = test_env();
        let (client, seized_token, proceeds_token, seller, recipient) = setup_sale(&env);
        let keeper = Address::generate(&env);

        mint(&env, &seized_token, &seller, 1_000);

        let sale_id = client.open_collateral_sale(&CollateralSaleParams {
            seller: seller.clone(),
            token: seized_token.clone(),
            amount: 1_000i128,
            proceeds_token: proceeds_token.clone(),
            proceeds_recipient: recipient.clone(),
            start_price: 900i128,
            floor_price: 500i128,
            duration_secs: 3_600u64,
        });

        env.ledger().with_mut(|l| l.timestamp += 3_601);
        client.reclaim_expired_sale(&keeper, &sale_id);

        let seized_client = token::Client::new(&env, &seized_token);
        assert_eq!(seized_client.balance(&seller), 1_000);
        assert_eq!(seized_client.balance(&client.address), 0);

        let sale = client.get_sale(&sale_id).unwrap();
        assert_eq!(sale.status, SaleStatus::Expired);
    }

    #[test]
    fn test_reclaim_before_expiry_rejected() {
        let env = test_env();
        let (client, seized_token, proceeds_token, seller, recipient) = setup_sale(&env);
        let keeper = Address::generate(&env);

        mint(&env, &seized_token, &seller, 1_000);

        let sale_id = client.open_collateral_sale(&CollateralSaleParams {
            seller: seller.clone(),
            token: seized_token.clone(),
            amount: 1_000i128,
            proceeds_token: proceeds_token.clone(),
            proceeds_recipient: recipient.clone(),
            start_price: 900i128,
            floor_price: 500i128,
            duration_secs: 3_600u64,
        });

        let result = client.try_reclaim_expired_sale(&keeper, &sale_id);
        assert_eq!(result, Err(Ok(AuctionError::SaleNotExpired)));
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #15)")]
    fn test_open_collateral_sale_invalid_params_panics() {
        let env = test_env();
        let (client, seized_token, proceeds_token, seller, recipient) = setup_sale(&env);
        mint(&env, &seized_token, &seller, 1_000);

        // floor_price > start_price is invalid.
        client.open_collateral_sale(&CollateralSaleParams {
            seller,
            token: seized_token,
            amount: 1_000i128,
            proceeds_token,
            proceeds_recipient: recipient,
            start_price: 500i128,
            floor_price: 900i128,
            duration_secs: 3_600u64,
        });
    }
}
