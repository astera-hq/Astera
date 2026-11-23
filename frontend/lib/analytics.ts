/**
 * Analytics data service for the admin analytics dashboard.
 * Aggregates on-chain data into chart-ready formats with 5-minute TTL caching.
 */

import { monitorService, ContractEvent } from './monitoring';
import {
  getInvoiceCount,
  getPoolConfig,
  getPoolTokenTotals,
  getMultipleInvoices,
} from './sdk';
import type { Invoice, InvoiceStatus } from './types';

// ---- Cache with 5-minute TTL ----

interface CacheEntry<T> {
  data: T;
  timestamp: number;
}

const CACHE_TTL = 5 * 60 * 1000; // 5 minutes
const INDEXER_URL = process.env.NEXT_PUBLIC_INDEXER_URL || 'http://localhost:3001';
const cache = new Map<string, CacheEntry<unknown>>();

function getCached<T>(key: string): T | null {
  const entry = cache.get(key) as CacheEntry<T> | undefined;
  if (!entry) return null;
  if (Date.now() - entry.timestamp > CACHE_TTL) {
    cache.delete(key);
    return null;
  }
  return entry.data;
}

function setCache<T>(key: string, data: T): void {
  cache.set(key, { data, timestamp: Date.now() });
}

// ---- Data Types ----

export interface PoolUtilizationPoint {
  time: string;
  utilization: number;
  deployed: number;
  deposited: number;
}

export interface YieldPoint {
  time: string;
  apy: number;
  interest: number;
}

export interface InvoiceFunnelData {
  stage: string;
  count: number;
  value: number;
}

export interface CreditScoreBucket {
  range: string;
  count: number;
}

export interface TopSme {
  address: string;
  invoiceCount: number;
  totalValue: number;
  defaultRate: number;
}

export interface AnalyticsDashboardData {
  poolUtilization: PoolUtilizationPoint[];
  yieldPerformance: YieldPoint[];
  invoiceFunnel: InvoiceFunnelData[];
  creditScoreDistribution: CreditScoreBucket[];
  topSmes: TopSme[];
  recentEvents: ContractEvent[];
}

// ---- Indexer-backed historical data ----

async function fetchPoolUtilizationHistory(): Promise<PoolUtilizationPoint[]> {
  const token = process.env.NEXT_PUBLIC_POOL_TOKEN || 'default';
  const response = await fetch(`${INDEXER_URL}/pool/${encodeURIComponent(token)}/utilization-history`, {
    cache: 'no-store',
  });
  if (!response.ok) throw new Error(`Indexer utilization history returned ${response.status}`);
  const payload = await response.json();
  const history = payload.utilizationHistory ?? payload.history ?? [];
  return history.map((row: any) => ({
    time: String(row.time),
    utilization: Number(row.utilization) || 0,
    deployed: Number(row.deployed) || 0,
    deposited: Number(row.deposited) || 0,
  }));
}

async function fetchAnalyticsSummary(): Promise<Record<string, number> | null> {
  const response = await fetch(`${INDEXER_URL}/analytics/summary`, { cache: 'no-store' });
  if (!response.ok) throw new Error(`Indexer analytics summary returned ${response.status}`);
  const payload = await response.json();
  return payload.latest ?? payload.summary ?? null;
}

// ---- Real-time Data Aggregation ----
// Aggregates on-chain invoice data into chart-ready formats.

function generateInvoiceFunnel(invoices: Invoice[]): InvoiceFunnelData[] {
  const stages: Record<string, { count: number; value: number }> = {
    Created: { count: 0, value: 0 },
    Verified: { count: 0, value: 0 },
    Funded: { count: 0, value: 0 },
    Paid: { count: 0, value: 0 },
    Defaulted: { count: 0, value: 0 },
  };

  for (const inv of invoices) {
    const status = inv.status as InvoiceStatus;
    const amount = Number(inv.amount) / 10_000_000;
    if (status in stages) {
      stages[status].count++;
      stages[status].value += amount;
    }
  }

  return Object.entries(stages).map(([stage, data]) => ({
    stage,
    count: data.count,
    value: Math.round(data.value),
  }));
}

function generateCreditScoreDistribution(invoices: Invoice[]): CreditScoreBucket[] {
  const buckets: CreditScoreBucket[] = [
    { range: '0-300', count: 0 },
    { range: '300-500', count: 0 },
    { range: '500-650', count: 0 },
    { range: '650-750', count: 0 },
    { range: '750-850', count: 0 },
    { range: '850+', count: 0 },
  ];

  // Simulate credit scores based on invoice status
  for (const inv of invoices) {
    let score: number;
    switch (inv.status as InvoiceStatus) {
      case 'Paid':
        score = 700;
        break;
      case 'Funded':
        score = 600;
        break;
      case 'Verified':
        score = 500;
        break;
      case 'Defaulted':
        score = 250;
        break;
      default:
        score = 400;
    }

    if (score >= 850) buckets[5].count++;
    else if (score >= 750) buckets[4].count++;
    else if (score >= 650) buckets[3].count++;
    else if (score >= 500) buckets[2].count++;
    else if (score >= 300) buckets[1].count++;
    else buckets[0].count++;
  }

  return buckets;
}

function generateTopSmes(invoices: Invoice[]): TopSme[] {
  const smeMap = new Map<
    string,
    { invoiceCount: number; totalValue: number; defaultCount: number }
  >();

  for (const inv of invoices) {
    const key = inv.owner;
    const existing = smeMap.get(key) || { invoiceCount: 0, totalValue: 0, defaultCount: 0 };
    existing.invoiceCount++;
    existing.totalValue += Number(inv.amount) / 10_000_000;
    if (inv.status === 'Defaulted') existing.defaultCount++;
    smeMap.set(key, existing);
  }

  return Array.from(smeMap.entries())
    .map(([address, data]) => ({
      address,
      invoiceCount: data.invoiceCount,
      totalValue: Math.round(data.totalValue),
      defaultRate:
        data.invoiceCount > 0 ? Math.round((data.defaultCount / data.invoiceCount) * 100) : 0,
    }))
    .sort((a, b) => b.totalValue - a.totalValue)
    .slice(0, 10);
}

// ---- Main Fetch Function ----

export async function fetchAnalyticsData(): Promise<AnalyticsDashboardData> {
  // Check cache first
  const cached = getCached<AnalyticsDashboardData>('analytics-dashboard');
  if (cached) return cached;

  try {
    // Fetch on-chain data in parallel
    const [invoiceCount, poolConfig, liveTotals] = await Promise.all([
      getInvoiceCount().catch(() => 0),
      getPoolConfig().catch(() => null),
      getPoolTokenTotals(process.env.NEXT_PUBLIC_POOL_TOKEN || 'default').catch(() => null),
    ]);

    const [poolUtilization, analyticsSummary] = await Promise.all([
      fetchPoolUtilizationHistory().catch(() => []),
      fetchAnalyticsSummary().catch(() => null),
    ]);

    // Fetch invoices (limit to reasonable number for dashboard)
    const invoiceIds = Array.from({ length: Math.min(invoiceCount, 50) }, (_, i) => i + 1);
    const invoices = await getMultipleInvoices(invoiceIds).catch(() => []);

    // Fetch recent events
    const events = await monitorService.pollEvents().catch(() => []);

    const yieldBps = poolConfig?.yieldBps;
    const yieldPerformance: YieldPoint[] = yieldBps
      ? [{ time: new Date().toISOString().slice(0, 10), apy: yieldBps / 100, interest: yieldBps / 100 }]
      : analyticsSummary
        ? [{
            time: String(analyticsSummary.time || new Date().toISOString().slice(0, 10)),
            apy: Number(analyticsSummary.defaultRate) || 0,
            interest: Number(analyticsSummary.defaultRate) || 0,
          }]
        : [];

    // Fall back to a live on-chain utilization point when the indexer has no
    // history yet, so the utilization chart is never empty on a fresh deployment.
    const utilizationSeries: PoolUtilizationPoint[] =
      poolUtilization.length > 0
        ? poolUtilization
        : liveTotals && liveTotals.totalDeposited > 0n
          ? [
              {
                time: new Date().toISOString().slice(0, 10),
                utilization:
                  (Number(liveTotals.totalDeployed) / Number(liveTotals.totalDeposited)) * 100,
                deployed: Number(liveTotals.totalDeployed) / 10_000_000,
                deposited: Number(liveTotals.totalDeposited) / 10_000_000,
              },
            ]
          : [];

    const data: AnalyticsDashboardData = {
      poolUtilization: utilizationSeries,
      yieldPerformance,
      invoiceFunnel: generateInvoiceFunnel(invoices),
      creditScoreDistribution: generateCreditScoreDistribution(invoices),
      topSmes: generateTopSmes(invoices),
      recentEvents: events.slice(0, 20),
    };

    // Cache the result
    setCache('analytics-dashboard', data);
    return data;
  } catch (error) {
    console.error('[Analytics] Failed to fetch analytics data:', error);
    // Return empty data structure on error
    return {
      poolUtilization: [],
      yieldPerformance: [],
      invoiceFunnel: [],
      creditScoreDistribution: [],
      topSmes: [],
      recentEvents: [],
    };
  }
}

// ---- Utility Functions ----

export function formatValue(value: number): string {
  if (value >= 1_000_000) return `$${(value / 1_000_000).toFixed(1)}M`;
  if (value >= 1_000) return `$${(value / 1_000).toFixed(1)}K`;
  return `$${value.toFixed(0)}`;
}

export function truncateAddress(address: string, chars = 6): string {
  if (!address) return '';
  return `${address.slice(0, chars)}...${address.slice(-chars)}`;
}

// ---- Cache Management ----

export function clearAnalyticsCache(): void {
  cache.clear();
}

export function getAnalyticsCacheTTL(): number {
  return CACHE_TTL;
}
