import useSWR from 'swr';
import { mutate } from 'swr';
import useSWRMutation from 'swr/mutation';
import {
  getInvoice,
  getInvoiceMetadata,
  getInvoiceCount,
  getPoolConfig,
  getAcceptedTokens,
  getPoolTokenTotals,
  getInvestorPosition,
  getFundedInvoice,
  buildDepositTx,
  buildWithdrawTx,
  buildCommitToInvoiceTx,
  buildCreateInvoiceTx,
  buildMarkDefaultedTx,
  buildInitCoFundingTx,
  buildSetYieldTx,
  submitTx,
} from './contracts';
import type {
  Invoice,
  InvestorPosition,
  PoolConfig,
  PoolTokenTotals,
  FundedInvoice,
  InvoiceMetadata,
} from './types';

// SWR Configuration
const SWR_CONFIG = {
  refreshInterval: 30000, // 30 seconds
  revalidateOnFocus: true,
  revalidateOnReconnect: true,
  dedupingInterval: 5000, // 5 seconds
};

const STALE_TIMES = {
  poolConfig: 300000, // 5 minutes - changes infrequently (admin updates)
  invoiceCount: 15000, // 15 seconds - changes with new invoices
  invoice: 10000, // 10 seconds - status changes frequently
  position: 15000, // 15 seconds - changes with deposits/commits
  tokens: 60000, // 1 minute - whitelist changes rarely
  tokenTotals: 20000, // 20 seconds - changes with deposits/deployments
  fundedInvoice: 10000, // 10 seconds - status changes
};

// Error type for contract calls
class ContractError extends Error {
  constructor(
    message: string,
    public code?: string,
  ) {
    super(message);
    this.name = 'ContractError';
  }
}

// Fetcher wrapper with error handling
async function fetcher<T>(fn: () => Promise<T>): Promise<T> {
  try {
    return await fn();
  } catch (error) {
    if (error instanceof Error) {
      throw new ContractError(error.message);
    }
    throw new ContractError('Unknown error occurred');
  }
}

// ---- Pool Config Cache ----

export function usePoolConfig() {
  return useSWR<PoolConfig, ContractError>('pool-config', () => fetcher(() => getPoolConfig()), {
    ...SWR_CONFIG,
    refreshInterval: STALE_TIMES.poolConfig,
  });
}

// ---- Accepted Tokens Cache ----

export function useAcceptedTokens() {
  return useSWR<string[], ContractError>(
    'accepted-tokens',
    () => fetcher(() => getAcceptedTokens()),
    {
      ...SWR_CONFIG,
      refreshInterval: STALE_TIMES.tokens,
    },
  );
}

// ---- Invoice Count Cache ----

export function useInvoiceCount() {
  return useSWR<number, ContractError>('invoice-count', () => fetcher(() => getInvoiceCount()), {
    ...SWR_CONFIG,
    refreshInterval: STALE_TIMES.invoiceCount,
  });
}

// ---- Single Invoice Cache ----

export function useInvoice(id: number | null) {
  return useSWR<Invoice, ContractError>(
    id !== null ? ['invoice', id] : null,
    () => fetcher(() => getInvoice(id!)),
    {
      ...SWR_CONFIG,
      refreshInterval: STALE_TIMES.invoice,
    },
  );
}

// ---- Invoice Metadata Cache ----

export function useInvoiceMetadata(id: number | null) {
  return useSWR<InvoiceMetadata, ContractError>(
    id !== null ? ['invoice-metadata', id] : null,
    () => fetcher(() => getInvoiceMetadata(id!)),
    {
      ...SWR_CONFIG,
      refreshInterval: STALE_TIMES.invoice,
    },
  );
}

// ---- Investor Position Cache ----

export function useInvestorPosition(investor: string | null, token: string | null) {
  return useSWR<InvestorPosition | null, ContractError>(
    investor && token ? ['position', investor, token] : null,
    () => fetcher(() => getInvestorPosition(investor!, token!)),
    {
      ...SWR_CONFIG,
      refreshInterval: STALE_TIMES.position,
    },
  );
}

// ---- Pool Token Totals Cache ----

export function usePoolTokenTotals(token: string | null) {
  return useSWR<PoolTokenTotals, ContractError>(
    token ? ['token-totals', token] : null,
    () => fetcher(() => getPoolTokenTotals(token!)),
    {
      ...SWR_CONFIG,
      refreshInterval: STALE_TIMES.tokenTotals,
    },
  );
}

// ---- Funded Invoice Cache ----

export function useFundedInvoice(invoiceId: number | null) {
  return useSWR<FundedInvoice | null, ContractError>(
    invoiceId !== null ? ['funded-invoice', invoiceId] : null,
    () => fetcher(() => getFundedInvoice(invoiceId!)),
    {
      ...SWR_CONFIG,
      refreshInterval: STALE_TIMES.fundedInvoice,
    },
  );
}

// ---- Mutations with Cache Invalidation ----

// Generic mutation helper type
type MutationContext = {
  invalidateKeys: (string | (string | number)[])[];
};

// Deposit mutation
async function depositMutation(
  _: string,
  { arg }: { arg: { investor: string; token: string; amount: bigint; signedXdr: string } },
) {
  const result = await submitTx(arg.signedXdr);
  return result;
}

export function useDeposit(investor: string, token: string) {
  return useSWRMutation<
    unknown,
    ContractError,
    string,
    { investor: string; token: string; amount: bigint; signedXdr: string },
    MutationContext
  >('deposit', depositMutation, {
    onSuccess: () => {
      // Invalidate position and token totals after deposit
      mutate(['position', investor, token]);
      mutate(['token-totals', token]);
    },
  });
}

// Withdraw mutation
async function withdrawMutation(
  _: string,
  { arg }: { arg: { investor: string; token: string; amount: bigint; signedXdr: string } },
) {
  return submitTx(arg.signedXdr);
}

export function useWithdraw(investor: string, token: string) {
  return useSWRMutation<
    unknown,
    ContractError,
    string,
    { investor: string; token: string; amount: bigint; signedXdr: string }
  >('withdraw', withdrawMutation);
}

// Commit to invoice mutation
async function commitMutation(
  _: string,
  { arg }: { arg: { investor: string; invoiceId: number; signedXdr: string } },
) {
  return submitTx(arg.signedXdr);
}

export function useCommitToInvoice(investor: string, invoiceId: number) {
  return useSWRMutation<
    unknown,
    ContractError,
    string,
    { investor: string; invoiceId: number; signedXdr: string }
  >('commit', commitMutation);
}

// Create invoice mutation
async function createInvoiceMutation(_: string, { arg }: { arg: { signedXdr: string } }) {
  return submitTx(arg.signedXdr);
}

export function useCreateInvoice() {
  return useSWRMutation<unknown, ContractError, string, { signedXdr: string }>(
    'create-invoice',
    createInvoiceMutation,
  );
}

// Mark defaulted mutation
async function markDefaultedMutation(
  _: string,
  { arg }: { arg: { admin: string; invoiceId: number; signedXdr: string } },
) {
  return submitTx(arg.signedXdr);
}

export function useMarkDefaulted(admin: string, invoiceId: number) {
  return useSWRMutation<
    unknown,
    ContractError,
    string,
    { admin: string; invoiceId: number; signedXdr: string }
  >('mark-defaulted', markDefaultedMutation);
}

// Init co-funding mutation
async function initCoFundingMutation(
  _: string,
  { arg }: { arg: { admin: string; invoiceId: number; signedXdr: string } },
) {
  return submitTx(arg.signedXdr);
}

export function useInitCoFunding(admin: string, invoiceId: number) {
  return useSWRMutation<
    unknown,
    ContractError,
    string,
    { admin: string; invoiceId: number; signedXdr: string }
  >('init-cofunding', initCoFundingMutation);
}

// Set yield mutation
async function setYieldMutation(_: string, { arg }: { arg: { admin: string; signedXdr: string } }) {
  return submitTx(arg.signedXdr);
}

export function useSetYield(admin: string) {
  return useSWRMutation<unknown, ContractError, string, { admin: string; signedXdr: string }>(
    'set-yield',
    setYieldMutation,
  );
}

// ---- Cache Invalidation Helpers ----

// Helper to revalidate all invoice-related cache
export function getInvoiceCacheKeys(invoiceId?: number) {
  const keys: (string | (string | number)[])[] = ['invoice-count'];

  if (invoiceId !== undefined) {
    keys.push(['invoice', invoiceId]);
    keys.push(['invoice-metadata', invoiceId]);
    keys.push(['funded-invoice', invoiceId]);
  }

  return keys;
}

// Helper to revalidate all position-related cache
export function getPositionCacheKeys(investor?: string, token?: string) {
  const keys: (string | (string | number)[])[] = [];

  if (investor && token) {
    keys.push(['position', investor, token]);
    keys.push(['token-totals', token]);
  }

  return keys;
}

// Export SWR provider config for app setup
export const swrConfig = {
  provider: () => new Map(),
  ...SWR_CONFIG,
};
