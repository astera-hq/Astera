'use client';

/**
 * Protocol Revenue Dashboard (#807)
 *
 * Surfaces the Astera protocol's factoring-fee revenue over time, treasury
 * status, and funded-volume metrics — all previously only visible by querying
 * the blockchain directly.
 *
 * Sections:
 * 1. Revenue Overview — cumulative fees, pending (unclaimed) fees, average fee
 *    per invoice, current fee rate
 * 2. Revenue Trend — monthly fee revenue chart (past 12 months)
 * 3. Fee Rate vs. Funded Volume — composed chart over the same window
 * 4. Treasury Status — per-token treasury balance (30s polling), last
 *    withdrawal, treasury address
 * 5. Volume Metrics — all-time funded volume, active today/week/month, unique
 *    borrowers & lenders
 *
 * Access: lives under `/admin`, so it is gated by the admin layout guard.
 * Detailed fee breakdowns are admin-only per the issue's access control.
 */

import { useEffect, useState, useCallback } from 'react';
import { useTranslations } from 'next-intl';
import { RevenueTrendChart, FeeVolumeChart } from '@/components/analytics';
import {
  fetchRevenueData,
  fetchTreasurySnapshot,
  clearRevenueCache,
  type RevenueDashboardData,
  type TreasurySnapshot,
} from '@/lib/revenue';
import { formatUSDC, truncateAddress } from '@/lib/stellar';

const REFRESH_INTERVAL_MS = 5 * 60 * 1000; // charts/data (matches cache TTL)
const TREASURY_POLL_MS = 30_000; // #807: treasury balance real-time polling

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

export default function RevenuePage() {
  const t = useTranslations('Admin.revenue');
  const [data, setData] = useState<RevenueDashboardData | null>(null);
  const [treasury, setTreasury] = useState<TreasurySnapshot | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [lastRefresh, setLastRefresh] = useState<Date | null>(null);

  const loadData = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const result = await fetchRevenueData();
      setData(result);
      setLastRefresh(new Date());
    } catch (err) {
      console.error('[Revenue] Failed to load dashboard data:', err);
      setError(t('error'));
    } finally {
      setIsLoading(false);
    }
  }, [t]);

  const loadTreasury = useCallback(async () => {
    // Nothing to poll when the pool has no treasury configured — avoid
    // pointless RPC churn every 30s against an unconfigured pool.
    if (data && !data.treasuryAddress) {
      setTreasury({ treasuryAddress: null, balances: [] });
      return;
    }
    const snapshot = await fetchTreasurySnapshot();
    setTreasury(snapshot);
  }, [data]);

  // Initial load + 5-minute data refresh (matching the service cache TTL).
  useEffect(() => {
    loadData();
    const interval = setInterval(() => {
      clearRevenueCache();
      loadData();
    }, REFRESH_INTERVAL_MS);
    return () => clearInterval(interval);
  }, [loadData]);

  // #807: treasury balance updates in real-time (30s polling).
  useEffect(() => {
    loadTreasury();
    const interval = setInterval(loadTreasury, TREASURY_POLL_MS);
    return () => clearInterval(interval);
  }, [loadTreasury]);

  const handleRefresh = () => {
    clearRevenueCache();
    loadData();
    loadTreasury();
  };

  const lastWithdrawal = data?.lastWithdrawal;

  return (
    <div className="space-y-8 animate-in fade-in duration-700">
      {/* Header */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div>
          <h1 className="text-3xl font-bold text-white mb-2">{t('title')}</h1>
          <p className="text-brand-muted">{t('description')}</p>
        </div>
        <div className="flex items-center gap-3">
          {lastRefresh && (
            <span className="text-xs text-brand-muted">
              {t('lastUpdated', { time: lastRefresh.toLocaleTimeString() })}
            </span>
          )}
          <button
            onClick={handleRefresh}
            disabled={isLoading}
            className="bg-brand-gold hover:bg-brand-gold-light disabled:opacity-50 text-brand-dark px-4 py-2 rounded-xl text-sm font-bold transition-all"
          >
            {isLoading ? t('loading') : t('refresh')}
          </button>
        </div>
      </div>

      {error && (
        <div className="bg-red-500/10 border border-red-500/30 rounded-2xl p-4 text-red-400 text-sm">
          {error}
        </div>
      )}

      {/* 1. Revenue Overview */}
      <section>
        <h2 className="text-xs font-bold text-brand-muted uppercase tracking-widest mb-4">
          {t('sectionRevenue')}
        </h2>
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
          <StatCard
            label={t('cumulativeFees')}
            value={formatUSDC(data?.cumulativeFees ?? 0n)}
            sub={t('cumulativeFeesDesc')}
            highlight
          />
          <StatCard
            label={t('pendingFees')}
            value={formatUSDC(data?.pendingFees ?? 0n)}
            sub={t('pendingFeesDesc')}
          />
          <StatCard
            label={t('avgFeePerInvoice')}
            value={data ? `$${(data.averageFeePerInvoice ?? 0).toFixed(2)}` : '—'}
            sub={t('avgFeePerInvoiceDesc')}
          />
          <StatCard
            label={t('feeRate')}
            value={data ? `${(data.feeRateBps / 100).toFixed(2)}%` : '—'}
            sub={t('feeRateDesc')}
          />
        </div>
      </section>

      {/* Per-token breakdown */}
      {data && data.perToken.length > 0 && (
        <section>
          <h2 className="text-xs font-bold text-brand-muted uppercase tracking-widest mb-4">
            {t('sectionPerToken')}
          </h2>
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="text-left text-xs text-brand-muted uppercase tracking-wider">
                  <th className="pb-3">{t('tableToken')}</th>
                  <th className="pb-3 text-right">{t('tableCumulative')}</th>
                  <th className="pb-3 text-right">{t('tablePending')}</th>
                  <th className="pb-3 text-right">{t('tableTreasury')}</th>
                </tr>
              </thead>
              <tbody>
                {data.perToken.map((row) => (
                  <tr key={row.token} className="border-t border-brand-border/50">
                    <td className="py-3 font-medium text-white">{row.label}</td>
                    <td className="py-3 text-right text-brand-gold">
                      {formatUSDC(row.cumulativeFees)}
                    </td>
                    <td className="py-3 text-right">{formatUSDC(row.pendingFees)}</td>
                    <td className="py-3 text-right">{formatUSDC(row.treasuryBalance)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>
      )}

      {/* 2 & 3. Charts */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <RevenueTrendChart
          data={data?.monthly ?? []}
          isLoading={isLoading}
          title={t('monthlyRevenueTitle')}
        />
        <RevenueTrendChart
          data={data?.weekly ?? []}
          isLoading={isLoading}
          title={t('weeklyRevenueTitle')}
        />
      </div>
      <div className="grid grid-cols-1 gap-6">
        <FeeVolumeChart data={data?.feeVsVolume ?? []} isLoading={isLoading} />
      </div>

      {/* 4. Treasury Status */}
      <section>
        <h2 className="text-xs font-bold text-brand-muted uppercase tracking-widest mb-4">
          {t('sectionTreasury')}
        </h2>
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
            <h3 className="text-lg font-bold text-white mb-4 flex items-center gap-2">
              <span className="w-2 h-6 bg-emerald-500 rounded-full" />
              {t('treasuryBalances')}
            </h3>
            <div className="space-y-3">
              {treasury && treasury.balances.length > 0 ? (
                treasury.balances.map((b) => (
                  <div
                    key={b.token}
                    className="flex items-center justify-between p-3 bg-brand-dark rounded-xl"
                  >
                    <div>
                      <p className="font-medium text-white">{b.label}</p>
                      <p className="text-xs text-brand-muted">{truncateAddress(b.token)}</p>
                    </div>
                    <p className="font-bold text-white">{formatUSDC(b.balance)}</p>
                  </div>
                ))
              ) : (
                <p className="text-sm text-brand-muted">
                  {data && !data.treasuryAddress ? t('treasuryNotConfigured') : t('noTreasuryData')}
                </p>
              )}
            </div>
            {treasury?.treasuryAddress && (
              <p className="text-xs text-brand-muted mt-4">
                {t('treasuryAddressLabel')}{' '}
                <span className="font-mono">{truncateAddress(treasury.treasuryAddress)}</span>
              </p>
            )}
          </div>

          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
            <h3 className="text-lg font-bold text-white mb-4 flex items-center gap-2">
              <span className="w-2 h-6 bg-amber-500 rounded-full" />
              {t('withdrawals')}
            </h3>
            {lastWithdrawal ? (
              <div className="space-y-2 text-sm">
                <div className="flex justify-between py-2 border-b border-brand-border/50">
                  <span className="text-brand-muted">{t('lastWithdrawal')}</span>
                  <span className="font-bold text-white">
                    {formatUSDC(lastWithdrawal.amount)} {lastWithdrawal.label}
                  </span>
                </div>
                <div className="flex justify-between py-2 border-b border-brand-border/50">
                  <span className="text-brand-muted">{t('lastWithdrawalDate')}</span>
                  <span className="text-white">
                    {new Date(lastWithdrawal.at * 1000).toLocaleDateString('en-US', {
                      year: 'numeric',
                      month: 'short',
                      day: 'numeric',
                    })}
                  </span>
                </div>
                <div className="flex justify-between py-2">
                  <span className="text-brand-muted">{t('toTreasury')}</span>
                  <span className="font-mono text-white">
                    {truncateAddress(lastWithdrawal.treasury)}
                  </span>
                </div>
              </div>
            ) : (
              <p className="text-sm text-brand-muted">{t('noWithdrawals')}</p>
            )}
            <p className="text-xs text-brand-muted mt-4">{t('treasuryPollNote')}</p>
          </div>
        </div>
      </section>

      {/* 5. Volume Metrics */}
      <section>
        <h2 className="text-xs font-bold text-brand-muted uppercase tracking-widest mb-4">
          {t('sectionVolume')}
        </h2>
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
          <StatCard
            label={t('totalFundedVolume')}
            value={formatUSDC(data?.volume.totalFunded ?? 0n)}
            sub={t('totalFundedVolumeDesc')}
          />
          <StatCard
            label={t('activeToday')}
            value={formatUSDC(data?.volume.activeToday ?? 0n)}
            sub={t('activeTodayDesc')}
          />
          <StatCard
            label={t('activeWeek')}
            value={formatUSDC(data?.volume.activeThisWeek ?? 0n)}
            sub={t('activeWeekDesc')}
          />
          <StatCard
            label={t('activeMonth')}
            value={formatUSDC(data?.volume.activeThisMonth ?? 0n)}
            sub={t('activeMonthDesc')}
          />
          <StatCard
            label={t('uniqueBorrowers')}
            value={String(data?.volume.uniqueBorrowers ?? 0)}
            sub={t('uniqueBorrowersDesc')}
          />
          <StatCard
            label={t('uniqueLenders')}
            value={String(data?.volume.uniqueLenders ?? 0)}
            sub={t('uniqueLendersDesc')}
          />
        </div>
      </section>

      <p className="text-xs text-brand-muted">{t('dataNote')}</p>
    </div>
  );
}
