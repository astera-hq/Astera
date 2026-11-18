# Defaulted Invoice Keeper Runbook (#1734)

This runbook describes the operational sequence after an invoice passes its
contract-enforced grace period. The calls are separate transactions: the pool
cannot call the insurance contract and then be called back by insurance while
the pool's liquidation operation is still on the Soroban call stack.

## Prerequisites

- The invoice, pool, insurance (if coverage is configured), and auction
  contracts are deployed on the same Stellar network.
- The invoice contract's pool address is configured correctly.
- A dedicated keeper account is funded for transaction fees.
- The keeper account is allowlisted on the invoice contract with
  `add_keeper(admin, keeper)`. A keeper can call `mark_defaulted` only; it
  does not receive admin or pool privileges.
- The operator has the contract IDs, RPC URL, network passphrase, and access
  to a secure secret store. Never commit `KEEPER_SECRET_KEY`.

## 1. Mark the invoice defaulted

Preconditions:

- The invoice exists and is in `Funded` status.
- Its due date plus the invoice-specific grace-period override (or global
  grace period when no override exists) has elapsed.
- The caller is either the configured pool contract or an allowlisted keeper.

Call:

```bash
soroban contract invoke \
  --id "$INVOICE_CONTRACT_ID" \
  --source "$KEEPER_ID" \
  --network "$NETWORK" \
  -- mark_defaulted --id "$INVOICE_ID" --pool "$KEEPER_ADDRESS"
```

Expected event: invoice contract event `default`, containing the invoice ID,
SME owner, and ledger timestamp. The invoice status changes from `Funded` to
`Defaulted`. Re-read `get_invoice(INVOICE_ID)` and verify the status before
continuing.

## 2. Liquidate collateral through the pool

This is a separate, permissionless call. The caller must authorize the
transaction, but does not need to be the invoice keeper or pool admin.

Preconditions:

- The invoice is in `Defaulted` status on the invoice contract.
- The pool has a funded-invoice record and an unsettled collateral deposit
  for the invoice.
- The invoice has not been fully repaid and collateral has not already been
  settled.

Call:

```bash
soroban contract invoke \
  --id "$POOL_CONTRACT_ID" \
  --source "$KEEPER_ID" \
  --network "$NETWORK" \
  -- liquidate_invoice --caller "$KEEPER_ADDRESS" --invoice_id "$INVOICE_ID"
```

Expected events: `liq_started` when the operation begins and `liq_seized`
when the collateral seizure succeeds. The latter includes invoice ID,
depositor, amount, triggering caller, and timestamp. Confirm the collateral
record is settled with `get_collateral_deposit(INVOICE_ID)` before filing
an insurance claim.

If the invoice has no collateral, is not defaulted, was already repaid, or
was already settled, the pool rejects the call. Do not repeatedly submit a
known-invalid transaction; re-read invoice and pool state first.

## 3. File the insurance claim

Insurance is deliberately not called from inside pool liquidation. Its
`file_claim` method reads the invoice and pool state and calls back into the
pool to credit the payout; performing that callback during the pool's active
liquidation call would violate Soroban's no-re-entrancy rule.

Preconditions:

- The insurance reserve is initialized and not paused.
- A coverage record exists for the invoice.
- The invoice is defaulted and collateral has been settled.
- There is a positive uncovered shortfall and available reserve balance.

Call in a new transaction:

```bash
soroban contract invoke \
  --id "$INSURANCE_CONTRACT_ID" \
  --source "$KEEPER_ID" \
  --network "$NETWORK" \
  -- file_claim --invoice_id "$INVOICE_ID"
```

Expected event: insurance event `claimed`, containing invoice ID and payout. The call is permissionless. It independently verifies the default, calculates
the remaining shortfall after collateral recovery, caps the payout by nominal
coverage and available reserves, transfers the payout to the pool, and credits
the pool through `receive_insurance_payout`. Check the returned payout and
the insurance claim-history/reserve views. A missing coverage record,
no-shortfall condition, or depleted reserve is a business outcome to record,
not a reason to resubmit indefinitely.

## 4. Handle collateral auction sales

Collateral liquidation and collateral auction sale creation are distinct
operations. `pool.liquidate_invoice` settles the collateral deposit into the
pool's accounting; it does **not** automatically open a Dutch auction.

When an operator has selected sale terms, the authorized seller can call
`auction.open_collateral_sale` with a `CollateralSaleParams` value containing:

- `seller`: account holding and authorizing the collateral transfer
- `token` and `amount`: collateral asset and quantity to consign
- `proceeds_token` and `proceeds_recipient`: payment asset and recipient
- `start_price`, `floor_price`, and `duration_secs`: explicit sale policy

The auction emits `sale_open`. Buyers can inspect `list_open_sales` and
`current_sale_price(sale_id)`, then call `take_collateral_sale` while the
sale is open and unexpired. The contract emits the sale-settlement event when
the swap completes. Do not invent sale prices or durations in the keeper:
these are risk and treasury policy inputs and must be supplied by the operator.

## Keeper monitor automation

`scripts/monitor_invoices.ts` is the polling entry point. It reads the invoice
count, grace period, and invoice records, and submits `mark_defaulted` for
funded invoices whose deadline has passed. It currently does **not** execute
pool liquidation, file insurance claims, or create collateral sales. Those
steps require separate transactions and state checks described above.

A production keeper workflow should:

1. Re-scan all active invoice IDs on every sweep (not only IDs created since
   the previous process start), so transient RPC failures and invoices that
   become overdue later are retried.
2. Read current invoice status and grace-period override before submitting
   `mark_defaulted`; treat an already-defaulted invoice as a resumable state.
3. Wait for transaction confirmation before submitting the next transaction.
4. After confirming `Defaulted`, call `pool.liquidate_invoice` and verify
   `liq_seized`/settled collateral state.
5. If an insurance contract is configured, call `insurance.file_claim` in a
   later transaction and record the returned payout or the explicit reason
   no payout was made.
6. Keep auction creation as a separately configured policy action; never
   infer pricing parameters from the invoice monitor.
7. Persist per-invoice stage and transaction hashes, use bounded retries with
   backoff for transient RPC failures, and make every stage safe to resume
   after a process restart.

### Monitor configuration and operation

```bash
cd scripts
npm install
INVOICE_CONTRACT_ID=... KEEPER_SECRET_KEY=... \
  RPC_URL=https://soroban-testnet.stellar.org \
  NETWORK_PASSPHRASE="Test SDF Network ; September 2015" \
  RUN_ONCE=true npx tsx monitor_invoices.ts
```

For continuous polling, omit `RUN_ONCE=true` and set
`POLL_INTERVAL_MS` (default: 60000). Use a dedicated signer, keep its secret
in a secret manager, and monitor transaction failures and keeper account
balances. For production, run one monitor instance per network and alert on
repeated failures; avoid concurrent instances using the same signer because
they can race on account sequence numbers.

## Failure and retry handling

| Stage | Typical failure | Response |
| --- | --- | --- |
| Read invoice | RPC timeout, missing record | Retry transient RPC errors with backoff; do not mark the ID permanently checked |
| Mark defaulted | Grace period not elapsed or status changed | Re-read state and retry only after the deadline/state permits |
| Liquidate invoice | Not defaulted, no collateral, already settled, or fully repaid | Reconcile invoice and pool state; skip terminal/already-settled cases |
| File claim | No coverage, no shortfall, reserve unavailable, insurance paused | Record the reason; retry only if the underlying condition can change |
| Auction sale | Invalid parameters, expired sale, insufficient buyer funds | Re-read sale state; only reopen with a newly approved sale policy |

Never retry a transaction merely because the client timed out after submission:
first inspect its hash/status and on-chain state to avoid duplicate actions.
