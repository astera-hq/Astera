import {
  rpcExecute,
  rpcGetEvents,
  rpcGetLatestLedger,
  INVOICE_CONTRACT_ID,
  POOL_CONTRACT_ID,
  GOVERNANCE_CONTRACT_ID,
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
import type {
  Invoice,
  InvoiceMetadata,
  InvestorPosition,
  PoolConfig,
  PoolTokenTotals,
  FundedInvoice,
  CollateralConfig,
  CollateralDeposit,
  GovernanceProposal,
} from './types';

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

  const sim = await simulateTx(
    INVOICE_CONTRACT_ID,
    'get_multiple_invoices',
    [xdr.ScVal.scvVec(ids.map((id) => nativeToScVal(BigInt(id), { type: 'u64' })))],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return scValToNative(result!.retval) as Invoice[];
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

export async function buildCreateInvoiceTx(params: {
  owner: string;
  debtor: string;
  amount: bigint;
  dueDate: number;
  description: string;
  verificationHash?: string;
  metadataUri?: string;
}): Promise<string> {
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
    admin: raw.admin as string,
    yieldBps: Number(raw.yield_bps),
    factoringFeeBps: Number(raw.factoring_fee_bps ?? 0),
    compoundInterest: Boolean(raw.compound_interest),
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

export async function fetchKycInvestors(): Promise<{ pending: KycInvestor[]; approved: KycInvestor[] }> {
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
        const topic = e.topic.map((t: string) => scValToNative(t));
        if (topic[1] === 'deposit') {
          const val = scValToNative(e.value) as unknown[];
          const investor = val[0] as string;
          const amount = val[1] as bigint;
          const timestamp = new Date(e.ledgerCloseAt).getTime();

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
      const isApproved = await getInvestorKyc(address);
      const investor: KycInvestor = {
        address,
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

export async function getInvestorKyc(investor: string): Promise<boolean> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'get_investor_kyc',
    [new Address(investor).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return Boolean(scValToNative(result!.retval));
}

export async function buildSetKycRequiredTx(admin: string, required: boolean): Promise<string> {
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
  admin: string,
  investor: string,
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

// ---- Governance ----

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

export async function buildExecuteProposalTx(executor: string, proposalId: number): Promise<string> {
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

export async function buildCancelProposalTx(cancelledBy: string, proposalId: number): Promise<string> {
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

export { submitTx };
