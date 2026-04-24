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
export const RPC_URL = 'https://soroban-testnet.stellar.org';
export const HORIZON_URL = 'https://horizon-testnet.stellar.org';

// Set these after deploying your contracts
export const INVOICE_CONTRACT_ID = process.env.NEXT_PUBLIC_INVOICE_CONTRACT_ID ?? '';
export const POOL_CONTRACT_ID = process.env.NEXT_PUBLIC_POOL_CONTRACT_ID ?? '';
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

interface PooledConnection {
  server: StellarRpc.Server;
  createdAt: number;
  lastUsed: number;
  healthy: boolean;
  inFlightRequests: number;
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
      this.connections.push({
        server: new StellarRpc.Server(RPC_URL),
        createdAt: now,
        lastUsed: now,
        healthy: true,
        inFlightRequests: 0,
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
    this.connections[index] = {
      server: new StellarRpc.Server(RPC_URL),
      createdAt: now,
      lastUsed: now,
      healthy: true,
      inFlightRequests: 0,
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
export const rpcExecute = rpcPool.execute.bind(rpcPool);

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

/** Submit a signed XDR transaction */
export async function submitTx(
  signedXDR: string,
  onProgress?: (progress: TransactionProgress) => void,
) {
  return rpcExecute(async (server) => {
    const tx = TransactionBuilder.fromXDR(signedXDR, NETWORK);
    const response = await server.sendTransaction(tx);

    if (response.status === 'ERROR') {
      const error = `Transaction failed: ${JSON.stringify(response)}`;
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
}

export { nativeToScVal, scValToNative, Address, xdr };
