'use client';

import { useEffect, useState, useCallback } from 'react';
import {
  PoolUtilizationChart,
  YieldPerformanceChart,
  InvoiceFunnelChart,
  CreditScoreDistributionChart,
  TopSmesTable,
  RecentEventsFeed,
} from '@/components/analytics';
import {
  fetchAnalyticsData,
  clearAnalyticsCache,
  type AnalyticsDashboardData,
} from '@/lib/analytics';

export default function AnalyticsPage() {
  const [data, setData] = useState<AnalyticsDashboardData | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [lastRefresh, setLastRefresh] = useState<Date | null>(null);

  const loadData = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const result = await fetchAnalyticsData();
      setData(result);
      setLastRefresh(new Date());
    } catch (err) {
      console.error('[Analytics] Failed to load dashboard data:', err);
      setError('Failed to load analytics data. Please try again.');
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  // Auto-refresh every 5 minutes (matching cache TTL)
  useEffect(() => {
    const interval = setInterval(
      () => {
        clearAnalyticsCache();
        loadData();
      },
      5 * 60 * 1000,
    );
    return () => clearInterval(interval);
  }, [loadData]);

  const handleRefresh = () => {
    clearAnalyticsCache();
    loadData();
  };

  return (
    <div className="space-y-8 animate-in fade-in duration-700">
      {/* Header */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div>
          <h1 className="text-3xl font-bold text-white mb-2">Analytics Dashboard</h1>
          <p className="text-brand-muted">
            Protocol performance metrics, invoice funnel analysis, and on-chain activity.
          </p>
        </div>
        <div className="flex items-center gap-3">
          {lastRefresh && (
            <span className="text-xs text-brand-muted">
              Last updated: {lastRefresh.toLocaleTimeString()}
            </span>
          )}
          <button
            onClick={handleRefresh}
            disabled={isLoading}
            className="bg-brand-gold hover:bg-brand-gold-light disabled:opacity-50 text-brand-dark px-4 py-2 rounded-xl text-sm font-bold transition-all"
          >
            {isLoading ? 'Loading...' : 'Refresh Data'}
          </button>
        </div>
      </div>

      {/* Error State */}
      {error && (
        <div className="bg-red-500/10 border border-red-500/30 rounded-2xl p-4 text-red-400 text-sm">
          {error}
        </div>
      )}

      {/* Charts Grid */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Pool Utilization Chart */}
        <div className="lg:col-span-2">
          <PoolUtilizationChart data={data?.poolUtilization ?? []} isLoading={isLoading} />
        </div>

        {/* Yield Performance Chart */}
        <YieldPerformanceChart data={data?.yieldPerformance ?? []} isLoading={isLoading} />

        {/* Invoice Funnel Chart */}
        <InvoiceFunnelChart data={data?.invoiceFunnel ?? []} isLoading={isLoading} />

        {/* Credit Score Distribution */}
        <CreditScoreDistributionChart
          data={data?.creditScoreDistribution ?? []}
          isLoading={isLoading}
        />
      </div>

      {/* Bottom Section: Top SMEs + Recent Events */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <TopSmesTable data={data?.topSmes ?? []} isLoading={isLoading} />
        <RecentEventsFeed events={data?.recentEvents ?? []} isLoading={isLoading} />
      </div>
    </div>
  );
}
