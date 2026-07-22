'use client';

import { useEffect, useState, useCallback } from 'react';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { StatCardSkeleton, Skeleton } from '@/components/Skeleton';
import { LiquidityForecastChart } from '@/components/analytics/LiquidityForecastChart';
import {
  getInvestorPosition,
  getPoolConfig,
  getAcceptedTokens,
  getPoolTokenTotals,
  getExchangeRate,
  estimateWithdrawalWait,
  getLiquidityForecast,
  buildRequestWithdrawalTx,
  buildCancelWithdrawalRequestTx,
  buildDrainWithdrawalQueueTx,
  submitTx,
} from '@/lib/contracts';
import { formatUSDC, stablecoinLabel, toStroops } from '@/lib/stellar';
import type { PoolTokenTotals, WaitEstimate, LiquidityForecastPoint } from '@/lib/types';

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
  waitEstimate: WaitEstimate | null;
  forecast: LiquidityForecastPoint[];
  /** Exchange rate in bps (10_000 = 1:1 USD) */
  rateBps: number;
}

/** Render a seconds duration as a short human string, e.g. "3h", "2d", "~1mo". */
function formatWaitEstimate(secs: number): string {
  if (secs <= 0) return 'moments';
  const day = 86_400;
  if (secs < 3_600) return `${Math.max(1, Math.round(secs / 60))}m`;
  if (secs < day) return `${Math.round(secs / 3_600)}h`;
  if (secs < day * 60) return `${Math.round(secs / day)}d`;
  return `~${Math.round(secs / (day * 30))}mo`;
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

/** Simple SVG pie chart for token allocation */
function AllocationPie({ slices }: { slices: { label: string; pct: number; color: string }[] }) {
  const COLORS = ['#F5A623', '#4ADE80', '#60A5FA', '#F472B6', '#A78BFA'];
  const r = 40;
  const cx = 50;
  const cy = 50;

  function polarToXY(pct: number) {
    const angle = (pct / 100) * 2 * Math.PI - Math.PI / 2;
    return { x: cx + r * Math.cos(angle), y: cy + r * Math.sin(angle) };
  }

  const cumulativeEnds = slices.reduce<number[]>((acc, s) => {
    const prev = acc.length > 0 ? acc[acc.length - 1] ?? 0 : 0;
    return [...acc, prev + s.pct];
  }, []);

  const paths = slices.map((s, i) => {
    const startPct = i === 0 ? 0 : cumulativeEnds[i - 1]!;
    const endPct = cumulativeEnds[i]!;
    const start = polarToXY(startPct);
    const end = polarToXY(endPct);
    const large = s.pct > 50 ? 1 : 0;
    const d = `M ${cx} ${cy} L ${start.x} ${start.y} A ${r} ${r} 0 ${large} 1 ${end.x} ${end.y} Z`;
    return <path key={s.label} d={d} fill={COLORS[i % COLORS.length]} opacity={0.85} />;
  });

  return (
    <div className="flex items-center gap-6">
      <svg viewBox="0 0 100 100" className="w-24 h-24 shrink-0">
        {paths}
      </svg>
      <div className="space-y-1.5">
        {slices.map((s, i) => (
          <div key={s.label} className="flex items-center gap-2 text-xs">
            <span
              className="w-2.5 h-2.5 rounded-full shrink-0"
              style={{ background: COLORS[i % COLORS.length] }}
            />
            <span className="text-brand-muted">{s.label}</span>
            <span className="text-white font-medium ml-auto">{s.pct.toFixed(1)}%</span>
          </div>
        ))}
      </div>
    </div>
  );
}

/** Convert raw token amount to USDC-equivalent using rateBps */
function toUsdcEquiv(amount: bigint, rateBps: number): bigint {
  return (amount * BigInt(rateBps)) / 10_000n;
}

export default function PortfolioPage() {
  const { wallet } = useStore();
  const [rows, setRows] = useState<TokenRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [lastRefresh, setLastRefresh] = useState<Date | null>(null);
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [requestAmounts, setRequestAmounts] = useState<Record<string, string>>({});
  const [queueActionLoading, setQueueActionLoading] = useState<Record<string, boolean>>({});

  const load = useCallback(async () => {
    if (!wallet.connected || !wallet.address) return;

    setLoading(true);
    setError(null);
    try {
      const [tokens] = await Promise.all([getAcceptedTokens(), getPoolConfig()]);

      const rowData: TokenRow[] = await Promise.all(
        tokens.map(async (token) => {
          const [totals, rawPos, rateBps, waitEstimate, forecast] = await Promise.all([
            getPoolTokenTotals(token),
            getInvestorPosition(wallet.address!, token),
            getExchangeRate(token).catch(() => 10_000),
            estimateWithdrawalWait(wallet.address!, token).catch(() => null),
            getLiquidityForecast(token, 30).catch(() => []),
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

          return { token, totals, position, waitEstimate, forecast, rateBps };
        }),
      );

      setRows(rowData);
      setLastRefresh(new Date());

      // Auto-collapse per-token sections when > 1 token
      if (rowData.length > 1) {
        const initial: Record<string, boolean> = {};
        rowData.forEach((r) => {
          initial[r.token] = true;
        });
        setCollapsed((prev) => ({ ...initial, ...prev }));
      }
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

  async function signAndSubmit(xdrPromise: Promise<string>): Promise<void> {
    const xdr = await xdrPromise;
    const freighter = await import('@stellar/freighter-api');
    const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
      networkPassphrase: 'Test SDF Network ; September 2015',
      address: wallet.address!,
    });
    if (signError) throw new Error(signError.message);
    await submitTx(signedTxXdr);
  }

  async function handleRequestWithdrawal(token: string) {
    if (!wallet.address) return;
    const raw = requestAmounts[token];
    if (!raw || parseFloat(raw) <= 0) return;

    setQueueActionLoading((p) => ({ ...p, [token]: true }));
    try {
      const shares = toStroops(parseFloat(raw));
      await signAndSubmit(buildRequestWithdrawalTx(wallet.address, token, shares));
      toast.success(`Withdrawal request submitted for ${formatUSDC(shares)}.`);
      setRequestAmounts((p) => ({ ...p, [token]: '' }));
      await load();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Failed to request withdrawal.');
    } finally {
      setQueueActionLoading((p) => ({ ...p, [token]: false }));
    }
  }

  async function handleCancelWithdrawal(token: string) {
    if (!wallet.address) return;
    setQueueActionLoading((p) => ({ ...p, [token]: true }));
    try {
      await signAndSubmit(buildCancelWithdrawalRequestTx(wallet.address, token));
      toast.success('Withdrawal request cancelled.');
      await load();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Failed to cancel withdrawal request.');
    } finally {
      setQueueActionLoading((p) => ({ ...p, [token]: false }));
    }
  }

  async function handleDrainQueue(token: string) {
    if (!wallet.address) return;
    setQueueActionLoading((p) => ({ ...p, [token]: true }));
    try {
      await signAndSubmit(buildDrainWithdrawalQueueTx(wallet.address, token));
      toast.success('Drain attempt submitted.');
      await load();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Failed to drain withdrawal queue.');
    } finally {
      setQueueActionLoading((p) => ({ ...p, [token]: false }));
    }
  }

  if (!wallet.connected) {
    return (
      <div className="min-h-screen pt-24 flex items-center justify-center">
        <div className="text-center space-y-3">
          <p className="text-brand-muted text-lg">Connect your wallet to view your portfolio.</p>
        </div>
      </div>
    );
  }

  // USDC-equivalent aggregates
  const usdcRows = rows.map((r) => ({
    ...r,
    usdcDeposited: r.position ? toUsdcEquiv(r.position.totalDeposited, r.rateBps) : 0n,
    usdcDeployed: r.position ? toUsdcEquiv(r.position.deployed, r.rateBps) : 0n,
    usdcEarned: r.position ? toUsdcEquiv(r.position.earned, r.rateBps) : 0n,
    usdcAvailable: r.position ? toUsdcEquiv(r.position.available, r.rateBps) : 0n,
  }));

  const totalUsdcDeposited = usdcRows.reduce((a, r) => a + r.usdcDeposited, 0n);
  const totalUsdcDeployed = usdcRows.reduce((a, r) => a + r.usdcDeployed, 0n);
  const totalUsdcEarned = usdcRows.reduce((a, r) => a + r.usdcEarned, 0n);
  const totalUsdcAvailable = usdcRows.reduce((a, r) => a + r.usdcAvailable, 0n);

  const totalPoolDeployed = rows.reduce((a, r) => a + r.totals.totalDeployed, 0n);
  const totalPoolDeposited = rows.reduce((a, r) => a + r.totals.totalDeposited, 0n);
  const utilisation =
    totalPoolDeposited > 0n ? Number(totalPoolDeployed) / Number(totalPoolDeposited) : 0;

  // Pie chart slices (by USDC-equivalent deposited)
  const pieSlices =
    totalUsdcDeposited > 0n
      ? usdcRows
          .filter((r) => r.usdcDeposited > 0n)
          .map((r) => ({
            label: stablecoinLabel(r.token),
            pct: Number((r.usdcDeposited * 10_000n) / totalUsdcDeposited) / 100,
            color: '',
          }))
      : [];

  // Weighted average APY (8% default per token, weighted by deposited)
  const DEFAULT_APY_BPS = 800;
  const weightedApy =
    totalUsdcDeposited > 0n
      ? usdcRows.reduce((acc, r) => {
          const weight =
            totalUsdcDeposited > 0n ? Number(r.usdcDeposited) / Number(totalUsdcDeposited) : 0;
          return acc + DEFAULT_APY_BPS * weight;
        }, 0)
      : DEFAULT_APY_BPS;

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
          {/* #293: Total Portfolio Value (USDC equivalent) */}
          <div className="bg-brand-card border border-brand-border rounded-2xl p-6 mb-6">
            <div className="flex flex-col sm:flex-row sm:items-start justify-between gap-6">
              <div className="flex-1">
                <p className="text-brand-muted text-sm mb-1">Total Portfolio Value</p>
                <p className="text-4xl font-bold text-white">{formatUSDC(totalUsdcDeposited)}</p>
                <p className="text-xs text-brand-muted mt-1">
                  ≈ USDC equivalent
                  <span
                    className="ml-1 cursor-help"
                    title="Conversion based on protocol exchange rates, not live market prices"
                  >
                    ⓘ
                  </span>
                </p>
                <div className="grid grid-cols-2 sm:grid-cols-4 gap-4 mt-4 text-sm">
                  <div>
                    <p className="text-brand-muted text-xs">Total Deposited</p>
                    <p className="text-white font-medium">{formatUSDC(totalUsdcDeposited)}</p>
                  </div>
                  <div>
                    <p className="text-brand-muted text-xs">Total Deployed</p>
                    <p className="text-white font-medium">{formatUSDC(totalUsdcDeployed)}</p>
                  </div>
                  <div>
                    <p className="text-brand-muted text-xs">Total Earned</p>
                    <p className="text-brand-gold font-medium">{formatUSDC(totalUsdcEarned)}</p>
                  </div>
                  <div>
                    <p className="text-brand-muted text-xs">Weighted APY</p>
                    <p className="text-green-400 font-medium">{(weightedApy / 100).toFixed(2)}%</p>
                  </div>
                </div>
              </div>

              {/* Pie chart */}
              {pieSlices.length > 0 && (
                <div className="shrink-0">
                  <p className="text-brand-muted text-xs mb-3">Token Allocation</p>
                  <AllocationPie slices={pieSlices} />
                </div>
              )}
            </div>
          </div>

          {/* Summary stats */}
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-8">
            <StatCard
              label="Available Liquidity"
              value={formatUSDC(totalUsdcAvailable)}
              sub="Withdrawable now"
            />
            <StatCard
              label="Deployed Capital"
              value={formatUSDC(totalUsdcDeployed)}
              sub="Funding active invoices"
            />
            <StatCard
              label="Yield Earned"
              value={formatUSDC(totalUsdcEarned)}
              sub="Cumulative interest"
              highlight
            />
            <StatCard
              label="Weighted APY"
              value={`${(weightedApy / 100).toFixed(2)}%`}
              sub="Across all positions"
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

          {/* Per-token positions (collapsible when > 1 token) */}
          <div className="space-y-4">
            <h2 className="text-white font-semibold text-lg">Token Positions</h2>
            {rows.map(({ token, position, totals, waitEstimate, forecast, rateBps }) => {
              const isCollapsed = collapsed[token] ?? false;
              const usdcDeposited = position ? toUsdcEquiv(position.totalDeposited, rateBps) : 0n;
              const dueDate = waitEstimate?.nearestInvoiceDueDate
                ? new Date(waitEstimate.nearestInvoiceDueDate * 1000).toLocaleDateString()
                : 'No active invoice due date';
              const isQueued = Boolean(waitEstimate && waitEstimate.queuePosition > 0);
              const actionLoading = queueActionLoading[token] ?? false;
              return (
                <div
                  key={token}
                  className="bg-brand-card border border-brand-border rounded-2xl p-6"
                >
                  <button
                    className="w-full flex items-center justify-between mb-4"
                    onClick={() =>
                      rows.length > 1 && setCollapsed((p) => ({ ...p, [token]: !p[token] }))
                    }
                  >
                    <div className="flex items-center gap-3">
                      <h3 className="text-white font-medium text-base">{stablecoinLabel(token)}</h3>
                      {rateBps !== 10_000 && (
                        <span className="text-xs text-brand-muted">
                          ≈ {formatUSDC(usdcDeposited)} USDC
                        </span>
                      )}
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
                    {rows.length > 1 && (
                      <span className="text-brand-muted text-sm">{isCollapsed ? '▸' : '▾'}</span>
                    )}
                  </button>

                  {!isCollapsed && (
                    <>
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
                            <p className="text-white font-medium">
                              {formatUSDC(position.available)}
                            </p>
                          </div>
                          <div>
                            <p className="text-brand-muted">Deployed</p>
                            <p className="text-white font-medium">
                              {formatUSDC(position.deployed)}
                            </p>
                          </div>
                          <div>
                            <p className="text-brand-muted">Earned</p>
                            <p className="text-brand-gold font-medium">
                              {formatUSDC(position.earned)}
                            </p>
                          </div>
                        </div>
                      ) : (
                        <p className="text-brand-muted text-sm">
                          You have no {stablecoinLabel(token)} position in this pool yet.
                        </p>
                      )}

                      <div className="mt-4 pt-4 border-t border-brand-border">
                        <UtilisationBar
                          utilisation={
                            totals.totalDeposited > 0n
                              ? Number(totals.totalDeployed) / Number(totals.totalDeposited)
                              : 0
                          }
                        />
                      </div>

                      {isQueued && waitEstimate && (
                        <div className="mt-4 rounded-xl border border-brand-border bg-black/20 p-4">
                          <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                            <div>
                              <p className="text-white font-medium">Withdrawal queue</p>
                              <p className="text-brand-muted text-xs mt-1">
                                Position {waitEstimate.queuePosition} with{' '}
                                {formatUSDC(waitEstimate.capitalAhead)} ahead
                              </p>
                              <p className="text-brand-muted text-xs mt-1">
                                Estimated wait:{' '}
                                <span className="text-white">
                                  {formatWaitEstimate(waitEstimate.estimatedWaitSecs)}
                                </span>{' '}
                                <span className="opacity-60">(estimate, not a guarantee)</span>
                              </p>
                            </div>
                            <div className="text-left sm:text-right">
                              <p className="text-brand-muted text-xs">Nearest invoice due</p>
                              <p className="text-white text-sm font-medium">{dueDate}</p>
                            </div>
                          </div>
                          <div className="flex flex-wrap gap-2 mt-3">
                            <button
                              onClick={() => handleCancelWithdrawal(token)}
                              disabled={actionLoading}
                              className="px-3 py-1.5 text-xs rounded-lg border border-brand-border text-white hover:bg-brand-border transition-colors disabled:opacity-50"
                            >
                              Cancel request
                            </button>
                            <button
                              onClick={() => handleDrainQueue(token)}
                              disabled={actionLoading}
                              className="px-3 py-1.5 text-xs rounded-lg border border-brand-border text-brand-gold hover:bg-brand-border transition-colors disabled:opacity-50"
                              title="Anyone can trigger a drain attempt against current liquidity"
                            >
                              Try drain now
                            </button>
                          </div>
                        </div>
                      )}

                      {!isQueued && position && position.available > 0n && (
                        <div className="mt-4 rounded-xl border border-brand-border bg-black/20 p-4">
                          <p className="text-white font-medium mb-2">Request withdrawal</p>
                          <p className="text-brand-muted text-xs mb-3">
                            If pool liquidity can&apos;t cover it immediately, your request is queued
                            and settled automatically (FIFO, oldest-first) as liquidity arrives.
                          </p>
                          <div className="flex flex-col sm:flex-row gap-2">
                            <input
                              type="number"
                              min="0"
                              step="any"
                              placeholder="Amount"
                              value={requestAmounts[token] ?? ''}
                              onChange={(e) =>
                                setRequestAmounts((p) => ({ ...p, [token]: e.target.value }))
                              }
                              className="flex-1 bg-brand-dark border border-brand-border rounded-lg px-3 py-1.5 text-sm text-white focus:outline-none focus:border-brand-gold"
                            />
                            <button
                              onClick={() => handleRequestWithdrawal(token)}
                              disabled={actionLoading || !requestAmounts[token]}
                              className="px-4 py-1.5 text-xs rounded-lg bg-brand-gold text-black font-medium hover:opacity-90 transition-opacity disabled:opacity-50"
                            >
                              {actionLoading ? 'Submitting…' : 'Request'}
                            </button>
                          </div>
                        </div>
                      )}

                      {forecast.length > 0 && (
                        <div className="mt-4">
                          <LiquidityForecastChart
                            data={forecast}
                            title="30-Day Liquidity Forecast"
                            queuedDemand={waitEstimate?.capitalAhead}
                          />
                        </div>
                      )}
                    </>
                  )}
                </div>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
}
