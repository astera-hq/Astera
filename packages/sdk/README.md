# @astera/sdk

Client library for interacting with Astera smart contracts on Stellar.

## Installation

```bash
npm install @astera/sdk
```

## Usage

### Per-Contract Clients

Each contract has its own client class that accepts a `ClientConfig` with `rpcUrl`, `network`, `contractId`, and an optional `signer`.

```typescript
import { InvoiceClient, PoolClient, CreditScoreClient } from '@astera/sdk';

const invoiceClient = new InvoiceClient({
  rpcUrl: 'https://soroban-testnet.stellar.org',
  network: 'Test SDF Network ; September 2015',
  contractId: 'C...',
});

const poolClient = new PoolClient({
  rpcUrl: 'https://soroban-testnet.stellar.org',
  network: 'Test SDF Network ; September 2015',
  contractId: 'C...',
});
```

### Read Methods (Simulation Only)

Read methods simulate the contract call without submitting a transaction:

```typescript
const invoice = await invoiceClient.getInvoice(1n);
console.log(invoice.status);

const poolConfig = await poolClient.getConfig();
console.log(poolConfig.admin);

const score = await creditScoreClient.getCreditScore('G...');
console.log(score.score);
```

### Write Methods (Signed Transactions)

Write methods require a `signer` function. Pass it per-call or set it once on the client config:

```typescript
const hash = await invoiceClient.createInvoice({
  signer: async (txXdr: string) => {
    const signed = await window.freighter.signTransaction(txXdr);
    return signed;
  },
  owner: 'G...',
  debtor: 'debtor-id',
  amount: 100_000_000n,
  dueDate: Math.floor(Date.now() / 1000) + 86400 * 30,
  description: 'Invoice for services',
  onProgress: (p) => console.log(p.status),
});
```

### Server-Side Signing

Provide a custom `Signer` for server-side key management:

```typescript
import { Keypair } from '@stellar/stellar-sdk';

const serverSigner: Signer = async (txXdr: string) => {
  const keypair = Keypair.fromSecret('S...');
  const tx = TransactionBuilder.fromXDR(txXdr, 'Test SDF Network ; September 2015');
  tx.sign(keypair);
  return tx.toXDR();
};

const client = new PoolClient({
  rpcUrl,
  network,
  contractId,
  signer: serverSigner,
});

const hash = await client.deposit({
  investor: keypair.publicKey(),
  token: 'C...',
  amount: 1_000_000_000n,
});
```

### Available Clients

| Client | Methods |
|---|---|
| `InvoiceClient` | `getInvoice`, `getMetadata`, `createInvoice`, `verifyInvoice` |
| `PoolClient` | `getConfig`, `getPosition`, `deposit`, `requestWithdrawal`, `cancelWithdrawalRequest`, `drainWithdrawalQueue`, `getWithdrawalQueue`, `estimateWithdrawalWait`, `getLiquidityForecast`, `repay`, `fundInvoice`, `fundInvoicesBatch`, `openCoFunding`, `commitToInvoice`, `finalizeCoFunding`, `withdrawCommitment`, `transferCoFundShare`, `getCoFundingRound`, `listCoFundingRounds`, `getInvestorCoFundPositions`, `getCoFundShare`, `getCurrentRate`, `getRateModelConfig`, `getRateHistory`, `previewRateAtUtilization`, `proposeRateModelChange`, `executeRateModelChange`, `cancelRateModelChange`, `getFundedInvoice`, `getPoolTokenTotals` |
| `CreditScoreClient` | `getCreditScore`, `simulateScoreWithAttestations`, `getAttestorInfo`, `listActiveAttestors`, `getAttestation`, `listSmeAttestations`, `registerAttestor`, `deactivateAttestor`, `submitAttestation`, `disputeAttestation`, `resolveAttestationDispute` |
| `OracleRegistryClient` | `openRound`, `register`, `vote`, `getRound`, `getOracleInfo` |
| `ComplianceClient` | `isCleared`, `getRecord`, `getHistory`, `listFlagged`, `listPendingReview`, `submitScreeningResult`, `requestReview` |
| `TrancheClient` | `getPool`, `getTrancheConfig`, `getTotals`, `getInvestorPosition`, `getLifetimeReturnBps`, `getInvoiceExposure`, `simulateWaterfall`, `deposit`, `withdraw`, `setTrancheConfig` |
| `AccessControlClient` | `getRoleConfig`, `isSigner`, `getProposal`, `listProposals`, `proposeAction`, `approveAction`, `revokeApproval`, `rejectAction`, `executeAction` |

Or use `AsteraClient`, a single convenience wrapper over all per-contract clients (`client.invoice`, `client.pool`, `client.tranche`, `client.accessControl`, etc.), configured once with an `AsteraConfig` of contract IDs.

### Events

```typescript
import { parseContractEvent } from '@astera/sdk';

const events = await server.getEvents({ contractIds: [poolId], startLedger: 1000 });
for (const event of events.events) {
  const parsed = parseContractEvent({ topic: event.topic, value: event.value });
  if (parsed?.type === 'pool:deposit') {
    console.log(parsed.depositor, parsed.amount.toString());
  }
}
```

## Build

```bash
npm run build
```

Outputs CJS, ESM, and `.d.ts` to `dist/`.
