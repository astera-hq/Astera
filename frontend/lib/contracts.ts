import {
  rpc,
  INVOICE_CONTRACT_ID,
  POOL_CONTRACT_ID,
  NETWORK,
  simulateTx,
  submitTx,
  nativeToScVal,
  scValToNative,
  Address,
} from './stellar';
import { TransactionBuilder, BASE_FEE, Contract, rpc as StellarRpc } from '@stellar/stellar-sdk';
import type {
  Invoice,
  InvestorPosition,
  PoolConfig,
  PoolTokenTotals,
  FundedInvoice,
  InvoiceMetadata,
} from './types';
import { performanceMonitor } from './performance';

// ---- Performance Optimizations ----

/** Simple in-memory cache for contract calls */
const contractCache = new Map<string, { data: any; timestamp: number; ttl: number }>();
const CACHE_TTL = 30_000; // 30 seconds

function getCachedResult<T>(key: string): T | null {
  const cached = contractCache.get(key);
  if (cached && Date.now() - cached.timestamp < cached.ttl) {
    performanceMonitor.recordCacheHit();
    return cached.data as T;
  }
  performanceMonitor.recordCacheMiss();
  return null;
}

function setCachedResult<T>(key: string, data: T, ttl: number = CACHE_TTL): void {
  contractCache.set(key, { data, timestamp: Date.now(), ttl });
}

/** Batch multiple contract reads into a single simulation where possible */
async function batchContractReads(calls: Array<{
  contractId: string;
  method: string;
  args: any[];
  key?: string;
  cacheTTL?: number;
}>): Promise<any[]> {
  // Check cache first for each call
  const uncachedCalls: typeof calls = [];
  const results: any[] = new Array(calls.length);

  calls.forEach((call, index) => {
    if (call.key) {
      const cached = getCachedResult(call.key);
      if (cached !== null) {
        results[index] = cached;
      } else {
        uncachedCalls.push({ ...call, originalIndex: index });
      }
    } else {
      uncachedCalls.push({ ...call, originalIndex: index });
    }
  });

  if (uncachedCalls.length === 0) {
    return results;
  }

  // For now, process uncached calls individually
  // In a future optimization, we could use Stellar's batch operations
  await Promise.all(
    uncachedCalls.map(async ({ contractId, method, args, key, cacheTTL, originalIndex }) => {
      try {
        const result = await simulateTx(
          contractId,
          method,
          args,
          'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
        );

        const simResult = (result as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
        const data = scValToNative(simResult!.retval);
        
        results[originalIndex] = data;
        
        if (key) {
          setCachedResult(key, data, cacheTTL);
        }
      } catch (error) {
        console.error(`Failed to call ${method}:`, error);
        results[originalIndex] = null;
      }
    })
  );

  return results;
}

// ---- Invoice Contract ----

export async function getInvoice(id: number): Promise<Invoice> {
  const cacheKey = `invoice_${id}`;
  const cached = getCachedResult<Invoice>(cacheKey);
  if (cached) return cached;

  const sim = await simulateTx(
    INVOICE_CONTRACT_ID,
    'get_invoice',
    [nativeToScVal(id, { type: 'u64' })],
    // read-only — use a zero address placeholder
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const invoice = scValToNative(result!.retval) as Invoice;
  setCachedResult(cacheKey, invoice, 60_000); // Cache for 1 minute
  return invoice;
}

export async function getInvoiceMetadata(id: number): Promise<InvoiceMetadata> {
  const cacheKey = `invoice_metadata_${id}`;
  const cached = getCachedResult<InvoiceMetadata>(cacheKey);
  if (cached) return cached;

  const sim = await simulateTx(
    INVOICE_CONTRACT_ID,
    'get_metadata',
    [nativeToScVal(id, { type: 'u64' })],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as Record<string, unknown>;
  const due = raw.due_date !== undefined ? Number(raw.due_date) : Number(raw.dueDate);

  const metadata: InvoiceMetadata = {
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
  
  setCachedResult(cacheKey, metadata, 60_000); // Cache for 1 minute
  return metadata;
}

export async function getInvoiceCount(): Promise<number> {
  const cacheKey = 'invoice_count';
  const cached = getCachedResult<number>(cacheKey);
  if (cached) return cached;

  const sim = await simulateTx(
    INVOICE_CONTRACT_ID,
    'get_invoice_count',
    [],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const count = Number(scValToNative(result!.retval));
  setCachedResult(cacheKey, count, 10_000); // Cache for 10 seconds
  return count;
}

/** Batch fetch multiple invoices and their metadata */
export async function getInvoicesBatch(ids: number[]): Promise<{
  invoices: Invoice[];
  metadata: InvoiceMetadata[];
}> {
  const calls = ids.flatMap(id => [
    {
      contractId: INVOICE_CONTRACT_ID,
      method: 'get_invoice',
      args: [nativeToScVal(id, { type: 'u64' })],
      key: `invoice_${id}`,
      cacheTTL: 60_000
    },
    {
      contractId: INVOICE_CONTRACT_ID,
      method: 'get_metadata',
      args: [nativeToScVal(id, { type: 'u64' })],
      key: `invoice_metadata_${id}`,
      cacheTTL: 60_000
    }
  ]);

  const results = await batchContractReads(calls);
  
  const invoices: Invoice[] = [];
  const metadata: InvoiceMetadata[] = [];
  
  for (let i = 0; i < ids.length; i++) {
    const invoiceResult = results[i * 2];
    const metadataResult = results[i * 2 + 1];
    
    if (invoiceResult) invoices.push(invoiceResult);
    if (metadataResult) metadata.push(metadataResult);
  }
  
  return { invoices, metadata };
}

export async function buildCreateInvoiceTx(params: {
  owner: string;
  debtor: string;
  amount: bigint;
  dueDate: number;
  description: string;
}): Promise<string> {
  const account = await rpc.getAccount(params.owner);
  const contract = new Contract(INVOICE_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'create_invoice',
        new Address(params.owner).toScVal(),
        nativeToScVal(params.debtor, { type: 'string' }),
        nativeToScVal(params.amount, { type: 'i128' }),
        nativeToScVal(params.dueDate, { type: 'u64' }),
        nativeToScVal(params.description, { type: 'string' }),
      ),
    )
    .setTimeout(30)
    .build();

  const sim = await rpc.simulateTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }

  const prepared = StellarRpc.assembleTransaction(tx, sim).build();
  return prepared.toXDR();
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
    admin: raw.admin as string,
    yieldBps: Number(raw.yield_bps),
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
  };
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
  const account = await rpc.getAccount(investor);
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

  const sim = await rpc.simulateTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }

  const prepared = StellarRpc.assembleTransaction(tx, sim).build();
  return prepared.toXDR();
}

export async function getFundedInvoice(invoiceId: number): Promise<FundedInvoice | null> {
  const cacheKey = `funded_invoice_${invoiceId}`;
  const cached = getCachedResult<FundedInvoice | null>(cacheKey);
  if (cached !== undefined) return cached;

  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'get_funded_invoice',
    [nativeToScVal(invoiceId, { type: 'u64' })],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval);
  if (!raw) {
    setCachedResult(cacheKey, null, 30_000); // Cache null for 30 seconds
    return null;
  }

  const fundedInvoice: FundedInvoice = {
    invoiceId: Number(raw.invoice_id),
    sme: raw.sme as string,
    token: raw.token as string,
    principal: BigInt(raw.principal as string),
    committed: BigInt(raw.committed as string),
    fundedAt: Number(raw.funded_at),
    dueDate: Number(raw.due_date),
    repaid: Boolean(raw.repaid),
  };
  
  setCachedResult(cacheKey, fundedInvoice, 30_000); // Cache for 30 seconds
  return fundedInvoice;
}

/** Batch fetch multiple funded invoices */
export async function getFundedInvoicesBatch(invoiceIds: number[]): Promise<FundedInvoice[]> {
  const calls = invoiceIds.map(id => ({
    contractId: POOL_CONTRACT_ID,
    method: 'get_funded_invoice',
    args: [nativeToScVal(id, { type: 'u64' })],
    key: `funded_invoice_${id}`,
    cacheTTL: 30_000
  }));

  const results = await batchContractReads(calls);
  return results.filter(r => r !== null) as FundedInvoice[];
}

export async function buildInitCoFundingTx(params: {
  admin: string;
  invoiceId: number;
  principal: bigint;
  sme: string;
  dueDate: number;
  token: string;
}): Promise<string> {
  const account = await rpc.getAccount(params.admin);
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

  const sim = await rpc.simulateTransaction(tx);
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
  const account = await rpc.getAccount(params.investor);
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

  const sim = await rpc.simulateTransaction(tx);
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
  const account = await rpc.getAccount(investor);
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

  const sim = await rpc.simulateTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }

  const prepared = StellarRpc.assembleTransaction(tx, sim).build();
  return prepared.toXDR();
}

export async function buildSetYieldTx(admin: string, yieldBps: number): Promise<string> {
  const account = await rpc.getAccount(admin);
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

  const sim = await rpc.simulateTransaction(tx);
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
  const account = await rpc.getAccount(admin);
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

  const sim = await rpc.simulateTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }

  const prepared = StellarRpc.assembleTransaction(tx, sim).build();
  return prepared.toXDR();
}

export { submitTx };
