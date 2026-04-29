# Interacting with Astera Smart Contracts

This guide is designed for frontend developers, protocol users, and non-Rust contributors who want to understand how the Astera contracts work and how to interact with them without writing Rust code.

## Prerequisites

1.  **Stellar CLI**: Install the [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools/cli/stellar-cli).
2.  **Testnet Account**: Create and fund a testnet account using `stellar keys fund` or the [Stellar Laboratory](https://laboratory.stellar.org/#account-creator?network=test).
3.  **Contract IDs**: Ensure you have the contract IDs for the Pool and Invoice contracts.

---

## Section 1: Reading Contract State (no wallet needed)

Reading from a contract is a "view" operation that doesn't cost gas or require a funded wallet.

### Get Pool Configuration
This returns the global settings of the pool, such as the authorized invoice contract and the current yield rate.

```bash
stellar contract invoke \
  --id $POOL_CONTRACT_ID \
  --network testnet \
  -- get_config
```

### Get Your Investor Position
Check how much you have deposited, how much is currently deployed in invoices, and how much interest you have earned.

```bash
stellar contract invoke \
  --id $POOL_CONTRACT_ID \
  --network testnet \
  -- get_position \
  --investor $YOUR_ADDRESS \
  --token $USDC_TOKEN_ADDRESS
```

---

## Section 2: Writing Transactions (wallet needed)

State-changing operations require a transaction signed by your secret key or a wallet like Freighter.

### Create an Invoice
SMEs can tokenize an invoice by submitting its details. Note that `amount` is in **stroops** (7 decimal places) and `due_date` is a Unix timestamp.

```bash
stellar contract invoke \
  --id $INVOICE_CONTRACT_ID \
  --source-account YOUR_SECRET_KEY \
  --network testnet \
  -- create_invoice \
  --owner $YOUR_ADDRESS \
  --debtor "Acme Corp" \
  --amount 1000000000 \
  --due_date 1735689600 \
  --description "Goods delivery #101" \
  --verification_hash "a1b2c3d4..."
```
*(1,000,000,000 stroops = 100.00 USDC)*

---

## Section 3: Using the JavaScript SDK

Once the Astera SDK is merged (see issue #165), you can interact with the contracts directly from your frontend or Node.js application.

```typescript
import { AsteraClient } from '@astera/sdk';

const client = new AsteraClient({ 
  network: 'testnet',
  rpcUrl: 'https://soroban-testnet.stellar.org'
});

// Fetch an invoice by its ID
const invoice = await client.invoice.get(42n);
console.log(`Invoice Principal: ${invoice.amount} stroops`);

// Check pool liquidity
const liquidity = await client.pool.getAvailableLiquidity(USDC_ADDRESS);
```

---

## Section 4: Understanding Contract Responses

### ScVal Encoding
The Stellar network uses `ScVal` (Smart Contract Value) to encode data. While the CLI decodes this for you, you should be aware of common types:
- **Address**: Represented as `G...` (accounts) or `C...` (contracts).
- **i128 / u64**: Large numbers are often returned as strings or BigInts in JS.

### Amounts (Stroops)
All token amounts in Astera use **7 decimal places** to align with Stellar's native USDC.
- `1.0000000` USDC = `10,000,000` stroops.
- `100.00` USDC = `1,000,000,000` stroops.

---

## Section 5: Common Tasks Cookbook

### "How do I check if my withdrawal is ready?"
Withdrawals may be subject to a cooldown or a queue. Check your request status:
```bash
stellar contract invoke \
  --id $POOL_CONTRACT_ID \
  --network testnet \
  -- get_withdrawal_queue \
  --token $USDC_TOKEN_ADDRESS
```

### "How do I see how much yield I've earned?"
Invoke `get_position` and look at the `earned` field. This shows the cumulative interest accrued to your position since your first deposit.

### "How do I check an SME's credit score?"
The credit score is managed by a dedicated contract. Use the SME's address to look up their history:
```bash
stellar contract invoke \
  --id $CREDIT_CONTRACT_ID \
  --network testnet \
  -- get_credit_score \
  --sme $SME_ADDRESS
```
