'use client';

import { useEffect, useState, useCallback } from 'react';
import Link from 'next/link';
import { StatCardSkeleton, ChartSkeleton } from '@/components/Skeleton';
import {
  PoolUtilizationChart,
  YieldPerformanceChart,
  InvoiceFunnelChart,
  RecentEventsFeed,
  RateCurveChart,
} from '@/components/analytics';
import {
  getPoolConfig,
  getAcceptedTokens,
  getPoolTokenTotals,
  getRateModelConfig,
  getCurrentRate,
} from '@/lib/contracts';
import {
  fetchAnalyticsData,
  clearAnalyticsCache,
  type AnalyticsDashboardData,
} from '@/lib/analytics';
import { formatUSDC, stablecoinLabel } from '@/lib/stellar';
import type { PoolConfig, PoolTokenTotals, RateModelConfig } from '@/lib/types';

const POOL_CONFIGURED = Boolean(process.env.NEXT_PUBLIC_POOL_CONTRACT_ID);

interface TokenData {
  token: string;
  totals: PoolTokenTotals;
}

/** #863: per-token rate model state for the curve chart. */
interface TokenRateModel {
  token: string;
  config: RateModelConfig;
  currentRateBps: number | null;
  utilizationBps: number;
}

function StatCard({
  label,
  value,
  sub,
  highlight,
}: {
  label: string;
  value: string;
  sub?: string;
  highlight?: boolean;
}) {
  return (
    <div className="p-5 bg-brand-card border border-brand-border rounded-2xl">
      <p className="text-xs text-brand-muted mb-1">{label}</p>
      <p className={`text-2xl font-bold truncate ${highlight ? 'text-brand-gold' : 'text-white'}`}>
        {value}
      </p>
      {sub && <p className="text-xs text-brand-muted mt-1">{sub}</p>}
    </div>
  );
}

function UtilizationBar({ deployed, total }: { deployed: bigint; total: bigint }) {
  const pct = total > 0n ? Number((deployed * 100n) / total) : 0;
  const barColor = pct > 90 ? 'bg-red-500' : pct > 70 ? 'bg-yellow-400' : 'bg-brand-gold';
  const textColor = pct > 90 ? 'text-red-400' : pct > 70 ? 'text-yellow-400' : 'text-white';
  return (
    <div className="space-y-1.5">
      <div className="flex justify-between text-xs text-brand-muted">
        <span>Utilization</span>
        <span className={textColor}>{pct.toFixed(1)}%</span>
      </div>
      <div className="h-2.5 bg-brand-dark rounded-full overflow-hidden border border-brand-border">
        <div
          className={`h-full rounded-full transition-all duration-500 ${barColor}`}
          style={{ width: `${Math.min(pct, 100)}%` }}
        />
      </div>
    </div>
  );
}

export default function AnalyticsPage() {
  const [config, setConfig] = useState<PoolConfig | null>(null);
  const [tokens, setTokens] = useState<TokenData[]>([]);
  const [rateModels, setRateModels] = useState<TokenRateModel[]>([]);
  const [statsLoading, setStatsLoading] = useState(true);
  const [statsError, setStatsError] = useState<string | null>(null);

  const [chartData, setChartData] = useState<AnalyticsDashboardData | null>(null);
  const [chartsLoading, setChartsLoading] = useState(true);
  const [lastRefresh, setLastRefresh] = useState<Date | null>(null);

  useEffect(() => {
    if (!POOL_CONFIGURED) {
      setStatsLoading(false);
      return;
    }
    loadStats();
  }, []);

  const loadCharts = useCallback(async () => {
    setChartsLoading(true);
    try {
      const data = await fetchAnalyticsData();
      setChartData(data);
      setLastRefresh(new Date());
    } catch {
      // non-fatal — charts degrade gracefully
    } finally {
      setChartsLoading(false);
    }
  }, []);

  useEffect(() => {
    loadCharts();
  }, [loadCharts]);

  useEffect(() => {
    const interval = setInterval(
      () => {
        clearAnalyticsCache();
        loadCharts();
      },
      5 * 60 * 1000,
    );
    return () => clearInterval(interval);
  }, [loadCharts]);

  async function loadStats() {
    try {
      const [cfg, acceptedTokens] = await Promise.all([getPoolConfig(), getAcceptedTokens()]);
      setConfig(cfg);
      const tokenData = await Promise.all(
        acceptedTokens.map(async (token) => ({
          token,
          totals: await getPoolTokenTotals(token),
        })),
      );
      setTokens(tokenData);

      // #863: load each token's rate model (if any) for the curve charts.
      const models = await Promise.all(
        tokenData.map(async ({ token, totals }) => {
          const modelConfig = await getRateModelConfig(token);
          if (!modelConfig) return null;
          const currentRateBps = await getCurrentRate(token);
          const utilizationBps =
            totals.totalDeposited > 0n
              ? Number((totals.totalDeployed * 10_000n) / totals.totalDeposited)
              : 0;
          return { token, config: modelConfig, currentRateBps, utilizationBps };
        }),
      );
      setRateModels(models.filter((m): m is TokenRateModel => m !== null));
    } catch (e) {
      setStatsError(e instanceof Error ? e.message : 'Failed to load analytics data.');
    } finally {
      setStatsLoading(false);
    }
  }

  const agg = tokens.reduce(
    (acc, { totals }) => ({
      poolValue: acc.poolValue + totals.totalDeposited,
      deployed: acc.deployed + totals.totalDeployed,
      paidOut: acc.paidOut + totals.totalPaidOut,
      feeRevenue: acc.feeRevenue + totals.totalFeeRevenue,
    }),
    { poolValue: 0n, deployed: 0n, paidOut: 0n, feeRevenue: 0n },
  );

  const available = agg.poolValue - agg.deployed;
  const apy = config ? (config.yieldBps / 100).toFixed(2) : '–';

  return (
    <div className="min-h-screen pt-24 pb-16 px-6">
      <div className="max-w-5xl mx-auto">
        {/* Header */}
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 mb-8">
          <div>
            <h1 className="text-3xl font-bold mb-1">Pool Analytics</h1>
            <p className="text-brand-muted">
              Real-time performance metrics and historical trends for the Astera liquidity pool.
            </p>
          </div>
          <div className="flex items-center gap-3 shrink-0">
            {lastRefresh && (
              <span className="text-xs text-brand-muted hidden sm:block">
                Updated {lastRefresh.toLocaleTimeString()}
              </span>
            )}
            <button
              onClick={() => {
                clearAnalyticsCache();
                loadCharts();
              }}
              disabled={chartsLoading}
              className="px-4 py-2 bg-brand-gold text-brand-dark text-sm font-bold rounded-xl hover:bg-brand-gold-light disabled:opacity-50 transition-all"
            >
              {chartsLoading ? 'Loading…' : 'Refresh'}
            </button>
          </div>
        </div>

        {!POOL_CONFIGURED && (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-brand-muted text-sm mb-6">
            Pool contracts are not yet deployed. Configure{' '}
            <code className="text-brand-gold text-xs">NEXT_PUBLIC_POOL_CONTRACT_ID</code> to see
            live data.
          </div>
        )}

        {/* Live pool stats */}
        {POOL_CONFIGURED && (
          <>
            {statsLoading ? (
              <div className="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
                <StatCardSkeleton />
                <StatCardSkeleton />
                <StatCardSkeleton />
                <StatCardSkeleton />
              </div>
            ) : statsError ? (
              <div className="p-4 bg-red-900/20 border border-red-800/50 rounded-2xl text-red-400 text-sm mb-6">
                {statsError}
              </div>
            ) : (
              <div className="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
                <StatCard
                  label="Total Pool Value"
                  value={formatUSDC(agg.poolValue)}
                  sub="Net asset value"
                  highlight
                />
                <StatCard
                  label="Deployed Capital"
                  value={formatUSDC(agg.deployed)}
                  sub="Funding active invoices"
                />
                <StatCard
                  label="Available Liquidity"
                  value={formatUSDC(available)}
                  sub="Ready to deploy"
                />
                <StatCard label="Target APY" value={`${apy}%`} sub="Current yield rate" highlight />
              </div>
            )}

            {/* Per-token utilization */}
            {!statsLoading && !statsError && tokens.length > 0 && (
              <div className="p-6 bg-brand-card border border-brand-border rounded-2xl mb-6">
                <h2 className="text-lg font-semibold mb-4">Capital Allocation by Token</h2>
                <div className="space-y-6">
                  {tokens.map(({ token, totals }) => (
                    <div key={token} className="space-y-3">
                      <p className="text-sm font-medium">{stablecoinLabel(token)}</p>
                      <UtilizationBar
                        deployed={totals.totalDeployed}
                        total={totals.totalDeposited}
                      />
                      <div className="grid grid-cols-3 gap-2 text-xs">
                        <div className="text-center p-2 bg-brand-dark rounded-lg">
                          <p className="text-brand-muted mb-0.5">Pool NAV</p>
                          <p className="text-white font-medium">
                            {formatUSDC(totals.totalDeposited)}
                          </p>
                        </div>
                        <div className="text-center p-2 bg-brand-dark rounded-lg">
                          <p className="text-brand-muted mb-0.5">Deployed</p>
                          <p className="text-brand-gold font-medium">
                            {formatUSDC(totals.totalDeployed)}
                          </p>
                        </div>
                        <div className="text-center p-2 bg-brand-dark rounded-lg">
                          <p className="text-brand-muted mb-0.5">Available</p>
                          <p className="text-white font-medium">
                            {formatUSDC(totals.totalDeposited - totals.totalDeployed)}
                          </p>
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* #863: utilization-driven rate curves (one per configured token) */}
            {!statsLoading && !statsError && rateModels.length > 0 && (
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-6">
                {rateModels.map((m) => (
                  <RateCurveChart
                    key={m.token}
                    config={m.config}
                    currentUtilizationBps={m.utilizationBps}
                    currentRateBps={m.currentRateBps ?? undefined}
                    title={`Rate Curve — ${stablecoinLabel(m.token)}`}
                  />
                ))}
              </div>
            )}
          </>
        )}

        {/* Charts */}
        <div className="space-y-6">
          {chartsLoading ? (
            <>
              <ChartSkeleton />
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
                <ChartSkeleton />
                <ChartSkeleton />
              </div>
            </>
          ) : (
            <>
              <PoolUtilizationChart data={chartData?.poolUtilization ?? []} isLoading={false} />
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
                <YieldPerformanceChart data={chartData?.yieldPerformance ?? []} isLoading={false} />
                <InvoiceFunnelChart data={chartData?.invoiceFunnel ?? []} isLoading={false} />
              </div>
              <RecentEventsFeed events={chartData?.recentEvents ?? []} isLoading={false} />
            </>
          )}
        </div>

        <div className="mt-6 p-4 bg-brand-dark border border-brand-border rounded-2xl text-xs text-brand-muted">
          Historical trends are derived from on-chain event data and current pool state. Data
          refreshes every 5 minutes.
        </div>

        <div className="flex items-center gap-4 mt-6">
          <Link
            href="/invest"
            className="px-5 py-2.5 bg-brand-gold text-brand-dark font-semibold rounded-xl hover:bg-brand-amber transition-colors text-sm"
          >
            Invest Now
          </Link>
          <Link
            href="/portfolio"
            className="px-5 py-2.5 border border-brand-border text-white rounded-xl hover:border-brand-gold/50 transition-colors text-sm"
          >
            View Portfolio
          </Link>
        </div>
      </div>
    </div>
  );
}
