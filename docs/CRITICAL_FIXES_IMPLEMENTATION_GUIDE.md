# Critical Fixes Implementation Guide

This document provides implementation guidance for four critical issues in the Astera smart contracts that require immediate attention.

---

## Issue #1510: Missing Events in Config Setters

### Problem
`set_credit_score_contract()` and `set_insurance_contract()` in `contracts/pool/src/lib.rs` write new addresses without emitting events, unlike other config setters like `set_risk_contract()`.

**Impact:** Off-chain monitors have no signal when trust anchors are rotated, particularly critical for `insurance_contract` which gates authorization in `receive_insurance_payout()`.

### Location
- File: `contracts/pool/src/lib.rs`
- Lines: 4329-4342 (`set_credit_score_contract`)
- Lines: 4349-4362 (`set_insurance_contract`)

### Solution

Add event emissions to both functions:

```rust
pub fn set_credit_score_contract(env: Env, new_address: Address) {
    require_admin(&env);
    
    let old_address = env.storage()
        .persistent()
        .get(&DataKey::CreditScoreContract);
    
    env.storage()
        .persistent()
        .set(&DataKey::CreditScoreContract, &new_address);
    
    // ADD THIS EVENT
    env.events().publish(
        (symbol_short!("pool"), symbol_short!("cfg_upd")),
        (symbol_short!("credit_score"), old_address, new_address.clone())
    );
}

pub fn set_insurance_contract(env: Env, new_address: Address) {
    require_admin(&env);
    
    let old_address = env.storage()
        .persistent()
        .get(&DataKey::InsuranceContract);
    
    env.storage()
        .persistent()
        .set(&DataKey::InsuranceContract, &new_address);
    
    // ADD THIS EVENT
    env.events().publish(
        (symbol_short!("pool"), symbol_short!("cfg_upd")),
        (symbol_short!("insurance"), old_address, new_address.clone())
    );
}
```

### Testing

Add tests to verify events are emitted:

```rust
#[test]
fn test_set_credit_score_contract_emits_event() {
    let env = Env::default();
    let contract = create_pool_contract(&env);
    
    let old_addr = Address::generate(&env);
    let new_addr = Address::generate(&env);
    
    contract.set_credit_score_contract(&new_addr);
    
    let events = env.events().all();
    assert!(events.iter().any(|e| {
        e.topics.contains(&symbol_short!("cfg_upd")) &&
        e.topics.contains(&symbol_short!("credit_score"))
    }));
}
```

---

## Issue #1503: Missing Validation and Event in add_funding

### Problem
`add_funding()` in `contracts/invoice/src/lib.rs:2753-2791` lacks:
1. Amount validation (`amount <= 0` check)
2. Event emission

**Impact:** Negative amounts corrupt `InvoiceFunding(id)` storage, and off-chain systems have no signal when funding is added.

### Location
- File: `contracts/invoice/src/lib.rs`
- Lines: 2753-2791

### Solution

Add validation and event:

```rust
pub fn add_funding(env: Env, invoice_id: u64, amount: i128, funder: Address) -> Result<(), InvoiceError> {
    funder.require_auth();
    
    // ADD THIS VALIDATION
    if amount <= 0 {
        return Err(InvoiceError::InvalidAmount);
    }
    
    let mut invoice = get_invoice(&env, invoice_id)?;
    
    let current_funding = env.storage()
        .persistent()
        .get(&DataKey::InvoiceFunding(invoice_id))
        .unwrap_or(0i128);
    
    let new_funded = current_funding
        .checked_add(amount)
        .ok_or(InvoiceError::ArithmeticOverflow)?;
    
    if new_funded > invoice.amount {
        return Err(InvoiceError::OverFunded);
    }
    
    env.storage()
        .persistent()
        .set(&DataKey::InvoiceFunding(invoice_id), &new_funded);
    
    // ADD THIS EVENT
    env.events().publish(
        (symbol_short!("invoice"), symbol_short!("funded")),
        (invoice_id, funder.clone(), amount, new_funded)
    );
    
    Ok(())
}
```

### Testing

```rust
#[test]
fn test_add_funding_rejects_negative_amount() {
    let env = Env::default();
    let contract = create_invoice_contract(&env);
    let invoice_id = create_test_invoice(&contract);
    
    let result = contract.add_funding(&invoice_id, &-100, &funder);
    assert_eq!(result.unwrap_err(), InvoiceError::InvalidAmount);
}

#[test]
fn test_add_funding_emits_event() {
    let env = Env::default();
    let contract = create_invoice_contract(&env);
    let invoice_id = create_test_invoice(&contract);
    
    contract.add_funding(&invoice_id, &1000, &funder);
    
    let events = env.events().all();
    assert!(events.iter().any(|e| {
        e.topics.contains(&symbol_short!("funded"))
    }));
}
```

---

## Issue #1509: Missing Price Validation in market_settle_listing

### Problem
`market_settle_listing()` in `contracts/pool/src/lib.rs:7456-7625` accepts `price: i128` from the secondary-market contract without validation.

**Impact:** A negative price inverts trade economics—buyer's balance increases while seller's decreases.

### Location
- File: `contracts/pool/src/lib.rs`
- Lines: ~7456-7625

### Solution

Add price validation:

```rust
pub fn market_settle_listing(
    env: Env,
    listing_id: u64,
    settlement: MarketSettlement
) -> Result<(), PoolError> {
    // Validate caller is the authorized secondary market contract
    let market_contract = env.storage()
        .persistent()
        .get(&DataKey::SecondaryMarketContract)
        .ok_or(PoolError::MarketNotConfigured)?;
    market_contract.require_auth();
    
    // ADD THIS VALIDATION
    if settlement.price <= 0 {
        return Err(PoolError::InvalidPrice);
    }
    
    let listing = get_listing(&env, listing_id)?;
    
    // Deduct from buyer
    let buyer_balance = get_balance(&env, &settlement.buyer)?;
    if buyer_balance < settlement.price {
        return Err(PoolError::InsufficientBalance);
    }
    let new_buyer_balance = buyer_balance - settlement.price;
    set_balance(&env, &settlement.buyer, new_buyer_balance);
    
    // Credit to seller
    let seller_balance = get_balance(&env, &listing.seller)?;
    let new_seller_balance = seller_balance
        .checked_add(settlement.price)
        .ok_or(PoolError::ArithmeticOverflow)?;
    set_balance(&env, &listing.seller, new_seller_balance);
    
    // Transfer shares
    transfer_shares(&env, &listing.seller, &settlement.buyer, listing.share_amount)?;
    
    // Mark listing as settled
    mark_listing_settled(&env, listing_id)?;
    
    env.events().publish(
        (symbol_short!("pool"), symbol_short!("mkt_stl")),
        (listing_id, settlement.buyer.clone(), settlement.price)
    );
    
    Ok(())
}
```

### Testing

```rust
#[test]
fn test_market_settle_listing_rejects_negative_price() {
    let env = Env::default();
    let contract = create_pool_contract(&env);
    let listing_id = create_test_listing(&contract);
    
    let settlement = MarketSettlement {
        buyer: Address::generate(&env),
        price: -1000, // Negative price
    };
    
    let result = contract.market_settle_listing(&listing_id, &settlement);
    assert_eq!(result.unwrap_err(), PoolError::InvalidPrice);
}

#[test]
fn test_market_settle_listing_rejects_zero_price() {
    let env = Env::default();
    let contract = create_pool_contract(&env);
    let listing_id = create_test_listing(&contract);
    
    let settlement = MarketSettlement {
        buyer: Address::generate(&env),
        price: 0,
    };
    
    let result = contract.market_settle_listing(&listing_id, &settlement);
    assert_eq!(result.unwrap_err(), PoolError::InvalidPrice);
}
```

---

## Issue #1508: Performance Issue in Co-Funding Position Queries

### Problem
`get_investor_co_fund_positions()` and `list_co_funding_rounds()` in `contracts/pool/src/lib.rs:6155-6189` iterate the entire `CoFundingRoundIds` registry (one storage read per round ever created) to find positions for a single investor.

**Impact:** Query cost grows unbounded with total rounds created, not with investor's actual position count. This becomes expensive as the protocol matures.

### Location
- File: `contracts/pool/src/lib.rs`
- Lines: ~6155-6189

### Solution

Add an investor-indexed lookup:

```rust
// Add new storage key
#[derive(Clone)]
pub enum DataKey {
    // ... existing keys
    InvestorRounds(Address), // Maps investor -> Vec<u64> of round IDs
}

// Update create_co_funding_round to index by investor
pub fn create_co_funding_round(env: Env, params: CoFundingParams) -> Result<u64, PoolError> {
    // ... existing creation logic
    
    let round_id = generate_round_id(&env);
    
    // Store in global registry (for admin/analytics)
    let mut all_rounds: Vec<u64> = env.storage()
        .persistent()
        .get(&DataKey::CoFundingRoundIds)
        .unwrap_or(Vec::new(&env));
    all_rounds.push_back(round_id);
    env.storage().persistent().set(&DataKey::CoFundingRoundIds, &all_rounds);
    
    // NEW: Index by lead investor
    let mut investor_rounds: Vec<u64> = env.storage()
        .persistent()
        .get(&DataKey::InvestorRounds(params.lead_investor.clone()))
        .unwrap_or(Vec::new(&env));
    investor_rounds.push_back(round_id);
    env.storage().persistent().set(
        &DataKey::InvestorRounds(params.lead_investor.clone()),
        &investor_rounds
    );
    
    Ok(round_id)
}

// Optimized query
pub fn get_investor_co_fund_positions(env: Env, investor: Address) -> Vec<CoFundPosition> {
    // Read only the rounds this investor participated in
    let investor_round_ids: Vec<u64> = env.storage()
        .persistent()
        .get(&DataKey::InvestorRounds(investor.clone()))
        .unwrap_or(Vec::new(&env));
    
    let mut positions = Vec::new(&env);
    
    for round_id in investor_round_ids.iter() {
        if let Some(round) = env.storage()
            .persistent()
            .get::<DataKey, CoFundingRound>(&DataKey::CoFundingRound(round_id))
        {
            if let Some(position) = round.participants.get(&investor) {
                positions.push_back(CoFundPosition {
                    round_id,
                    amount: position.amount,
                    share_balance: position.shares,
                });
            }
        }
    }
    
    positions
}
```

### Migration Strategy

Since this changes storage layout, existing deployments need migration:

```rust
pub fn migrate_investor_index(env: Env) -> Result<(), PoolError> {
    require_admin(&env);
    
    let all_rounds: Vec<u64> = env.storage()
        .persistent()
        .get(&DataKey::CoFundingRoundIds)
        .unwrap_or(Vec::new(&env));
    
    for round_id in all_rounds.iter() {
        if let Some(round) = env.storage()
            .persistent()
            .get::<DataKey, CoFundingRound>(&DataKey::CoFundingRound(round_id))
        {
            // Index each participant
            for (investor, _) in round.participants.iter() {
                let mut investor_rounds: Vec<u64> = env.storage()
                    .persistent()
                    .get(&DataKey::InvestorRounds(investor.clone()))
                    .unwrap_or(Vec::new(&env));
                
                if !investor_rounds.contains(&round_id) {
                    investor_rounds.push_back(round_id);
                    env.storage().persistent().set(
                        &DataKey::InvestorRounds(investor.clone()),
                        &investor_rounds
                    );
                }
            }
        }
    }
    
    Ok(())
}
```

### Testing

```rust
#[test]
fn test_investor_positions_query_efficiency() {
    let env = Env::default();
    let contract = create_pool_contract(&env);
    
    let investor = Address::generate(&env);
    
    // Create 1000 rounds, investor participates in 10
    for i in 0..1000 {
        let params = if i % 100 == 0 {
            create_round_with_investor(&investor)
        } else {
            create_round_without_investor()
        };
        contract.create_co_funding_round(&params);
    }
    
    // Query should only read 10 rounds, not 1000
    let start_budget = env.budget().cpu_insn_count();
    let positions = contract.get_investor_co_fund_positions(&investor);
    let cost = start_budget - env.budget().cpu_insn_count();
    
    assert_eq!(positions.len(), 10);
    // Assert cost is proportional to position count, not total rounds
    assert!(cost < 50_000); // Threshold for 10 reads
}
```

---

## Testing Checklist

### For All Fixes

- [ ] Unit tests added for each fix
- [ ] Integration tests verify end-to-end behavior
- [ ] Negative test cases included
- [ ] Event emission verified where applicable
- [ ] Storage operations don't corrupt existing data
- [ ] Contract upgrade simulation passes
- [ ] Gas/instruction costs remain reasonable

### Specific Tests

**#1510 (Config Events):**
- [ ] Event emitted when config changed
- [ ] Old and new addresses in event payload
- [ ] Indexer can parse events
- [ ] Admin-only enforcement still works

**#1503 (Funding Validation):**
- [ ] Negative amount rejected
- [ ] Zero amount rejected
- [ ] Positive amount succeeds
- [ ] Event emitted with correct values
- [ ] Overflow protection still works

**#1509 (Price Validation):**
- [ ] Negative price rejected
- [ ] Zero price rejected
- [ ] Positive price succeeds
- [ ] Balance arithmetic correct
- [ ] Authorization check still enforced

**#1508 (Co-Funding Performance):**
- [ ] Query cost proportional to position count
- [ ] Migration rebuilds index correctly
- [ ] Index updated on new round creation
- [ ] No duplicate entries in index
- [ ] Old query behavior preserved (backwards compatible)

---

## Deployment Strategy

1. **Deploy to Testnet First**
   - Run full test suite
   - Deploy contracts
   - Run migration (#1508 only)
   - Verify via frontend/API

2. **Monitor Events**
   - Verify indexer picks up new config events (#1510)
   - Verify funding events appear (#1503)

3. **Performance Validation**
   - Query co-funding positions for high-activity investors
   - Measure cost reduction (#1508)

4. **Mainnet Deployment**
   - Schedule maintenance window
   - Deploy contracts
   - Run migration if needed
   - Announce changes to integrators

---

## References

- **Contract Files:**
  - `contracts/pool/src/lib.rs` (Issues #1510, #1509, #1508)
  - `contracts/invoice/src/lib.rs` (Issue #1503)

- **Related Documentation:**
  - [Contract Upgrade Guide](./contract-upgrade-guide.md)
  - [Event Schema](./events.md)
  - [Testing Guide](../contracts/README.md#testing)

---

**Priority:** Critical  
**Estimated Effort:** 4-6 hours for implementation + testing  
**Risk Level:** Medium (requires contract upgrade)  
**Last Updated:** October 2024
