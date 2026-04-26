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
} from './contracts';
import type { Invoice, InvoiceStatus } from './types';

// ---- Cache with 5-minute TTL ----

interface CacheEntry<T> {
  data: T;
  timestamp: number;
}

const CACHE_TTL = 5 * 60 * 1000; // 5 minutes
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

// ---- Simulated Historical Data Generator ----
// In production, this would query a time-series DB or subgraph.
// For now, we generate realistic mock data based on current on-chain state.

function generateHistoricalUtilization(
  currentDeposited: bigint,
  currentDeployed: bigint,
  points: number = 30,
): PoolUtilizationPoint[] {
  const data: PoolUtilizationPoint[] = [];
  const now = Date.now();
  const depositedNum = Number(currentDeposited) / 10_000_000;
  const deployedNum = Number(currentDeployed) / 10_000_000;

  for (let i = points - 1; i >= 0; i--) {
    const date = new Date(now - i * 24 * 60 * 60 * 1000);
    // Simulate gradual growth with some variance
    const growthFactor = 1 - i / points;
    const variance = 0.8 + Math.random() * 0.4;
    const dep = Math.round(depositedNum * growthFactor * variance);
    const dployed = Math.min(dep, Math.round(deployedNum * growthFactor * variance));
    const util = dep > 0 ? Math.round((dployed / dep) * 100) : 0;

    data.push({
      time: date.toISOString().slice(0, 10),
      utilization: Math.min(util, 100),
      deployed: dployed,
      deposited: dep,
    });
  }

  return data;
}

function generateYieldData(yieldBps: number, points: number = 30): YieldPoint[] {
  const data: YieldPoint[] = [];
  const now = Date.now();
  const apy = yieldBps / 100;

  for (let i = points - 1; i >= 0; i--) {
    const date = new Date(now - i * 24 * 60 * 60 * 1000);
    // Simulate slight APY variance around the target
    const variance = apy * (0.95 + Math.random() * 0.1);
    const interest = Math.round(variance * 10) / 10;

    data.push({
      time: date.toISOString().slice(0, 10),
      apy: Math.round(variance * 100) / 100,
      interest,
    });
  }

  return data;
}

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
        score = 700 + Math.floor(Math.random() * 150);
        break;
      case 'Funded':
        score = 500 + Math.floor(Math.random() * 250);
        break;
      case 'Verified':
        score = 400 + Math.floor(Math.random() * 300);
        break;
      case 'Defaulted':
        score = 200 + Math.floor(Math.random() * 300);
        break;
      default:
        score = 300 + Math.floor(Math.random() * 550);
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
    const [invoiceCount, poolConfig, poolTokenTotals] = await Promise.all([
      getInvoiceCount().catch(() => 0),
      getPoolConfig().catch(() => null),
      getPoolTokenTotals('').catch(() => null),
    ]);

    // Fetch invoices (limit to reasonable number for dashboard)
    const invoiceIds = Array.from({ length: Math.min(invoiceCount, 50) }, (_, i) => i + 1);
    const invoices = await getMultipleInvoices(invoiceIds).catch(() => []);

    // Fetch recent events
    const events = await monitorService.pollEvents().catch(() => []);

    const deposited = poolTokenTotals?.totalDeposited ?? 0n;
    const deployed = poolTokenTotals?.totalDeployed ?? 0n;
    const yieldBps = poolConfig?.yieldBps ?? 800;

    const data: AnalyticsDashboardData = {
      poolUtilization: generateHistoricalUtilization(deposited, deployed),
      yieldPerformance: generateYieldData(yieldBps),
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
