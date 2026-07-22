import {
  rpcExecute,
  rpcGetEvents,
  rpcGetLatestLedger,
  INVOICE_CONTRACT_ID,
  POOL_CONTRACT_ID,
  CREDIT_SCORE_CONTRACT_ID,
  GOVERNANCE_CONTRACT_ID,
  ORACLE_REGISTRY_CONTRACT_ID,
  NETWORK,
  simulateTx,
  submitTx,
  nativeToScVal,
  scValToNative,
  Address,
  xdr,
  ContractError,
  parseSimulationError,
} from './stellar';
import { TransactionBuilder, BASE_FEE, Contract, rpc as StellarRpc } from '@stellar/stellar-sdk';
import { parseStellarAddress } from './types';
import type {
  Invoice,
  InvoiceMetadata,
  InvestorPosition,
  PoolConfig,
  PoolTokenTotals,
  WaitEstimate,
  FundedInvoice,
  CollateralConfig,
  CollateralDeposit,
  GovernanceConfig,
  GovernanceProposal,
  StellarAddress,
  OracleInfo,
  VerificationRound,
  OracleRegistryConfig,
} from './types';
// Auto-generated contract bindings (single source of truth for the on-chain
// ABI — methods, struct shapes and error codes). Regenerate with
// `./scripts/gen-bindings.sh`; see CONTRIBUTING.md.
import { Errors as InvoiceErrors } from '@/src/generated/invoice';
import { Errors as CreditScoreErrors } from '@/src/generated/credit_score';

// Re-export the generated contract clients and raw ABI types so SDK authors
// and frontend code can consume them through this module instead of reaching
// into the generated files directly.
export { InvoiceContract, CreditScoreContract } from '@/src/generated';
export type {
  Invoice as InvoiceAbi,
  InvoiceStatus as InvoiceStatusAbi,
  InvoiceMetadata as InvoiceMetadataAbi,
} from '@/src/generated/invoice';
export type { CreditScoreResponse } from '@/src/generated/credit_score';

// ── Contract ID validation (#399) ────────────────────────────────────────────

function validateContractId(id: string, name: string): string {
  if (process.env.NODE_ENV === 'test') return id;
  if (!id) return id;
  if (!/^C[A-Z2-7]{55}$/.test(id)) {
    throw new Error(`Invalid contract ID for ${name}: "${id}"`);
  }
  return id;
}

validateContractId(INVOICE_CONTRACT_ID, 'invoice');
validateContractId(POOL_CONTRACT_ID, 'pool');
validateContractId(CREDIT_SCORE_CONTRACT_ID, 'credit_score');
if (GOVERNANCE_CONTRACT_ID) {
  validateContractId(GOVERNANCE_CONTRACT_ID, 'governance');
}
if (ORACLE_REGISTRY_CONTRACT_ID) {
  validateContractId(ORACLE_REGISTRY_CONTRACT_ID, 'oracle_registry');
}

// ── Mock mode (#229) ─────────────────────────────────────────────────────────
// Set NEXT_PUBLIC_USE_MOCK=true to read from the local json-server instead of
// making live Soroban RPC calls. Useful for frontend-only development when no
// Stellar node is available. See mock-service/README.md for setup instructions.

const USE_MOCK = process.env.NEXT_PUBLIC_USE_MOCK === 'true';
const MOCK_API_URL = process.env.NEXT_PUBLIC_MOCK_API_URL ?? 'http://localhost:4000';

type RpcAccount = Awaited<ReturnType<StellarRpc.Server['getAccount']>>;
type RpcBuiltTransaction = Parameters<StellarRpc.Server['simulateTransaction']>[0];

function getRpcAccount(address: string): Promise<RpcAccount> {
  return rpcExecute<RpcAccount>((server) => server.getAccount(address));
}

function simulateRpcTransaction(
  tx: RpcBuiltTransaction,
): Promise<StellarRpc.Api.SimulateTransactionResponse> {
  return rpcExecute<StellarRpc.Api.SimulateTransactionResponse>((server) =>
    server.simulateTransaction(tx),
  );
}

async function mockFetch<T>(path: string): Promise<T> {
  const res = await fetch(`${MOCK_API_URL}${path}`);
  if (!res.ok) throw new Error(`Mock API error: ${res.status} ${path}`);
  return res.json() as Promise<T>;
}

// ---- Invoice Contract ----

export async function getInvoice(id: number): Promise<Invoice> {
  if (USE_MOCK) return mockFetch<Invoice>(`/invoices/${id}`);
  const sim = await simulateTx(
    INVOICE_CONTRACT_ID,
    'get_invoice',
    [nativeToScVal(id, { type: 'u64' })],
    // read-only — use a zero address placeholder
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return scValToNative(result!.retval) as Invoice;
}

export async function getMultipleInvoices(ids: number[]): Promise<Invoice[]> {
  if (ids.length === 0) return [];

  const invoices = await Promise.all(ids.map((id) => getInvoice(id)));
  return invoices;
}

export async function getInvoiceMetadata(id: number): Promise<InvoiceMetadata> {
  const sim = await simulateTx(
    INVOICE_CONTRACT_ID,
    'get_metadata',
    [nativeToScVal(id, { type: 'u64' })],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as Record<string, unknown>;
  const due = raw.due_date !== undefined ? Number(raw.due_date) : Number(raw.dueDate);

  return {
    name: raw.name as string,
    description: raw.description as string,
    image: raw.image as string,
    amount: BigInt(String(raw.amount)),
    debtor: raw.debtor as string,
    dueDate: due,
    status: raw.status as InvoiceMetadata['status'],
    symbol: raw.symbol as string,
    decimals: Number(raw.decimals),
  };
}

export async function getInvoiceCount(): Promise<number> {
  const sim = await simulateTx(
    INVOICE_CONTRACT_ID,
    'get_invoice_count',
    [],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return Number(scValToNative(result!.retval));
}

export async function getMaxInvoiceAmount(): Promise<number> {
  const sim = await simulateTx(
    INVOICE_CONTRACT_ID,
    'get_max_invoice_amount',
    [],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return Number(scValToNative(result!.retval));
}

export async function buildCreateInvoiceTx(params: {
  owner: string;
  debtor: string;
  amount: bigint;
  dueDate: number;
  description: string;
  verificationHash?: string;
  metadataUri?: string;
}): Promise<string> {
  // ── Input validation (#687) ────────────────────────────────────────────────
  // Reject obviously invalid invoices client-side before spending an RPC round
  // trip on a simulation that the contract would reject anyway.
  if (!params.debtor || params.debtor.trim() === '') {
    throw new Error('Debtor name is required');
  }
  if (params.amount <= 0n) {
    throw new Error('Amount must be greater than zero');
  }
  const nowSecs = Math.floor(Date.now() / 1000);
  if (!Number.isFinite(params.dueDate) || params.dueDate <= nowSecs) {
    throw new Error('Due date must be in the future');
  }

  const account = await getRpcAccount(params.owner);
  const contract = new Contract(INVOICE_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'create_invoice_with_metadata',
        new Address(params.owner).toScVal(),
        nativeToScVal(params.debtor, { type: 'string' }),
        nativeToScVal(params.amount, { type: 'i128' }),
        nativeToScVal(params.dueDate, { type: 'u64' }),
        nativeToScVal(params.description, { type: 'string' }),
        nativeToScVal(params.verificationHash ?? '', { type: 'string' }),
        params.metadataUri
          ? nativeToScVal(params.metadataUri, { type: 'string' })
          : xdr.ScVal.scvVoid(),
      ),
    )
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new ContractError(parseSimulationError(sim));
  }

  const prepared = StellarRpc.assembleTransaction(tx, sim).build();
  return prepared.toXDR();
}

export async function buildRenewInvoiceTtlTx(params: {
  operator: string;
  invoiceId: number;
}): Promise<string> {
  const account = await getRpcAccount(params.operator);
  const contract = new Contract(INVOICE_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(contract.call('renew_ttl', nativeToScVal(params.invoiceId, { type: 'u64' })))
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

// ---- Pool Contract ----

export async function getPoolConfig(): Promise<PoolConfig> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'get_config',
    [],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as Record<string, unknown>;

  return {
    invoiceContract: raw.invoice_contract as string,
    admin: raw.admin as StellarAddress,
    yieldBps: Number(raw.yield_bps),
    factoringFeeBps: Number(raw.factoring_fee_bps ?? 0),
    compoundInterest: Boolean(raw.compound_interest),
    proposedYieldBps: Number(raw.proposed_yield_bps ?? 0),
    yieldProposalAt: Number(raw.yield_proposal_at ?? 0),
    yieldTimelockSecs: Number(raw.yield_timelock_secs ?? 0),
    maxSingleInvestorBps: Number(raw.max_single_investor_bps ?? 0),
    maxWithdrawalQueueAgeDays: Number(raw.max_withdrawal_queue_age_days ?? 0),
  };
}

export async function estimateWithdrawalWait(
  investor: string,
  token: string,
): Promise<WaitEstimate | null> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'estimate_withdrawal_wait',
    [new Address(investor).toScVal(), new Address(token).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval);
  if (!raw) return null;

  const estimate = raw as Record<string, unknown>;
  return {
    queuePosition: Number(estimate.queue_position ?? 0),
    capitalAhead: BigInt(String(estimate.capital_ahead ?? 0)),
    nearestInvoiceDueDate: Number(estimate.nearest_invoice_due_date ?? 0),
  };
}

export async function getAcceptedTokens(): Promise<string[]> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'accepted_tokens',
    [],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as string[];
  return Array.isArray(raw) ? raw : [];
}

export async function getPoolTokenTotals(token: string): Promise<PoolTokenTotals> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'get_token_totals',
    [new Address(token).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as Record<string, unknown>;
  return {
    totalDeposited: BigInt(raw.total_deposited as string),
    totalDeployed: BigInt(raw.total_deployed as string),
    totalPaidOut: BigInt(raw.total_paid_out as string),
    totalFeeRevenue: BigInt((raw.total_fee_revenue as string | number | bigint) ?? 0),
  };
}

export async function getTokenDepositCap(token: string): Promise<bigint> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'get_token_deposit_cap',
    [new Address(token).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return BigInt(String(scValToNative(result!.retval) ?? 0));
}

export async function getInvestorPosition(
  investor: string,
  token: string,
): Promise<InvestorPosition | null> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'get_position',
    [new Address(investor).toScVal(), new Address(token).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval);
  if (!raw) return null;

  const pos = raw as Record<string, unknown>;
  return {
    deposited: BigInt(pos.deposited as string),
    available: BigInt(pos.available as string),
    deployed: BigInt(pos.deployed as string),
    earned: BigInt(pos.earned as string),
    depositCount: Number(pos.deposit_count),
  };
}

export async function buildDepositTx(
  investor: string,
  token: string,
  amount: bigint,
): Promise<string> {
  const account = await getRpcAccount(investor);
  const contract = new Contract(POOL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'deposit',
        new Address(investor).toScVal(),
        new Address(token).toScVal(),
        nativeToScVal(amount, { type: 'i128' }),
      ),
    )
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }

  const prepared = StellarRpc.assembleTransaction(tx, sim).build();
  return prepared.toXDR();
}

export async function getFundedInvoice(invoiceId: number): Promise<FundedInvoice | null> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'get_funded_invoice',
    [nativeToScVal(invoiceId, { type: 'u64' })],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval);
  if (!raw) return null;

  const r = raw as Record<string, unknown>;
  return {
    invoiceId: Number(r.invoice_id),
    sme: r.sme as string,
    token: r.token as string,
    principal: BigInt(r.principal as string),
    committed: BigInt(r.committed as string),
    fundedAt: Number(r.funded_at),
    factoringFee: BigInt((r.factoring_fee as string | number | bigint) ?? 0),
    dueDate: Number(r.due_date),
    repaidAmount: BigInt((r.repaid_amount as string | number | bigint) ?? 0),
  };
}

export async function buildInitCoFundingTx(params: {
  admin: string;
  invoiceId: number;
  principal: bigint;
  sme: string;
  dueDate: number;
  token: string;
}): Promise<string> {
  const account = await getRpcAccount(params.admin);
  const contract = new Contract(POOL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'init_co_funding',
        new Address(params.admin).toScVal(),
        nativeToScVal(params.invoiceId, { type: 'u64' }),
        nativeToScVal(params.principal, { type: 'i128' }),
        new Address(params.sme).toScVal(),
        nativeToScVal(params.dueDate, { type: 'u64' }),
        new Address(params.token).toScVal(),
      ),
    )
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }

  const prepared = StellarRpc.assembleTransaction(tx, sim).build();
  return prepared.toXDR();
}

export async function buildCommitToInvoiceTx(params: {
  investor: string;
  invoiceId: number;
  amount: bigint;
}): Promise<string> {
  const account = await getRpcAccount(params.investor);
  const contract = new Contract(POOL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'commit_to_invoice',
        new Address(params.investor).toScVal(),
        nativeToScVal(params.invoiceId, { type: 'u64' }),
        nativeToScVal(params.amount, { type: 'i128' }),
      ),
    )
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }

  const prepared = StellarRpc.assembleTransaction(tx, sim).build();
  return prepared.toXDR();
}

export async function buildRepayTx(params: {
  payer: string;
  invoiceId: number;
  amount: bigint;
}): Promise<string> {
  const account = await getRpcAccount(params.payer);
  const contract = new Contract(POOL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'repay_invoice',
        nativeToScVal(params.invoiceId, { type: 'u64' }),
        new Address(params.payer).toScVal(),
        nativeToScVal(params.amount, { type: 'i128' }),
      ),
    )
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }

  const prepared = StellarRpc.assembleTransaction(tx, sim).build();
  return prepared.toXDR();
}

export async function buildWithdrawTx(
  investor: string,
  token: string,
  amount: bigint,
): Promise<string> {
  const account = await getRpcAccount(investor);
  const contract = new Contract(POOL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'withdraw',
        new Address(investor).toScVal(),
        new Address(token).toScVal(),
        nativeToScVal(amount, { type: 'i128' }),
      ),
    )
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }

  const prepared = StellarRpc.assembleTransaction(tx, sim).build();
  return prepared.toXDR();
}

export async function buildSetYieldTx(admin: string, yieldBps: number): Promise<string> {
  const account = await getRpcAccount(admin);
  const contract = new Contract(POOL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'set_yield',
        new Address(admin).toScVal(),
        nativeToScVal(yieldBps, { type: 'u32' }),
      ),
    )
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }

  const prepared = StellarRpc.assembleTransaction(tx, sim).build();
  return prepared.toXDR();
}

export async function buildSetFactoringFeeTx(
  admin: string,
  factoringFeeBps: number,
): Promise<string> {
  const account = await getRpcAccount(admin);
  const contract = new Contract(POOL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'set_factoring_fee',
        new Address(admin).toScVal(),
        nativeToScVal(factoringFeeBps, { type: 'u32' }),
      ),
    )
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }

  const prepared = StellarRpc.assembleTransaction(tx, sim).build();
  return prepared.toXDR();
}

/**
 * NOTE: mark_defaulted currently requires pool.require_auth() in the Invoice contract.
 * Since the Pool contract lacks a wrapper, this call may fail from a standard admin wallet
 * unless the contract admin is also the pool address stored in the invoice.
 */
export async function buildMarkDefaultedTx(admin: string, invoiceId: number): Promise<string> {
  const account = await getRpcAccount(admin);
  const contract = new Contract(INVOICE_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'mark_defaulted',
        nativeToScVal(invoiceId, { type: 'u64' }),
        new Address(POOL_CONTRACT_ID).toScVal(), // Attempting with Pool contract ID
      ),
    )
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }

  const prepared = StellarRpc.assembleTransaction(tx, sim).build();
  return prepared.toXDR();
}

export async function isProtocolPaused(): Promise<boolean> {
  const sim = await simulateTx(
    INVOICE_CONTRACT_ID,
    'is_paused',
    [],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return Boolean(scValToNative(result!.retval));
}

export async function buildPauseProtocolTx(admin: string): Promise<string> {
  const account = await getRpcAccount(admin);
  const contract = new Contract(INVOICE_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(contract.call('pause', new Address(admin).toScVal()))
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }

  const prepared = StellarRpc.assembleTransaction(tx, sim).build();
  return prepared.toXDR();
}

export async function buildUnpauseProtocolTx(admin: string): Promise<string> {
  const account = await getRpcAccount(admin);
  const contract = new Contract(INVOICE_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(contract.call('unpause', new Address(admin).toScVal()))
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }

  const prepared = StellarRpc.assembleTransaction(tx, sim).build();
  return prepared.toXDR();
}

export async function buildDisputeTx(params: {
  disputer: string;
  invoiceId: number;
  reason: string;
  oracleHash?: string;
}): Promise<string> {
  const account = await getRpcAccount(params.disputer);
  const contract = new Contract(INVOICE_CONTRACT_ID);
  const oracleHash = params.oracleHash ?? '';

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'verify_invoice',
        nativeToScVal(params.invoiceId, { type: 'u64' }),
        new Address(params.disputer).toScVal(),
        nativeToScVal(false, { type: 'bool' }),
        nativeToScVal(params.reason, { type: 'string' }),
        nativeToScVal(oracleHash, { type: 'string' }),
      ),
    )
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }

  const prepared = StellarRpc.assembleTransaction(tx, sim).build();
  return prepared.toXDR();
}

// ---- #109: KYC / investor whitelist ----

export interface KycInvestor {
  address: string;
  totalDeposited: bigint;
  firstSeenAt: number;
  isApproved: boolean;
}

export async function fetchKycInvestors(): Promise<{
  pending: KycInvestor[];
  approved: KycInvestor[];
}> {
  try {
    const latestLedger = await rpcGetLatestLedger();
    // Look back ~30 days (17280 * 30 ledgers) or as far as the RPC allows to find depositors
    const startLedger = Math.max(1, latestLedger.sequence - 17280 * 30);

    const response = await rpcGetEvents({
      startLedger,
      filters: [{ contractIds: [POOL_CONTRACT_ID] }],
    });

    const depositors = new Map<string, { total: bigint; firstSeen: number }>();

    for (const e of response.events) {
      try {
        const topic = e.topic.map((t) => scValToNative(t as any));
        if (topic[1] === 'deposit') {
          const val = scValToNative(e.value) as unknown[];
          const investor = val[0] as string;
          const amount = val[1] as bigint;
          const timestamp = new Date(
            (e as any).ledgerClosedAt ?? (e as any).ledgerCloseAt,
          ).getTime();

          const existing = depositors.get(investor);
          if (existing) {
            depositors.set(investor, {
              total: existing.total + amount,
              firstSeen: Math.min(existing.firstSeen, timestamp),
            });
          } else {
            depositors.set(investor, { total: amount, firstSeen: timestamp });
          }
        }
      } catch (err) {
        // skip parse errors
      }
    }

    const pending: KycInvestor[] = [];
    const approved: KycInvestor[] = [];

    // Map each unique depositor to their KYC status
    for (const [address, data] of Array.from(depositors.entries())) {
      let investorAddress: StellarAddress;
      try {
        investorAddress = parseStellarAddress(address);
      } catch {
        continue;
      }
      const isApproved = await getInvestorKyc(investorAddress);
      const investor: KycInvestor = {
        address: investorAddress,
        totalDeposited: data.total,
        firstSeenAt: data.firstSeen,
        isApproved,
      };
      if (isApproved) {
        approved.push(investor);
      } else {
        pending.push(investor);
      }
    }

    pending.sort((a, b) => b.firstSeenAt - a.firstSeenAt);
    approved.sort((a, b) => b.firstSeenAt - a.firstSeenAt);

    return { pending, approved };
  } catch (error) {
    console.error('Failed to fetch KYC investors:', error);
    return { pending: [], approved: [] };
  }
}

export async function getKycRequired(): Promise<boolean> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'kyc_required',
    [],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return Boolean(scValToNative(result!.retval));
}

export async function getInvestorKyc(investor: StellarAddress): Promise<boolean> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'get_investor_kyc',
    [new Address(investor).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return Boolean(scValToNative(result!.retval));
}

export async function buildSetKycRequiredTx(
  admin: StellarAddress,
  required: boolean,
): Promise<string> {
  const account = await getRpcAccount(admin);
  const contract = new Contract(POOL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'set_kyc_required',
        new Address(admin).toScVal(),
        nativeToScVal(required, { type: 'bool' }),
      ),
    )
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

export async function buildSetInvestorKycTx(
  admin: StellarAddress,
  investor: StellarAddress,
  approved: boolean,
): Promise<string> {
  const account = await getRpcAccount(admin);
  const contract = new Contract(POOL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'set_investor_kyc',
        new Address(admin).toScVal(),
        new Address(investor).toScVal(),
        nativeToScVal(approved, { type: 'bool' }),
      ),
    )
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

// ---- #111: Exchange rate ----

export async function getExchangeRate(token: string): Promise<number> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'get_exchange_rate',
    [new Address(token).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return Number(scValToNative(result!.retval));
}

export async function buildSetExchangeRateTx(
  admin: string,
  token: string,
  rateBps: number,
): Promise<string> {
  const account = await getRpcAccount(admin);
  const contract = new Contract(POOL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'set_exchange_rate',
        new Address(admin).toScVal(),
        new Address(token).toScVal(),
        nativeToScVal(rateBps, { type: 'u32' }),
      ),
    )
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

// ---- #157: SSE Events convenience wrapper ----

/**
 * Fetch the current investor position for a given wallet address.
 * Used by the SSE polling service to refresh portfolio data automatically.
 */
export async function fetchInvestorPosition(investor: string): Promise<InvestorPosition | null> {
  // Try USDC first (most common), fall back to EURC
  const USDC_TOKEN_ID = process.env.NEXT_PUBLIC_USDC_TOKEN_ID ?? '';
  const EURC_TOKEN_ID = process.env.NEXT_PUBLIC_EURC_TOKEN_ID ?? '';

  try {
    if (USDC_TOKEN_ID) {
      const pos = await getInvestorPosition(investor, USDC_TOKEN_ID);
      if (pos) return pos;
    }
  } catch {
    // Fall through to EURC
  }

  try {
    if (EURC_TOKEN_ID) {
      const pos = await getInvestorPosition(investor, EURC_TOKEN_ID);
      if (pos) return pos;
    }
  } catch {
    // No position found
  }

  return null;
}

// ---- Error message mapping (issue #163) ----
// Maps contract panic strings to user-friendly messages.
// Full error code reference: docs/API_REFERENCE.md

const CONTRACT_ERROR_MESSAGES: Record<string, string> = {
  // Invoice contract errors
  'already initialized': 'This contract has already been set up.',
  'not initialized': 'The contract is not yet configured. Please contact support.',
  unauthorized: 'You are not authorised to perform this action.',
  'unauthorized pool': 'The pool contract is not authorised for this invoice.',
  'amount must be positive': 'Amount must be greater than zero.',
  'due date must be in the future': 'The due date must be a future date.',
  'invoice not found': 'Invoice not found. Please check the invoice ID.',
  'invoice is not pending': 'This invoice is not in a pending state.',
  'invoice is not funded': 'This invoice has not been funded yet.',
  'contract is paused': 'The contract is currently paused. Please try again later.',
  // Pool contract errors
  'token not accepted': 'This token is not supported by the pool.',
  'insufficient available liquidity': 'The pool does not have enough liquidity for this invoice.',
  'invoice already funded': 'This invoice has already been funded.',
  'invoice already fully repaid': 'This invoice has already been fully repaid.',
  'payment exceeds total due': 'The payment amount exceeds the total amount owed.',
  'shares must be positive': 'Share amount must be greater than zero.',
  'insufficient shares': 'You do not have enough shares to withdraw that amount.',
  'yield cannot exceed 50%': 'Yield rate cannot exceed 50% APY.',
  // Credit score contract errors
  'invoice already processed': 'This invoice has already been recorded in the credit score.',
};

/**
 * Converts a raw contract panic string to a user-friendly message.
 * Falls back to the original message if no mapping is found.
 */
export function getContractErrorMessage(raw: string): string {
  const lower = raw.toLowerCase();
  for (const [key, friendly] of Object.entries(CONTRACT_ERROR_MESSAGES)) {
    if (lower.includes(key)) return friendly;
  }
  return raw;
}

// Soroban surfaces contract errors as `Error(Contract, #<code>)`. The generated
// bindings expose the authoritative code → variant-name mapping per contract,
// so we resolve the variant name from the bindings and then reuse the
// human-friendly text above. This keeps the error catalogue in sync with the
// contract source automatically (issue #163 / bindings sync).
type GeneratedErrorMap = Record<number, { message: string }>;

const CONTRACT_ERROR_MAPS = {
  invoice: InvoiceErrors as GeneratedErrorMap,
  credit_score: CreditScoreErrors as GeneratedErrorMap,
} as const;

export type ContractName = keyof typeof CONTRACT_ERROR_MAPS;

/**
 * Resolves a numeric contract error code to a user-friendly message using the
 * generated bindings as the source of truth for the variant name. Falls back to
 * the raw variant name, then to a generic message if the code is unknown.
 */
export function getContractErrorByCode(contract: ContractName, code: number): string {
  const variant = CONTRACT_ERROR_MAPS[contract]?.[code]?.message;
  if (!variant) return `Unknown ${contract} error (#${code}).`;
  // Variant names are PascalCase (e.g. "InvoiceNotFound"); turn them into a
  // lookup key that matches CONTRACT_ERROR_MESSAGES' lowercased phrases.
  const friendly = getContractErrorMessage(variant.replace(/([a-z])([A-Z])/g, '$1 $2'));
  return friendly === variant.replace(/([a-z])([A-Z])/g, '$1 $2') ? variant : friendly;
}

// ---- Collateral ----

export async function getCollateralConfig(): Promise<CollateralConfig> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'get_collateral_config',
    [],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as Record<string, unknown>;
  return {
    threshold: BigInt(String(raw.threshold)),
    collateralBps: Number(raw.collateral_bps),
  };
}

export async function getCollateralDeposit(invoiceId: number): Promise<CollateralDeposit | null> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'get_collateral_deposit',
    [nativeToScVal(invoiceId, { type: 'u64' })],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval);
  if (!raw) return null;
  const r = raw as Record<string, unknown>;
  return {
    invoiceId: Number(r.invoice_id),
    depositor: r.depositor as string,
    token: r.token as string,
    amount: BigInt(String(r.amount)),
    settled: Boolean(r.settled),
  };
}

export async function buildDepositCollateralTx(params: {
  invoiceId: number;
  depositor: string;
  token: string;
  amount: bigint;
}): Promise<string> {
  const account = await getRpcAccount(params.depositor);
  const contract = new Contract(POOL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'deposit_collateral',
        nativeToScVal(params.invoiceId, { type: 'u64' }),
        new Address(params.depositor).toScVal(),
        new Address(params.token).toScVal(),
        nativeToScVal(params.amount, { type: 'i128' }),
      ),
    )
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

// ---- Credit Score Contract ----

export async function getCreditScoreStatus(
  sme: string,
): Promise<{ isStale: boolean; score: number } | null> {
  if (!CREDIT_SCORE_CONTRACT_ID) return null;
  try {
    const sim = await simulateTx(
      CREDIT_SCORE_CONTRACT_ID,
      'get_credit_score',
      [new Address(sme).toScVal()],
      sme,
    );
    const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
    const data = scValToNative(result!.retval) as { score: number; is_stale: boolean };
    return { isStale: Boolean(data.is_stale), score: Number(data.score) };
  } catch {
    return null;
  }
}

// ---- Governance ----

export async function getGovernanceConfig(): Promise<GovernanceConfig | null> {
  if (!GOVERNANCE_CONTRACT_ID) return null;

  try {
    const sim = await simulateTx(
      GOVERNANCE_CONTRACT_ID,
      'get_config',
      [],
      'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
    );

    const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
    const raw = scValToNative(result!.retval) as Record<string, unknown>;

    return {
      admin: raw.admin as StellarAddress,
      shareToken: raw.share_token as string,
      votingPeriodSecs: Number(raw.voting_period_secs),
      quorumBps: Number(raw.quorum_bps),
      passBps: Number(raw.pass_bps),
      executionDelaySecs: Number(raw.execution_delay_secs),
      minShareBalance: BigInt(String(raw.min_share_balance ?? 0)),
    };
  } catch {
    return null;
  }
}

export async function getShareBalance(shareTokenId: string, address: string): Promise<bigint> {
  try {
    const sim = await simulateTx(
      shareTokenId,
      'balance',
      [new Address(address).toScVal()],
      address,
    );

    const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
    return BigInt(String(scValToNative(result!.retval) ?? 0));
  } catch {
    return 0n;
  }
}

export async function listGovernanceProposals(): Promise<GovernanceProposal[]> {
  if (!GOVERNANCE_CONTRACT_ID) return [];

  const sim = await simulateTx(
    GOVERNANCE_CONTRACT_ID,
    'list_proposals',
    [],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as Array<Record<string, unknown>>;
  return raw.map((proposal) => ({
    id: Number(proposal.id),
    proposer: proposal.proposer as string,
    description: proposal.description as string,
    targetContract: proposal.target_contract as string,
    functionName: String(proposal.function_name),
    calldata: String(proposal.calldata),
    votesFor: BigInt(String(proposal.votes_for)),
    votesAgainst: BigInt(String(proposal.votes_against)),
    status: proposal.status as GovernanceProposal['status'],
    createdAt: Number(proposal.created_at),
    votingEndsAt: Number(proposal.voting_ends_at),
    executionDelay: Number(proposal.execution_delay),
  }));
}

export async function buildCreateProposalTx(params: {
  proposer: string;
  description: string;
  targetContract: string;
  functionName: string;
  calldata: string;
}): Promise<string> {
  const account = await getRpcAccount(params.proposer);
  const contract = new Contract(GOVERNANCE_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'create_proposal',
        nativeToScVal(params.description, { type: 'string' }),
        new Address(params.targetContract).toScVal(),
        nativeToScVal(params.functionName, { type: 'string' }),
        nativeToScVal(params.calldata, { type: 'string' }),
      ),
    )
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

export async function buildVoteProposalTx(params: {
  voter: string;
  proposalId: number;
  inFavor: boolean;
}): Promise<string> {
  const account = await getRpcAccount(params.voter);
  const contract = new Contract(GOVERNANCE_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'vote',
        nativeToScVal(params.proposalId, { type: 'u64' }),
        nativeToScVal(params.inFavor, { type: 'bool' }),
      ),
    )
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

export async function buildExecuteProposalTx(
  executor: string,
  proposalId: number,
): Promise<string> {
  const account = await getRpcAccount(executor);
  const contract = new Contract(GOVERNANCE_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(contract.call('execute_proposal', nativeToScVal(proposalId, { type: 'u64' })))
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

export async function buildCancelProposalTx(
  cancelledBy: string,
  proposalId: number,
): Promise<string> {
  const account = await getRpcAccount(cancelledBy);
  const contract = new Contract(GOVERNANCE_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(contract.call('cancel_proposal', nativeToScVal(proposalId, { type: 'u64' })))
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

// ---- #861: Oracle Registry (N-of-M staked oracle consensus network) ----
// ORACLE_REGISTRY_CONTRACT_ID is optional — unset until the registry is
// deployed, mirroring how GOVERNANCE_CONTRACT_ID is handled above.

export async function getRegistryConfig(): Promise<OracleRegistryConfig> {
  const sim = await simulateTx(
    ORACLE_REGISTRY_CONTRACT_ID,
    'get_registry_config',
    [],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as Record<string, unknown>;

  return {
    minStake: BigInt(String(raw.min_stake)),
    stakeToken: raw.stake_token as string,
    requiredVotes: Number(raw.required_votes),
    quorumBps: Number(raw.quorum_bps),
    roundDurationSecs: Number(raw.round_duration_secs),
    deregisterCooldownSecs: Number(raw.deregister_cooldown_secs),
    treasury: (raw.treasury as string | null) ?? null,
  };
}

export async function listActiveOracles(): Promise<StellarAddress[]> {
  const sim = await simulateTx(
    ORACLE_REGISTRY_CONTRACT_ID,
    'list_active_oracles',
    [],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as unknown[];
  return (raw ?? []) as StellarAddress[];
}

export async function getOracleInfo(operator: string): Promise<OracleInfo | null> {
  const sim = await simulateTx(
    ORACLE_REGISTRY_CONTRACT_ID,
    'get_oracle_info',
    [new Address(operator).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval);
  if (!raw) return null;
  const info = raw as Record<string, unknown>;

  return {
    address: info.address as StellarAddress,
    stakeAmount: BigInt(String(info.stake_amount)),
    stakeToken: info.stake_token as string,
    isActive: Boolean(info.is_active),
    totalVerifications: Number(info.total_verifications),
    totalSlashes: Number(info.total_slashes),
    registeredAt: Number(info.registered_at),
    deregisterRequestedAt:
      info.deregister_requested_at !== undefined && info.deregister_requested_at !== null
        ? Number(info.deregister_requested_at)
        : null,
  };
}

export async function getVerificationRound(invoiceId: number): Promise<VerificationRound | null> {
  const sim = await simulateTx(
    ORACLE_REGISTRY_CONTRACT_ID,
    'get_verification_round',
    [nativeToScVal(invoiceId, { type: 'u64' })],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval);
  if (!raw) return null;
  const round = raw as Record<string, unknown>;

  return {
    invoiceId: Number(round.invoice_id),
    requiredVotes: Number(round.required_votes),
    totalRegisteredOracles: Number(round.total_registered_oracles),
    weightFor: BigInt(String(round.weight_for)),
    weightAgainst: BigInt(String(round.weight_against)),
    totalStakeSnapshot: BigInt(String(round.total_stake_snapshot)),
    quorumBps: Number(round.quorum_bps),
    status: round.status as VerificationRound['status'],
    openedAt: Number(round.opened_at),
    deadline: Number(round.deadline),
    oracleHash: round.oracle_hash as string,
  };
}

export async function buildAdminResolveRoundTx(params: {
  admin: StellarAddress;
  invoiceId: number;
  approved: boolean;
  reason: string;
}): Promise<string> {
  const account = await getRpcAccount(params.admin);
  const contract = new Contract(ORACLE_REGISTRY_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'admin_resolve_round',
        new Address(params.admin).toScVal(),
        nativeToScVal(params.invoiceId, { type: 'u64' }),
        nativeToScVal(params.approved, { type: 'bool' }),
        nativeToScVal(params.reason, { type: 'string' }),
      ),
    )
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

export async function buildSlashOracleTx(params: {
  admin: StellarAddress;
  operator: StellarAddress;
  bps: number;
}): Promise<string> {
  const account = await getRpcAccount(params.admin);
  const contract = new Contract(ORACLE_REGISTRY_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'slash_oracle',
        new Address(params.admin).toScVal(),
        new Address(params.operator).toScVal(),
        nativeToScVal(params.bps, { type: 'u32' }),
      ),
    )
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

export { submitTx };
