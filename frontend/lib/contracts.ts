import {
  rpcExecute,
  rpcGetEvents,
  rpcGetLatestLedger,
  INVOICE_CONTRACT_ID,
  POOL_CONTRACT_ID,
  SECONDARY_MARKET_CONTRACT_ID,
  AUCTION_CONTRACT_ID,
  CREDIT_SCORE_CONTRACT_ID,
  GOVERNANCE_CONTRACT_ID,
  ORACLE_REGISTRY_CONTRACT_ID,
  COMPLIANCE_CONTRACT_ID,
  REFERRAL_CONTRACT_ID,
  TRANCHE_CONTRACT_ID,
  ACCESS_CONTROL_CONTRACT_ID,
  ARBITRATION_CONTRACT_ID,
  INSURANCE_CONTRACT_ID,
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
  WithdrawalRequest,
  LiquidityForecastPoint,
  FundedInvoice,
  CollateralConfig,
  CollateralDeposit,
  CollateralRiskConfig,
  GovernanceConfig,
  GovernanceProposal,
  StellarAddress,
  CoFundingRound,
  OracleInfo,
  VerificationRound,
  OracleRegistryConfig,
  AttestorType,
  AttestorInfo,
  Attestation,
  FullCreditScore,
  RateModelConfig,
  RateSnapshot,
  ReferralStats,
  Role,
  MultiSigConfig,
  ActionPayload,
  Proposal,
  Listing,
  ListingKind,
  ListingStatus,
  Order,
  OrderSide,
  OrderStatus,
  DisputeRecord,
  DisputeResolution,
  DisputeCase,
  EvidenceEntry,
  JurorInfo,
  JurorVoteStatus,
  ArbitrationConfig,
} from './types';
import { ALL_ROLES } from './types';
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
if (COMPLIANCE_CONTRACT_ID) {
  validateContractId(COMPLIANCE_CONTRACT_ID, 'compliance');
}
if (REFERRAL_CONTRACT_ID) {
  validateContractId(REFERRAL_CONTRACT_ID, 'referral');
}
if (ARBITRATION_CONTRACT_ID) {
  validateContractId(ARBITRATION_CONTRACT_ID, 'arbitration');
}

// ── Mock mode (#229) ─────────────────────────────────────────────────────────
// Set NEXT_PUBLIC_USE_MOCK=true to read from the local json-server instead of
// making live Soroban RPC calls. Useful for frontend-only development when no
// Stellar node is available. See mock-service/README.md for setup instructions.

const USE_MOCK = process.env.NEXT_PUBLIC_USE_MOCK === 'true';
const MOCK_API_URL = process.env.NEXT_PUBLIC_MOCK_API_URL ?? 'http://localhost:4000';

type RpcAccount = Awaited<ReturnType<StellarRpc.Server['getAccount']>> & AccountWithBalances;
type RpcBuiltTransaction = Parameters<StellarRpc.Server['simulateTransaction']>[0];

interface AccountWithBalances {
  balances: Array<{ asset_type: string; balance: string }>;
}

function getRpcAccount(address: string): Promise<RpcAccount> {
  return rpcExecute((server) => server.getAccount(address) as Promise<RpcAccount>);
}

function getNativeBalanceStroops(account: AccountWithBalances | undefined): bigint {
  if (!account?.balances) return 0n;
  const nativeBalance = account.balances.find((balance) => balance.asset_type === 'native');
  if (!nativeBalance?.balance) return 0n;
  return BigInt(Math.round(Number.parseFloat(nativeBalance.balance) * 1_000_000));
}

function ensureSufficientNativeBalance(
  account: AccountWithBalances,
  requiredStroops = BigInt(BASE_FEE),
) {
  if (getNativeBalanceStroops(account) < requiredStroops) {
    throw new Error('Insufficient balance for this transaction');
  }
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

/** #775: whether the borrower has opted this invoice out of the public sharing link. */
export async function isInvoicePrivate(id: number): Promise<boolean> {
  const sim = await simulateTx(
    INVOICE_CONTRACT_ID,
    'is_invoice_private',
    [nativeToScVal(id, { type: 'u64' })],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return Boolean(scValToNative(result!.retval));
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
    maxWithdrawalQueueDepth: Number(raw.max_withdrawal_queue_depth ?? 0),
  };
}

export async function estimateWithdrawalWait(
  investor: string,
  token: string,
): Promise<WaitEstimate | null> {
  const sim = await simulateTx(
    SECONDARY_MARKET_CONTRACT_ID,
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
    estimatedWaitSecs: Number(estimate.estimated_wait_secs ?? 0),
  };
}

export async function getWithdrawalQueue(token: string): Promise<WithdrawalRequest[]> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'get_withdrawal_queue',
    [new Address(token).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as Record<string, unknown>[];
  if (!Array.isArray(raw)) return [];

  return raw.map((r) => ({
    investor: r.investor as string,
    token: r.token as string,
    shares: BigInt(String(r.shares ?? 0)),
    requestedAt: Number(r.requested_at ?? 0),
    requestId: Number(r.request_id ?? 0),
  }));
}

export async function getLiquidityForecast(
  token: string,
  horizonDays: number,
): Promise<LiquidityForecastPoint[]> {
  const sim = await simulateTx(
    SECONDARY_MARKET_CONTRACT_ID,
    'get_liquidity_forecast',
    [new Address(token).toScVal(), nativeToScVal(horizonDays, { type: 'u32' })],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as Record<string, unknown>[];
  if (!Array.isArray(raw)) return [];

  return raw.map((r) => ({
    day: Number(r.day ?? 0),
    projectedAvailable: BigInt(String(r.projected_available ?? 0)),
  }));
}

// ---- #1025/#1044: secondary market for pool positions and co-funding shares ----

function listingKindToScVal(kind: ListingKind): xdr.ScVal {
  return xdr.ScVal.scvVec([nativeToScVal(kind, { type: 'symbol' })]);
}

function listingFromRaw(raw: Record<string, unknown>): Listing {
  return {
    listingId: Number(raw.listing_id),
    invoiceId: Number(raw.invoice_id),
    seller: raw.seller as string,
    token: raw.token as string,
    kind: enumTagFromNative<ListingKind>(raw.kind),
    amountOrBps: BigInt(String(raw.amount_or_bps)),
    price: BigInt(String(raw.price)),
    createdAt: Number(raw.created_at),
    status: enumTagFromNative<ListingStatus>(raw.status),
  };
}

/** List part or all of a position for sale on the secondary market. */
export async function buildListPositionTx(params: {
  seller: string;
  invoiceId: number;
  kind: ListingKind;
  amountOrBps: bigint;
  price: bigint;
}): Promise<string> {
  const account = await getRpcAccount(params.seller);
  const contract = new Contract(SECONDARY_MARKET_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'list_position',
        new Address(params.seller).toScVal(),
        nativeToScVal(params.invoiceId, { type: 'u64' }),
        listingKindToScVal(params.kind),
        nativeToScVal(params.amountOrBps, { type: 'u64' }),
        nativeToScVal(params.price, { type: 'i128' }),
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

/** Cancel an open listing. Only the original seller may cancel. */
export async function buildCancelListingTx(seller: string, listingId: number): Promise<string> {
  const account = await getRpcAccount(seller);
  const contract = new Contract(SECONDARY_MARKET_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'cancel_listing',
        new Address(seller).toScVal(),
        nativeToScVal(listingId, { type: 'u64' }),
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

/** Buy an open listing. Buyer's available balance is debited by the listing price. */
export async function buildBuyListingTx(buyer: string, listingId: number): Promise<string> {
  const account = await getRpcAccount(buyer);
  const contract = new Contract(SECONDARY_MARKET_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'buy_listing',
        new Address(buyer).toScVal(),
        nativeToScVal(listingId, { type: 'u64' }),
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

/** Fetch a single listing by ID. Returns null if not found. */
export async function getListing(listingId: number): Promise<Listing | null> {
  const sim = await simulateTx(
    SECONDARY_MARKET_CONTRACT_ID,
    'get_listing',
    [nativeToScVal(listingId, { type: 'u64' })],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval);
  if (!raw) return null;
  return listingFromRaw(raw as Record<string, unknown>);
}

/** All listing IDs for a given invoice (open and closed). */
export async function listListingsForInvoice(invoiceId: number): Promise<number[]> {
  const sim = await simulateTx(
    SECONDARY_MARKET_CONTRACT_ID,
    'list_listings_for_invoice',
    [nativeToScVal(invoiceId, { type: 'u64' })],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as unknown[];
  if (!Array.isArray(raw)) return [];
  return raw.map((id) => Number(id));
}

/** All listing IDs created by a given seller (open and closed). */
export async function listListingsForInvestor(seller: string): Promise<number[]> {
  const sim = await simulateTx(
    SECONDARY_MARKET_CONTRACT_ID,
    'list_listings_for_investor',
    [new Address(seller).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as unknown[];
  if (!Array.isArray(raw)) return [];
  return raw.map((id) => Number(id));
}

// ---- #1035: limit order book, sitting alongside the #1025 listing flow above ----

function orderSideToScVal(side: OrderSide): xdr.ScVal {
  return xdr.ScVal.scvVec([nativeToScVal(side, { type: 'symbol' })]);
}

// `place_order` takes a single PlaceOrderRequest struct rather than
// individual scalar params (mirrors `openCoFundingRequestToScVal` above).
// Soroban encodes named-field #[contracttype] structs as an ScMap keyed by
// field-name Symbols in alphabetical order — NOT declaration order — so
// the entries below are deliberately sorted (amount_or_bps, expires_at,
// invoice_id, kind, price, side).
function placeOrderRequestToScVal(params: {
  invoiceId: number;
  kind: ListingKind;
  side: OrderSide;
  amountOrBps: bigint;
  price: bigint;
  expiresAt?: number;
}): xdr.ScVal {
  const entry = (key: string, val: xdr.ScVal) =>
    new xdr.ScMapEntry({ key: nativeToScVal(key, { type: 'symbol' }), val });
  return xdr.ScVal.scvMap([
    entry('amount_or_bps', nativeToScVal(params.amountOrBps, { type: 'u64' })),
    entry('expires_at', nativeToScVal(params.expiresAt ?? 0, { type: 'u64' })),
    entry('invoice_id', nativeToScVal(params.invoiceId, { type: 'u64' })),
    entry('kind', listingKindToScVal(params.kind)),
    entry('price', nativeToScVal(params.price, { type: 'i128' })),
    entry('side', orderSideToScVal(params.side)),
  ]);
}

function orderFromRaw(raw: Record<string, unknown>): Order {
  return {
    orderId: Number(raw.order_id),
    invoiceId: Number(raw.invoice_id),
    owner: raw.owner as string,
    token: raw.token as string,
    kind: enumTagFromNative<ListingKind>(raw.kind),
    side: enumTagFromNative<OrderSide>(raw.side),
    price: BigInt(String(raw.price)),
    amountOrBps: BigInt(String(raw.amount_or_bps)),
    remaining: BigInt(String(raw.remaining)),
    createdAt: Number(raw.created_at),
    expiresAt: Number(raw.expires_at),
    status: enumTagFromNative<OrderStatus>(raw.status),
  };
}

/**
 * Place a resting limit order (bid or ask). Matches immediately against
 * crossing resting orders on the opposite side; any unfilled remainder
 * rests on the book. `price` is per-unit, scaled by 1e7 — not a flat total
 * like `buildListPositionTx`'s `price`. `expiresAt` of `0` means no expiry.
 */
export async function buildPlaceOrderTx(params: {
  owner: string;
  invoiceId: number;
  kind: ListingKind;
  side: OrderSide;
  amountOrBps: bigint;
  price: bigint;
  expiresAt?: number;
}): Promise<string> {
  const account = await getRpcAccount(params.owner);
  const contract = new Contract(SECONDARY_MARKET_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'place_order',
        new Address(params.owner).toScVal(),
        placeOrderRequestToScVal(params),
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

/** Cancel a resting or partially-filled order. Only the original owner may cancel. */
export async function buildCancelOrderTx(owner: string, orderId: number): Promise<string> {
  const account = await getRpcAccount(owner);
  const contract = new Contract(SECONDARY_MARKET_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'cancel_order',
        new Address(owner).toScVal(),
        nativeToScVal(orderId, { type: 'u64' }),
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
 * Permissionlessly expire a resting order past its `expiresAt`. Anyone
 * (the connected wallet, in the frontend's case) can submit this — it only
 * prunes stale book state, no funds move.
 */
export async function buildExpireOrderTx(caller: string, orderId: number): Promise<string> {
  const account = await getRpcAccount(caller);
  const contract = new Contract(SECONDARY_MARKET_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(contract.call('expire_order', nativeToScVal(orderId, { type: 'u64' })))
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }

  const prepared = StellarRpc.assembleTransaction(tx, sim).build();
  return prepared.toXDR();
}

/** Fetch a single order by ID. Returns null if not found. */
export async function getOrder(orderId: number): Promise<Order | null> {
  const sim = await simulateTx(
    SECONDARY_MARKET_CONTRACT_ID,
    'get_order',
    [nativeToScVal(orderId, { type: 'u64' })],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval);
  if (!raw) return null;
  return orderFromRaw(raw as Record<string, unknown>);
}

/** Resting bid/ask order IDs for an invoice's order book, as `{ bidIds, askIds }`. */
export async function getOrderBook(
  invoiceId: number,
  kind: ListingKind,
): Promise<{ bidIds: number[]; askIds: number[] }> {
  const sim = await simulateTx(
    SECONDARY_MARKET_CONTRACT_ID,
    'get_order_book',
    [nativeToScVal(invoiceId, { type: 'u64' }), listingKindToScVal(kind)],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as [unknown[], unknown[]];
  if (!Array.isArray(raw)) return { bidIds: [], askIds: [] };
  const [bids, asks] = raw;
  return {
    bidIds: (bids ?? []).map((id) => Number(id)),
    askIds: (asks ?? []).map((id) => Number(id)),
  };
}

/** All order IDs ever placed by `owner` (any status). */
export async function listOrdersForOwner(owner: string): Promise<number[]> {
  const sim = await simulateTx(
    SECONDARY_MARKET_CONTRACT_ID,
    'list_orders_for_owner',
    [new Address(owner).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as unknown[];
  if (!Array.isArray(raw)) return [];
  return raw.map((id) => Number(id));
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

// #1036: read-only Reflector-oracle price lookup for a single accepted token
// (same scale/precision as every other call to this contract fn — only ratios
// between two calls' results are meaningful, not the raw magnitude). Used by
// the collateral-asset selector to convert "value required in the funding
// token" into "how much of the asset I actually pick do I need to post."
export async function getAssetPrice(token: string): Promise<bigint> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'get_asset_price',
    [new Address(token).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return BigInt(String(scValToNative(result!.retval)));
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
    // Rust struct field is `pool_value` (normalized deposited/deployed
    // capital across accepted tokens), not `total_deposited`.
    totalDeposited: BigInt(raw.pool_value as string),
    totalDeployed: BigInt(raw.total_deployed as string),
    totalPaidOut: BigInt(raw.total_paid_out as string),
    totalFeeRevenue: BigInt((raw.total_fee_revenue as string | number | bigint) ?? 0),
  };
}

/**
 * #776: live on-chain token balance held by the pool contract, read
 * directly from the token contract rather than from the normalized
 * `pool_value`/`total_deployed` accounting in `getPoolTokenTotals()`.
 */
export async function getPoolBalance(token: string): Promise<bigint> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'get_pool_balance',
    [new Address(token).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return BigInt(String(scValToNative(result!.retval) ?? 0));
}

/**
 * #807: pool treasury address for protocol revenue withdrawals, or `null` when
 * the pool has not been configured with one (`TreasuryNotConfigured`).
 *
 * Only `TreasuryNotConfigured` (a simulation error) and transport failures
 * return `null`, letting the dashboard degrade gracefully (callers also
 * `.catch(() => null)`).
 */
export async function getTreasuryAddress(): Promise<string | null> {
  try {
    const sim = await simulateTx(
      POOL_CONTRACT_ID,
      'get_treasury',
      [],
      'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
    );
    if (StellarRpc.Api.isSimulationError(sim)) {
      return null;
    }
    const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
    if (!result?.retval) return null;
    return scValToNative(result.retval) as string;
  } catch {
    return null;
  }
}

/**
 * #807: unclaimed (pending) protocol fee revenue held in the pool contract for
 * a token — the amount `withdraw_revenue` is allowed to move to the treasury.
 */
export async function getProtocolRevenue(token: string): Promise<bigint> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'get_protocol_revenue',
    [new Address(token).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return BigInt(String(scValToNative(result!.retval) ?? 0));
}

/**
 * #807: live on-chain balance of `address` in the given token contract, read
 * directly from the token (used for the treasury address's balance).
 */
export async function getTokenBalanceOf(token: string, address: string): Promise<bigint> {
  const sim = await simulateTx(
    token,
    'balance',
    [new Address(address).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );

  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return BigInt(String(scValToNative(result!.retval) ?? 0));
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
  ensureSufficientNativeBalance(account);
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

/** Build a lender yield-claim transaction for a single pool token. */
export async function buildClaimYieldTx(investor: string, token: string): Promise<string> {
  const account = await getRpcAccount(investor);
  const contract = new Contract(POOL_CONTRACT_ID);
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(
      contract.call('claim_yield', new Address(investor).toScVal(), new Address(token).toScVal()),
    )
    .setTimeout(30)
    .build();
  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
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
    coFundingRoundId:
      r.co_funding_round_id !== undefined && r.co_funding_round_id !== null
        ? Number(r.co_funding_round_id)
        : undefined,
  };
}

// ---- #860: multi-investor co-funding rounds ----
//
// `open_co_funding` takes a single OpenCoFundingRequest struct rather than
// individual scalar params. Soroban encodes named-field #[contracttype]
// structs as an ScMap keyed by field-name Symbols in alphabetical order —
// NOT declaration order — so the entries below are deliberately sorted
// (due_date, funding_deadline, invoice_id, max_investor_bps, min_commitment,
// sme, target_principal, token).
function openCoFundingRequestToScVal(params: {
  invoiceId: number;
  token: string;
  targetPrincipal: bigint;
  sme: string;
  dueDate: number;
  fundingDeadline: number;
  minCommitment: bigint;
  maxInvestorBps: number;
}): xdr.ScVal {
  const entry = (key: string, val: xdr.ScVal) =>
    new xdr.ScMapEntry({ key: nativeToScVal(key, { type: 'symbol' }), val });
  return xdr.ScVal.scvMap([
    entry('due_date', nativeToScVal(params.dueDate, { type: 'u64' })),
    entry('funding_deadline', nativeToScVal(params.fundingDeadline, { type: 'u64' })),
    entry('invoice_id', nativeToScVal(params.invoiceId, { type: 'u64' })),
    entry('max_investor_bps', nativeToScVal(params.maxInvestorBps, { type: 'u32' })),
    entry('min_commitment', nativeToScVal(params.minCommitment, { type: 'i128' })),
    entry('sme', new Address(params.sme).toScVal()),
    entry('target_principal', nativeToScVal(params.targetPrincipal, { type: 'i128' })),
    entry('token', new Address(params.token).toScVal()),
  ]);
}

function coFundingRoundFromScVal(raw: Record<string, unknown>): CoFundingRound {
  return {
    invoiceId: Number(raw.invoice_id),
    token: raw.token as string,
    sme: raw.sme as string,
    dueDate: Number(raw.due_date),
    targetPrincipal: BigInt(String(raw.target_principal)),
    committedPrincipal: BigInt(String(raw.committed_principal)),
    fundingDeadline: Number(raw.funding_deadline),
    status: raw.status as CoFundingRound['status'],
    minCommitment: BigInt(String(raw.min_commitment)),
    maxInvestorBps: Number(raw.max_investor_bps),
    participants: (raw.participants as string[]) ?? [],
  };
}

export async function buildOpenCoFundingTx(params: {
  admin: string;
  invoiceId: number;
  token: string;
  targetPrincipal: bigint;
  sme: string;
  dueDate: number;
  fundingDeadline: number;
  minCommitment: bigint;
  maxInvestorBps: number;
}): Promise<string> {
  const account = await getRpcAccount(params.admin);
  const contract = new Contract(POOL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'open_co_funding',
        new Address(params.admin).toScVal(),
        openCoFundingRequestToScVal(params),
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
  ensureSufficientNativeBalance(account);
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

export async function buildFinalizeCoFundingTx(params: {
  caller: string;
  invoiceId: number;
}): Promise<string> {
  const account = await getRpcAccount(params.caller);
  const contract = new Contract(POOL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'finalize_co_funding',
        new Address(params.caller).toScVal(),
        nativeToScVal(params.invoiceId, { type: 'u64' }),
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

export async function buildWithdrawCoFundingCommitmentTx(params: {
  investor: string;
  invoiceId: number;
}): Promise<string> {
  const account = await getRpcAccount(params.investor);
  const contract = new Contract(POOL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'withdraw_co_funding_commitment',
        new Address(params.investor).toScVal(),
        nativeToScVal(params.invoiceId, { type: 'u64' }),
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

export async function buildCancelCoFundingRoundTx(params: {
  admin: string;
  invoiceId: number;
}): Promise<string> {
  const account = await getRpcAccount(params.admin);
  const contract = new Contract(POOL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'cancel_co_funding_round',
        new Address(params.admin).toScVal(),
        nativeToScVal(params.invoiceId, { type: 'u64' }),
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

export async function buildTransferCoFundShareTx(params: {
  from: string;
  invoiceId: number;
  token: string;
  to: string;
  bps: number;
}): Promise<string> {
  const account = await getRpcAccount(params.from);
  const contract = new Contract(POOL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'transfer_co_fund_share',
        new Address(params.from).toScVal(),
        nativeToScVal(params.invoiceId, { type: 'u64' }),
        new Address(params.token).toScVal(),
        new Address(params.to).toScVal(),
        nativeToScVal(params.bps, { type: 'u32' }),
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

export async function getCoFundingRound(invoiceId: number): Promise<CoFundingRound | null> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'get_co_funding_round',
    [nativeToScVal(invoiceId, { type: 'u64' })],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval);
  if (!raw) return null;
  return coFundingRoundFromScVal(raw as Record<string, unknown>);
}

export async function listCoFundingRounds(): Promise<number[]> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'list_co_funding_rounds',
    [],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as unknown[];
  return (raw ?? []).map((id) => Number(id));
}

export async function getInvestorCoFundPositions(
  investor: string,
): Promise<Array<{ invoiceId: number; bps: number }>> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'get_investor_co_fund_positions',
    [new Address(investor).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as [number | string | bigint, number][];
  return (raw ?? []).map(([invoiceId, bps]) => ({ invoiceId: Number(invoiceId), bps }));
}

export async function getCoFundShare(invoiceId: number, investor: string): Promise<number> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'get_co_fund_share',
    [nativeToScVal(invoiceId, { type: 'u64' }), new Address(investor).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return Number(scValToNative(result!.retval));
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

export async function buildRequestWithdrawalTx(
  investor: string,
  token: string,
  shares: bigint,
): Promise<string> {
  const account = await getRpcAccount(investor);
  const contract = new Contract(POOL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'request_withdrawal',
        new Address(investor).toScVal(),
        new Address(token).toScVal(),
        nativeToScVal(shares, { type: 'i128' }),
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

export async function buildCancelWithdrawalRequestTx(
  investor: string,
  token: string,
): Promise<string> {
  const account = await getRpcAccount(investor);
  const contract = new Contract(POOL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'cancel_withdrawal_request',
        new Address(investor).toScVal(),
        new Address(token).toScVal(),
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

/** Permissionless: anyone can trigger a drain attempt against current liquidity. */
export async function buildDrainWithdrawalQueueTx(caller: string, token: string): Promise<string> {
  const account = await getRpcAccount(caller);
  const contract = new Contract(POOL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'drain_withdrawal_queue',
        new Address(caller).toScVal(),
        new Address(token).toScVal(),
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

// ── #863: utilization-driven kinked interest-rate model ─────────────────────

// Soroban encodes named-field structs as an ScMap keyed by field-name Symbols
// in alphabetical order: (base_rate_bps, max_rate_bps,
// optimal_utilization_bps, slope1_bps, slope2_bps).
function rateModelConfigToScVal(config: RateModelConfig): xdr.ScVal {
  const entry = (key: string, val: xdr.ScVal) =>
    new xdr.ScMapEntry({ key: nativeToScVal(key, { type: 'symbol' }), val });
  return xdr.ScVal.scvMap([
    entry('base_rate_bps', nativeToScVal(config.baseRateBps, { type: 'u32' })),
    entry('max_rate_bps', nativeToScVal(config.maxRateBps, { type: 'u32' })),
    entry('optimal_utilization_bps', nativeToScVal(config.optimalUtilizationBps, { type: 'u32' })),
    entry('slope1_bps', nativeToScVal(config.slope1Bps, { type: 'u32' })),
    entry('slope2_bps', nativeToScVal(config.slope2Bps, { type: 'u32' })),
  ]);
}

function rateModelConfigFromRaw(raw: Record<string, unknown>): RateModelConfig {
  return {
    baseRateBps: Number(raw.base_rate_bps ?? 0),
    optimalUtilizationBps: Number(raw.optimal_utilization_bps ?? 0),
    slope1Bps: Number(raw.slope1_bps ?? 0),
    slope2Bps: Number(raw.slope2_bps ?? 0),
    maxRateBps: Number(raw.max_rate_bps ?? 0),
  };
}

/** Rate (bps) a new funding for `token` would lock right now, or null when
 * the token has no rate model configured (caller falls back to the static
 * `PoolConfig.yieldBps`). */
export async function getCurrentRate(token: string): Promise<number | null> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'get_current_rate',
    [new Address(token).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  if (StellarRpc.Api.isSimulationError(sim)) {
    // RateModelNotConfigured (80) — treated as "no curve" by callers.
    return null;
  }
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return Number(scValToNative(result!.retval));
}

/** The token's curve parameters, or null when no rate model is configured. */
export async function getRateModelConfig(token: string): Promise<RateModelConfig | null> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'get_rate_model_config',
    [new Address(token).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  if (StellarRpc.Api.isSimulationError(sim)) return null;
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval);
  if (!raw) return null;
  return rateModelConfigFromRaw(raw as Record<string, unknown>);
}

/** Up to `limit` most recent rate samples, chronological (oldest-first). */
export async function getRateHistory(token: string, limit: number): Promise<RateSnapshot[]> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'get_rate_history',
    [new Address(token).toScVal(), nativeToScVal(limit, { type: 'u32' })],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  if (StellarRpc.Api.isSimulationError(sim)) return [];
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as Record<string, unknown>[];
  if (!Array.isArray(raw)) return [];
  return raw.map((r) => ({
    timestamp: Number(r.timestamp ?? 0),
    utilizationBps: Number(r.utilization_bps ?? 0),
    rateBps: Number(r.rate_bps ?? 0),
  }));
}

/** What the rate would be at a hypothetical utilization, or null when the
 * token has no rate model configured. */
export async function previewRateAtUtilization(
  token: string,
  utilizationBps: number,
): Promise<number | null> {
  const sim = await simulateTx(
    POOL_CONTRACT_ID,
    'preview_rate_at_utilization',
    [new Address(token).toScVal(), nativeToScVal(utilizationBps, { type: 'u32' })],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  if (StellarRpc.Api.isSimulationError(sim)) return null;
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return Number(scValToNative(result!.retval));
}

/** Admin: propose new curve parameters; executable after the yield timelock. */
export async function buildProposeRateModelTx(
  admin: string,
  token: string,
  config: RateModelConfig,
): Promise<string> {
  const account = await getRpcAccount(admin);
  const contract = new Contract(POOL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'propose_rate_model_change',
        new Address(admin).toScVal(),
        new Address(token).toScVal(),
        rateModelConfigToScVal(config),
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

/** Anyone: execute a rate-model proposal once its timelock has elapsed. */
export async function buildExecuteRateModelTx(caller: string, token: string): Promise<string> {
  const account = await getRpcAccount(caller);
  const contract = new Contract(POOL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(contract.call('execute_rate_model_change', new Address(token).toScVal()))
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }

  const prepared = StellarRpc.assembleTransaction(tx, sim).build();
  return prepared.toXDR();
}

/** Admin: cancel a pending rate-model proposal. */
export async function buildCancelRateModelTx(admin: string, token: string): Promise<string> {
  const account = await getRpcAccount(admin);
  const contract = new Contract(POOL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'cancel_rate_model_change',
        new Address(admin).toScVal(),
        new Address(token).toScVal(),
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

/** #775: owner-only opt in/out of the public invoice sharing link. */
export async function buildSetInvoicePrivateTx(params: {
  owner: string;
  invoiceId: number;
  private: boolean;
}): Promise<string> {
  const account = await getRpcAccount(params.owner);
  const contract = new Contract(INVOICE_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'set_invoice_private',
        nativeToScVal(params.invoiceId, { type: 'u64' }),
        new Address(params.owner).toScVal(),
        nativeToScVal(params.private, { type: 'bool' }),
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
    postedAt: Number(r.posted_at),
    releasedAt: Number(r.released_at),
    seizedAt: Number(r.seized_at),
    collateralBpsAtDeposit: Number(r.collateral_bps_at_deposit),
    thresholdAtDeposit: BigInt(String(r.threshold_at_deposit)),
  };
}

// #1036: read-only — the ledger timestamp a position was first flagged
// at-risk, or null if not currently flagged. Tracked entirely in the auction
// satellite's own storage — it's monitoring state, not fund-movement state,
// so pool's CollateralDeposit doesn't carry it.
export async function getAtRiskSince(invoiceId: number): Promise<number | null> {
  const sim = await simulateTx(
    AUCTION_CONTRACT_ID,
    'get_at_risk_since',
    [nativeToScVal(invoiceId, { type: 'u64' })],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval);
  return raw != null ? Number(raw) : null;
}

// #1036: read-only — the live, oracle-priced collateral ratio (bps) for a
// funded invoice's posted collateral. 10_000 = exactly covers the requirement
// at today's prices. Requires the invoice to already be funded. Served by the
// auction satellite contract now, not pool directly — see
// contracts/auction's collateral-risk-response entrypoints.
export async function getLiveCollateralRatio(invoiceId: number): Promise<number> {
  const sim = await simulateTx(
    AUCTION_CONTRACT_ID,
    'get_live_collateral_ratio',
    [nativeToScVal(invoiceId, { type: 'u64' })],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return Number(scValToNative(result!.retval));
}

export async function getCollateralRiskConfig(): Promise<CollateralRiskConfig> {
  const sim = await simulateTx(
    AUCTION_CONTRACT_ID,
    'get_collateral_risk_config',
    [],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as Record<string, unknown>;
  return {
    dangerBps: Number(raw.danger_bps),
    gracePeriodSecs: Number(raw.grace_period_secs),
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

export async function buildTopUpCollateralTx(params: {
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
        'top_up_collateral',
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

// #1036: permissionless keeper call — recomputes the live ratio and flips the
// at-risk flag, tracked entirely in the auction satellite's own storage.
// `caller` is an explicit argument (not just the fee-paying source account)
// since it's re-authorized as the transaction signer, not implied by the
// source account.
export async function buildCheckCollateralRiskTx(params: {
  invoiceId: number;
  caller: string;
}): Promise<string> {
  const account = await getRpcAccount(params.caller);
  const contract = new Contract(AUCTION_CONTRACT_ID);
  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'check_collateral_risk',
        new Address(params.caller).toScVal(),
        nativeToScVal(params.invoiceId, { type: 'u64' }),
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

// #1036: permissionless keeper call — seizes a deposit that's been at-risk
// for at least the configured grace period and is still below the danger
// threshold on a fresh price recheck. Routed through the auction satellite
// contract (see buildCheckCollateralRiskTx for why `caller` is explicit).
export async function buildLiquidateCollateralTx(params: {
  invoiceId: number;
  caller: string;
}): Promise<string> {
  const account = await getRpcAccount(params.caller);
  const contract = new Contract(AUCTION_CONTRACT_ID);
  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'liquidate_collateral',
        new Address(params.caller).toScVal(),
        nativeToScVal(params.invoiceId, { type: 'u64' }),
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

export async function getCreditScoreStatus(sme: string): Promise<{
  isStale: boolean;
  score: number;
  finalScore: number;
  riskAdjustmentPts: number;
  trendAdjustmentPts: number;
} | null> {
  if (!CREDIT_SCORE_CONTRACT_ID) return null;
  try {
    const sim = await simulateTx(
      CREDIT_SCORE_CONTRACT_ID,
      'get_credit_score',
      [new Address(sme).toScVal()],
      sme,
    );
    const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
    const data = scValToNative(result!.retval) as {
      score: number;
      is_stale: boolean;
      final_score?: number;
      risk_adjustment_pts?: number;
      trend_adjustment_pts?: number;
    };
    return {
      isStale: Boolean(data.is_stale),
      score: Number(data.score),
      // #1041: fall back to `score` for a contract predating these fields.
      finalScore: Number(data.final_score ?? data.score),
      riskAdjustmentPts: Number(data.risk_adjustment_pts ?? 0),
      trendAdjustmentPts: Number(data.trend_adjustment_pts ?? 0),
    };
  } catch {
    return null;
  }
}

// #868: credit_score v2 — external attestations + dispute mechanism.
// Soroban encodes a unit-variant Rust enum (no associated data, e.g.
// `AttestorType`/`AttestationStatus`) as a one-element ScVec containing the
// variant name as an ScSymbol. Raw `scValToNative` (unlike the generated
// contract Client, which has the full spec) decodes that vec to a
// one-element JS array rather than a bare string, so reads are unwrapped
// defensively and writes are built by hand.
function attestorTypeToScVal(variant: AttestorType): xdr.ScVal {
  return xdr.ScVal.scvVec([nativeToScVal(variant, { type: 'symbol' })]);
}

function enumTagFromNative<T extends string>(raw: unknown): T {
  return (Array.isArray(raw) ? raw[0] : raw) as T;
}

function attestationFromScVal(raw: Record<string, unknown>): Attestation {
  return {
    id: Number(raw.id),
    sme: raw.sme as StellarAddress,
    attestor: raw.attestor as StellarAddress,
    attestationType: enumTagFromNative(raw.attestation_type),
    scoreContribution: Number(raw.score_contribution),
    evidenceHash: raw.evidence_hash as string,
    submittedAt: Number(raw.submitted_at),
    expiresAt: Number(raw.expires_at),
    status: enumTagFromNative(raw.status),
  };
}

function attestorInfoFromScVal(raw: Record<string, unknown>): AttestorInfo {
  return {
    address: raw.address as StellarAddress,
    attestorType: enumTagFromNative(raw.attestor_type),
    isActive: Boolean(raw.is_active),
    weightBps: Number(raw.weight_bps),
    registeredAt: Number(raw.registered_at),
  };
}

function fullCreditScoreFromScVal(raw: Record<string, unknown>): FullCreditScore {
  return {
    sme: raw.sme as StellarAddress,
    score: Number(raw.score),
    totalInvoices: Number(raw.total_invoices),
    paidOnTime: Number(raw.paid_on_time),
    paidLate: Number(raw.paid_late),
    defaulted: Number(raw.defaulted),
    totalVolume: BigInt(String(raw.total_volume)),
    averagePaymentDays: Number(raw.average_payment_days),
    lastUpdated: Number(raw.last_updated),
    scoreVersion: Number(raw.score_version),
    configVersion: Number(raw.config_version),
    isStale: Boolean(raw.is_stale),
    blendedScore: Number(raw.blended_score),
    riskAdjustmentPts: Number(raw.risk_adjustment_pts),
    trendAdjustmentPts: Number(raw.trend_adjustment_pts),
    finalScore: Number(raw.final_score),
  };
}

export async function getFullCreditScore(sme: string): Promise<FullCreditScore | null> {
  if (!CREDIT_SCORE_CONTRACT_ID) return null;
  try {
    const sim = await simulateTx(
      CREDIT_SCORE_CONTRACT_ID,
      'get_credit_score',
      [new Address(sme).toScVal()],
      sme,
    );
    const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
    return fullCreditScoreFromScVal(scValToNative(result!.retval) as Record<string, unknown>);
  } catch {
    return null;
  }
}

export async function getAttestation(id: number): Promise<Attestation | null> {
  const sim = await simulateTx(
    CREDIT_SCORE_CONTRACT_ID,
    'get_attestation',
    [nativeToScVal(id, { type: 'u64' })],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval);
  if (!raw) return null;
  return attestationFromScVal(raw as Record<string, unknown>);
}

export async function listSmeAttestations(sme: string): Promise<Attestation[]> {
  const sim = await simulateTx(
    CREDIT_SCORE_CONTRACT_ID,
    'list_sme_attestations',
    [new Address(sme).toScVal()],
    sme,
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as Record<string, unknown>[];
  return (raw ?? []).map(attestationFromScVal);
}

export async function getAttestorInfo(address: string): Promise<AttestorInfo | null> {
  const sim = await simulateTx(
    CREDIT_SCORE_CONTRACT_ID,
    'get_attestor_info',
    [new Address(address).toScVal()],
    address,
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval);
  if (!raw) return null;
  return attestorInfoFromScVal(raw as Record<string, unknown>);
}

export async function listActiveAttestors(): Promise<AttestorInfo[]> {
  const sim = await simulateTx(
    CREDIT_SCORE_CONTRACT_ID,
    'list_active_attestors',
    [],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as Record<string, unknown>[];
  return (raw ?? []).map(attestorInfoFromScVal);
}

export async function simulateScoreWithAttestations(
  sme: string,
  hypothetical: Array<{ weightBps: number; scoreContribution: number }>,
): Promise<number> {
  const hypotheticalScVal = xdr.ScVal.scvVec(
    hypothetical.map((h) =>
      xdr.ScVal.scvVec([
        nativeToScVal(h.weightBps, { type: 'u32' }),
        nativeToScVal(h.scoreContribution, { type: 'u32' }),
      ]),
    ),
  );
  const sim = await simulateTx(
    CREDIT_SCORE_CONTRACT_ID,
    'simulate_score_with_attestations',
    [new Address(sme).toScVal(), hypotheticalScVal],
    sme,
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return Number(scValToNative(result!.retval));
}

export async function buildRegisterAttestorTx(params: {
  admin: string;
  address: string;
  attestorType: AttestorType;
  weightBps: number;
}): Promise<string> {
  const account = await getRpcAccount(params.admin);
  const contract = new Contract(CREDIT_SCORE_CONTRACT_ID);

  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(
      contract.call(
        'register_attestor',
        new Address(params.admin).toScVal(),
        new Address(params.address).toScVal(),
        attestorTypeToScVal(params.attestorType),
        nativeToScVal(params.weightBps, { type: 'u32' }),
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

export async function buildDeactivateAttestorTx(params: {
  admin: string;
  address: string;
}): Promise<string> {
  const account = await getRpcAccount(params.admin);
  const contract = new Contract(CREDIT_SCORE_CONTRACT_ID);

  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(
      contract.call(
        'deactivate_attestor',
        new Address(params.admin).toScVal(),
        new Address(params.address).toScVal(),
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

export async function buildSubmitAttestationTx(params: {
  attestor: string;
  sme: string;
  attestationType: AttestorType;
  scoreContribution: number;
  evidenceHash: string;
  expiresAt: number;
}): Promise<string> {
  const account = await getRpcAccount(params.attestor);
  const contract = new Contract(CREDIT_SCORE_CONTRACT_ID);

  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(
      contract.call(
        'submit_attestation',
        new Address(params.attestor).toScVal(),
        new Address(params.sme).toScVal(),
        attestorTypeToScVal(params.attestationType),
        nativeToScVal(params.scoreContribution, { type: 'u32' }),
        nativeToScVal(params.evidenceHash, { type: 'string' }),
        nativeToScVal(params.expiresAt, { type: 'u64' }),
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

export async function buildDisputeAttestationTx(params: {
  caller: string;
  attestationId: number;
  reasonHash: string;
}): Promise<string> {
  const account = await getRpcAccount(params.caller);
  const contract = new Contract(CREDIT_SCORE_CONTRACT_ID);

  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(
      contract.call(
        'dispute_attestation',
        new Address(params.caller).toScVal(),
        nativeToScVal(params.attestationId, { type: 'u64' }),
        nativeToScVal(params.reasonHash, { type: 'string' }),
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

export async function buildResolveAttestationDisputeTx(params: {
  admin: string;
  attestationId: number;
  upheld: boolean;
}): Promise<string> {
  const account = await getRpcAccount(params.admin);
  const contract = new Contract(CREDIT_SCORE_CONTRACT_ID);

  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(
      contract.call(
        'resolve_attestation_dispute',
        new Address(params.admin).toScVal(),
        nativeToScVal(params.attestationId, { type: 'u64' }),
        nativeToScVal(params.upheld, { type: 'bool' }),
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

// ---- #1043: structured multi-party dispute arbitration ----
// ARBITRATION_CONTRACT_ID is optional — unset until the contract is deployed,
// mirroring how ORACLE_REGISTRY_CONTRACT_ID is handled above. `raise_dispute`
// itself lives on the invoice contract (it's what routes an above-threshold
// dispute into arbitration); everything else here talks to the arbitration
// contract directly.
const ARBITRATION_SIM_SOURCE = 'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN';

/** Raises a dispute on a defaulted invoice. `respondent` represents the
 * debtor's side of the case for arbitration purposes (see #1043's PR
 * description for why this can't yet be the debtor's own address). */
export async function buildRaiseDisputeTx(params: {
  borrower: StellarAddress;
  invoiceId: number;
  evidenceHash: string;
  respondent: StellarAddress;
}): Promise<string> {
  const account = await getRpcAccount(params.borrower);
  const contract = new Contract(INVOICE_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'raise_dispute',
        nativeToScVal(params.invoiceId, { type: 'u64' }),
        new Address(params.borrower).toScVal(),
        nativeToScVal(params.evidenceHash, { type: 'string' }),
        new Address(params.respondent).toScVal(),
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

export async function getDispute(invoiceId: number): Promise<DisputeRecord | null> {
  const sim = await simulateTx(
    INVOICE_CONTRACT_ID,
    'get_dispute',
    [nativeToScVal(invoiceId, { type: 'u64' })],
    ARBITRATION_SIM_SOURCE,
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval);
  if (!raw) return null;
  const r = raw as Record<string, unknown>;
  return {
    evidenceHash: r.evidence_hash as string,
    filedAt: Number(r.filed_at),
    resolvedAt: Number(r.resolved_at),
    outcome: r.outcome as DisputeResolution,
  };
}

function decodeArbitrationCase(raw: Record<string, unknown>): DisputeCase {
  return {
    id: Number(raw.id),
    invoiceId: Number(raw.invoice_id),
    claimant: raw.claimant as StellarAddress,
    respondent: raw.respondent as StellarAddress,
    amount: BigInt(String(raw.amount)),
    openedAt: Number(raw.opened_at),
    evidenceDeadline: Number(raw.evidence_deadline),
    commitDeadline: Number(raw.commit_deadline),
    revealDeadline: Number(raw.reveal_deadline),
    jurors: (raw.jurors as StellarAddress[]) ?? [],
    status: raw.status as DisputeCase['status'],
    resolution: raw.resolution as DisputeResolution,
    retryCount: Number(raw.retry_count),
  };
}

export async function getArbitrationCase(caseId: number): Promise<DisputeCase | null> {
  const sim = await simulateTx(
    ARBITRATION_CONTRACT_ID,
    'get_case',
    [nativeToScVal(caseId, { type: 'u64' })],
    ARBITRATION_SIM_SOURCE,
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval);
  if (!raw) return null;
  return decodeArbitrationCase(raw as Record<string, unknown>);
}

/** Looks up the most recently opened case for an invoice — for callers that
 * only know "this invoice is Disputed" and need to find the case to show. */
export async function getArbitrationCaseByInvoice(invoiceId: number): Promise<DisputeCase | null> {
  const sim = await simulateTx(
    ARBITRATION_CONTRACT_ID,
    'get_case_by_invoice',
    [nativeToScVal(invoiceId, { type: 'u64' })],
    ARBITRATION_SIM_SOURCE,
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval);
  if (!raw) return null;
  return decodeArbitrationCase(raw as Record<string, unknown>);
}

export async function getArbitrationEvidence(caseId: number): Promise<EvidenceEntry[]> {
  const sim = await simulateTx(
    ARBITRATION_CONTRACT_ID,
    'get_evidence',
    [nativeToScVal(caseId, { type: 'u64' })],
    ARBITRATION_SIM_SOURCE,
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = (scValToNative(result!.retval) as Record<string, unknown>[]) ?? [];
  return raw.map((e) => ({
    submitter: e.submitter as StellarAddress,
    party: e.party as EvidenceEntry['party'],
    evidenceHash: e.evidence_hash as string,
    submittedAt: Number(e.submitted_at),
  }));
}

export async function getJurorInfo(operator: string): Promise<JurorInfo | null> {
  const sim = await simulateTx(
    ARBITRATION_CONTRACT_ID,
    'get_juror',
    [new Address(operator).toScVal()],
    ARBITRATION_SIM_SOURCE,
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
    casesServed: Number(info.cases_served),
    timesSlashed: Number(info.times_slashed),
    nonRevealStrikes: Number(info.non_reveal_strikes),
    registeredAt: Number(info.registered_at),
    deregisterRequestedAt:
      info.deregister_requested_at !== undefined && info.deregister_requested_at !== null
        ? Number(info.deregister_requested_at)
        : null,
  };
}

export async function listActiveJurors(): Promise<StellarAddress[]> {
  const sim = await simulateTx(
    ARBITRATION_CONTRACT_ID,
    'list_active_jurors',
    [],
    ARBITRATION_SIM_SOURCE,
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return ((scValToNative(result!.retval) as unknown[]) ?? []) as StellarAddress[];
}

export async function getJurorCases(operator: string): Promise<number[]> {
  const sim = await simulateTx(
    ARBITRATION_CONTRACT_ID,
    'get_juror_cases',
    [new Address(operator).toScVal()],
    ARBITRATION_SIM_SOURCE,
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = (scValToNative(result!.retval) as unknown[]) ?? [];
  return raw.map((v) => Number(v));
}

export async function getJurorVoteStatus(
  caseId: number,
  juror: string,
): Promise<JurorVoteStatus | null> {
  const sim = await simulateTx(
    ARBITRATION_CONTRACT_ID,
    'get_vote',
    [nativeToScVal(caseId, { type: 'u64' }), new Address(juror).toScVal()],
    ARBITRATION_SIM_SOURCE,
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval);
  if (!raw) return null;
  const v = raw as Record<string, unknown>;
  return {
    hasCommitted: v.commit_hash !== undefined && v.commit_hash !== null,
    revealedVote:
      v.revealed_vote !== undefined && v.revealed_vote !== null ? Boolean(v.revealed_vote) : null,
  };
}

export async function getArbitrationConfig(): Promise<ArbitrationConfig> {
  const sim = await simulateTx(ARBITRATION_CONTRACT_ID, 'get_config', [], ARBITRATION_SIM_SOURCE);
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as Record<string, unknown>;
  return {
    minStake: BigInt(String(raw.min_stake)),
    stakeToken: raw.stake_token as string,
    committeeSize: Number(raw.committee_size),
    quorumFloor: Number(raw.quorum_floor),
    evidenceWindowSecs: Number(raw.evidence_window_secs),
    commitWindowSecs: Number(raw.commit_window_secs),
    revealWindowSecs: Number(raw.reveal_window_secs),
    deregisterCooldownSecs: Number(raw.deregister_cooldown_secs),
    slashBps: Number(raw.slash_bps),
    nonRevealSlashBps: Number(raw.non_reveal_slash_bps),
    lopsidedConfidenceBps: Number(raw.lopsided_confidence_bps),
    maxRetries: Number(raw.max_retries),
  };
}

export async function buildSubmitEvidenceTx(params: {
  submitter: StellarAddress;
  caseId: number;
  evidenceHash: string;
}): Promise<string> {
  const account = await getRpcAccount(params.submitter);
  const contract = new Contract(ARBITRATION_CONTRACT_ID);
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(
      contract.call(
        'submit_evidence',
        nativeToScVal(params.caseId, { type: 'u64' }),
        new Address(params.submitter).toScVal(),
        nativeToScVal(params.evidenceHash, { type: 'string' }),
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

export async function buildRegisterJurorTx(params: {
  operator: StellarAddress;
  stakeAmount: bigint;
}): Promise<string> {
  const account = await getRpcAccount(params.operator);
  const contract = new Contract(ARBITRATION_CONTRACT_ID);
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(
      contract.call(
        'register_juror',
        new Address(params.operator).toScVal(),
        nativeToScVal(params.stakeAmount, { type: 'i128' }),
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

export async function buildDeregisterJurorTx(params: {
  operator: StellarAddress;
}): Promise<string> {
  const account = await getRpcAccount(params.operator);
  const contract = new Contract(ARBITRATION_CONTRACT_ID);
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(contract.call('deregister_juror', new Address(params.operator).toScVal()))
    .setTimeout(30)
    .build();
  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

/** Permissionless tick — `caller` just pays the fee, no special role required. */
export async function buildSelectJurorsTx(params: {
  caller: StellarAddress;
  caseId: number;
}): Promise<string> {
  const account = await getRpcAccount(params.caller);
  const contract = new Contract(ARBITRATION_CONTRACT_ID);
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(contract.call('select_jurors', nativeToScVal(params.caseId, { type: 'u64' })))
    .setTimeout(30)
    .build();
  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

export async function buildCommitVoteTx(params: {
  juror: StellarAddress;
  caseId: number;
  commitHash: Uint8Array;
}): Promise<string> {
  const account = await getRpcAccount(params.juror);
  const contract = new Contract(ARBITRATION_CONTRACT_ID);
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(
      contract.call(
        'commit_vote',
        nativeToScVal(params.caseId, { type: 'u64' }),
        new Address(params.juror).toScVal(),
        nativeToScVal(Buffer.from(params.commitHash), { type: 'bytes' }),
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

export async function buildRevealVoteTx(params: {
  juror: StellarAddress;
  caseId: number;
  voteChoice: boolean;
  salt: Uint8Array;
}): Promise<string> {
  const account = await getRpcAccount(params.juror);
  const contract = new Contract(ARBITRATION_CONTRACT_ID);
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(
      contract.call(
        'reveal_vote',
        nativeToScVal(params.caseId, { type: 'u64' }),
        new Address(params.juror).toScVal(),
        nativeToScVal(params.voteChoice, { type: 'bool' }),
        nativeToScVal(Buffer.from(params.salt), { type: 'bytes' }),
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

/** Permissionless tick, same caller-just-pays-fees shape as `buildSelectJurorsTx`. */
export async function buildFinalizeCaseTx(params: {
  caller: StellarAddress;
  caseId: number;
}): Promise<string> {
  const account = await getRpcAccount(params.caller);
  const contract = new Contract(ARBITRATION_CONTRACT_ID);
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(contract.call('finalize_case', nativeToScVal(params.caseId, { type: 'u64' })))
    .setTimeout(30)
    .build();
  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

export async function buildAdminResolveNoQuorumTx(params: {
  admin: StellarAddress;
  caseId: number;
  resolution: 'InFavorOfSME' | 'InFavorOfDebtor';
}): Promise<string> {
  const account = await getRpcAccount(params.admin);
  const contract = new Contract(ARBITRATION_CONTRACT_ID);
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(
      contract.call(
        'admin_resolve_no_quorum',
        new Address(params.admin).toScVal(),
        nativeToScVal(params.caseId, { type: 'u64' }),
        xdr.ScVal.scvVec([nativeToScVal(params.resolution, { type: 'symbol' })]),
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

/** Client-side commit-reveal helpers — `vote=true` means "in favor of the
 * debtor" (mirrors arbitration::DisputeResolution::InFavorOfDebtor). */
export async function computeArbitrationCommitHash(
  vote: boolean,
  salt: Uint8Array,
): Promise<Uint8Array> {
  const preimage = new Uint8Array(1 + salt.length);
  preimage[0] = vote ? 1 : 0;
  preimage.set(salt, 1);
  const digest = await crypto.subtle.digest('SHA-256', preimage);
  return new Uint8Array(digest);
}

export function generateArbitrationSalt(): Uint8Array {
  const salt = new Uint8Array(32);
  crypto.getRandomValues(salt);
  return salt;
}

// ---- #867: Compliance registry ----

export type ComplianceStatusUi = 'Unscreened' | 'Cleared' | 'Flagged' | 'Blocked' | 'PendingReview';

export type RiskTierUi = 'Low' | 'Medium' | 'High';

export interface ComplianceRecordUi {
  address: string;
  status: ComplianceStatusUi;
  reasonCode: number;
  riskTier: RiskTierUi;
  screenedAt: number;
  screenedBy: string;
  expiresAt: number;
  notesHash: string;
}

function requireComplianceContractId(): string {
  if (!COMPLIANCE_CONTRACT_ID) {
    throw new Error('NEXT_PUBLIC_COMPLIANCE_CONTRACT_ID is not configured');
  }
  return COMPLIANCE_CONTRACT_ID;
}

function unitEnumToScVal(variant: string): xdr.ScVal {
  return xdr.ScVal.scvVec([nativeToScVal(variant, { type: 'symbol' })]);
}

export async function getComplianceIsCleared(address: StellarAddress): Promise<boolean> {
  const sim = await simulateTx(
    requireComplianceContractId(),
    'is_cleared',
    [new Address(address).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return Boolean(scValToNative(result!.retval));
}

export async function getComplianceRecord(
  address: StellarAddress,
): Promise<ComplianceRecordUi | null> {
  const sim = await simulateTx(
    requireComplianceContractId(),
    'get_compliance_record',
    [new Address(address).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as Record<string, unknown> | null;
  if (!raw) return null;
  return {
    address: String(raw.address ?? address),
    status: (raw.status as ComplianceStatusUi) ?? 'Unscreened',
    reasonCode: Number(raw.reason_code ?? 0),
    riskTier: (raw.risk_tier as RiskTierUi) ?? 'Low',
    screenedAt: Number(raw.screened_at ?? 0),
    screenedBy: String(raw.screened_by ?? ''),
    expiresAt: Number(raw.expires_at ?? 0),
    notesHash: String(raw.notes_hash ?? ''),
  };
}

export async function getComplianceHistory(address: StellarAddress): Promise<ComplianceRecordUi[]> {
  const sim = await simulateTx(
    requireComplianceContractId(),
    'get_screening_history',
    [new Address(address).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = (scValToNative(result!.retval) as Record<string, unknown>[]) ?? [];
  return raw.map((e) => ({
    address,
    status: (e.status as ComplianceStatusUi) ?? 'Unscreened',
    reasonCode: Number(e.reason_code ?? 0),
    riskTier: (e.risk_tier as RiskTierUi) ?? 'Low',
    screenedAt: Number(e.screened_at ?? 0),
    screenedBy: String(e.screened_by ?? ''),
    expiresAt: Number(e.expires_at ?? 0),
    notesHash: String(e.notes_hash ?? ''),
  }));
}

export async function listComplianceFlagged(): Promise<string[]> {
  const sim = await simulateTx(
    requireComplianceContractId(),
    'list_flagged',
    [],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return (scValToNative(result!.retval) as string[]) ?? [];
}

export async function listCompliancePendingReview(): Promise<string[]> {
  const sim = await simulateTx(
    requireComplianceContractId(),
    'list_pending_review',
    [],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return (scValToNative(result!.retval) as string[]) ?? [];
}

export async function buildSubmitScreeningResultTx(params: {
  screener: StellarAddress;
  address: StellarAddress;
  status: ComplianceStatusUi;
  reasonCode: number;
  riskTier: RiskTierUi;
  expiresAt: number;
  notesHash: string;
}): Promise<string> {
  const account = await getRpcAccount(params.screener);
  const contract = new Contract(requireComplianceContractId());

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call(
        'submit_screening_result',
        new Address(params.screener).toScVal(),
        new Address(params.address).toScVal(),
        unitEnumToScVal(params.status),
        nativeToScVal(params.reasonCode, { type: 'u32' }),
        unitEnumToScVal(params.riskTier),
        nativeToScVal(params.expiresAt, { type: 'u64' }),
        nativeToScVal(params.notesHash, { type: 'string' }),
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

export async function fetchComplianceServiceFlags(): Promise<
  Array<{ id: string; address: string; reason: string; at: string; pattern: string }>
> {
  const base = process.env.NEXT_PUBLIC_COMPLIANCE_SERVICE_URL ?? 'http://localhost:8081';
  const token = process.env.NEXT_PUBLIC_COMPLIANCE_ADMIN_TOKEN ?? '';
  try {
    const res = await fetch(`${base}/flags`, {
      headers: token ? { 'x-admin-token': token } : {},
      cache: 'no-store',
    });
    if (!res.ok) return [];
    const body = (await res.json()) as {
      alerts?: Array<{ id: string; address: string; reason: string; at: string; pattern: string }>;
    };
    return body.alerts ?? [];
  } catch {
    return [];
  }
}

// ---- #799: Referral program ----

export async function getReferrer(referee: string): Promise<string | null> {
  const sim = await simulateTx(
    REFERRAL_CONTRACT_ID,
    'get_referrer',
    [new Address(referee).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval);
  return raw ? (raw as string) : null;
}

export async function getReferralStats(referrer: string): Promise<ReferralStats> {
  const sim = await simulateTx(
    REFERRAL_CONTRACT_ID,
    'get_stats',
    [new Address(referrer).toScVal()],
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as Record<string, unknown>;
  return {
    referrer: raw.referrer as StellarAddress,
    referralCount: Number(raw.referral_count ?? 0),
  };
}

export async function buildRegisterReferralTx(referee: string, referrer: string): Promise<string> {
  const account = await getRpcAccount(referee);
  const contract = new Contract(REFERRAL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(
      contract.call('register', new Address(referee).toScVal(), new Address(referrer).toScVal()),
    )
    .setTimeout(30)
    .build();

  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

// ── #862: Tranche helpers ─────────────────────────────────────────────────────

import { TrancheClass } from '@/../packages/sdk/src/generated/tranche';
import type { TranchePool, TrancheConfig } from '@/../packages/sdk/src/generated/tranche';

export type { TranchePool, TrancheConfig };
export { TrancheClass };

const DUMMY_CALLER = 'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN';

function trancheClassToScVal(tc: TrancheClass): xdr.ScVal {
  const name = tc === TrancheClass.Senior ? 'Senior' : 'Junior';
  return xdr.ScVal.scvVec([nativeToScVal(name, { type: 'symbol' })]);
}

function accountingFromRaw(r: Record<string, unknown>) {
  return {
    deposited: BigInt(String(r.deposited ?? 0)),
    available: BigInt(String(r.available ?? 0)),
    deployed: BigInt(String(r.deployed ?? 0)),
    earned: BigInt(String(r.earned ?? 0)),
    losses: BigInt(String(r.losses ?? 0)),
  };
}

export async function getTranchePool(token: string): Promise<TranchePool> {
  const sim = await simulateTx(
    TRANCHE_CONTRACT_ID,
    'get_pool',
    [new Address(token).toScVal()],
    DUMMY_CALLER,
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as Record<string, unknown>;
  return {
    senior: accountingFromRaw(raw.senior as Record<string, unknown>),
    junior: accountingFromRaw(raw.junior as Record<string, unknown>),
    config: raw.config as TrancheConfig,
  };
}

export async function getTrancheInvestorPosition(
  investor: string,
  token: string,
  trancheClass: TrancheClass,
): Promise<{ deposited: bigint; shares: bigint; earned: bigint; losses: bigint }> {
  const sim = await simulateTx(
    TRANCHE_CONTRACT_ID,
    'get_position',
    [
      new Address(investor).toScVal(),
      new Address(token).toScVal(),
      trancheClassToScVal(trancheClass),
    ],
    DUMMY_CALLER,
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as Record<string, unknown>;
  return {
    deposited: BigInt(String(raw.deposited ?? 0)),
    shares: BigInt(String(raw.shares ?? 0)),
    earned: BigInt(String(raw.earned ?? 0)),
    losses: BigInt(String(raw.losses ?? 0)),
  };
}

export async function buildTrancheDepositTx(
  investor: string,
  token: string,
  trancheClass: TrancheClass,
  amount: bigint,
): Promise<string> {
  const account = await getRpcAccount(investor);
  const contract = new Contract(TRANCHE_CONTRACT_ID);
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(
      contract.call(
        'deposit_tranche',
        new Address(investor).toScVal(),
        new Address(token).toScVal(),
        trancheClassToScVal(trancheClass),
        nativeToScVal(amount, { type: 'i128' }),
      ),
    )
    .setTimeout(30)
    .build();
  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

export async function buildSetTrancheConfigTx(
  admin: string,
  token: string,
  seniorTargetYieldBps: number,
  seniorAdvanceRateBps: number,
  juniorFirstLossBps: number,
): Promise<string> {
  const account = await getRpcAccount(admin);
  const contract = new Contract(TRANCHE_CONTRACT_ID);
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(
      contract.call(
        'set_tranche_config',
        new Address(admin).toScVal(),
        new Address(token).toScVal(),
        nativeToScVal(seniorTargetYieldBps, { type: 'u32' }),
        nativeToScVal(seniorAdvanceRateBps, { type: 'u32' }),
        nativeToScVal(juniorFirstLossBps, { type: 'u32' }),
      ),
    )
    .setTimeout(30)
    .build();
  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

// ── #1055: Insurance helpers ─────────────────────────────────────────────────

import type {
  PremiumConfig,
  InsuranceRiskTier,
  ReserveFund,
  CoverageRecord,
  ClaimHistoryItem,
  ReserveHealth,
} from '@/../packages/sdk/src/generated/insurance';

export type {
  PremiumConfig,
  InsuranceRiskTier,
  ReserveFund,
  CoverageRecord,
  ClaimHistoryItem,
  ReserveHealth,
};

// Soroban encodes named-field structs as an ScMap keyed by field-name Symbols
// in alphabetical order (see #863's comment above for the same convention).
function riskTierToScVal(tier: InsuranceRiskTier): xdr.ScVal {
  const entry = (key: string, val: xdr.ScVal) =>
    new xdr.ScMapEntry({ key: nativeToScVal(key, { type: 'symbol' }), val });
  return xdr.ScVal.scvMap([
    entry('max_score', nativeToScVal(tier.max_score, { type: 'u32' })),
    entry('min_score', nativeToScVal(tier.min_score, { type: 'u32' })),
    entry('risk_multiplier_bps', nativeToScVal(tier.risk_multiplier_bps, { type: 'u32' })),
  ]);
}

function riskTierFromRaw(r: Record<string, unknown>): InsuranceRiskTier {
  return {
    min_score: Number(r.min_score),
    max_score: Number(r.max_score),
    risk_multiplier_bps: Number(r.risk_multiplier_bps),
  };
}

function premiumConfigToScVal(config: PremiumConfig): xdr.ScVal {
  const entry = (key: string, val: xdr.ScVal) =>
    new xdr.ScMapEntry({ key: nativeToScVal(key, { type: 'symbol' }), val });
  return xdr.ScVal.scvMap([
    entry('base_rate_bps', nativeToScVal(config.base_rate_bps, { type: 'u32' })),
    entry('default_coverage_bps', nativeToScVal(config.default_coverage_bps, { type: 'u32' })),
    entry(
      'default_risk_multiplier_bps',
      nativeToScVal(config.default_risk_multiplier_bps, { type: 'u32' }),
    ),
    entry('max_premium_bps', nativeToScVal(config.max_premium_bps, { type: 'u32' })),
    entry('min_premium_bps', nativeToScVal(config.min_premium_bps, { type: 'u32' })),
    entry('risk_tiers', xdr.ScVal.scvVec(config.risk_tiers.map(riskTierToScVal))),
    entry('tenor_bps_per_day', nativeToScVal(config.tenor_bps_per_day, { type: 'u32' })),
  ]);
}

function premiumConfigFromRaw(raw: Record<string, unknown>): PremiumConfig {
  return {
    base_rate_bps: Number(raw.base_rate_bps),
    tenor_bps_per_day: Number(raw.tenor_bps_per_day),
    risk_tiers: ((raw.risk_tiers as Record<string, unknown>[]) ?? []).map(riskTierFromRaw),
    default_risk_multiplier_bps: Number(raw.default_risk_multiplier_bps),
    min_premium_bps: Number(raw.min_premium_bps),
    max_premium_bps: Number(raw.max_premium_bps),
    default_coverage_bps: Number(raw.default_coverage_bps),
  };
}

function reserveFundFromRaw(raw: Record<string, unknown>): ReserveFund {
  return {
    total_reserves: BigInt(String(raw.total_reserves ?? 0)),
    total_premiums_collected: BigInt(String(raw.total_premiums_collected ?? 0)),
    total_claims_paid: BigInt(String(raw.total_claims_paid ?? 0)),
    total_covered_exposure: BigInt(String(raw.total_covered_exposure ?? 0)),
    coverage_ratio_bps: Number(raw.coverage_ratio_bps),
    min_coverage_ratio_bps: Number(raw.min_coverage_ratio_bps),
  };
}

function coverageRecordFromRaw(raw: Record<string, unknown>): CoverageRecord {
  return {
    invoice_id: BigInt(String(raw.invoice_id)),
    token: raw.token as string,
    principal: BigInt(String(raw.principal)),
    premium_paid: BigInt(String(raw.premium_paid)),
    coverage_bps: Number(raw.coverage_bps),
    purchased_at: BigInt(String(raw.purchased_at)),
    claimed: Boolean(raw.claimed),
  };
}

function claimHistoryItemFromRaw(raw: Record<string, unknown>): ClaimHistoryItem {
  return {
    invoice_id: BigInt(String(raw.invoice_id)),
    token: raw.token as string,
    payout: BigInt(String(raw.payout)),
    shortfalls: BigInt(String(raw.shortfalls)),
    timestamp: BigInt(String(raw.timestamp)),
  };
}

function reserveHealthFromRaw(raw: Record<string, unknown>): ReserveHealth {
  return {
    token: raw.token as string,
    total_reserves: BigInt(String(raw.total_reserves ?? 0)),
    coverage_ratio_bps: Number(raw.coverage_ratio_bps),
    min_reserve_amount: BigInt(String(raw.min_reserve_amount ?? 0)),
    is_healthy: Boolean(raw.is_healthy),
    needs_top_up: Boolean(raw.needs_top_up),
  };
}

export async function estimateInsurancePremium(
  principal: bigint,
  sme: string,
  tenorDays: number,
  token: string,
): Promise<bigint> {
  const sim = await simulateTx(
    INSURANCE_CONTRACT_ID,
    'estimate_premium',
    [
      nativeToScVal(principal, { type: 'i128' }),
      new Address(sme).toScVal(),
      nativeToScVal(tenorDays, { type: 'u32' }),
      new Address(token).toScVal(),
    ],
    DUMMY_CALLER,
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return BigInt(String(scValToNative(result!.retval)));
}

export async function getCoverageRecord(
  invoiceId: bigint | number,
): Promise<CoverageRecord | null> {
  const sim = await simulateTx(
    INSURANCE_CONTRACT_ID,
    'get_coverage_record',
    [nativeToScVal(invoiceId, { type: 'u64' })],
    DUMMY_CALLER,
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval);
  return raw ? coverageRecordFromRaw(raw as Record<string, unknown>) : null;
}

export async function getClaimHistory(invoiceId: bigint | number): Promise<ClaimHistoryItem[]> {
  const sim = await simulateTx(
    INSURANCE_CONTRACT_ID,
    'get_claim_history',
    [nativeToScVal(invoiceId, { type: 'u64' })],
    DUMMY_CALLER,
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as Record<string, unknown>[];
  return (raw ?? []).map(claimHistoryItemFromRaw);
}

export async function getReserveStatus(token: string): Promise<ReserveFund> {
  const sim = await simulateTx(
    INSURANCE_CONTRACT_ID,
    'get_reserve_status',
    [new Address(token).toScVal()],
    DUMMY_CALLER,
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return reserveFundFromRaw(scValToNative(result!.retval) as Record<string, unknown>);
}

export async function checkReserveHealth(token: string): Promise<ReserveHealth> {
  const sim = await simulateTx(
    INSURANCE_CONTRACT_ID,
    'check_reserve_health',
    [new Address(token).toScVal()],
    DUMMY_CALLER,
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return reserveHealthFromRaw(scValToNative(result!.retval) as Record<string, unknown>);
}

export async function getPremiumConfig(): Promise<PremiumConfig | null> {
  const sim = await simulateTx(INSURANCE_CONTRACT_ID, 'get_premium_config', [], DUMMY_CALLER);
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval);
  return raw ? premiumConfigFromRaw(raw as Record<string, unknown>) : null;
}

export async function getMinReserveAmount(token: string): Promise<bigint> {
  const sim = await simulateTx(
    INSURANCE_CONTRACT_ID,
    'get_min_reserve_amount',
    [new Address(token).toScVal()],
    DUMMY_CALLER,
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return BigInt(String(scValToNative(result!.retval)));
}

export async function getInsuranceContractLink(): Promise<string | null> {
  const sim = await simulateTx(POOL_CONTRACT_ID, 'get_insurance_contract', [], DUMMY_CALLER);
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval);
  return raw ? (raw as string) : null;
}

export async function buildPurchaseCoverageTx(
  payer: string,
  invoiceId: bigint | number,
  principal: bigint,
  sme: string,
  dueDate: number,
  token: string,
): Promise<string> {
  const account = await getRpcAccount(payer);
  const contract = new Contract(INSURANCE_CONTRACT_ID);
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(
      contract.call(
        'purchase_coverage',
        new Address(payer).toScVal(),
        nativeToScVal(invoiceId, { type: 'u64' }),
        nativeToScVal(principal, { type: 'i128' }),
        new Address(sme).toScVal(),
        nativeToScVal(dueDate, { type: 'u64' }),
        new Address(token).toScVal(),
      ),
    )
    .setTimeout(30)
    .build();
  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

export async function buildFileClaimTx(
  caller: string,
  invoiceId: bigint | number,
): Promise<string> {
  const account = await getRpcAccount(caller);
  const contract = new Contract(INSURANCE_CONTRACT_ID);
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(
      contract.call(
        'file_claim',
        new Address(caller).toScVal(),
        nativeToScVal(invoiceId, { type: 'u64' }),
      ),
    )
    .setTimeout(30)
    .build();
  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

export async function buildSetPremiumConfigTx(
  admin: string,
  config: PremiumConfig,
): Promise<string> {
  const account = await getRpcAccount(admin);
  const contract = new Contract(INSURANCE_CONTRACT_ID);
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(
      contract.call(
        'set_premium_config',
        new Address(admin).toScVal(),
        premiumConfigToScVal(config),
      ),
    )
    .setTimeout(30)
    .build();
  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

export async function buildSetMinCoverageRatioTx(
  admin: string,
  token: string,
  minRatioBps: number,
): Promise<string> {
  const account = await getRpcAccount(admin);
  const contract = new Contract(INSURANCE_CONTRACT_ID);
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(
      contract.call(
        'set_min_coverage_ratio',
        new Address(admin).toScVal(),
        new Address(token).toScVal(),
        nativeToScVal(minRatioBps, { type: 'u32' }),
      ),
    )
    .setTimeout(30)
    .build();
  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

export async function buildSetMinReserveAmountTx(
  admin: string,
  token: string,
  minAmount: bigint,
): Promise<string> {
  const account = await getRpcAccount(admin);
  const contract = new Contract(INSURANCE_CONTRACT_ID);
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(
      contract.call(
        'set_min_reserve_amount',
        new Address(admin).toScVal(),
        new Address(token).toScVal(),
        nativeToScVal(minAmount, { type: 'i128' }),
      ),
    )
    .setTimeout(30)
    .build();
  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

export async function buildFundReserveFromTreasuryTx(
  admin: string,
  token: string,
  amount: bigint,
): Promise<string> {
  const account = await getRpcAccount(admin);
  const contract = new Contract(INSURANCE_CONTRACT_ID);
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(
      contract.call(
        'fund_reserve_from_treasury',
        new Address(admin).toScVal(),
        new Address(token).toScVal(),
        nativeToScVal(amount, { type: 'i128' }),
      ),
    )
    .setTimeout(30)
    .build();
  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

/** Links the pool contract to the insurance reserve so `fund_invoice_request`
 * can non-fatally purchase coverage automatically at funding time. */
export async function buildSetInsuranceContractTx(
  admin: string,
  insuranceContract: string,
): Promise<string> {
  const account = await getRpcAccount(admin);
  const contract = new Contract(POOL_CONTRACT_ID);
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(
      contract.call(
        'set_insurance_contract',
        new Address(admin).toScVal(),
        new Address(insuranceContract).toScVal(),
      ),
    )
    .setTimeout(30)
    .build();
  const sim = await simulateRpcTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
  return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
}

// ---- #864: role-based multisig access control ----

const SIMULATION_SOURCE = 'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN';

function roleToScVal(role: Role): xdr.ScVal {
  return xdr.ScVal.scvVec([nativeToScVal(role, { type: 'symbol' })]);
}

/**
 * Encodes an `ActionPayload` (mirrors contracts/access_control/src/lib.rs's
 * `ActionPayload` enum) the same way `attestorTypeToScVal` above encodes a
 * unit-variant enum, extended for tuple-variant payloads: a Vec whose first
 * element is the variant name (Symbol) followed by its fields in order.
 */
function actionPayloadToScVal(action: ActionPayload): xdr.ScVal {
  const tag = nativeToScVal(action.tag, { type: 'symbol' });
  switch (action.tag) {
    case 'SetPaused':
    case 'SetKycRequired':
      return xdr.ScVal.scvVec([tag, nativeToScVal(action.values[0], { type: 'bool' })]);
    case 'SetYield':
    case 'SetMaxUtilization':
      return xdr.ScVal.scvVec([tag, nativeToScVal(action.values[0], { type: 'u32' })]);
    case 'SetTreasury':
    case 'SetOracleContract':
    case 'SetOracle':
    case 'AddKeeper':
      return xdr.ScVal.scvVec([tag, new Address(action.values[0]).toScVal()]);
    case 'WithdrawRevenue':
      return xdr.ScVal.scvVec([
        tag,
        new Address(action.values[0]).toScVal(),
        nativeToScVal(action.values[1], { type: 'i128' }),
      ]);
    case 'SetInvestorKyc':
      return xdr.ScVal.scvVec([
        tag,
        new Address(action.values[0]).toScVal(),
        nativeToScVal(action.values[1], { type: 'bool' }),
      ]);
    case 'RegisterDebtor':
      return xdr.ScVal.scvVec([
        tag,
        nativeToScVal(action.values[0], { type: 'string' }),
        nativeToScVal(action.values[1], { type: 'string' }),
        nativeToScVal(action.values[2], { type: 'i128' }),
      ]);
    case 'DeactivateDebtor':
      return xdr.ScVal.scvVec([tag, nativeToScVal(action.values[0], { type: 'string' })]);
    case 'SetLateThreshold':
      return xdr.ScVal.scvVec([tag, nativeToScVal(action.values[0], { type: 'i64' })]);
    case 'SetScoreThresholds':
      return xdr.ScVal.scvVec([
        tag,
        nativeToScVal(action.values[0], { type: 'u32' }),
        nativeToScVal(action.values[1], { type: 'u32' }),
        nativeToScVal(action.values[2], { type: 'u32' }),
        nativeToScVal(action.values[3], { type: 'u32' }),
      ]);
    case 'RegisterAttestor':
      return xdr.ScVal.scvVec([
        tag,
        new Address(action.values[0]).toScVal(),
        nativeToScVal(action.values[1], { type: 'u32' }),
        nativeToScVal(action.values[2], { type: 'u32' }),
      ]);
    case 'AddSigner':
    case 'RemoveSigner':
      return xdr.ScVal.scvVec([
        tag,
        roleToScVal(action.values[0]),
        new Address(action.values[1]).toScVal(),
      ]);
    case 'SetThreshold':
      return xdr.ScVal.scvVec([
        tag,
        roleToScVal(action.values[0]),
        nativeToScVal(action.values[1], { type: 'u32' }),
      ]);
    case 'SetOracleRegistryInvoiceContract':
    case 'SetReferralPool':
      return xdr.ScVal.scvVec([tag, new Address(action.values[0]).toScVal()]);
    case 'SetOracleRegistryTreasury':
      return xdr.ScVal.scvVec([
        tag,
        action.values[0] === undefined
          ? xdr.ScVal.scvVoid()
          : new Address(action.values[0]).toScVal(),
      ]);
    case 'SetOracleRegistryConfig':
      return xdr.ScVal.scvVec([
        tag,
        nativeToScVal(action.values[0], { type: 'i128' }),
        nativeToScVal(action.values[1], { type: 'u32' }),
        nativeToScVal(action.values[2], { type: 'u32' }),
        nativeToScVal(action.values[3], { type: 'u64' }),
        nativeToScVal(action.values[4], { type: 'u64' }),
      ]);
    case 'SetOracleRegistryPaused':
    case 'SetCompliancePaused':
    case 'SetReferralPaused':
      return xdr.ScVal.scvVec([tag, nativeToScVal(action.values[0], { type: 'bool' })]);
    case 'SlashOracle':
      return xdr.ScVal.scvVec([
        tag,
        new Address(action.values[0]).toScVal(),
        nativeToScVal(action.values[1], { type: 'u32' }),
        nativeToScVal(action.values[2], { type: 'u64' }),
        nativeToScVal(action.values[3], { type: 'string' }),
      ]);
    case 'AdminResolveRound':
      return xdr.ScVal.scvVec([
        tag,
        nativeToScVal(action.values[0], { type: 'u64' }),
        nativeToScVal(action.values[1], { type: 'bool' }),
        nativeToScVal(action.values[2], { type: 'string' }),
      ]);
    case 'RegisterScreener':
    case 'ConfirmScreenerRegistration':
    case 'DeregisterScreener':
      return xdr.ScVal.scvVec([tag, new Address(action.values[0]).toScVal()]);
    case 'SetRescreeningInterval':
    case 'SetScreenerTimelock':
      return xdr.ScVal.scvVec([tag, nativeToScVal(action.values[0], { type: 'u64' })]);
    case 'UpdateGovernanceConfig':
      return xdr.ScVal.scvVec([
        tag,
        nativeToScVal(action.values[0], { type: 'u32' }),
        nativeToScVal(action.values[1], { type: 'u32' }),
      ]);
    case 'SetCategoryQuorum':
      return xdr.ScVal.scvVec([
        tag,
        nativeToScVal(action.values[0], { type: 'u32' }),
        nativeToScVal(action.values[1], { type: 'u32' }),
      ]);
    case 'SetBorrowRewardBps':
    case 'SetDepositRewardBps':
      return xdr.ScVal.scvVec([tag, nativeToScVal(action.values[0], { type: 'u32' })]);
    case 'SetInvoiceAccessControl':
    case 'SetCreditScoreAccessControl':
    case 'SetOracleRegistryAccessControl':
    case 'SetComplianceAccessControl':
    case 'SetGovernanceAccessControl':
    case 'SetReferralAccessControl':
      return xdr.ScVal.scvVec([tag, new Address(action.values[0]).toScVal()]);
  }
}

/**
 * `scValToNative` decodes our `[tag, ...fields]` vec encoding of an
 * `ActionPayload` variant into a flat native array (e.g. `['SetYield', 650]`),
 * not the `{ tag, values }` shape `ActionPayload` expects — reshape it here.
 */
function actionPayloadFromNative(raw: unknown): ActionPayload {
  const [tag, ...values] = raw as [ActionPayload['tag'], ...unknown[]];
  return { tag, values } as ActionPayload;
}

function proposalFromScVal(raw: Record<string, unknown>): Proposal {
  return {
    role: enumTagFromNative<Role>(raw.role),
    target: raw.target as StellarAddress,
    action: actionPayloadFromNative(raw.action),
    proposer: raw.proposer as StellarAddress,
    approvals: (raw.approvals as StellarAddress[]) ?? [],
    createdAt: Number(raw.created_at ?? 0),
    expiresAt: Number(raw.expires_at ?? 0),
    status: enumTagFromNative<ProposalStatusRaw>(raw.status) as unknown as Proposal['status'],
  };
}

type ProposalStatusRaw = 'Pending' | 'Approved' | 'Executed' | 'Rejected';

export async function getRoleConfig(role: Role): Promise<MultiSigConfig | null> {
  const sim = await simulateTx(
    ACCESS_CONTROL_CONTRACT_ID,
    'get_role_config',
    [roleToScVal(role)],
    SIMULATION_SOURCE,
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as Record<string, unknown> | null;
  if (!raw) return null;
  return {
    signers: (raw.signers as StellarAddress[]) ?? [],
    threshold: Number(raw.threshold ?? 0),
  };
}

/** Fetches every role's config in one round-trip, for the admin roles page. */
export async function listAllRoleConfigs(): Promise<Record<Role, MultiSigConfig | null>> {
  const entries = await Promise.all(
    ALL_ROLES.map(async (role) => [role, await getRoleConfig(role)] as const),
  );
  return Object.fromEntries(entries) as Record<Role, MultiSigConfig | null>;
}

export async function isRoleSigner(role: Role, address: string): Promise<boolean> {
  const sim = await simulateTx(
    ACCESS_CONTROL_CONTRACT_ID,
    'is_signer',
    [roleToScVal(role), new Address(address).toScVal()],
    SIMULATION_SOURCE,
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return Boolean(scValToNative(result!.retval));
}

export async function getProposal(proposalId: number): Promise<Proposal | null> {
  const sim = await simulateTx(
    ACCESS_CONTROL_CONTRACT_ID,
    'get_proposal',
    [nativeToScVal(proposalId, { type: 'u64' })],
    SIMULATION_SOURCE,
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  const raw = scValToNative(result!.retval) as Record<string, unknown> | null;
  return raw ? proposalFromScVal(raw) : null;
}

async function getNextProposalId(): Promise<number> {
  const sim = await simulateTx(
    ACCESS_CONTROL_CONTRACT_ID,
    'get_next_proposal_id',
    [],
    SIMULATION_SOURCE,
  );
  const result = (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result;
  return Number(scValToNative(result!.retval));
}

/**
 * Fetches every proposal in `0..getNextProposalId()` — no pagination, since
 * this governance system is meant for tens of proposals, not thousands.
 */
export async function listProposals(): Promise<Array<{ id: number; proposal: Proposal }>> {
  if (!ACCESS_CONTROL_CONTRACT_ID) return [];
  const nextId = await getNextProposalId();
  const results: Array<{ id: number; proposal: Proposal }> = [];
  for (let id = 0; id < nextId; id++) {
    const proposal = await getProposal(id);
    if (proposal) results.push({ id, proposal });
  }
  return results;
}

export async function buildProposeActionTx(params: {
  role: Role;
  proposer: string;
  target: string;
  action: ActionPayload;
}): Promise<string> {
  const account = await getRpcAccount(params.proposer);
  const contract = new Contract(ACCESS_CONTROL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(
      contract.call(
        'propose_action',
        roleToScVal(params.role),
        new Address(params.proposer).toScVal(),
        new Address(params.target).toScVal(),
        actionPayloadToScVal(params.action),
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

function buildSignerOnlyActionTx(
  method: 'approve_action' | 'reject_action' | 'revoke_approval',
): (params: { signer: string; proposalId: number }) => Promise<string> {
  return async ({ signer, proposalId }) => {
    const account = await getRpcAccount(signer);
    const contract = new Contract(ACCESS_CONTROL_CONTRACT_ID);

    const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
      .addOperation(
        contract.call(
          method,
          new Address(signer).toScVal(),
          nativeToScVal(proposalId, { type: 'u64' }),
        ),
      )
      .setTimeout(30)
      .build();

    const sim = await simulateRpcTransaction(tx);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    return StellarRpc.assembleTransaction(tx, sim).build().toXDR();
  };
}

export const buildApproveActionTx = buildSignerOnlyActionTx('approve_action');
export const buildRejectActionTx = buildSignerOnlyActionTx('reject_action');
export const buildRevokeApprovalTx = buildSignerOnlyActionTx('revoke_approval');

export async function buildExecuteActionTx(params: {
  caller: string;
  proposalId: number;
}): Promise<string> {
  const account = await getRpcAccount(params.caller);
  const contract = new Contract(ACCESS_CONTROL_CONTRACT_ID);

  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: NETWORK })
    .addOperation(
      contract.call(
        'execute_action',
        new Address(params.caller).toScVal(),
        nativeToScVal(params.proposalId, { type: 'u64' }),
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
