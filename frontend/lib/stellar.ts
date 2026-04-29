import {
  Networks,
  TransactionBuilder,
  BASE_FEE,
  Contract,
  rpc as StellarRpc,
  scValToNative,
  nativeToScVal,
  Address,
  xdr,
} from '@stellar/stellar-sdk';

export const NETWORK = Networks.TESTNET;
export const RPC_ENDPOINTS = [
  process.env.NEXT_PUBLIC_STELLAR_RPC_URL,
  process.env.NEXT_PUBLIC_STELLAR_RPC_FALLBACK_1,
  process.env.NEXT_PUBLIC_STELLAR_RPC_FALLBACK_2,
  'https://soroban-testnet.stellar.org',
  'https://rpc-testnet.stellar.org',
].filter(Boolean) as string[];
export const RPC_URL = RPC_ENDPOINTS[0];
export const HORIZON_URL = 'https://horizon-testnet.stellar.org';

// Set these after deploying your contracts
export const INVOICE_CONTRACT_ID = process.env.NEXT_PUBLIC_INVOICE_CONTRACT_ID ?? '';
export const POOL_CONTRACT_ID = process.env.NEXT_PUBLIC_POOL_CONTRACT_ID ?? '';
export const GOVERNANCE_CONTRACT_ID = process.env.NEXT_PUBLIC_GOVERNANCE_CONTRACT_ID ?? '';
export const USDC_TOKEN_ID = process.env.NEXT_PUBLIC_USDC_TOKEN_ID ?? '';
export const EURC_TOKEN_ID = process.env.NEXT_PUBLIC_EURC_TOKEN_ID ?? '';
// #111: additional stablecoin support
export const USDT_TOKEN_ID = process.env.NEXT_PUBLIC_USDT_TOKEN_ID ?? '';
export const USDP_TOKEN_ID = process.env.NEXT_PUBLIC_USDP_TOKEN_ID ?? '';

// ---- RPC Connection Pool ----

/** Configuration for the RPC connection pool */
const RPC_POOL_CONFIG = {
  /** Maximum number of RPC server instances in the pool */
  poolSize: 3,
  /** Health check interval in milliseconds (60 seconds) */
  healthCheckInterval: 60_000,
  /** Maximum age of a connection before recycling (5 minutes) */
  maxConnectionAge: 300_000,
  /** Request timeout in milliseconds */
  requestTimeout: 15_000,
  /** Maximum retry attempts for failed requests */
  maxRetries: 3,
  /** Base delay between retries in milliseconds (exponential backoff) */
  retryBaseDelay: 1_000,
};

const RPC_READ_LIMIT = {
  maxConcurrent: 5,
  maxStartsPerSecond: 10,
  windowMs: 1_000,
};

type QueuedRead<T> = {
  fn: () => Promise<T>;
  resolve: (value: T) => void;
  reject: (reason: unknown) => void;
};

const rpcReadQueue: QueuedRead<unknown>[] = [];
const rpcReadStartTimes: number[] = [];
let activeRpcReads = 0;
let rpcReadTimer: ReturnType<typeof setTimeout> | null = null;

function pruneReadStartTimes(now: number): void {
  while (rpcReadStartTimes.length > 0 && now - rpcReadStartTimes[0] >= RPC_READ_LIMIT.windowMs) {
    rpcReadStartTimes.shift();
  }
}

function scheduleReadQueue(): void {
  if (rpcReadTimer || rpcReadQueue.length === 0) return;

  const now = Date.now();
  pruneReadStartTimes(now);

  if (rpcReadStartTimes.length < RPC_READ_LIMIT.maxStartsPerSecond) {
    queueMicrotask(processReadQueue);
    return;
  }

  const nextSlotIn = Math.max(0, RPC_READ_LIMIT.windowMs - (now - rpcReadStartTimes[0]));
  rpcReadTimer = setTimeout(() => {
    rpcReadTimer = null;
    processReadQueue();
  }, nextSlotIn);
}

function processReadQueue(): void {
  rpcReadTimer = null;
  const now = Date.now();
  pruneReadStartTimes(now);

  while (
    activeRpcReads < RPC_READ_LIMIT.maxConcurrent &&
    rpcReadQueue.length > 0 &&
    rpcReadStartTimes.length < RPC_READ_LIMIT.maxStartsPerSecond
  ) {
    const task = rpcReadQueue.shift()!;
    activeRpcReads++;
    rpcReadStartTimes.push(Date.now());

    task
      .fn()
      .then(task.resolve, task.reject)
      .finally(() => {
        activeRpcReads = Math.max(0, activeRpcReads - 1);
        processReadQueue();
      });
  }

  scheduleReadQueue();
}

export function readContract<T>(fn: () => Promise<T>): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    rpcReadQueue.push({
      fn,
      resolve: resolve as (value: unknown) => void,
      reject,
    });
    processReadQueue();
  });
}

interface PooledConnection {
  server: StellarRpc.Server;
  createdAt: number;
  lastUsed: number;
  healthy: boolean;
  inFlightRequests: number;
  url: string;
}

class RpcConnectionPool {
  private connections: PooledConnection[] = [];
  private healthCheckTimer: ReturnType<typeof setInterval> | null = null;

  constructor() {
    this.initPool();
    this.startHealthChecks();
  }

  /** Initialize the connection pool with fresh server instances */
  private initPool(): void {
    const now = Date.now();
    for (let i = 0; i < RPC_POOL_CONFIG.poolSize; i++) {
      const url = RPC_ENDPOINTS[i % RPC_ENDPOINTS.length] || RPC_URL;
      this.connections.push({
        server: new StellarRpc.Server(url),
        createdAt: now,
        lastUsed: now,
        healthy: true,
        inFlightRequests: 0,
        url,
      });
    }
  }

  /** Start periodic health checks */
  private startHealthChecks(): void {
    // Only run in browser (not during SSR)
    if (typeof window === 'undefined') return;

    this.healthCheckTimer = setInterval(() => {
      this.performHealthChecks();
    }, RPC_POOL_CONFIG.healthCheckInterval);
  }

  /** Check health of all connections and recycle stale ones */
  private async performHealthChecks(): Promise<void> {
    const now = Date.now();
    for (let i = 0; i < this.connections.length; i++) {
      const conn = this.connections[i];

      // Recycle connections that are too old
      if (now - conn.createdAt > RPC_POOL_CONFIG.maxConnectionAge) {
        this.recycleConnection(i);
        continue;
      }

      // Ping the server to check health
      try {
        await conn.server.getHealth();
        conn.healthy = true;
      } catch {
        conn.healthy = false;
        this.recycleConnection(i);
      }
    }
  }

  /** Replace a connection with a fresh one */
  private recycleConnection(index: number): void {
    const now = Date.now();
    // Try the next endpoint in the fallback list
    const currentUrl = this.connections[index]?.url || RPC_URL;
    const nextUrlIndex = (RPC_ENDPOINTS.indexOf(currentUrl) + 1) % RPC_ENDPOINTS.length;
    const nextUrl = RPC_ENDPOINTS[nextUrlIndex] || RPC_URL;

    this.connections[index] = {
      server: new StellarRpc.Server(nextUrl),
      createdAt: now,
      lastUsed: now,
      healthy: true,
      inFlightRequests: 0,
      url: nextUrl,
    };
  }

  /**
   * Get the best available connection from the pool.
   * Prefers healthy connections with the fewest in-flight requests.
   */
  getConnection(): PooledConnection {
    // Sort by: healthy first, then fewest in-flight requests
    const sorted = [...this.connections]
      .filter((c) => c.healthy)
      .sort((a, b) => a.inFlightRequests - b.inFlightRequests);

    const conn = sorted[0] ?? this.connections[0];
    conn.lastUsed = Date.now();
    return conn;
  }

  /**
   * Execute an RPC call with automatic retry and connection failover.
   * Uses exponential backoff between retries.
   */
  async execute<T>(fn: (server: StellarRpc.Server) => Promise<T>): Promise<T> {
    let lastError: Error | null = null;

    for (let attempt = 0; attempt < RPC_POOL_CONFIG.maxRetries; attempt++) {
      const conn = this.getConnection();
      conn.inFlightRequests++;

      try {
        const result = await Promise.race([
          fn(conn.server),
          new Promise<never>((_, reject) =>
            setTimeout(
              () => reject(new Error('RPC request timeout')),
              RPC_POOL_CONFIG.requestTimeout,
            ),
          ),
        ]);
        conn.inFlightRequests = Math.max(0, conn.inFlightRequests - 1);
        return result;
      } catch (err) {
        conn.inFlightRequests = Math.max(0, conn.inFlightRequests - 1);
        lastError = err instanceof Error ? err : new Error(String(err));

        // Mark connection as unhealthy on network errors
        if (
          lastError.message.includes('timeout') ||
          lastError.message.includes('fetch') ||
          lastError.message.includes('network')
        ) {
          conn.healthy = false;
        }

        // Exponential backoff before retry
        if (attempt < RPC_POOL_CONFIG.maxRetries - 1) {
          const delay = RPC_POOL_CONFIG.retryBaseDelay * Math.pow(2, attempt);
          await new Promise((r) => setTimeout(r, delay));
        }
      }
    }

    throw lastError ?? new Error('RPC request failed after retries');
  }

  /** Clean up timers (for testing or unmounting) */
  destroy(): void {
    if (this.healthCheckTimer) {
      clearInterval(this.healthCheckTimer);
      this.healthCheckTimer = null;
    }
  }
}

/** Singleton RPC connection pool instance */
const rpcPool = new RpcConnectionPool();

/** Primary RPC server instance (for backward compatibility) */
export const rpc = rpcPool.getConnection().server;

/** Execute an RPC call with connection pooling, retry, and timeout */
export function rpcExecute<T>(fn: (server: StellarRpc.Server) => Promise<T>): Promise<T> {
  return readContract(() => rpcPool.execute(fn));
}

export type RpcLatestLedger = Awaited<ReturnType<StellarRpc.Server['getLatestLedger']>>;
export type RpcEventsRequest = Parameters<StellarRpc.Server['getEvents']>[0];
export type RpcEventsResponse = Awaited<ReturnType<StellarRpc.Server['getEvents']>>;

export function rpcGetLatestLedger(): Promise<RpcLatestLedger> {
  return rpcExecute<RpcLatestLedger>((server) => server.getLatestLedger());
}

export function rpcGetEvents(request: RpcEventsRequest): Promise<RpcEventsResponse> {
  return rpcExecute<RpcEventsResponse>((server) => server.getEvents(request));
}

// ---- Utility Functions ----

/** Convert USDC amount (human) to stroops (7 decimals) */
export function toStroops(amount: number): bigint {
  return BigInt(Math.round(amount * 10_000_000));
}

/** Convert stroops to human USDC */
export function fromStroops(stroops: bigint): number {
  return Number(stroops) / 10_000_000;
}

/** Format a stroops bigint as a USD string */
export function formatUSDC(stroops: bigint): string {
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    minimumFractionDigits: 2,
  }).format(fromStroops(stroops));
}

/** Format a unix timestamp as a readable date */
export function formatDate(ts: number): string {
  return new Date(ts * 1000).toLocaleDateString('en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  });
}

/** Days remaining until due date */
export function daysUntil(ts: number): number {
  return Math.ceil((ts * 1000 - Date.now()) / 86_400_000);
}

/** Truncate a Stellar address for display */
export function truncateAddress(addr: string): string {
  if (!addr) return '';
  return `${addr.slice(0, 6)}...${addr.slice(-4)}`;
}

// ---- Stellar Explorer Deep Links (#228) ----

export type ExplorerEntity = 'account' | 'transaction' | 'contract' | 'ledger';
export type StellarNetwork = 'testnet' | 'mainnet';

const EXPLORER_BASES: Record<StellarNetwork, string> = {
  testnet: 'https://stellar.expert/explorer/testnet',
  mainnet: 'https://stellar.expert/explorer/public',
};

/**
 * Build a deep link to stellar.expert for any on-chain entity.
 *
 * @param type    - 'account' | 'transaction' | 'contract' | 'ledger'
 * @param id      - The entity identifier (address, hash, or ledger number)
 * @param network - 'testnet' (default) or 'mainnet'
 */
export function explorerUrl(
  type: ExplorerEntity,
  id: string,
  network: StellarNetwork = (process.env.NEXT_PUBLIC_STELLAR_NETWORK as StellarNetwork) ??
    'testnet',
): string {
  const base = EXPLORER_BASES[network] ?? EXPLORER_BASES.testnet;
  return `${base}/${type}/${encodeURIComponent(id)}`;
}

// ---- BigInt handling ----
//
// Contract responses decoded via `scValToNative` may include `bigint` values
// for Soroban numeric types such as `i128` and `u64`. `JSON.stringify` cannot
// serialize `bigint` and throws `TypeError: Do not know how to serialize a
// BigInt` when encountered (e.g. when logging, persisting, or sending to an
// API). Any object that may contain contract-derived numbers MUST be passed
// through `safeSerialize` / `safeStringify` before being stored in Zustand,
// written to `localStorage`, sent via `fetch`, or handed to a JSON logger.
//
// Consumer code that needs the numeric value should continue to work with
// `bigint` directly (for math) and only serialize at the system boundary.

/**
 * Deeply convert any `bigint` values in a value to strings so it can be
 * safely passed to `JSON.stringify`. Arrays and plain objects are recursed
 * into; other values are returned unchanged.
 *
 * The return type is intentionally `unknown` — callers should narrow to the
 * JSON-serializable shape they expect.
 */
export function safeSerialize<T>(value: T): unknown {
  if (typeof value === 'bigint') {
    return value.toString();
  }
  if (value === null || value === undefined) {
    return value;
  }
  if (Array.isArray(value)) {
    return value.map((item) => safeSerialize(item));
  }
  if (typeof value === 'object') {
    // Only recurse into plain objects. Leave class instances (Date, Map, etc.)
    // untouched so JSON.stringify can apply its normal semantics.
    const proto = Object.getPrototypeOf(value);
    if (proto === Object.prototype || proto === null) {
      const out: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
        out[k] = safeSerialize(v);
      }
      return out;
    }
  }
  return value;
}

/**
 * Convenience wrapper: `JSON.stringify` with BigInt support.
 * Use whenever an object that may contain contract-derived numbers is about
 * to be serialized (logs, fetch bodies, `localStorage`, etc.).
 */
export function safeStringify(value: unknown, space?: number): string {
  return JSON.stringify(safeSerialize(value), null, space);
}

/** Human label for a pool stablecoin (matches env-known tokens). */
export function stablecoinLabel(tokenId: string): string {
  if (tokenId === USDC_TOKEN_ID) return 'USDC';
  if (tokenId === EURC_TOKEN_ID) return 'EURC';
  if (USDT_TOKEN_ID && tokenId === USDT_TOKEN_ID) return 'USDT';
  if (USDP_TOKEN_ID && tokenId === USDP_TOKEN_ID) return 'USDP';
  return truncateAddress(tokenId);
}

/** Build and simulate a Soroban transaction */
export async function simulateTx(
  contractId: string,
  method: string,
  args: xdr.ScVal[],
  sourceAddress: string,
): Promise<StellarRpc.Api.SimulateTransactionResponse> {
  return rpcExecute(async (server) => {
    const account = await server.getAccount(sourceAddress);
    const contract = new Contract(contractId);

    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: NETWORK,
    })
      .addOperation(contract.call(method, ...args))
      .setTimeout(30)
      .build();

    return server.simulateTransaction(tx);
  });
}

/** Submit a signed XDR transaction */
export type TransactionProgress = {
  status: 'pending' | 'confirmed' | 'failed';
  hash: string;
  error?: string;
};

export class ContractError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ContractError';
  }
}

export function parseSimulationError(sim: unknown): string {
  const raw = safeStringify(sim).toLowerCase();
  if (raw.includes('insufficientliquidity') || raw.includes('insufficient liquidity')) {
    return 'Not enough liquidity in pool to fund this invoice';
  }
  if (raw.includes('unauthorized')) {
    return 'You are not authorized to perform this action';
  }
  if (raw.includes('contractpaused') || raw.includes('contract is paused')) {
    return 'The protocol is currently paused';
  }
  return 'Transaction simulation failed. Please review inputs and try again.';
}

const pendingTransactions = new Set<string>();

function transactionKey(tx: { hash?: () => Uint8Array | string }, signedXDR: string): string {
  try {
    const hash = tx.hash?.();
    if (typeof hash === 'string') return hash;
    if (hash instanceof Uint8Array) {
      return Array.from(hash)
        .map((byte) => byte.toString(16).padStart(2, '0'))
        .join('');
    }
  } catch {
    // Fall back to the signed payload when hashing is unavailable in tests.
  }

  return signedXDR;
}

/** Submit a signed XDR transaction */
export async function submitTx(
  signedXDR: string,
  onProgress?: (progress: TransactionProgress) => void,
) {
  const tx = TransactionBuilder.fromXDR(signedXDR, NETWORK);
  const txKey = transactionKey(tx, signedXDR);

  if (pendingTransactions.has(txKey)) {
    throw new Error('Transaction already in progress');
  }

  pendingTransactions.add(txKey);

  try {
    return await rpcExecute(async (server) => {
      const response = await server.sendTransaction(tx);

      if (response.status === 'ERROR') {
        const error = `Transaction failed: ${safeStringify(response)}`;
        onProgress?.({ status: 'failed', hash: response.hash, error });
        throw new Error(error);
      }

      onProgress?.({ status: 'pending', hash: response.hash });
      let result = await server.getTransaction(response.hash);
      let attempts = 0;

      while (
        (String(result.status) === 'NOT_FOUND' || String(result.status) === 'PENDING') &&
        attempts < 20
      ) {
        onProgress?.({ status: 'pending', hash: response.hash });
        await new Promise((r) => setTimeout(r, 1500));
        result = await server.getTransaction(response.hash);
        attempts++;
      }

      if (String(result.status) === 'FAILED') {
        const error = 'Transaction failed on-chain';
        onProgress?.({ status: 'failed', hash: response.hash, error });
        throw new Error(error);
      }

      if (String(result.status) === 'NOT_FOUND' || String(result.status) === 'PENDING') {
        const error = 'Transaction confirmation timed out';
        onProgress?.({ status: 'failed', hash: response.hash, error });
        throw new Error(error);
      }

      onProgress?.({ status: 'confirmed', hash: response.hash });
      return result;
    });
  } finally {
    pendingTransactions.delete(txKey);
  }
}

export { nativeToScVal, scValToNative, Address, xdr };

// Contributed for bounty: $1000000000
