'use client';

import { useEffect, useState, useCallback } from 'react';
import { useStore } from '@/lib/store';
import { StatCardSkeleton, Skeleton } from '@/components/Skeleton';
import {
  getInvestorPosition,
  getPoolConfig,
  getAcceptedTokens,
  getPoolTokenTotals,
} from '@/lib/contracts';
import { formatUSDC, stablecoinLabel } from '@/lib/stellar';
import type { PoolTokenTotals } from '@/lib/types';

interface PortfolioSnapshot {
  totalDeposited: bigint;
  available: bigint;
  deployed: bigint;
  earned: bigint;
  depositCount: number;
}

interface TokenRow {
  token: string;
  totals: PoolTokenTotals;
  position: PortfolioSnapshot | null;
}

function TokenPositionSkeleton() {
  return (
    <div className="bg-brand-card border border-brand-border rounded-2xl p-6 animate-pulse">
      <div className="flex items-center justify-between mb-4">
        <Skeleton className="h-5 w-20" />
        <Skeleton className="h-5 w-28 rounded-lg" />
      </div>

      <div className="grid grid-cols-2 sm:grid-cols-4 gap-4 text-sm">
        {[1, 2, 3, 4].map((i) => (
          <div key={i}>
            <Skeleton className="h-3 w-16 mb-1" />
            <Skeleton className="h-5 w-24" />
          </div>
        ))}
      </div>

      <div className="mt-4 pt-4 border-t border-brand-border">
        <div className="space-y-1">
          <div className="flex justify-between text-xs">
            <Skeleton className="h-3 w-24" />
            <Skeleton className="h-3 w-8" />
          </div>
          <div className="h-2 rounded-full bg-brand-border overflow-hidden">
            <div className="h-full rounded-full bg-brand-gold/40" style={{ width: '45%' }} />
          </div>
        </div>
      </div>
    </div>
  );
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
    <div className="bg-brand-card border border-brand-border rounded-2xl p-6 flex flex-col gap-1">
      <p className="text-brand-muted text-sm font-medium">{label}</p>
      <p className={`text-2xl font-bold ${highlight ? 'text-brand-gold' : 'text-white'}`}>
        {value}
      </p>
      {sub && <p className="text-xs text-brand-muted mt-1">{sub}</p>}
    </div>
  );
}

function UtilisationBar({ utilisation }: { utilisation: number }) {
  const pct = Math.min(100, Math.round(utilisation * 100));
  const color = pct >= 90 ? 'bg-red-500' : pct >= 70 ? 'bg-yellow-400' : 'bg-brand-gold';
  return (
    <div className="space-y-1">
      <div className="flex justify-between text-xs text-brand-muted">
        <span>Pool utilisation</span>
        <span>{pct}%</span>
      </div>
      <div className="h-2 rounded-full bg-brand-border overflow-hidden">
        <div
          className={`h-full rounded-full transition-all duration-500 ${color}`}
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  );
}

export default function PortfolioPage() {
  const { wallet } = useStore();
  const [rows, setRows] = useState<TokenRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [lastRefresh, setLastRefresh] = useState<Date | null>(null);

  const load = useCallback(async () => {
    if (!wallet.connected || !wallet.address) return;

    setLoading(true);
    setError(null);
    try {
      const [tokens] = await Promise.all([getAcceptedTokens(), getPoolConfig()]);

      const rowData: TokenRow[] = await Promise.all(
        tokens.map(async (token) => {
          const [totals, rawPos] = await Promise.all([
            getPoolTokenTotals(token),
            getInvestorPosition(wallet.address!, token),
          ]);

          const position: PortfolioSnapshot | null = rawPos
            ? {
                totalDeposited: rawPos.deposited,
                available: rawPos.available,
                deployed: rawPos.deployed,
                earned: rawPos.earned,
                depositCount: rawPos.depositCount,
              }
            : null;

          return { token, totals, position };
        }),
      );

      setRows(rowData);
      setLastRefresh(new Date());
    } catch (e) {
      console.error('[Portfolio] Load error:', e);
      setError('Failed to load portfolio data. Please try again.');
    } finally {
      setLoading(false);
    }
  }, [wallet.address, wallet.connected]);

  useEffect(() => {
    load();
  }, [load]);

  if (!wallet.connected) {
    return (
      <div className="min-h-screen pt-24 flex items-center justify-center">
        <div className="text-center space-y-3">
          <p className="text-brand-muted text-lg">Connect your wallet to view your portfolio.</p>
        </div>
      </div>
    );
  }

  // Aggregate totals across all tokens (using USDC equivalents, single-token
  // pools are common so we just sum the raw bigint values per-field)
  const aggregate = rows.reduce(
    (acc, r) => {
      if (!r.position) return acc;
      return {
        deposited: acc.deposited + r.position.totalDeposited,
        available: acc.available + r.position.available,
        deployed: acc.deployed + r.position.deployed,
        earned: acc.earned + r.position.earned,
        depositCount: acc.depositCount + r.position.depositCount,
      };
    },
    { deposited: 0n, available: 0n, deployed: 0n, earned: 0n, depositCount: 0 },
  );

  const totalPoolDeployed = rows.reduce((a, r) => a + r.totals.totalDeployed, 0n);
  const totalPoolDeposited = rows.reduce((a, r) => a + r.totals.totalDeposited, 0n);
  const utilisation =
    totalPoolDeposited > 0n ? Number(totalPoolDeployed) / Number(totalPoolDeposited) : 0;

  return (
    <div className="min-h-screen pt-24 pb-16 px-4 max-w-7xl mx-auto">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 mb-8">
        <div>
          <h1 className="text-3xl font-bold text-white">Investor Portfolio</h1>
          <p className="text-brand-muted mt-1 text-sm">
            {lastRefresh
              ? `Last updated: ${lastRefresh.toLocaleTimeString()}`
              : 'Loading your positions…'}
          </p>
        </div>
        <button
          onClick={load}
          disabled={loading}
          className="flex items-center gap-2 px-4 py-2 bg-brand-card border border-brand-border rounded-xl text-sm text-white hover:bg-brand-border transition-colors disabled:opacity-50"
        >
          {loading ? (
            <span className="w-4 h-4 border-2 border-brand-gold border-t-transparent rounded-full animate-spin" />
          ) : null}
          Refresh
        </button>
      </div>

      {/* Error state */}
      {error && (
        <div
          role="alert"
          className="mb-6 flex items-center justify-between bg-red-900/30 border border-red-800/50 text-red-400 rounded-xl px-4 py-3 text-sm"
        >
          <span>{error}</span>
          <button onClick={load} className="underline ml-4 shrink-0">
            Retry
          </button>
        </div>
      )}

      {/* Loading skeleton */}
      {loading && rows.length === 0 && (
        <div className="space-y-6">
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
            <StatCardSkeleton />
            <StatCardSkeleton />
            <StatCardSkeleton />
            <StatCardSkeleton />
          </div>
          <div className="bg-brand-card border border-brand-border rounded-2xl p-6 animate-pulse">
            <Skeleton className="h-5 w-40 mb-4" />
            <div className="space-y-1">
              <div className="flex justify-between text-xs">
                <Skeleton className="h-3 w-24" />
                <Skeleton className="h-3 w-8" />
              </div>
              <div className="h-2 rounded-full bg-brand-border overflow-hidden">
                <div className="h-full rounded-full bg-brand-gold/40" style={{ width: '60%' }} />
              </div>
            </div>
            <div className="grid grid-cols-2 sm:grid-cols-4 gap-4 mt-6 text-sm">
              {[1, 2, 3, 4].map((i) => (
                <div key={i}>
                  <Skeleton className="h-3 w-20 mb-1" />
                  <Skeleton className="h-5 w-24" />
                </div>
              ))}
            </div>
          </div>
          <TokenPositionSkeleton />
        </div>
      )}

      {!loading && rows.length === 0 && !error && (
        <div className="text-center py-24 text-brand-muted">
          No pool tokens found. Contact the pool admin.
        </div>
      )}

      {rows.length > 0 && (
        <>
          {/* Summary stats */}
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-8">
            <StatCard
              label="Total Deposited"
              value={formatUSDC(aggregate.deposited)}
              sub={`Across ${aggregate.depositCount} deposit(s)`}
            />
            <StatCard
              label="Available Liquidity"
              value={formatUSDC(aggregate.available)}
              sub="Withdrawable now"
            />
            <StatCard
              label="Deployed Capital"
              value={formatUSDC(aggregate.deployed)}
              sub="Funding active invoices"
            />
            <StatCard
              label="Yield Earned"
              value={formatUSDC(aggregate.earned)}
              sub="Cumulative interest"
              highlight
            />
          </div>

          {/* Pool utilisation */}
          <div className="bg-brand-card border border-brand-border rounded-2xl p-6 mb-8">
            <h2 className="text-white font-semibold mb-4">Pool Utilisation</h2>
            <UtilisationBar utilisation={utilisation} />
            <div className="grid grid-cols-2 sm:grid-cols-4 gap-4 mt-6 text-sm">
              <div>
                <p className="text-brand-muted">Total Pool Deposits</p>
                <p className="text-white font-medium">{formatUSDC(totalPoolDeposited)}</p>
              </div>
              <div>
                <p className="text-brand-muted">Total Deployed</p>
                <p className="text-white font-medium">{formatUSDC(totalPoolDeployed)}</p>
              </div>
              <div>
                <p className="text-brand-muted">Total Paid Out</p>
                <p className="text-white font-medium">
                  {formatUSDC(rows.reduce((a, r) => a + r.totals.totalPaidOut, 0n))}
                </p>
              </div>
              <div>
                <p className="text-brand-muted">Fee Revenue</p>
                <p className="text-brand-gold font-medium">
                  {formatUSDC(rows.reduce((a, r) => a + r.totals.totalFeeRevenue, 0n))}
                </p>
              </div>
            </div>
          </div>

          {/* Per-token positions */}
          <div className="space-y-4">
            <h2 className="text-white font-semibold text-lg">Token Positions</h2>
            {rows.map(({ token, position, totals }) => (
              <div key={token} className="bg-brand-card border border-brand-border rounded-2xl p-6">
                <div className="flex items-center justify-between mb-4">
                  <h3 className="text-white font-medium text-base">{stablecoinLabel(token)}</h3>
                  {position ? (
                    <span className="text-xs px-2 py-1 rounded-lg bg-green-900/30 border border-green-800/50 text-green-400">
                      Active position
                    </span>
                  ) : (
                    <span className="text-xs px-2 py-1 rounded-lg bg-brand-border text-brand-muted">
                      No position
                    </span>
                  )}
                </div>

                {position ? (
                  <div className="grid grid-cols-2 sm:grid-cols-4 gap-4 text-sm">
                    <div>
                      <p className="text-brand-muted">Deposited</p>
                      <p className="text-white font-medium">
                        {formatUSDC(position.totalDeposited)}
                      </p>
                    </div>
                    <div>
                      <p className="text-brand-muted">Available</p>
                      <p className="text-white font-medium">{formatUSDC(position.available)}</p>
                    </div>
                    <div>
                      <p className="text-brand-muted">Deployed</p>
                      <p className="text-white font-medium">{formatUSDC(position.deployed)}</p>
                    </div>
                    <div>
                      <p className="text-brand-muted">Earned</p>
                      <p className="text-brand-gold font-medium">{formatUSDC(position.earned)}</p>
                    </div>
                  </div>
                ) : (
                  <p className="text-brand-muted text-sm">
                    You have no {stablecoinLabel(token)} position in this pool yet.
                  </p>
                )}

                {/* Token pool summary */}
                <div className="mt-4 pt-4 border-t border-brand-border">
                  <UtilisationBar
                    utilisation={
                      totals.totalDeposited > 0n
                        ? Number(totals.totalDeployed) / Number(totals.totalDeposited)
                        : 0
                    }
                  />
                </div>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
