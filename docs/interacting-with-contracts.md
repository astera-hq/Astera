# Interacting with Contracts

## Secondary Market for Pool Positions and Co-Funding Shares

Investors who have deployed capital into a funded invoice can list their
position for sale before the invoice repays, providing early-exit liquidity
without touching the withdrawal queue.

### Concepts

| Term | Meaning |
|------|---------|
| `CoFunding` listing | Seller offers some or all of their `CoFundShare` bps in a filled co-funding round |
| `SingleFunded` listing | Seller offers a raw token amount of their `deployed` principal in a single-funded invoice |
| `price` | Flat token amount the buyer pays from their `available` pool balance |

### Entrypoints

#### `list_position(seller, invoice_id, kind, amount_or_bps, price) -> u64`

Creates an open listing and returns its `listing_id`.

- `kind = CoFunding`: `amount_or_bps` is the bps of the seller's `CoFundShare`
  to offer (must be ≤ seller's current share; round must be `Filled`).
- `kind = SingleFunded`: `amount_or_bps` is the raw token amount of deployed
  principal to offer (must be ≤ seller's `deployed` balance).
- Compliance gate and KYC are checked on the seller at listing time.
- At most `50` open listings per invoice are allowed.

```bash
stellar contract invoke --id <POOL_CONTRACT_ID> --source seller --network testnet \
  -- list_position \
  --seller <SELLER_ADDRESS> \
  --invoice_id 42 \
  --kind '{"CoFunding": []}' \
  --amount_or_bps 5000 \
  --price 4800
```

#### `cancel_listing(seller, listing_id)`

Cancels an open listing. Only the original seller may cancel.

```bash
stellar contract invoke --id <POOL_CONTRACT_ID> --source seller --network testnet \
  -- cancel_listing \
  --seller <SELLER_ADDRESS> \
  --listing_id 1
```

#### `buy_listing(buyer, listing_id)`

Atomically:
1. Debits `listing.price` from the buyer's `available` balance.
2. Credits `listing.price` to the seller's `available` balance.
3. Transfers the claim (CoFundShare bps or deployed principal slice) to the buyer.
4. Marks the listing `Filled`.

Compliance, KYC, and the per-investor concentration cap
(`PoolConfig.max_single_investor_bps`) are enforced on the buyer.

```bash
stellar contract invoke --id <POOL_CONTRACT_ID> --source buyer --network testnet \
  -- buy_listing \
  --buyer <BUYER_ADDRESS> \
  --listing_id 1
```

#### `get_listing(listing_id) -> Option<Listing>`

Read a single listing by ID.

#### `list_listings_for_invoice(invoice_id) -> Vec<u64>`

Returns all listing IDs (open and closed) for a given invoice.

#### `list_listings_for_investor(seller) -> Vec<u64>`

Returns all listing IDs (open and closed) created by a given seller.

### Repayment after a transfer

When `repay_invoice` is called after a secondary-market transfer:

- **Co-funded invoices**: `repay_invoice_request` distributes proceeds
  pro-rata by the *current* `CoFundShare` bps, so the buyer (new holder)
  receives the repayment, not the original seller.
- **Single-funded invoices**: the buyer's `deployed` balance is credited
  when the invoice repays via the `reward_per_share` accumulator, which
  tracks the current holder's share token balance.

### Default after a transfer

If `mark_defaulted` fires after a secondary-market transfer, any
insurance-reserve payout resolves to the *current* holder of the claim,
consistent with the repayment logic above.

### SDK usage

```typescript
import { PoolClient } from '@astera/sdk';

const pool = new PoolClient({ rpcUrl, network, contractId: POOL_CONTRACT_ID });

// List a co-funding share
const listingId = await pool.listPosition({
  signer,
  seller: sellerAddress,
  invoiceId: 42n,
  kind: 'CoFunding',
  amountOrBps: 5000n,
  price: 4800n,
});

// Buy a listing
await pool.buyListing({ signer, buyer: buyerAddress, listingId });

// Cancel a listing
await pool.cancelListing({ signer, seller: sellerAddress, listingId });

// Query
const listing = await pool.getListing(listingId);
const invoiceListings = await pool.listListingsForInvoice(42n);
const myListings = await pool.listListingsForInvestor(sellerAddress);
```

### Events

| Event symbol | Payload | Description |
|---|---|---|
| `lst_open` | `(listing_id, invoice_id, seller, amount_or_bps, price)` | New listing created |
| `lst_cncl` | `(listing_id, invoice_id, seller)` | Listing cancelled by seller |
| `lst_buy`  | `(listing_id, invoice_id, seller, buyer, price)` | Listing filled by buyer |

## Invoice Contract

Manages invoice lifecycle from creation through repayment or default. Invoices can be funded by multiple co-investors and track repayment schedules with grace periods.

### Entrypoints

- `create_invoice(creator, due_date, amount)` — Create a new invoice
- `deploy_capital(investor, invoice_id, amount)` — Deploy capital into an invoice
- `repay_invoice(repayer, invoice_id, amount)` — Repay an invoice
- `mark_defaulted(keeper, invoice_id)` — Mark invoice as defaulted
- `get_invoice(id)` — Retrieve invoice by ID
- `get_invoice_count()` — Get total number of invoices
- `get_grace_period()` — Get default grace period in days

## Pool Contract

The main contract managing pool operations, investor positions, and capital deployment across multiple invoices and tokens.

### Entrypoints

- `initialize(admin, token, share_token, invoice_contract)` — Initialize the pool with a primary stablecoin and share token
- `deposit(investor, token, amount)` — Deposit stablecoin, receive share tokens proportionally
- `withdraw(investor, token, shares)` — Burn share tokens and withdraw stablecoin
- `request_withdrawal(investor, token, shares)` — Queue a withdrawal when liquidity is insufficient
- `cancel_withdrawal_request(investor, token, request_id)` — Cancel a queued withdrawal request
- `fund_invoice(admin, invoice_id, amount, sme, due_date, token)` — Deploy capital to fund an invoice
- `repay_invoice(invoice_id, payer, amount)` — Accept a repayment (partial or full) for a funded invoice
- `estimate_repayment(invoice_id)` — View the current amount owed including accrued interest
- `estimate_deposit_yield(token, amount, horizon_secs)` — Preview the interest a deposit would earn over a horizon at the current utilization and rate, before calling `deposit`
- `claim_yield(investor, token)` — Claim accrued yield for an investor's token position
- `get_investor_position_summary(investor)` — Read all non-empty token deposit positions, claimable yield per token, and co-funding principal shares in one call
- `get_token_totals(token)` — Get pool value, deployed amount, and fee totals for a token
- `get_funded_invoice(invoice_id)` — Retrieve a funded invoice record
- `available_liquidity(token)` — Get uninvested token balance available for new invoices
- `accepted_tokens()` — List all accepted stablecoin addresses
- `add_token(admin, token, share_token)` — Add a new accepted stablecoin with its share token
- `remove_token(admin, token)` — Remove a token when balances are zero
- `get_config()` — Retrieve pool configuration (fees, yield, rate model settings)
- `set_factoring_fee(admin, fee_bps)` — Update the factoring fee in basis points
- `get_storage_stats()` — Get funded invoice counts and totals

`get_investor_position_summary(investor)` returns `deposits` (one entry per token with a non-empty position or pending yield) and `co_funding` (one entry per invoice). Each deposit entry includes the token's pending yield; amounts remain denominated in their respective tokens.

## Tranche Contract

Divides pool positions into tranches with different seniority levels and risk profiles.

### Entrypoints

- `create_tranche(pool_id, name, seniority_level)` — Create a new tranche
- `deploy_to_tranche(investor, tranche_id, amount)` — Deploy capital into a tranche
- `get_tranche(tranche_id)` — Retrieve tranche details
- `distribute_returns(invoice_id, amount)` — Distribute repayment pro-rata by tranche seniority

## Arbitration Contract

Handles dispute resolution between parties (borrowers, investors, or the pool).

### Entrypoints

- `raise_dispute(initiator, dispute_type, details)` — Initiate a dispute
- `submit_evidence(juror, dispute_id, evidence)` — Submit evidence for a dispute
- `vote(juror, dispute_id, verdict)` — Cast a vote on a dispute outcome
- `finalize_dispute(dispute_id)` — Finalize dispute and execute verdict
- `get_dispute(dispute_id)` — Retrieve dispute details

## Auction Contract

Enables reverse auctions for invoice pricing and settlement matching between borrowers and investors.

### Entrypoints

- `create_auction(invoice_id, target_amount, duration_secs)` — Create a new auction
- `place_bid(bidder, auction_id, rate)` — Submit a bid rate
- `settle_auction(auction_id)` — Settle auction and commit winning rate
- `get_auction(auction_id)` — Retrieve auction details

## Access Control Contract

Manages role-based permissions across the platform (admin, borrower, investor, etc.).

### Entrypoints

- `set_role(admin, subject, role)` — Assign a role to an address
- `check_role(subject, role)` — Verify if subject has a role
- `revoke_role(admin, subject, role)` — Remove a role from an address
- `has_admin_role(subject)` — Check admin status

## Compliance Contract

Performs sanctions screening and risk assessment on addresses and transactions.

### Entrypoints

- `screen_address(address, name, jurisdiction)` — Screen an address for sanctions and risk
- `submit_screening_result(screener, address, status, reason_code, risk_tier, expires_at, notes)` — Submit screening decision
- `get_screening_result(address)` — Retrieve latest screening result for an address
- `get_risk_tier(address)` — Get current risk tier
- `update_structuring_check(address, amount)` — Track structuring-check violations

## Insurance Contract

Provides default insurance for invoices, protecting investors against borrower default.

### Entrypoints

- `create_insurance_pool()` — Initialize insurance pool
- `deposit_premium(investor, invoice_id, amount)` — Deposit insurance premium
- `claim_payout(investor, invoice_id)` — Claim insurance payout on default
- `get_insurance_pool()` — Retrieve pool details

## Oracle Registry Contract

Manages oracle providers and price feeds for the platform.

### Entrypoints

- `register_oracle(oracle_address, asset_pair)` — Register an oracle provider
- `submit_price_feed(oracle, asset_pair, price, timestamp)` — Submit price data
- `get_price(asset_pair)` — Retrieve latest price for an asset pair
- `list_oracles()` — List all registered oracles

## Governance Contract

Enables governance proposals and voting on protocol changes.

### Entrypoints

- `submit_proposal(proposer, description, target_contract, action)` — Submit a proposal
- `vote_on_proposal(voter, proposal_id, vote)` — Vote on a proposal
- `execute_proposal(proposal_id)` — Execute an approved proposal
- `get_proposal(proposal_id)` — Retrieve proposal details

## Referral Contract

Tracks and rewards referrals for borrowers and investors joining the platform.

### Entrypoints

- `register_referral(referrer, referee)` — Register a referral relationship
- `claim_referral_reward(referrer)` — Claim accrued referral rewards
- `get_referral_stats(address)` — Get referral count and reward balance
- `get_referral_rate()` — Get current referral reward rate

## Share Contract

A flat SEP-41-style fungible token contract representing pool deposit shares. There is no `invoice_id` parameter — balances are tracked per address only.

### Entrypoints

- `initialize(admin, decimals, name, symbol)` — Initialize the token with metadata
- `mint(to, amount)` — Mint shares to an address (admin only)
- `burn(from, amount)` — Burn shares from an address (admin only)
- `transfer(from, to, amount)` — Transfer shares between addresses
- `transfer_from(spender, from, to, amount)` — Transfer using an approved allowance
- `approve(owner, spender, amount)` — Set a spending allowance
- `allowance(owner, spender)` — Read current allowance
- `balance(id)` — Get share balance of an address
- `total_supply()` — Get total outstanding share supply
- `decimals()` — Get token decimal places
- `name()` — Get token name
- `symbol()` — Get token symbol
