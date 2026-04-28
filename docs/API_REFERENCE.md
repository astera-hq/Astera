# Astera Contract API Reference

## Overview

Astera is a real-world assets (RWA) platform on Stellar that enables SMEs to tokenize unpaid invoices and tap into a community-funded liquidity pool. The system consists of two smart contracts: an **Invoice Contract** that manages the lifecycle of tokenized invoices, and a **Pool Contract** that handles investor deposits, co-funding, yield distribution, and repayment settlements. This API reference documents every public method in both contracts with complete parameter specifications, return types, error conditions, and usage examples.

---

## Table of Contents

### Invoice Contract
- [`initialize`](#initialize-invoice)
- [`create_invoice`](#create_invoice)
- [`set_daily_invoice_limit`](#set_daily_invoice_limit)
- [`get_daily_invoice_limit`](#get_daily_invoice_limit)
- [`mark_funded`](#mark_funded)
- [`mark_paid`](#mark_paid)
- [`mark_defaulted`](#mark_defaulted)
- [`get_invoice`](#get_invoice)
- [`get_metadata`](#get_metadata)
- [`get_invoice_count`](#get_invoice_count)
- [`resolve_dispute`](#resolve_dispute)
- [`set_dispute_window`](#set_dispute_window)
- [`set_pool`](#set_pool)

### Pool Contract
- [`initialize`](#initialize-pool)
- [`add_token`](#add_token)
- [`remove_token`](#remove_token)
- [`deposit`](#deposit)
- [`init_co_funding`](#init_co_funding)
- [`commit_to_invoice`](#commit_to_invoice)
- [`repay_invoice`](#repay_invoice)
- [`withdraw`](#withdraw)
- [`set_rate_bounds`](#set_rate_bounds)
- [`get_rate_bounds`](#get_rate_bounds)
- [`set_exchange_rate`](#set_exchange_rate)
- [`get_exchange_rate`](#get_exchange_rate)
- [`set_yield`](#set_yield)
- [`get_config`](#get_config)
- [`accepted_tokens`](#accepted_tokens)
- [`get_token_totals`](#get_token_totals)
- [`get_position`](#get_position)
- [`get_funded_invoice`](#get_funded_invoice)
- [`get_co_fund_share`](#get_co_fund_share)
- [`available_liquidity`](#available_liquidity)
- [`estimate_repayment`](#estimate_repayment)

### Supporting Sections
- [Data Types](#data-types)
- [Error Reference](#error-reference)

---

## Data Types

### Invoice Contract Types

#### `Invoice`
The core record storing invoice metadata and lifecycle state.

| Field | Type | Description |
|-------|------|-------------|
| `id` | `u64` | Unique invoice identifier, sequential starting from 1 |
| `owner` | `Address` | Invoice owner (typically SME/creator) |
| `debtor` | `String` | Counterparty name or label (e.g., "ACME Corp") |
| `amount` | `i128` | Face value in smallest units (7 decimals for USDC on Stellar; 1 USDC = 10,000,000) |
| `due_date` | `u64` | Unix timestamp (ledger seconds) when payment is due |
| `description` | `String` | Human-readable invoice description (e.g., "Invoice #001 - Goods delivery") |
| `status` | `InvoiceStatus` | Current state: `Pending`, `Funded`, `Paid`, or `Defaulted` |
| `created_at` | `u64` | Unix timestamp when invoice was created |
| `funded_at` | `u64` | Unix timestamp when invoice was funded (0 until funded) |
| `paid_at` | `u64` | Unix timestamp when invoice was marked paid (0 until paid) |
| `pool_contract` | `Address` | Address of the pool contract that funded this invoice |
| `disputed_at` | `u64` | Unix timestamp when invoice was marked as disputed (0 if not disputed) |

**Location in Source:** [contracts/invoice/src/lib.rs](../contracts/invoice/src/lib.rs)

#### `InvoiceStatus`
Enum representing the state of an invoice.

| Variant | Meaning |
|---------|---------|
| `Pending` | Invoice created but not yet funded |
| `Funded` | Invoice has been fully funded by the pool, funds disbursed to SME |
| `Paid` | Invoice repaid in full |
| `Disputed` | Invoice verification failed or was contested; awaiting resolution |
| `Cancelled` | Invoice cancelled following a dispute resolution in favor of the debtor |
| `Defaulted` | Invoice missed due date with no repayment |

**Location in Source:** [contracts/invoice/src/lib.rs](../contracts/invoice/src/lib.rs)

#### `InvoiceMetadata`
A wallet and explorer-friendly view of an invoice, derived from the stored `Invoice` record without duplicating storage. Aligns with SEP-0041 conventions for token metadata.

| Field | Type | Description |
|-------|------|-------------|
| `name` | `String` | Human-readable title, e.g. `Astera Invoice #1` |
| `description` | `String` | Invoice description from the underlying `Invoice` record |
| `image` | `String` | Placeholder asset URI for artwork (currently a static placeholder; future versions will support per-invoice art) |
| `amount` | `i128` | Face value in smallest units (same as underlying `Invoice.amount`) |
| `debtor` | `String` | Counterparty/debtor label (same as underlying `Invoice.debtor`) |
| `due_date` | `u64` | Unix timestamp when payment is due (same as underlying `Invoice.due_date`) |
| `status` | `InvoiceStatus` | Current invoice status (`Pending`, `Funded`, `Paid`, or `Defaulted`) |
| `symbol` | `String` | Short ticker in SEP-0041 format, e.g. `INV-1`, `INV-2`, etc. |
| `decimals` | `u32` | Always `7`, indicating amounts are in units of 10^-7 (aligns with USDC on Stellar) |

**Location in Source:** [contracts/invoice/src/lib.rs](../contracts/invoice/src/lib.rs)

**SEP Alignment Notes:**
- `name`, `symbol`, and `decimals` correspond to SEP-0041 token metadata fields
- `image` follows standard JSON metadata conventions (e.g., NFT-style metadata)
- UIsto serialize `InvoiceMetadata` to JSON for wallet display

---

### Pool Contract Types

#### `PoolConfig`
Configuration and aggregated metrics for the pool.

| Field | Type | Description |
|-------|------|-------------|
| `invoice_contract` | `Address` | Address of the authorized Invoice Contract |
| `admin` | `Address` | Current admin address; required for privileged operations |
| `yield_bps` | `u32` | Annual yield in basis points (e.g., 800 = 8% APY); cannot exceed 5000 (50%) |

**Location in Source:** [contracts/pool/src/lib.rs](../contracts/pool/src/lib.rs)

**Notes:** Default `yield_bps` is 800 (8% APY). The pool does not store total_deposited, total_deployed, total_paid_out in PoolConfig; those are tracked per token in `PoolTokenTotals`.

#### `PoolTokenTotals`
Aggregated accounting metrics for a specific stablecoin within the pool.

| Field | Type | Description |
|-------|------|-------------|
| `total_deposited` | `i128` | Cumulative amount deposited (including earned interest credited back) |
| `total_deployed` | `i128` | Amount currently locked in active invoices |
| `total_paid_out` | `i128` | Cumulative total paid out (principal + interest) |

**Location in Source:** [contracts/pool/src/lib.rs](../contracts/pool/src/lib.rs)

**Notes:** `available_liquidity` for a token = `total_deposited - total_deployed`. Interest earnings are added to `total_deposited` when invoices are repaid.

#### `InvestorPosition`
Per-investor, per-token accounting of deposits, deployments, earnings, and deposit history.

| Field | Type | Description |
|-------|------|-------------|
| `deposited` | `i128` | Net amount ever deposited by this investor in this token (cumulative, net of withdrawals) |
| `available` | `i128` | Undeployed balance available for withdrawal or future commitments |
| `deployed` | `i128` | Amount currently locked in active invoices (unavailable for withdrawal until invoices are repaid) |
| `earned` | `i128` | Total interest accumulated from repaid invoices in this token |
| `deposit_count` | `u32` | Count of successful deposit operations (for UX/analytics) |

**Location in Source:** [contracts/pool/src/lib.rs](../contracts/pool/src/lib.rs)

**Notes:** Invariant: `available + deployed = deposited + earned` (at any state). Available balance can be withdrawn at any time.

#### `FundedInvoice`
Record of an invoice opened for co-funding. Tracks funding progress and repayment state.

| Field | Type | Description |
|-------|------|-------------|
| `invoice_id` | `u64` | ID of the invoice from the Invoice Contract |
| `sme` | `Address` | Address of the SME/recipient receiving the funds |
| `token` | `Address` | Stablecoin token contract address used for this invoice |
| `principal` | `i128` | Total funding target (required amount) |
| `committed` | `i128` | Amount committed by all co-funders so far; equals `principal` when fully funded |
| `funded_at` | `u64` | Unix timestamp when fully funded (0 while still open for commitments) |
| `due_date` | `u64` | Invoice due date from the Invoice Contract |
| `repaid` | `bool` | Whether invoice has been repaid in full |

**Location in Source:** [contracts/pool/src/lib.rs](../contracts/pool/src/lib.rs)

**Notes:** When `committed == principal`, funds are automatically transferred to SME and `funded_at` is set to current ledger timestamp.

#### `CoFundKey`
Composite key uniquely identifying an investor's share in a specific invoice.

| Field | Type | Description |
|-------|------|-------------|
| `invoice_id` | `u64` | Invoice ID |
| `investor` | `Address` | Investor address |

**Location in Source:** [contracts/pool/src/lib.rs](../contracts/pool/src/lib.rs)

**Notes:** Used internally for storage lookups; the value stored under this key is an `i128` representing the investor's committed share.

---

## Invoice Contract Methods

### `initialize` (Invoice)

Initializes the invoice contract with an admin address and authorized pool contract address. Can only be called once.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `admin` | `Address` | Yes | Address that will be authorized to call `set_pool` and perform other privileged operations |
| `pool` | `Address` | Yes | Address of the pool contract authorized to call `mark_funded`, `mark_paid`, and `mark_defaulted` |

#### Returns

`()` — No return value.

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"already initialized"` | Contract has been initialized before | `initialize` |

#### Access Control

None — The first caller wins. After initialization, the contract is locked. Use `set_pool` to update the pool contract address later (admin-only).

#### Example Invocation

From test [contracts/invoice/src/lib.rs#L287](../contracts/invoice/src/lib.rs#L287):

```rust
let admin = Address::generate(env);
let pool = Address::generate(env);
client.initialize(&admin, &pool);
```

CLI invocation:
```bash
stellar contract invoke \
  --id <INVOICE_CONTRACT_ID> \
  --source invoker \
  -- initialize \
  --admin <ADMIN_ADDRESS> \
  --pool <POOL_CONTRACT_ID>
```

#### State Changes

- Sets `DataKey::Admin` to the provided `admin` address
- Sets `DataKey::Pool` to the provided `pool` address
- Initializes `DataKey::InvoiceCount` to `0`
- Sets `DataKey::Initialized` flag to `true`

---

### `create_invoice`

SME creates a new tokenized invoice on-chain. The owner must sign the transaction. Validates that amount is positive and due date is in the future.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `owner` | `Address` | Yes | Address of the invoice creator (typically the SME); must sign the transaction |
| `debtor` | `String` | Yes | Name or label of the counterparty/debtor (e.g., "ACME Corp") |
| `amount` | `i128` | Yes | Face value in smallest units (7 decimals for USDC); must be > 0 |
| `due_date` | `u64` | Yes | Unix timestamp (ledger seconds) when payment is due; must be > current ledger timestamp |
| `description` | `String` | Yes | Human-readable invoice description (e.g., "Invoice #001 - Goods delivery") |

#### Returns

`u64` — The new invoice ID (sequential, starting from 1).

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"amount must be positive"` | `amount <= 0` | `create_invoice` |
| `"due date must be in the future"` | `due_date <= env.ledger().timestamp()` | `create_invoice` |
| `"daily invoice limit exceeded"` | Per-address daily creation counter has reached the configured limit | `create_invoice` |

#### Access Control

Requires `owner.require_auth()` — the caller must provide a cryptographic signature from the `owner` address.

#### Example Invocation

From test [contracts/invoice/src/lib.rs#L309](../contracts/invoice/src/lib.rs#L309):

```rust
let id = client.create_invoice(
    &sme,
    &String::from_str(&env, "ACME Corp"),
    &1_000_000_000i128,                      // 1000 USDC (7 decimals)
    &(env.ledger().timestamp() + 2_592_000), // 30 days
    &String::from_str(&env, "Invoice #001 - Goods delivery"),
);
// Returns invoice ID (1 for first invoice)
```

CLI invocation:
```bash
stellar contract invoke \
  --id <INVOICE_CONTRACT_ID> \
  --source owner_secret \
  -- create_invoice \
  --owner <OWNER_ADDRESS> \
  --debtor "ACME Corp" \
  --amount 1000000000 \
  --due_date 1735689600 \
  --description "Invoice #001 - Goods delivery"
```

#### State Changes

- Increments `DataKey::InvoiceCount`
- Creates and stores a new `Invoice` record with `status = InvoiceStatus::Pending`
- Sets `created_at` to current ledger timestamp
- Publishes an `INVOICE.created` event with `(id, owner, amount)`

#### Security Notes

**Trust Model:** Any address can create an invoice. The `owner` field identifies who can claim ownership.

**Invariants:**
- `amount > 0` — enforced
- `due_date > current_ledger_timestamp` — enforced
- New invoices always start in `Pending` state

**Validation:**
- Amount must be positive (rejects zero and negative)
- Due date must be in the future (rejects past or current timestamps)
- The per-address creation count must remain below the configured daily limit

---

### `set_daily_invoice_limit`

Admin updates the per-address invoice creation cap used by `create_invoice`. If no value is set, the historical default of 10 invoices per day is used.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `admin` | `Address` | Yes | Admin address; must sign the transaction |
| `limit` | `u32` | Yes | New daily limit; must be between 1 and 1000 |

#### Returns

`()` — No return value.

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"unauthorized"` | Caller is not the stored admin | `set_daily_invoice_limit` |
| `"daily invoice limit must be positive"` | `limit == 0` | `set_daily_invoice_limit` |
| `"daily invoice limit too high"` | `limit > 1000` | `set_daily_invoice_limit` |

#### Access Control

Requires `admin.require_auth()` and stored-admin verification.

---

### `get_daily_invoice_limit`

Returns the active daily invoice creation cap. If no override has been stored, this returns the default value of `10`.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |

#### Returns

`u32` — The configured daily limit, or `10` when unset.

---

### `mark_funded`

Called by the authorized pool contract when an invoice is fully funded and funds are disbursed to the SME. Transitions invoice status from `Pending` to `Funded`. Only the authorized pool address may call this method.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `id` | `u64` | Yes | Invoice ID (must exist and be in `Pending` state) |
| `pool` | `Address` | Yes | Pool contract address; must match the authorized pool set during initialization or via `set_pool` |

#### Returns

`()` — No return value.

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"not initialized"` | Contract not initialized | `mark_funded` |
| `"unauthorized pool"` | `pool` does not match the authorized pool address | `mark_funded` |
| `"invoice not found"` | Invoice ID does not exist | `mark_funded` |
| `"invoice is not pending"` | Invoice is not in `Pending` state | `mark_funded` |

#### Access Control

Requires `pool.require_auth()` — the authorized pool contract must provide a cryptographic signature.

#### Example Invocation

From test [contracts/invoice/src/lib.rs#L329](../contracts/invoice/src/lib.rs#L329):

```rust
client.mark_funded(&id, &pool);
// Status transitions to Funded, funded_at is set
```

CLI invocation:
```bash
stellar contract invoke \
  --id <INVOICE_CONTRACT_ID> \
  --source pool_secret \
  -- mark_funded \
  --id 1 \
  --pool <POOL_CONTRACT_ID>
```

#### State Changes

- Updates invoice status to `InvoiceStatus::Funded`
- Sets `funded_at` to current ledger timestamp
- Updates `pool_contract` field to the pool address
- Publishes an `INVOICE.funded` event with `(id)`

#### Security Notes

**Trust Model:** Only the registered pool contract can mark invoices as funded. This prevents unauthorized state transitions.

**Invariants:**
- Invoice must exist
- Invoice must be in `Pending` state (cannot re-fund an already funded or paid invoice)
- Caller must be the authorized pool address

---

### `verify_invoice`

Called by the configured oracle to approve or dispute an invoice. Verifies the supplied oracle hash against the stored `verification_hash` before transitioning status.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `id` | `u64` | Yes | Invoice ID to verify |
| `oracle` | `Address` | Yes | Stored oracle address |
| `approved` | `bool` | Yes | Oracle approval flag |
| `reason` | `String` | Yes | Dispute reason when `approved` is false |
| `oracle_hash` | `String` | Yes | Oracle-supplied invoice verification hash |

#### Returns

`()` — No return value.

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"unauthorized oracle"` | Caller is not the configured oracle | `verify_invoice` |
| `"invoice not found"` | Invoice ID does not exist | `verify_invoice` |
| `"invoice is not awaiting verification"` | Invoice is not awaiting verification | `verify_invoice` |
| `"hash mismatch"` | Oracle hash does not match stored verification hash | `verify_invoice` |

#### Access Control

Requires `oracle.require_auth()` and the oracle must match the stored oracle address.

#### Example Invocation

From test [contracts/invoice/src/lib.rs#L1801](../contracts/invoice/src/lib.rs#L1801):

```rust
client.verify_invoice(
    &id,
    &oracle,
    &true,
    &String::from_str(&env, ""),
    &hash,
)
.unwrap();
```

#### State Changes

- Validates `oracle_hash` against the stored invoice `verification_hash`
- Updates invoice status to `InvoiceStatus::Verified` when approved
- Updates invoice status to `InvoiceStatus::Disputed` and stores `dispute_reason` when rejected
- Publishes an `INVOICE.verified` or `INVOICE.disputed` event

---

### `mark_paid`

Marks an invoice as repaid in full. Can be called by the invoice owner, the pool contract, or the admin. Transitions invoice status from `Funded` to `Paid`.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `id` | `u64` | Yes | Invoice ID (must exist and be in `Funded` state) |
| `caller` | `Address` | Yes | Address marking the invoice as paid; must be owner, pool, or admin |

#### Returns

`()` — No return value.

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"not initialized"` | Contract not initialized | `mark_paid` |
| `"invoice not found"` | Invoice ID does not exist | `mark_paid` |
| `"unauthorized"` | Caller is not owner, pool, or admin | `mark_paid` |
| `"invoice is not funded"` | Invoice is not in `Funded` state | `mark_paid` |

#### Access Control

Requires `caller.require_auth()` — the caller must be the invoice owner, the authorized pool contract, or the admin address.

#### Example Invocation

From test [contracts/invoice/src/lib.rs#L331](../contracts/invoice/src/lib.rs#L331):

```rust
client.mark_paid(&id, &sme);  // sme can mark paid
// or
client.mark_paid(&id, &pool); // pool can mark paid
// or
client.mark_paid(&id, &admin); // admin can mark paid
```

CLI invocation:
```bash
stellar contract invoke \
  --id <INVOICE_CONTRACT_ID> \
  --source caller_secret \
  -- mark_paid \
  --id 1 \
  --caller <CALLER_ADDRESS>
```

#### State Changes

- Updates invoice status to `InvoiceStatus::Paid`
- Sets `paid_at` to current ledger timestamp
- Publishes an `INVOICE.paid` event with `(id)`

#### Security Notes

**Trust Model:** Three parties can mark an invoice as paid: the SME/owner, the pool, or the admin. This provides flexibility for different settlement workflows.

**Invariants:**
- Invoice must exist
- Invoice must be in `Funded` state
- Caller must be owner, pool, or admin

**Validation:**
- Enforces authorization check before allowing state transition

---

### `mark_defaulted`

Marks an invoice as defaulted (missed due date with no repayment). Called exclusively by the pool contract. Transitions invoice status from `Funded` to `Defaulted`.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `id` | `u64` | Yes | Invoice ID (must exist and be in `Funded` state) |
| `pool` | `Address` | Yes | Pool contract address; must match the authorized pool |

#### Returns

`()` — No return value.

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"not initialized"` | Contract not initialized | `mark_defaulted` |
| `"unauthorized pool"` | `pool` does not match authorized pool | `mark_defaulted` |
| `"invoice not found"` | Invoice ID does not exist | `mark_defaulted` |
| `"invoice is not funded"` | Invoice is not in `Funded` state | `mark_defaulted` |

#### Access Control

Requires `pool.require_auth()` — only the authorized pool contract can mark invoices as defaulted.

#### Example Invocation

From test [contracts/invoice/src/lib.rs#L355](../contracts/invoice/src/lib.rs#L355):

```rust
client.mark_defaulted(&id, &pool);
// Status transitions to Defaulted
```

CLI invocation:
```bash
stellar contract invoke \
  --id <INVOICE_CONTRACT_ID> \
  --source pool_secret \
  -- mark_defaulted \
  --id 1 \
  --pool <POOL_CONTRACT_ID>
```

#### State Changes

- Updates invoice status to `InvoiceStatus::Defaulted`
- Publishes an `INVOICE.default` event with `(id)`

#### Security Notes

**Trust Model:** Only the pool contract can mark invoices as defaulted. This ensures consistent lifecycle management.

**Invariants:**
- Invoice must exist
- Invoice must be in `Funded` state (defaulted invoices do not transition further)
- Caller must be the authorized pool address

---

### `cancel_invoice`

Cancels a non-funded invoice. Can be called by the invoice owner when the invoice is still unfunded, or by the admin for any non-funded invoice state.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `id` | `u64` | Yes | Invoice ID to cancel |
| `caller` | `Address` | Yes | Invoice owner or admin address |

#### Returns

`()` — No return value.

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"unauthorized"` | Caller is not invoice owner or admin | `cancel_invoice` |
| `"invalid status transition"` | Invoice is funded, paid, or already completed | `cancel_invoice` |
| `"invoice not found"` | Invoice ID does not exist | `cancel_invoice` |

#### Access Control

Requires `caller.require_auth()` and either invoice ownership or admin privileges.

#### Example Invocation

From test [contracts/invoice/src/lib.rs#L1918](../contracts/invoice/src/lib.rs#L1918):

```rust
client.cancel_invoice(&id, &sme);
```

#### State Changes

- Updates invoice status to `InvoiceStatus::Cancelled`
- Sets a shorter completed TTL
- Publishes an `INVOICE.cancelled` event

---

### `get_invoice`

Retrieves the full invoice record by ID. Read-only query.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `id` | `u64` | Yes | Invoice ID to retrieve |

#### Returns

`Invoice` — The complete invoice record (see [Data Types](#invoice) for field descriptions).

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"invoice not found"` | Invoice ID does not exist | `get_invoice` |

#### Access Control

None — this is a read-only query available to any caller.

#### Example Invocation

From test [contracts/invoice/src/lib.rs#L325](../contracts/invoice/src/lib.rs#L325):

```rust
let invoice = client.get_invoice(&id);
assert!(matches!(invoice.status, InvoiceStatus::Pending));
```

CLI invocation:
```bash
stellar contract invoke \
  --id <INVOICE_CONTRACT_ID> \
  -- get_invoice \
  --id 1
```

#### State Changes

None — read-only operation.

---

### `get_metadata`

Retrieves a wallet and explorer-friendly metadata view of an invoice, derived from the stored `Invoice` record without duplicating storage. Returns an `InvoiceMetadata` struct with SEP-0041–aligned fields suitable for JSON serialization and display in wallets.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `id` | `u64` | Yes | Invoice ID to retrieve metadata for |

#### Returns

`InvoiceMetadata` — computed metadata structure (see [Data Types](#invoicemetadata) for field descriptions).

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"invoice not found"` | Invoice ID does not exist | `get_metadata` |

#### Access Control

None — this is a read-only query available to any caller.

#### Example Invocation

From test [contracts/invoice/src/lib.rs#L320](../contracts/invoice/src/lib.rs#L320):

```rust
let meta = client.get_metadata(&id);
assert_eq!(meta.status, InvoiceStatus::Pending);
assert_eq!(meta.amount, 1_000_000_000i128);
assert_eq!(meta.decimals, 7u32);
assert_eq!(meta.symbol, String::from_str(&env, "INV-1"));
assert_eq!(meta.name, String::from_str(&env, "Astera Invoice #1"));
```

CLI invocation:
```bash
stellar contract invoke \
  --id <INVOICE_CONTRACT_ID> \
  -- get_metadata \
  --id 1
```

#### State Changes

None — read-only operation. Fields are computed from the stored `Invoice` record on each call.

**Computed Fields:**
- `name` — constructed as `"Astera Invoice #{id}"` (e.g., `"Astera Invoice #1"`)
- `symbol` — constructed as `"INV-{id}"` (e.g., `"INV-1"`)
- `image` — static placeholder: `"https://astera.io/metadata/invoice/placeholder.svg"`
- `decimals` — always `7`

---

### `get_invoice_count`

Retrieves the total number of invoices created so far. Returns the current invoice count (used to generate the next invoice ID).

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |

#### Returns

`u64` — Current invoice count (1 means 1 invoice exists with ID 1, 0 means no invoices created yet).

#### Errors

None — always succeeds.

#### Access Control

None — this is a read-only query.

#### Example Invocation

From test [contracts/invoice/src/lib.rs#L376](../contracts/invoice/src/lib.rs#L376):

```rust
assert_eq!(client.get_invoice_count(), 3);  // 3 invoices created
```

CLI invocation:
```bash
stellar contract invoke \
  --id <INVOICE_CONTRACT_ID> \
  -- get_invoice_count
```

#### State Changes

None — read-only operation.
    
---

### `resolve_dispute`

Resolves a disputed invoice. Can be called by the authorized oracle or the admin. If called by the admin, a resolution window must have passed since the dispute began.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `id` | `u64` | Yes | Invoice ID to resolve |
| `caller` | `Address` | Yes | Address of the resolver (must be Admin or Oracle) |
| `resolution` | `DisputeResolution` | Yes | Outcome: `InFavorOfSME` or `InFavorOfDebtor` |

#### Returns

`()` — No return value.

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"invoice not found"` | Invoice ID does not exist | `resolve_dispute` |
| `"invoice is not disputed"` | Invoice is not in `Disputed` state | `resolve_dispute` |
| `"unauthorized"` | Caller is not Admin or Oracle | `resolve_dispute` |
| `"dispute resolution window not yet passed for admin"` | Admin attempted to resolve before the configured window passed | `resolve_dispute` |

#### Access Control

Requires `caller.require_auth()`. Oracle can resolve at any time. Admin can resolve only after the `DisputeResolutionWindow` has elapsed since `disputed_at`.

#### Example Invocation

```rust
client.resolve_dispute(&id, &oracle, &DisputeResolution::InFavorOfSME);
```

#### State Changes

- **InFavorOfSME**: Sets status to `Verified`, clears `dispute_reason`, sets `oracle_verified = true`.
- **InFavorOfDebtor**: Sets status to `Cancelled`, decrements active invoices count, removes from persistent storage (via cleanup if applicable).
- Publishes an `INVOICE.resolved` event with `(id, resolution, caller)`.

---

### `set_dispute_window`

Updates the time window that must elapse before the admin can force-resolve a dispute. Admin-only.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `admin` | `Address` | Yes | Admin address; must sign |
| `window` | `u64` | Yes | Window duration in seconds (default: 30 days) |

---

---

### `set_pool`

Updates the authorized pool contract address. Admin-only. Allows the admin to point the invoice contract to a new or replacement pool contract.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `admin` | `Address` | Yes | Current admin address; must sign and match the stored admin |
| `pool` | `Address` | Yes | New pool contract address to authorize |

#### Returns

`()` — No return value.

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"not initialized"` | Contract not initialized | `set_pool` |
| `"unauthorized"` | `admin` does not match the stored admin address | `set_pool` |

#### Access Control

Requires `admin.require_auth()` — the admin address must provide a cryptographic signature and match the stored admin.

#### Example Invocation

From test [contracts/invoice/src/lib.rs#L397](../contracts/invoice/src/lib.rs#L397):

```rust
let new_pool = Address::generate(&env);
client.set_pool(&admin, &new_pool);
```

CLI invocation:
```bash
stellar contract invoke \
  --id <INVOICE_CONTRACT_ID> \
  --source admin_secret \
  -- set_pool \
  --admin <ADMIN_ADDRESS> \
  --pool <NEW_POOL_ADDRESS>
```

#### State Changes

- Updates `DataKey::Pool` to the new pool address

#### Security Notes

**Trust Model:** Only the stored admin address can change the pool contract. This protects against unauthorized pool changes.

**Invariants:**
- Caller must be the stored admin

---

## Pool Contract Methods

### `initialize` (Pool)

Initializes the pool contract with an admin address, an initial stablecoin token, and the invoice contract address. Can only be called once. Sets up the default yield (800 basis points = 8% APY) and creates the initial token whitelist.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `admin` | `Address` | Yes | Pool administrator address (required for `add_token`, `remove_token`, `init_co_funding`, `set_yield`) |
| `initial_token` | `Address` | Yes | Address of the first accepted stablecoin (typically USDC); can add more later via `add_token` |
| `invoice_contract` | `Address` | Yes | Address of the authorized Invoice Contract |

#### Returns

`()` — No return value.

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"already initialized"` | Contract has been initialized before | `initialize` |

#### Access Control

None — the first caller wins. After initialization, the contract state is locked. Use privileged methods like `add_token` and `set_yield` to modify pool parameters.

#### Example Invocation

From test [contracts/pool/src/lib.rs#L855](../contracts/pool/src/lib.rs#L855):

```rust
let admin = Address::generate(env);
let usdc_id = env.register_stellar_asset_contract_v2(token_admin).address();
let invoice_contract = Address::generate(env);
client.initialize(&admin, &usdc_id, &invoice_contract);
```

CLI invocation:
```bash
stellar contract invoke \
  --id <POOL_CONTRACT_ID> \
  --source initializer \
  -- initialize \
  --admin <ADMIN_ADDRESS> \
  --initial_token <USDC_TOKEN_ADDRESS> \
  --invoice_contract <INVOICE_CONTRACT_ADDRESS>
```

#### State Changes

- Sets `DataKey::Config` with admin, invoice_contract, and default yield_bps (800)
- Initializes `DataKey::AcceptedTokens` with a vector containing `[initial_token]`
- Creates `DataKey::TokenTotals(initial_token)` with all zeros
- Sets `DataKey::Initialized` flag to `true`

---

### `add_token`

Admin adds a stablecoin to the whitelist of accepted tokens. Investors can only deposit tokens on the whitelist. Fails if the token is already whitelisted.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `admin` | `Address` | Yes | Admin address; must sign and match the stored admin |
| `token` | `Address` | Yes | Token contract address to add to the whitelist |

#### Returns

`()` — No return value.

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"not initialized"` | Contract not initialized | `add_token` |
| `"unauthorized"` | `admin` does not match stored admin | `add_token` |
| `"token already accepted"` | Token is already in the whitelist | `add_token` |

#### Access Control

Requires `admin.require_auth()` and `require_admin` check — only the stored admin can add tokens.

#### Example Invocation

From test [contracts/pool/src/lib.rs#L906](../contracts/pool/src/lib.rs#L906):

```rust
let eur_admin = Address::generate(&env);
let eurc_id = env.register_stellar_asset_contract_v2(eur_admin).address();
client.add_token(&admin, &eurc_id);
```

CLI invocation:
```bash
stellar contract invoke \
  --id <POOL_CONTRACT_ID> \
  --source admin_secret \
  -- add_token \
  --admin <ADMIN_ADDRESS> \
  --token <NEW_TOKEN_ADDRESS>
```

#### State Changes

- Appends the token to `DataKey::AcceptedTokens` vector
- If `DataKey::TokenTotals(token)` doesn't exist, creates it with zeros

---

### `remove_token`

Admin removes a stablecoin from the whitelist. Only succeeds if the token has zero balances across all pools (total_deposited, total_deployed, and total_paid_out must all be 0). Prevents accidental removal of actively used tokens.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `admin` | `Address` | Yes | Admin address; must sign and match the stored admin |
| `token` | `Address` | Yes | Token contract address to remove from the whitelist |

#### Returns

`()` — No return value.

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"not initialized"` | Contract not initialized | `remove_token` |
| `"unauthorized"` | `admin` does not match stored admin | `remove_token` |
| `"token not in whitelist"` | Token is not in the accepted list | `remove_token` |
| `"token has non-zero pool balances"` | Token has nonzero total_deposited, total_deployed, or total_paid_out | `remove_token` |

#### Access Control

Requires `admin.require_auth()` and `require_admin` check — only the stored admin can remove tokens.

#### Example Invocation

From test [contracts/pool/src/lib.rs#L919](../contracts/pool/src/lib.rs#L919):

```rust
client.remove_token(&admin, &eurc_id);
```

CLI invocation:
```bash
stellar contract invoke \
  --id <POOL_CONTRACT_ID> \
  --source admin_secret \
  -- remove_token \
  --admin <ADMIN_ADDRESS> \
  --token <TOKEN_ADDRESS>
```

#### State Changes

- Removes the token from `DataKey::AcceptedTokens` vector

#### Security Notes

**Invariants:**
- Token balances must be zero before removal prevents data loss and ensures clean state

---

### `deposit`

Investor deposits an accepted stablecoin into the pool. Funds are transferred from the investor's wallet to the pool contract. Creates or updates the investor's position for that token.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `investor` | `Address` | Yes | Investor address; must sign the transaction |
| `token` | `Address` | Yes | Token address (must be on the whitelist) |
| `amount` | `i128` | Yes | Amount to deposit in smallest units; must be > 0 |

#### Returns

`()` — No return value.

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"not initialized"` | Contract not initialized | `deposit` |
| `"amount must be positive"` | `amount <= 0` | `deposit` |
| `"token not accepted"` | Token is not on the whitelist | `deposit` |

#### Access Control

Requires `investor.require_auth()` — the investor must provide a cryptographic signature. Token transfer is initiated by the pool contract and requires the investor's approval/signature for the transfer operation.

#### Example Invocation

From test [contracts/pool/src/lib.rs#L869](../contracts/pool/src/lib.rs#L869):

```rust
let investor = Address::generate(&env);
mint(&env, &usdc_id, &investor, 5_000_000_000);
client.deposit(&investor, &usdc_id, &2_000_000_000);

let pos = client.get_position(&investor, &usdc_id).unwrap();
assert_eq!(pos.deposited, 2_000_000_000);
assert_eq!(pos.available, 2_000_000_000);
```

CLI invocation:
```bash
stellar contract invoke \
  --id <POOL_CONTRACT_ID> \
  --source investor_secret \
  -- deposit \
  --investor <INVESTOR_ADDRESS> \
  --token <TOKEN_ADDRESS> \
  --amount 2000000000
```

#### State Changes

- Creates or updates investor's `InvestorPosition` for the token
- Increments `position.deposited` by amount
- Increments `position.available` by amount
- Increments `position.deposit_count` by 1
- Updates `TokenTotals.total_deposited` by amount
- Publishes a `POOL.deposit` event with `(investor, amount)`

#### Security Notes

**Trust Model:** Any address can deposit; positions are tracked per address and token.

**Invariants:**
- Amount must be positive
- Token must be whitelisted
- Investor's available balance increases by deposit amount

---

### `init_co_funding`

Admin registers an invoice for co-funding in a specific stablecoin. Establishes the principal target and sets the SME recipient. Investors then call `commit_to_invoice` to fill their shares.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `admin` | `Address` | Yes | Admin address; must sign and match the stored admin |
| `invoice_id` | `u64` | Yes | Invoice ID from the Invoice Contract (must be unique across pool calls) |
| `principal` | `i128` | Yes | Total funding target; must be > 0 |
| `sme` | `Address` | Yes | Recipient SME address (will receive funds when fully funded) |
| `due_date` | `u64` | Yes | Invoice due date (informational, corresponds to Invoice Contract record) |
| `token` | `Address` | Yes | Token for this invoice (must be on the whitelist) |

#### Returns

`()` — No return value.

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"not initialized"` | Contract not initialized | `init_co_funding` |
| `"unauthorized"` | `admin` does not match stored admin | `init_co_funding` |
| `"token not accepted"` | Token is not on the whitelist | `init_co_funding` |
| `"principal must be positive"` | `principal <= 0` | `init_co_funding` |
| `"invoice already registered for funding"` | Invoice ID was already opened for co-funding | `init_co_funding` |

#### Access Control

Requires `admin.require_auth()` and `require_admin` check — only the admin can initiate co-funding.

#### Example Invocation

From test [contracts/pool/src/lib.rs#L901](../contracts/pool/src/lib.rs#L901):

```rust
let invoice_id: u64 = 1;
let principal: i128 = 3_000_000_000;
let due_date = env.ledger().timestamp() + 2_592_000;

client.init_co_funding(&admin, &invoice_id, &principal, &sme, &due_date, &usdc_id);
```

CLI invocation:
```bash
stellar contract invoke \
  --id <POOL_CONTRACT_ID> \
  --source admin_secret \
  -- init_co_funding \
  --admin <ADMIN_ADDRESS> \
  --invoice_id 1 \
  --principal 3000000000 \
  --sme <SME_ADDRESS> \
  --due_date 1735689600 \
  --token <TOKEN_ADDRESS>
```

#### State Changes

- Creates `FundedInvoice` record with `committed = 0` and `funded_at = 0`
- Stores it under `DataKey::FundedInvoice(invoice_id)`
- Initializes empty `CoFunders` vector under `DataKey::CoFunders(invoice_id)`

---

### `commit_to_invoice`

Investor commits available balance toward an invoice. Moves funds from available to deployed in the investor's position. When the total committed reaches the principal target, USDC is automatically transferred to the SME and the invoice is marked as fully funded.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `investor` | `Address` | Yes | Investor address; must sign the transaction |
| `invoice_id` | `u64` | Yes | Invoice ID to commit to (must exist and be registered for co-funding) |
| `amount` | `i128` | Yes | Commitment amount; must be > 0 and <= remaining gap |

#### Returns

`()` — No return value.

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"amount must be positive"` | `amount <= 0` | `commit_to_invoice` |
| `"invoice not registered for co-funding"` | Invoice ID not found | `commit_to_invoice` |
| `"invoice already fully funded"` | `funded_at != 0` | `commit_to_invoice` |
| `"invoice already repaid"` | `repaid == true` | `commit_to_invoice` |
| `"amount exceeds remaining funding gap"` | `amount > (principal - committed)` | `commit_to_invoice` |
| `"investor has no position in this invoice token"` | Investor never deposited in this token | `commit_to_invoice` |
| `"insufficient available balance"` | `position.available < amount` | `commit_to_invoice` |

#### Access Control

Requires `investor.require_auth()` — the investor must provide a cryptographic signature.

#### Example Invocation

From test [contracts/pool/src/lib.rs#L900](../contracts/pool/src/lib.rs#L900):

```rust
client.commit_to_invoice(&investor1, &invoice_id, &2_000_000_000);
let record = client.get_funded_invoice(&invoice_id).unwrap();
assert_eq!(record.committed, 2_000_000_000);
assert_eq!(record.funded_at, 0);  // Not yet fully funded

client.commit_to_invoice(&investor2, &invoice_id, &1_000_000_000);
let record = client.get_funded_invoice(&invoice_id).unwrap();
assert_eq!(record.committed, principal);
assert!(record.funded_at != 0);  // Now fully funded
```

CLI invocation:
```bash
stellar contract invoke \
  --id <POOL_CONTRACT_ID> \
  --source investor_secret \
  -- commit_to_invoice \
  --investor <INVESTOR_ADDRESS> \
  --invoice_id 1 \
  --amount 2000000000
```

#### State Changes

- Updates investor's `InvestorPosition` for the invoice's token:
  - Decrements `available` by amount
  - Increments `deployed` by amount
- Updates `FundedInvoice.committed` by amount
- If this is the investor's first commitment to this invoice, adds investor to `CoFunders` list
- Stores investor's commitment under `CoFundShare(invoice_id, investor)` (cumulative; can commit multiple times)
- Updates `TokenTotals.total_deployed` by amount
- **If `committed == principal`:**
  - Transfers `principal` amount of stablecoin from pool contract to SME
  - Sets `FundedInvoice.funded_at` to current ledger timestamp
  - Publishes `POOL.funded` event with `(invoice_id, sme, principal)`

#### Security Notes

**Trust Model:** Investors can commit available balance; no approval from other parties required as long as available balance is sufficient.

**Invariants:**
- Investor must have deposited in the correct token first
- Available balance must be sufficient
- Cannot over-commit beyond the remaining funding gap
- Funds are automatically transferred to SME when fully funded (atomic operation)

---

### `repay_invoice`

SME repays the invoice. Principal and pro-rata yield are credited back to each co-funder's available balance. Interest is calculated based on time elapsed since funding. Only allowed after invoice is fully funded and before being marked as repaid.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `invoice_id` | `u64` | Yes | Invoice ID to repay (must be funded and not already repaid) |
| `payer` | `Address` | Yes | Payer address (typically the SME); must sign the transaction |

#### Returns

`()` — No return value.

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"not initialized"` | Contract not initialized | `repay_invoice` |
| `"invoice not found"` | Invoice ID doesn't exist in pool records | `repay_invoice` |
| `"invoice not fully funded yet"` | `funded_at == 0` | `repay_invoice` |
| `"already repaid"` | `repaid == true` | `repay_invoice` |

#### Access Control

Requires `payer.require_auth()` — the payer must provide a cryptographic signature. Token transfer requires approval/signature from the payer.

#### Example Invocation

From test [contracts/pool/src/lib.rs#L1054](../contracts/pool/src/lib.rs#L1054):

```rust
env.ledger().with_mut(|l| l.timestamp += 2_592_000);  // 30 days pass

client.repay_invoice(&invoice_id, &sme);

let pos = client.get_position(&investor, &usdc_id).unwrap();
assert_eq!(pos.deployed, 0);
assert!(pos.available >= principal);  // Principal + interest
assert!(pos.earned > 0);
```

CLI invocation:
```bash
stellar contract invoke \
  --id <POOL_CONTRACT_ID> \
  --source payer_secret \
  -- repay_invoice \
  --invoice_id 1 \
  --payer <PAYER_ADDRESS>
```

#### State Changes

- Transfers total repayment (`principal + interest`) from payer to pool contract
- For each co-funder in the invoice:
  - Calculates their pro-rata share of interest
  - Updates their `InvestorPosition`:
    - Increments `available` by `(share + interest_share)`
    - Decrements `deployed` by `share`
    - Increments `earned` by `interest_share`
- Sets `FundedInvoice.repaid` to `true`
- Updates `TokenTotals`:
  - Decrements `total_deployed` by principal
  - Increments `total_paid_out` by total_due (principal + interest)
  - Increments `total_deposited` by interest (returned earnings)
- Publishes `POOL.repaid` event with `(invoice_id, principal, interest)`

**Interest Calculation:**
```
total_interest = (principal × yield_bps × elapsed_seconds) / (10000 × 31536000)
investor_interest = (total_interest × investor_share) / principal
```

#### Security Notes

**Trust Model:** SME or any authorized payer can repay. Repayment updates all investor positions atomically.

**Invariants:**
- Invoice must be fully funded before repayment
- Invoice can only be repaid once
- Interest is calculated and distributed based on actual elapsed time
- All co-funders' positions updated atomically
- No state changes if repayment fails (atomic)

---

### `withdraw`

Investor withdraws available (undeployed) balance in a given token. Transfers stablecoin from pool contract to investor.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `investor` | `Address` | Yes | Investor address; must sign the transaction |
| `token` | `Address` | Yes | Token address (must be on the whitelist) |
| `amount` | `i128` | Yes | Amount to withdraw; must be > 0 and <= available balance |

#### Returns

`()` — No return value.

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"amount must be positive"` | `amount <= 0` | `withdraw` |
| `"not initialized"` | Contract not initialized | `withdraw` |
| `"token not accepted"` | Token is not on the whitelist | `withdraw` |
| `"no position found"` | Investor has no position in this token | `withdraw` |
| `"insufficient available balance"` | `position.available < amount` | `withdraw` |

#### Access Control

Requires `investor.require_auth()` — the investor must provide a cryptographic signature.

#### Example Invocation

From test [contracts/pool/src/lib.rs#L891](../contracts/pool/src/lib.rs#L891):

```rust
client.withdraw(&investor, &usdc_id, &500_000_000);
let pos = client.get_position(&investor, &usdc_id).unwrap();
assert_eq!(pos.available, 1_500_000_000);
```

CLI invocation:
```bash
stellar contract invoke \
  --id <POOL_CONTRACT_ID> \
  --source investor_secret \
  -- withdraw \
  --investor <INVESTOR_ADDRESS> \
  --token <TOKEN_ADDRESS> \
  --amount 500000000
```

#### State Changes

- Transfer amount of stablecoin from pool contract to investor
- Updates investor's `InvestorPosition` for the token:
  - Decrements `available` by amount
  - Decrements `deposited` by amount
- Updates `TokenTotals.total_deposited` by amount
- Publishes `POOL.withdraw` event with `(investor, amount)`

#### Security Notes

**Trust Model:** Investors can withdraw available balance at any time.

**Invariants:**
- Only available (non-deployed) balance can be withdrawn
- Withdrawal amount must not exceed available balance

---

### `set_rate_bounds`

Admin defines the minimum and maximum allowed exchange rate for a token. These bounds are enforced by `set_exchange_rate` and provide a lightweight guardrail until live oracle validation is introduced.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `admin` | `Address` | Yes | Admin address; must sign the transaction |
| `token` | `Address` | Yes | Accepted token whose exchange-rate guardrails are being set |
| `min_bps` | `u32` | Yes | Minimum allowed rate in basis points |
| `max_bps` | `u32` | Yes | Maximum allowed rate in basis points |

#### Returns

`()` — No return value.

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"unauthorized"` | Caller is not the stored admin | `set_rate_bounds` |
| `"token not accepted"` | Token is not on the whitelist | `set_rate_bounds` |
| `"rate bounds must be positive"` | `min_bps == 0` or `max_bps == 0` | `set_rate_bounds` |
| `"invalid rate bounds"` | `min_bps > max_bps` | `set_rate_bounds` |

---

### `get_rate_bounds`

Returns the current exchange-rate bounds for a token. Tokens without explicit bounds fall back to a narrow `10000..10000` 1:1 default.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `token` | `Address` | Yes | Accepted token address |

#### Returns

`ExchangeRateBounds` — Struct containing `min_bps` and `max_bps`.

---

### `set_exchange_rate`

Admin updates the normalized exchange rate for a token, subject to the configured bounds for that token.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `admin` | `Address` | Yes | Admin address; must sign the transaction |
| `token` | `Address` | Yes | Accepted token address |
| `rate_bps` | `u32` | Yes | New exchange rate in basis points vs USD normalization |

#### Returns

`()` — No return value.

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"unauthorized"` | Caller is not the stored admin | `set_exchange_rate` |
| `"token not accepted"` | Token is not on the whitelist | `set_exchange_rate` |
| `"rate must be positive"` | `rate_bps == 0` | `set_exchange_rate` |
| `"rate out of bounds"` | `rate_bps` falls outside stored bounds for the token | `set_exchange_rate` |

#### Notes

- The current implementation uses configured bounds rather than a live oracle feed.
- Oracle-backed validation is planned as a future enhancement, not part of the active contract interface today.

---

### `get_exchange_rate`

Returns the current exchange rate for a token. Tokens without an explicit override fall back to `10000` (1:1).

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `token` | `Address` | Yes | Accepted token address |

#### Returns

`u32` — The current rate in basis points.

---

### `set_yield`

Admin updates the pool yield rate (in basis points). Affects how much interest SMEs will owe when they repay invoices.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `admin` | `Address` | Yes | Admin address; must sign and match the stored admin |
| `yield_bps` | `u32` | Yes | New yield in basis points (e.g., 800 = 8% APY); must be <= 5000 (50%) |

#### Returns

`()` — No return value.

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"not initialized"` | Contract not initialized | `set_yield` |
| `"unauthorized"` | `admin` does not match stored admin | `set_yield` |
| `"yield cannot exceed 50%"` | `yield_bps > 5000` | `set_yield` |

#### Access Control

Requires `admin.require_auth()` and `require_admin` check — only the admin can modify yield.

#### Example Invocation

From test [contracts/pool/src/lib.rs#L1196](../contracts/pool/src/lib.rs#L1196):

```rust
client.set_yield(&admin, &5_000);  // Set to exactly 50% max

let config = client.get_config();
assert_eq!(config.yield_bps, 5_000);
```

CLI invocation:
```bash
stellar contract invoke \
  --id <POOL_CONTRACT_ID> \
  --source admin_secret \
  -- set_yield \
  --admin <ADMIN_ADDRESS> \
  --yield_bps 1000
```

#### State Changes

- Updates `PoolConfig.yield_bps` to the new value

#### Security Notes

**Trust Model:** Only admin can set yield.

**Invariants:**
- Yield capped at 5000 basis points (50% APY) to prevent excessive rates
- New yield applies to all future repayments

---

### `get_config`

Retrieves the current pool configuration. Read-only query.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |

#### Returns

`PoolConfig` — Current pool configuration (see [Data Types](#poolconfig) for field descriptions).

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"not initialized"` | Contract not initialized | `get_config` |

#### Access Control

None — read-only query.

#### Example Invocation

From test [contracts/pool/src/lib.rs#L891](../contracts/pool/src/lib.rs#L891):

```rust
let config = client.get_config();
assert_eq!(config.yield_bps, DEFAULT_YIELD_BPS);
```

CLI invocation:
```bash
stellar contract invoke \
  --id <POOL_CONTRACT_ID> \
  -- get_config
```

#### State Changes

None — read-only operation.

---

### `accepted_tokens`

Retrieves the whitelist of accepted stablecoin tokens. Read-only query.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |

#### Returns

`Vec<Address>` — Vector of accepted token addresses in order of addition.

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"not initialized"` | Contract not initialized | `accepted_tokens` |

#### Access Control

None — read-only query.

#### Example Invocation

From test [contracts/pool/src/lib.rs#L906](../contracts/pool/src/lib.rs#L906):

```rust
assert_eq!(client.accepted_tokens().len(), 2);  // USDC + EURC
```

CLI invocation:
```bash
stellar contract invoke \
  --id <POOL_CONTRACT_ID> \
  -- accepted_tokens
```

#### State Changes

None — read-only operation.

---

### `get_token_totals`

Retrieves aggregated accounting metrics for a specific token. Read-only query.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `token` | `Address` | Yes | Token address |

#### Returns

`PoolTokenTotals` — Aggregated totals for the token (see [Data Types](#pooltoken totals) for field descriptions).

#### Errors

None — returns `PoolTokenTotals` with all zeros if token has never been used.

#### Access Control

None — read-only query.

#### Example Invocation

From test [contracts/pool/src/lib.rs#L891](../contracts/pool/src/lib.rs#L891):

```rust
let tt = client.get_token_totals(&usdc_id);
assert_eq!(tt.total_deposited, 1_500_000_000);
```

CLI invocation:
```bash
stellar contract invoke \
  --id <POOL_CONTRACT_ID> \
  -- get_token_totals \
  --token <TOKEN_ADDRESS>
```

#### State Changes

None — read-only operation.

---

### `get_position`

Retrieves an investor's position in a specific token. Read-only query.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `investor` | `Address` | Yes | Investor address |
| `token` | `Address` | Yes | Token address |

#### Returns

`Option<InvestorPosition>` — Position record if investor has deposited in this token, otherwise `None`.

#### Errors

None — returns `None` if no position exists.

#### Access Control

None — read-only query.

#### Example Invocation

From test [contracts/pool/src/lib.rs#L876](../contracts/pool/src/lib.rs#L876):

```rust
let pos = client.get_position(&investor, &usdc_id).unwrap();
assert_eq!(pos.deposited, 2_000_000_000);
assert_eq!(pos.available, 2_000_000_000);
```

CLI invocation:
```bash
stellar contract invoke \
  --id <POOL_CONTRACT_ID> \
  -- get_position \
  --investor <INVESTOR_ADDRESS> \
  --token <TOKEN_ADDRESS>
```

#### State Changes

None — read-only operation.

---

### `get_funded_invoice`

Retrieves details of a funded invoice. Read-only query.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `invoice_id` | `u64` | Yes | Invoice ID |

#### Returns

`Option<FundedInvoice>` — Invoice record if registered for co-funding, otherwise `None`.

#### Errors

None — returns `None` if invoice not found.

#### Access Control

None — read-only query.

#### Example Invocation

From test [contracts/pool/src/lib.rs#L903](../contracts/pool/src/lib.rs#L903):

```rust
let record = client.get_funded_invoice(&invoice_id).unwrap();
assert_eq!(record.committed, 2_000_000_000);
assert_eq!(record.funded_at, 0);
```

CLI invocation:
```bash
stellar contract invoke \
  --id <POOL_CONTRACT_ID> \
  -- get_funded_invoice \
  --invoice_id 1
```

#### State Changes

None — read-only operation.

---

### `get_co_fund_share`

Retrieves an investor's commitment share in a specific invoice. Read-only query.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `invoice_id` | `u64` | Yes | Invoice ID |
| `investor` | `Address` | Yes | Investor address |

#### Returns

`i128` — The investor's total committed share (0 if no commitment exists).

#### Errors

None — returns 0 if no share found.

#### Access Control

None — read-only query.

#### Example Invocation

From test [contracts/pool/src/lib.rs#L912](../contracts/pool/src/lib.rs#L912):

```rust
assert_eq!(client.get_co_fund_share(&invoice_id, &investor1), 2_000_000_000);
assert_eq!(client.get_co_fund_share(&invoice_id, &investor2), 1_000_000_000);
```

CLI invocation:
```bash
stellar contract invoke \
  --id <POOL_CONTRACT_ID> \
  -- get_co_fund_share \
  --invoice_id 1 \
  --investor <INVESTOR_ADDRESS>
```

#### State Changes

None — read-only operation.

---

### `available_liquidity`

Returns the available undeployed liquidity in the pool for a given token. Computed as `total_deposited - total_deployed`. Read-only query.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `token` | `Address` | Yes | Token address |

#### Returns

`i128` — Available liquidity in smallest units.

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"not initialized"` | Contract not initialized | `available_liquidity` |

#### Access Control

None — read-only query.

#### Example Invocation

```rust
let avail = client.available_liquidity(&usdc_id);
// avail = total_deposited - total_deployed
```

CLI invocation:
```bash
stellar contract invoke \
  --id <POOL_CONTRACT_ID> \
  -- available_liquidity \
  --token <TOKEN_ADDRESS>
```

#### State Changes

None — read-only operation.

---

### `estimate_repayment`

Estimates the total repayment amount (principal + accrued interest) for an invoice at the current ledger time. If the invoice has not been funded yet, returns the principal amount. Read-only query.

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `env` | `Env` | Yes | Soroban environment context |
| `invoice_id` | `u64` | Yes | Invoice ID |

#### Returns

`i128` — Estimated total repayment (principal if not yet funded, principal + interest otherwise).

#### Errors

| Error | Condition | Affected Methods |
|-------|-----------|------------------|
| `"not initialized"` | Contract not initialized | `estimate_repayment` |
| `"invoice not funded"` | Invoice not registered for co-funding | `estimate_repayment` |

#### Access Control

None — read-only query.

#### Example Invocation

```rust
let estimated = client.estimate_repayment(&invoice_id);
// Returns principal if funded_at == 0
// Returns principal + interest otherwise
```

CLI invocation:
```bash
stellar contract invoke \
  --id <POOL_CONTRACT_ID> \
  -- estimate_repayment \
  --invoice_id 1
```

#### State Changes

None — read-only operation.

---

## Error Reference

This section consolidates every error that can be returned by any public contract method.
Errors are grouped by contract with assigned numeric code ranges for stable frontend mapping.

### Error Code Namespace

Each contract owns a distinct numeric range so codes never collide across contracts:

| Range | Contract |
|---|---|
| `1xx` (100–199) | Invoice contract |
| `2xx` (200–299) | Pool contract |
| `3xx` (300–399) | Credit Score contract |
| `4xx` (400–499) | Share Token contract |

> **Note:** Contracts currently use `panic!` strings rather than structured error enums.
> The codes in the tables below are the canonical identifiers to use in the frontend
> error-message map (`frontend/lib/contracts.ts`) and in any external tooling.
> They map 1-to-1 to the panic strings shown.

### Invoice Contract Errors (codes 100–199)

| Code | Error Message | Meaning | Affected Methods |
|---|------|---------|---------|
| 100 | `"already initialized"` | Contract has been initialized before. Each contract can only be initialized once. | `initialize` |
| 101 | `"not initialized"` | Contract has not been initialized yet. All methods (except `initialize`) require initialization first. | `mark_funded`, `mark_paid`, `mark_defaulted`, `get_invoice`, `get_metadata`, `set_pool` |
| 102 | `"unauthorized"` | Caller is not authorized. For `mark_paid`: must be owner, pool, or admin. For `set_pool`: must be admin. | `mark_paid`, `set_pool` |
| 103 | `"unauthorized pool"` | Pool address is not the authorized pool contract. | `mark_funded`, `mark_defaulted` |
| 104 | `"amount must be positive"` | Amount parameter is ≤ 0. | `create_invoice` |
| 105 | `"due date must be in the future"` | Due date is not strictly after the current ledger timestamp. | `create_invoice` |
| 106 | `"invoice not found"` | Invoice ID does not exist in contract state. | `get_invoice`, `mark_funded`, `mark_paid`, `mark_defaulted` |
| 107 | `"invoice is not pending"` | Invoice is not in `Pending` state; `mark_funded` requires `Pending`. | `mark_funded` |
| 108 | `"invoice is not funded"` | Invoice is not in `Funded` state; `mark_paid` requires `Funded`. | `mark_paid` |
| 109 | `"contract is paused"` | Contract is administratively paused. | all state-mutating methods |

### Pool Contract Errors (codes 200–299)

| Code | Error Message | Meaning | Affected Methods |
|---|------|---------|---------|
| 200 | `"already initialized"` | Contract has been initialized before. | `initialize` |
| 201 | `"unauthorized"` | Caller is not the stored admin address. | `add_token`, `remove_token`, `fund_invoice`, `set_yield` |
| 202 | `"amount must be positive"` | Amount parameter is ≤ 0. | `deposit`, `withdraw`, `repay_invoice` |
| 203 | `"token not accepted"` | Token is not on the accepted-tokens whitelist. | `deposit`, `fund_invoice` |
| 204 | `"token already accepted"` | Token is already on the whitelist. | `add_token` |
| 205 | `"token not in whitelist"` | Token was never added. | `remove_token` |
| 206 | `"token has non-zero pool balances"` | Token has active balances; cannot be removed. | `remove_token` |
| 207 | `"principal must be positive"` | Principal is ≤ 0. | `fund_invoice` |
| 208 | `"insufficient available liquidity"` | Pool has less available liquidity than the requested principal. | `fund_invoice` |
| 209 | `"invoice already funded"` | Invoice was already funded by the pool. | `fund_invoice` |
| 210 | `"invoice not found"` | Invoice ID not found in pool records. | `repay_invoice` |
| 211 | `"invoice already fully repaid"` | Invoice has already been fully repaid. | `repay_invoice` |
| 212 | `"payment exceeds total due"` | Payment amount would exceed the total owed. | `repay_invoice` |
| 213 | `"shares must be positive"` | Share amount for withdrawal is ≤ 0. | `withdraw` |
| 214 | `"insufficient shares"` | Investor's share balance is less than requested. | `withdraw` |
| 215 | `"yield cannot exceed 50%"` | Yield in basis points exceeds 5000. | `set_yield` |
| 216 | `"contract is paused"` | Contract is administratively paused. | all state-mutating methods |

---

## Additional Notes

### Interest Calculation

Interest on invoices is calculated using a simple formula, not compound interest:

$$\text{interest} = \frac{\text{principal} \times \text{yield\_bps} \times \text{elapsed\_seconds}}{10000 \times 31536000}$$

Where:
- **principal**: Invoice principal amount
- **yield_bps**: Annual yield in basis points (e.g., 800 = 8%)
- **elapsed_seconds**: Time in seconds from when the invoice was fully funded to when it is repaid
- **10000**: Basis points denominator
- **31536000**: Seconds per year (365 days)

When multiple investors have co-funded an invoice, each investor's share of interest is:

$$\text{investor\_interest} = \frac{\text{total\_interest} \times \text{investor\_share}}{\text{principal}}$$

### Decimal Conventions

- **USDC on Stellar**: 7 decimals (1 USDC = 10,000,000 in smallest units)
- All amounts in the contract are `i128` integers representing the smallest units
- When displaying to users, divide by $10^7$ for USDC

### Event Publishing

Both contracts emit events for tracking state changes. Events are published with a contract-specific topic:
- **Invoice Contract**: Topic `INVOICE`
- **Pool Contract**: Topic `POOL`

Refer to the contract source code for the complete event schemas.

### Storage Organization

- **Instance Storage**: Fast, limited capacity; used for configuration and initialization flags
- **Persistent Storage**: Slower, unlimited capacity; used for Invoice records, investor positions, and funded invoice details

Queries against persistent storage may have latency; batch queries when possible.
