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

export const rpc = new StellarRpc.Server(RPC_URL);

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
  return truncateAddress(tokenId);
}

/** Build and simulate a Soroban transaction */
export async function simulateTx(
  contractId: string,
  method: string,
  args: xdr.ScVal[],
  sourceAddress: string,
): Promise<StellarRpc.Api.SimulateTransactionResponse> {
  const account = await rpc.getAccount(sourceAddress);
  const contract = new Contract(contractId);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK,
  })
    .addOperation(contract.call(method, ...args))
    .setTimeout(30)
    .build();

  return rpc.simulateTransaction(tx);
}

/** Submit a signed XDR transaction */
export async function submitTx(signedXDR: string) {
  const tx = TransactionBuilder.fromXDR(signedXDR, NETWORK);
  const response = await rpc.sendTransaction(tx);

  if (response.status === 'ERROR') {
    throw new Error(`Transaction failed: ${JSON.stringify(response)}`);
  }

  // Poll for confirmation
  let result = await rpc.getTransaction(response.hash);
  let attempts = 0;

  while (result.status === 'NOT_FOUND' && attempts < 20) {
    await new Promise((r) => setTimeout(r, 1500));
    result = await rpc.getTransaction(response.hash);
    attempts++;
  }

  if (result.status === 'FAILED') {
    throw new Error('Transaction failed on-chain');
  }

  return result;
}

export { nativeToScVal, scValToNative, Address };
