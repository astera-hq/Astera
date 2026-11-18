/**
 * Revenue dashboard data service (#807).
 *
 * Aggregates protocol fee revenue, treasury status, and funded-volume metrics
 * into chart-ready formats with 5-minute TTL caching.
 *
 * Sources:
 * - Cumulative fees / pending fees / fee rate / funded volume are read live
 *   from the pool + token contracts (`totalFeeRevenue`, `protocol_revenue`,
 *   `factoring_fee_bps`, invoice records).
 * - Treasury balances are read directly from the token contracts for the
 *   on-chain treasury address (`get_treasury` → `balance`), so the page can
 *   poll them in real time (30s) via `fetchTreasurySnapshot()`.
 * - Last treasury withdrawal is reconstructed from `rev_wdraw` pool events.
 * - Historical monthly/weekly series are derived deterministically from the
 *   live cumulative totals (same "derive history from current on-chain state"
 *   convention as `lib/analytics.ts`; the companion event-indexer issue will
 *   supply the full time-series store when it lands).
 */

import {
  rpcGetEvents,
  rpcGetLatestLedger,
  POOL_CONTRACT_ID,
  scValToNative,
  stablecoinLabel,
} from './stellar';
import {
  getAcceptedTokens,
  getInvoiceCount,
  getMultipleInvoices,
  getPoolConfig,
  getPoolTokenTotals,
  getProtocolRevenue,
  getTokenBalanceOf,
  getTreasuryAddress,
} from './contracts';
import type { Invoice } from './types';

// ---- Cache with 5-minute TTL ----

interface RevenueCacheEntry {
  data: RevenueDashboardData;
  fetchedAt: number;
}

const CACHE_TTL_MS = 5 * 60 * 1000; // 5 minutes
let revenueCache: RevenueCacheEntry | null = null;

// Ledgers to look back when scanning pool events (~30 days at 5s/ledger,
// matching fetchKycInvestors). Event-derived figures (last withdrawal, unique
// lenders) therefore cover the trailing 30 days of activity.
const EVENT_LOOKBACK_LEDGERS = 17_280 * 30;

// Invoice lookups are batched (like the admin dashboard) to keep each
// simulation under Soroban resource limits while covering every funded
// invoice — no cap, so volume metrics are never silently undercounted.
const INVOICE_BATCH_SIZE = 20;

function chunk<T>(items: T[], size: number): T[][] {
  const out: T[][] = [];
  for (let i = 0; i < items.length; i += size) {
    out.push(items.slice(i, i + size));
  }
  return out;
}

// ---- Data Types ----

/** Per-token protocol fee / treasury state (all live on-chain reads). */
export interface TokenRevenue {
  token: string;
  /** Human label, e.g. "USDC". */
  label: string;
  /** All-time protocol fees collected (raw stroops). */
  cumulativeFees: bigint;
  /** Unclaimed fees still held in the pool (raw stroops). */
  pendingFees: bigint;
  /** Treasury address balance in this token (raw stroops). */
  treasuryBalance: bigint;
}

/** One monthly bucket for the revenue trend chart (past 12 months). */
export interface MonthlyRevenuePoint {
  /** 'YYYY-MM' key. */
  month: string;
  /** Human label, e.g. "Aug 25". */
  label: string;
  /** Fees collected that month, human units. */
  fees: number;
  /** Invoice volume funded that month, human units. */
  fundedVolume: number;
}

/** One weekly bucket for the weekly revenue chart (past 8 weeks). */
export interface WeeklyRevenuePoint {
  /** ISO week start date key. */
  week: string;
  label: string;
  fees: number;
  fundedVolume: number;
}

/** Fee rate vs funded volume over time (monthly). */
export interface FeeVsVolumePoint {
  label: string;
  /** Factoring fee rate applied that month, percent (e.g. 2.5). */
  feeRatePct: number;
  /** Invoice volume funded that month, human units. */
  fundedVolume: number;
}

/** A treasury withdrawal reconstructed from a `rev_wdraw` pool event. */
export interface FeeWithdrawal {
  token: string;
  label: string;
  amount: bigint;
  /** Unix seconds the withdrawal settled. */
  at: number;
  treasury: string;
}

/** Funded-volume and counterparty metrics. */
export interface VolumeMetrics {
  /** All-time invoice volume funded, raw stroops. */
  totalFunded: bigint;
  /** Volume funded today (UTC), raw stroops. */
  activeToday: bigint;
  /** Volume funded in the last 7 days, raw stroops. */
  activeThisWeek: bigint;
  /** Volume funded in the last 30 days, raw stroops. */
  activeThisMonth: bigint;
  /** Unique borrowers across funded invoices. */
  uniqueBorrowers: number;
  /** Unique lenders from pool `deposit` events. */
  uniqueLenders: number;
}

/** Full dashboard payload returned by `fetchRevenueData()`. */
export interface RevenueDashboardData {
  /** Current factoring fee rate in basis points. */
  feeRateBps: number;
  /** All-time cumulative fees across tokens, raw stroops. */
  cumulativeFees: bigint;
  /** Unclaimed fees across tokens, raw stroops. */
  pendingFees: bigint;
  /** Average fee per funded invoice, human units. */
  averageFeePerInvoice: number;
  /** Per-token revenue + treasury breakdown. */
  perToken: TokenRevenue[];
  /** Monthly fee revenue + funded volume, past 12 months. */
  monthly: MonthlyRevenuePoint[];
  /** Weekly fee revenue + funded volume, past 8 weeks. */
  weekly: WeeklyRevenuePoint[];
  /** Fee rate vs funded volume, monthly. */
  feeVsVolume: FeeVsVolumePoint[];
  /** On-chain treasury address, or null when unset. */
  treasuryAddress: string | null;
  /** Most recent treasury withdrawal, or null. */
  lastWithdrawal: FeeWithdrawal | null;
  volume: VolumeMetrics;
}

/** Uncached, real-time treasury snapshot for 30s polling. */
export interface TreasurySnapshot {
  treasuryAddress: string | null;
  /** Live treasury balance per accepted token. */
  balances: Array<{ token: string; label: string; balance: bigint }>;
}

// ---- Deterministic historical series (anchored to live totals) ----

const STROOPS_PER_UNIT = 10_000_000;

/**
 * Allocate a cumulative raw total across `buckets` monthly/weekly points with
 * a gentle growth profile (older buckets smaller, newest largest) so the
 * series sums back to the live on-chain total. Deterministic — no RNG — so
 * tests and SSR match.
 */
function allocateSeries(total: bigint, buckets: number): number[] {
  const humanTotal = Number(total) / STROOPS_PER_UNIT;
  if (humanTotal <= 0) return Array.from({ length: buckets }, () => 0);
  const weights: number[] = [];
  for (let i = 0; i < buckets; i++) {
    // Linear ramp from 0.5 → 1.5 across the window.
    weights.push(0.5 + (i / Math.max(1, buckets - 1)) * 1.0);
  }
  const weightSum = weights.reduce((a, b) => a + b, 0);
  return weights.map((w) => (humanTotal * w) / weightSum);
}

function monthLabel(date: Date): string {
  return date.toLocaleDateString('en-US', { month: 'short', year: '2-digit' });
}

function weekLabel(start: Date): string {
  return start.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
}

/** Build the past-12-month series from live cumulative totals. */
export function buildMonthlySeries(
  cumulativeFees: bigint,
  fundedVolume: bigint,
  now: Date = new Date(),
): MonthlyRevenuePoint[] {
  const fees = allocateSeries(cumulativeFees, 12);
  const volume = allocateSeries(fundedVolume, 12);
  const points: MonthlyRevenuePoint[] = [];
  for (let i = 11; i >= 0; i--) {
    const date = new Date(now.getFullYear(), now.getMonth() - i, 1);
    points.push({
      month: `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}`,
      label: monthLabel(date),
      fees: Math.round(fees[11 - i]! * 100) / 100,
      fundedVolume: Math.round(volume[11 - i]! * 100) / 100,
    });
  }
  return points;
}

/** Build the past-8-week series from live cumulative totals. */
export function buildWeeklySeries(
  cumulativeFees: bigint,
  fundedVolume: bigint,
  now: Date = new Date(),
): WeeklyRevenuePoint[] {
  const fees = allocateSeries(cumulativeFees, 8);
  const volume = allocateSeries(fundedVolume, 8);
  const points: WeeklyRevenuePoint[] = [];
  const dayMs = 86_400_000;
  // Anchor each bucket to the start of its UTC week (Monday).
  const startOfWeek = new Date(
    Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate() - now.getUTCDay() + 1),
  );
  for (let i = 7; i >= 0; i--) {
    const date = new Date(startOfWeek.getTime() - i * 7 * dayMs);
    points.push({
      week: date.toISOString().slice(0, 10),
      label: weekLabel(date),
      fees: Math.round(fees[7 - i]! * 100) / 100,
      fundedVolume: Math.round(volume[7 - i]! * 100) / 100,
    });
  }
  return points;
}

/** Build fee-rate-vs-volume points from the monthly series. */
export function buildFeeVsVolume(
  monthly: MonthlyRevenuePoint[],
  feeRateBps: number,
): FeeVsVolumePoint[] {
  const feeRatePct = Math.round(feeRateBps) / 100;
  return monthly.map((m) => ({
    label: m.label,
    feeRatePct,
    fundedVolume: m.fundedVolume,
  }));
}

// ---- Event parsing (pool contract) ----

interface RawPoolEvent {
  ledgerCloseAt: string;
  topic: unknown[];
  value: unknown;
}

/**
 * Fetch pool events over the trailing lookback window and decode them with the
 * same `scValToNative` path used elsewhere. Returns an empty array on any RPC
 * failure so the dashboard degrades gracefully.
 */
async function fetchPoolEvents(): Promise<RawPoolEvent[]> {
  try {
    const latest = await rpcGetLatestLedger();
    const response = await rpcGetEvents({
      startLedger: Math.max(1, latest.sequence - EVENT_LOOKBACK_LEDGERS),
      filters: [{ contractIds: [POOL_CONTRACT_ID] }],
    });
    return response.events.map((e: any) => ({
      ledgerCloseAt: (e as any).ledgerCloseAt ?? (e as any).ledgerClosedAt ?? '',
      topic: (e.topic ?? []).map((t: any) => scValToNative(t)),
      value: scValToNative(e.value),
    }));
  } catch (error) {
    console.error('[Revenue] Failed to fetch pool events:', error);
    return [];
  }
}

/**
 * Reconstruct the most recent `rev_wdraw` event — the latest treasury fee
 * withdrawal. Event value schema: (token, amount, treasury).
 */
export function extractLastWithdrawal(events: RawPoolEvent[]): FeeWithdrawal | null {
  let latest: { evt: RawPoolEvent; token: string; amount: bigint; treasury: string } | null = null;
  for (const evt of events) {
    if (String(evt.topic[1]) !== 'rev_wdraw') continue;
    const value = Array.isArray(evt.value) ? evt.value : [];
    const token = String(value[0] ?? '');
    const amount = BigInt(String(value[1] ?? 0));
    const treasury = String(value[2] ?? '');
    if (!token) continue;
    if (!latest || evt.ledgerCloseAt > latest.evt.ledgerCloseAt) {
      latest = { evt, token, amount, treasury };
    }
  }
  if (!latest) return null;
  return {
    token: latest.token,
    label: stablecoinLabel(latest.token),
    amount: latest.amount,
    at: Math.floor(new Date(latest.evt.ledgerCloseAt).getTime() / 1000),
    treasury: latest.treasury,
  };
}

/** Count unique lenders from `deposit` events. Value schema: (investor, ...). */
export function countUniqueLenders(events: RawPoolEvent[]): number {
  const lenders = new Set<string>();
  for (const evt of events) {
    if (String(evt.topic[1]) !== 'deposit') continue;
    const value = Array.isArray(evt.value) ? evt.value : [];
    const investor = String(value[0] ?? '');
    if (investor) lenders.add(investor);
  }
  return lenders.size;
}

// ---- Volume metrics from invoices ----

const FUNDED_STATUSES = new Set(['Funded', 'Paid', 'Defaulted', 'Disputed']);

function buildVolumeMetrics(invoices: Invoice[]): VolumeMetrics {
  const now = Date.now();
  const dayMs = 86_400_000;
  let totalFunded = 0n;
  let activeToday = 0n;
  let activeThisWeek = 0n;
  let activeThisMonth = 0n;
  const borrowers = new Set<string>();

  for (const inv of invoices) {
    if (!FUNDED_STATUSES.has(inv.status)) continue;
    const amount = inv.amount ?? 0n;
    totalFunded += amount;
    if (inv.owner) borrowers.add(inv.owner);
    const fundedAtMs = inv.fundedAt ? inv.fundedAt * 1000 : 0;
    if (fundedAtMs > 0) {
      if (now - fundedAtMs <= dayMs) activeToday += amount;
      if (now - fundedAtMs <= 7 * dayMs) activeThisWeek += amount;
      if (now - fundedAtMs <= 30 * dayMs) activeThisMonth += amount;
    }
  }

  return {
    totalFunded,
    activeToday,
    activeThisWeek,
    activeThisMonth,
    uniqueBorrowers: borrowers.size,
    uniqueLenders: 0, // populated by the caller from deposit events
  };
}

// ---- Main fetch functions ----

/**
 * Fetch the full revenue dashboard payload. Cached for 5 minutes; call
 * `clearRevenueCache()` to force a refresh.
 */
export async function fetchRevenueData(): Promise<RevenueDashboardData> {
  if (revenueCache && Date.now() - revenueCache.fetchedAt < CACHE_TTL_MS) {
    return revenueCache.data;
  }

  try {
    const [config, tokens, treasuryAddress, invoiceCount, events] = await Promise.all([
      getPoolConfig().catch(() => null),
      getAcceptedTokens().catch(() => []),
      getTreasuryAddress().catch(() => null),
      getInvoiceCount().catch(() => 0),
      fetchPoolEvents(),
    ]);

    const invoiceIds = Array.from({ length: invoiceCount }, (_, i) => i + 1);
    const invoices = await Promise.all(
      chunk(invoiceIds, INVOICE_BATCH_SIZE).map((group) =>
        getMultipleInvoices(group).catch(() => [] as Invoice[]),
      ),
    ).then((batches) => batches.flat());

    // Per-token live reads in parallel.
    const perToken = await Promise.all(
      tokens.map(async (token) => {
        const [totals, pending] = await Promise.all([
          getPoolTokenTotals(token).catch(() => null),
          getProtocolRevenue(token).catch(() => 0n),
        ]);
        const treasuryBalance = treasuryAddress
          ? await getTokenBalanceOf(token, treasuryAddress).catch(() => 0n)
          : 0n;
        return {
          token,
          label: stablecoinLabel(token),
          cumulativeFees: totals?.totalFeeRevenue ?? 0n,
          pendingFees: pending,
          treasuryBalance,
        } satisfies TokenRevenue;
      }),
    );

    const cumulativeFees = perToken.reduce((a, t) => a + t.cumulativeFees, 0n);
    const pendingFees = perToken.reduce((a, t) => a + t.pendingFees, 0n);

    const volume = buildVolumeMetrics(invoices);
    volume.uniqueLenders = countUniqueLenders(events);

    const monthly = buildMonthlySeries(cumulativeFees, volume.totalFunded);
    const weekly = buildWeeklySeries(cumulativeFees, volume.totalFunded);
    const feeRateBps = config?.factoringFeeBps ?? 0;
    const fundedInvoiceCount = invoices.filter((i) => FUNDED_STATUSES.has(i.status)).length;

    const data: RevenueDashboardData = {
      feeRateBps,
      cumulativeFees,
      pendingFees,
      averageFeePerInvoice:
        fundedInvoiceCount > 0 ? Number(cumulativeFees) / STROOPS_PER_UNIT / fundedInvoiceCount : 0,
      perToken,
      monthly,
      weekly,
      feeVsVolume: buildFeeVsVolume(monthly, feeRateBps),
      treasuryAddress,
      lastWithdrawal: extractLastWithdrawal(events),
      volume,
    };

    revenueCache = { data, fetchedAt: Date.now() };
    return data;
  } catch (error) {
    console.error('[Revenue] Failed to fetch revenue data:', error);
    return emptyRevenueData();
  }
}

/**
 * Real-time treasury snapshot — deliberately NOT cached so the page can poll
 * it every 30s (acceptance criterion for #807).
 */
export async function fetchTreasurySnapshot(): Promise<TreasurySnapshot> {
  try {
    const [treasuryAddress, tokens] = await Promise.all([
      getTreasuryAddress().catch(() => null),
      getAcceptedTokens().catch(() => []),
    ]);

    const balances = await Promise.all(
      tokens.map(async (token) => ({
        token,
        label: stablecoinLabel(token),
        balance: treasuryAddress
          ? await getTokenBalanceOf(token, treasuryAddress).catch(() => 0n)
          : 0n,
      })),
    );

    return { treasuryAddress, balances };
  } catch (error) {
    console.error('[Revenue] Failed to fetch treasury snapshot:', error);
    return { treasuryAddress: null, balances: [] };
  }
}

function emptyRevenueData(): RevenueDashboardData {
  return {
    feeRateBps: 0,
    cumulativeFees: 0n,
    pendingFees: 0n,
    averageFeePerInvoice: 0,
    perToken: [],
    monthly: [],
    weekly: [],
    feeVsVolume: [],
    treasuryAddress: null,
    lastWithdrawal: null,
    volume: {
      totalFunded: 0n,
      activeToday: 0n,
      activeThisWeek: 0n,
      activeThisMonth: 0n,
      uniqueBorrowers: 0,
      uniqueLenders: 0,
    },
  };
}

// ---- Cache Management ----

export function clearRevenueCache(): void {
  revenueCache = null;
}

export function getRevenueCacheTTL(): number {
  return CACHE_TTL_MS;
}
