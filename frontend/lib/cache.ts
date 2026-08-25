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
  listCoFundingRounds,
  getCoFundingRound,
  getInvestorCoFundPositions,
  getListing,
  listListingsForInvestor,
  getOrder,
  getOrderBook,
  listOrdersForOwner,
  buildDepositTx,
  buildWithdrawTx,
  buildCommitToInvoiceTx,
  buildCreateInvoiceTx,
  buildMarkDefaultedTx,
  buildSetYieldTx,
  submitTx,
  getReferrer,
  getReferralStats,
  buildRegisterReferralTx,
} from './contracts';
import type {
  Invoice,
  InvestorPosition,
  PoolConfig,
  PoolTokenTotals,
  FundedInvoice,
  InvoiceMetadata,
  ReferralStats,
  CoFundingRound,
  Listing,
  ListingKind,
  Order,
  OrderBookLevel,
} from './types';

type SWRCacheEntry = {
  refreshInterval: number;
  revalidateOnFocus: boolean;
  revalidateOnReconnect: boolean;
  dedupingInterval: number;
};

/** Per-data-type TTLs (milliseconds). Import and refer to these directly. */
export const CACHE_TTL = {
  poolConfig: 5 * 60_000,
  invoiceStatus: 15_000,
  creditScore: 60_000,
  walletBalance: 30_000,
} as const;

/** Per-resource SWR configuration. Import and spread into useSWR options. */
export const CACHE_CONFIG: Record<string, SWRCacheEntry> = {
  poolConfig: {
    refreshInterval: CACHE_TTL.poolConfig,
    revalidateOnFocus: true,
    revalidateOnReconnect: true,
    dedupingInterval: CACHE_TTL.poolConfig,
  },
  invoiceCount: {
    refreshInterval: CACHE_TTL.invoiceStatus,
    revalidateOnFocus: true,
    revalidateOnReconnect: true,
    dedupingInterval: CACHE_TTL.invoiceStatus,
  },
  invoice: {
    refreshInterval: CACHE_TTL.invoiceStatus,
    revalidateOnFocus: true,
    revalidateOnReconnect: true,
    dedupingInterval: CACHE_TTL.invoiceStatus,
  },
  position: {
    refreshInterval: CACHE_TTL.invoiceStatus,
    revalidateOnFocus: true,
    revalidateOnReconnect: true,
    dedupingInterval: CACHE_TTL.invoiceStatus,
  },
  tokens: {
    refreshInterval: CACHE_TTL.creditScore,
    revalidateOnFocus: true,
    revalidateOnReconnect: true,
    dedupingInterval: CACHE_TTL.creditScore,
  },
  tokenTotals: {
    refreshInterval: CACHE_TTL.walletBalance,
    revalidateOnFocus: true,
    revalidateOnReconnect: true,
    dedupingInterval: CACHE_TTL.walletBalance,
  },
  fundedInvoice: {
    refreshInterval: CACHE_TTL.invoiceStatus,
    revalidateOnFocus: true,
    revalidateOnReconnect: true,
    dedupingInterval: CACHE_TTL.invoiceStatus,
  },
  referral: {
    refreshInterval: CACHE_TTL.creditScore,
    revalidateOnFocus: true,
    revalidateOnReconnect: true,
    dedupingInterval: CACHE_TTL.creditScore,
  },
  coFundingRounds: {
    refreshInterval: CACHE_TTL.invoiceStatus,
    revalidateOnFocus: true,
    revalidateOnReconnect: true,
    dedupingInterval: CACHE_TTL.invoiceStatus,
  },
  coFundingPositions: {
    refreshInterval: CACHE_TTL.invoiceStatus,
    revalidateOnFocus: true,
    revalidateOnReconnect: true,
    dedupingInterval: CACHE_TTL.invoiceStatus,
  },
  secondaryMarketListings: {
    refreshInterval: CACHE_TTL.invoiceStatus,
    revalidateOnFocus: true,
    revalidateOnReconnect: true,
    dedupingInterval: CACHE_TTL.invoiceStatus,
  },
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
    ...CACHE_CONFIG.poolConfig,
  });
}

// ---- Accepted Tokens Cache ----

export function useAcceptedTokens() {
  return useSWR<string[], ContractError>(
    'accepted-tokens',
    () => fetcher(() => getAcceptedTokens()),
    {
      ...CACHE_CONFIG.tokens,
    },
  );
}

// ---- Invoice Count Cache ----

export function useInvoiceCount() {
  return useSWR<number, ContractError>('invoice-count', () => fetcher(() => getInvoiceCount()), {
    ...CACHE_CONFIG.invoiceCount,
  });
}

// ---- Single Invoice Cache ----

export function useInvoice(id: number | null) {
  return useSWR<Invoice, ContractError>(
    id !== null ? ['invoice', id] : null,
    () => fetcher(() => getInvoice(id!)),
    {
      ...CACHE_CONFIG.invoice,
    },
  );
}

// ---- Invoice Metadata Cache ----

export function useInvoiceMetadata(id: number | null) {
  return useSWR<InvoiceMetadata, ContractError>(
    id !== null ? ['invoice-metadata', id] : null,
    () => fetcher(() => getInvoiceMetadata(id!)),
    {
      ...CACHE_CONFIG.invoice,
    },
  );
}

// ---- Investor Position Cache ----

export function useInvestorPosition(investor: string | null, token: string | null) {
  return useSWR<InvestorPosition | null, ContractError>(
    investor && token ? ['position', investor, token] : null,
    () => fetcher(() => getInvestorPosition(investor!, token!)),
    {
      ...CACHE_CONFIG.position,
    },
  );
}

// ---- Pool Token Totals Cache ----

export function usePoolTokenTotals(token: string | null) {
  return useSWR<PoolTokenTotals, ContractError>(
    token ? ['token-totals', token] : null,
    () => fetcher(() => getPoolTokenTotals(token!)),
    {
      ...CACHE_CONFIG.tokenTotals,
    },
  );
}

// ---- Funded Invoice Cache ----

export function useFundedInvoice(invoiceId: number | null) {
  return useSWR<FundedInvoice | null, ContractError>(
    invoiceId !== null ? ['funded-invoice', invoiceId] : null,
    () => fetcher(() => getFundedInvoice(invoiceId!)),
    {
      ...CACHE_CONFIG.fundedInvoice,
    },
  );
}

// ---- #799: Referral Program Cache ----

export function useReferrer(referee: string | null) {
  return useSWR<string | null, ContractError>(
    referee ? ['referrer', referee] : null,
    () => fetcher(() => getReferrer(referee!)),
    {
      ...CACHE_CONFIG.referral,
    },
  );
}

export function useReferralStats(referrer: string | null) {
  return useSWR<ReferralStats, ContractError>(
    referrer ? ['referral-stats', referrer] : null,
    () => fetcher(() => getReferralStats(referrer!)),
    {
      ...CACHE_CONFIG.referral,
    },
  );
}

// ---- Co-Funding Rounds Cache ----

export function useCoFundingRounds() {
  return useSWR<CoFundingRound[], ContractError>(
    'co-funding-rounds',
    () =>
      fetcher(async () => {
        const ids = await listCoFundingRounds();
        const rounds = await Promise.all(ids.map((id) => getCoFundingRound(id)));
        return rounds.filter((r): r is CoFundingRound => r !== null);
      }),
    {
      ...CACHE_CONFIG.coFundingRounds,
    },
  );
}

// ---- Co-Funding Positions Cache ----

export function useCoFundingPositions(investor: string | null) {
  return useSWR<Array<{ invoiceId: number; bps: number }>, ContractError>(
    investor ? ['co-funding-positions', investor] : null,
    () => fetcher(() => getInvestorCoFundPositions(investor!)),
    {
      ...CACHE_CONFIG.coFundingPositions,
    },
  );
}

// ---- #1044: Secondary Market Listings Cache ----
//
// Unlike co-funding rounds (which the pool contract can enumerate directly
// via list_co_funding_rounds), secondary_market has no "list all listings"
// entrypoint — only per-invoice and per-seller lookups. Browsing all open
// listings across the platform is instead discovered from the indexer's
// already-ingested lst_open/lst_cncl/lst_buy event history (see #1044's
// indexer/src/parser.ts + api.ts changes), then hydrated with a live
// getListing() read per id, mirroring the discover-then-hydrate shape of
// useCoFundingRounds above.

const INDEXER_URL = process.env.NEXT_PUBLIC_INDEXER_URL || 'http://localhost:3001';

async function fetchListingIds(path: string): Promise<number[]> {
  const res = await fetch(`${INDEXER_URL}${path}`);
  if (!res.ok) throw new Error(`Indexer error: ${res.status}`);
  const data = (await res.json()) as { listingIds: Array<string | number> };
  return (data.listingIds ?? []).map((id) => Number(id));
}

async function hydrateListings(ids: number[]): Promise<Listing[]> {
  const listings = await Promise.all(ids.map((id) => getListing(id)));
  return listings.filter((l): l is Listing => l !== null);
}

/** All currently-open secondary-market listings (across every seller/invoice). */
export function useOpenListings() {
  return useSWR<Listing[], ContractError>(
    'secondary-market-open-listings',
    () =>
      fetcher(async () => {
        const ids = await fetchListingIds('/secondary-market/listings/open');
        return hydrateListings(ids);
      }),
    {
      ...CACHE_CONFIG.secondaryMarketListings,
    },
  );
}

/** Every listing (open, filled, or cancelled) a given seller has created. */
export function useMyListings(seller: string | null) {
  return useSWR<Listing[], ContractError>(
    seller ? ['secondary-market-my-listings', seller] : null,
    () =>
      fetcher(async () => {
        const ids = await listListingsForInvestor(seller!);
        return hydrateListings(ids);
      }),
    {
      ...CACHE_CONFIG.secondaryMarketListings,
    },
  );
}

// ---- #1035: Order Book Cache ----
//
// Unlike the listings above, `get_order_book` is scoped to a single
// (invoiceId, kind) pair and reads on-chain state directly — no indexer
// discovery step needed, since there's no "browse every invoice's book at
// once" need in the UI (a seller/buyer works one invoice at a time).

async function hydrateOrders(ids: number[]): Promise<Order[]> {
  const orders = await Promise.all(ids.map((id) => getOrder(id)));
  return orders.filter((o): o is Order => o !== null);
}

/** Resting bid/ask depth levels for one invoice's order book, as `{ bids, asks }` (#1133). */
export function useOrderBook(invoiceId: number | null, kind: ListingKind | null) {
  return useSWR<{ bids: OrderBookLevel[]; asks: OrderBookLevel[] }, ContractError>(
    invoiceId !== null && kind ? ['secondary-market-order-book', invoiceId, kind] : null,
    () =>
      fetcher(async () => {
        const { bids, asks } = await getOrderBook(invoiceId!, kind!);
        return { bids, asks };
      }),
    {
      ...CACHE_CONFIG.secondaryMarketListings,
    },
  );
}

/** Every order (any status) a given owner has placed. */
export function useMyOrders(owner: string | null) {
  return useSWR<Order[], ContractError>(
    owner ? ['secondary-market-my-orders', owner] : null,
    () =>
      fetcher(async () => {
        const ids = await listOrdersForOwner(owner!);
        return hydrateOrders(ids);
      }),
    {
      ...CACHE_CONFIG.secondaryMarketListings,
    },
  );
}

async function registerReferralMutation(
  _: string,
  { arg }: { arg: { referee: string; referrer: string; signedXdr: string } },
) {
  return submitTx(arg.signedXdr);
}

export function useRegisterReferral(referee: string) {
  return useSWRMutation<
    unknown,
    ContractError,
    string,
    { referee: string; referrer: string; signedXdr: string }
  >('register-referral', registerReferralMutation, {
    onSuccess: () => {
      mutate(['referrer', referee]);
    },
  });
}

export { buildRegisterReferralTx };

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
    {
      onSuccess: () => {
        mutate('pool-config');
      },
    },
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
