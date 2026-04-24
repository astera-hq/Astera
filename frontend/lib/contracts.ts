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
  InvoiceMetadata,
  InvestorPosition,
  PoolConfig,
  PoolTokenTotals,
  FundedInvoice,
} from './types';

// ---- Invoice Contract ----

export async function getInvoice(id: number): Promise<Invoice> {
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
    repaid: Boolean(r.repaid),
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

export async function buildSetFactoringFeeTx(
  admin: string,
  factoringFeeBps: number,
): Promise<string> {
  const account = await rpc.getAccount(admin);
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

// ---- #109: KYC / investor whitelist ----

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
  const account = await rpc.getAccount(admin);
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

  const sim = await rpc.simulateTransaction(tx);
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
  const account = await rpc.getAccount(admin);
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

  const sim = await rpc.simulateTransaction(tx);
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
  const account = await rpc.getAccount(admin);
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

  const sim = await rpc.simulateTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

export { submitTx };
