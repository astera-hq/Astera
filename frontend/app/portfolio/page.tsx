'use client';

import { useEffect, useState, useCallback } from 'react';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { StatCardSkeleton, Skeleton } from '@/components/Skeleton';
import { LiquidityForecastChart } from '@/components/analytics/LiquidityForecastChart';
import { TrancheClass } from '@/../packages/sdk/src/generated/tranche';
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
  getTrancheInvestorPosition,
} from '@/lib/contracts';
import { NETWORK, formatUSDC, stablecoinLabel, toStroops } from '@/lib/stellar';
import type { PoolTokenTotals, WaitEstimate, LiquidityForecastPoint } from '@/lib/types';
import { useTranslations } from 'next-intl';

interface PortfolioSnapshot {
  totalDeposited: bigint;
  available: bigint;
  deployed: bigint;
  earned: bigint;
  depositCount: number;
}

interface TranchePositionData {
  senior: { deposited: bigint; earned: bigint; losses: bigint };
  junior: { deposited: bigint; earned: bigint; losses: bigint };
  seniorApyPct: number;
  juniorApyPct: number;
}

interface TokenRow {
  token: string;
  totals: PoolTokenTotals;
  position: PortfolioSnapshot | null;
  waitEstimate: WaitEstimate | null;
  forecast: LiquidityForecastPoint[];
  rateBps: number;
  tranche: TranchePositionData | null;
}

const INDEXER_URL = process.env.NEXT_PUBLIC_INDEXER_URL || 'http://localhost:3001';

async function fetchTranchePositions(investor: string, token: string): Promise<TranchePositionData | null> {
  try {
    const [senior, junior] = await Promise.all([
      getTrancheInvestorPosition(investor, token, TrancheClass.Senior).catch(() => null),
      getTrancheInvestorPosition(investor, token, TrancheClass.Junior).catch(() => null),
    ]);
    if (!senior && !junior) return null;
    if ((senior?.deposited ?? 0n) === 0n && (junior?.deposited ?? 0n) === 0n) return null;
    let seniorApyPct = 0;
    let juniorApyPct = 0;
    try {
      const res = await fetch(`${INDEXER_URL}/tranches/${encodeURIComponent(token)}/apy`);
      if (res.ok) {
        const data = await res.json();
        seniorApyPct = data.senior?.realizedApyPct ?? 0;
        juniorApyPct = data.junior?.realizedApyPct ?? 0;
      }
    } catch { /* indexer optional */ }
    return {
      senior: { deposited: senior?.deposited ?? 0n, earned: senior?.earned ?? 0n, losses: senior?.losses ?? 0n },
      junior: { deposited: junior?.deposited ?? 0n, earned: junior?.earned ?? 0n, losses: junior?.losses ?? 0n },
      seniorApyPct,
      juniorApyPct,
    };
  } catch {
    return null;
  }
}

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

function StatCard({ label, value, sub, highlight }: { label: string; value: string; sub?: string; highlight?: boolean }) {
  return (
    <div className="bg-brand-card border border-brand-border rounded-2xl p-6 flex flex-col gap-1">
      <p className="text-brand-muted text-sm font-medium">{label}</p>
      <p className={`text-2xl font-bold ${highlight ? 'text-brand-gold' : 'text-brand-text'}`}>{value}</p>
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
        <div className={`h-full rounded-full transition-all duration-500 ${color}`} style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}

function AllocationPie({ slices }: { slices: { label: string; pct: number; color: string }[] }) {
  const COLORS = ['#F5A623', '#4ADE80', '#60A5FA', '#F472B6', '#A78BFA'];
  const r = 40, cx = 50, cy = 50;
  function polarToXY(pct: number) {
    const angle = (pct / 100) * 2 * Math.PI - Math.PI / 2;
    return { x: cx + r * Math.cos(angle), y: cy + r * Math.sin(angle) };
  }
  const cumulativeEnds = slices.reduce<number[]>((acc, s) => {
    const prev = acc.length > 0 ? (acc[acc.length - 1] ?? 0) : 0;
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
      <svg viewBox="0 0 100 100" className="w-24 h-24 shrink-0">{paths}</svg>
      <div className="space-y-1.5">
        {slices.map((s, i) => (
          <div key={s.label} className="flex items-center gap-2 text-xs">
            <span className="w-2.5 h-2.5 rounded-full shrink-0" style={{ background: COLORS[i % COLORS.length] }} />
            <span className="text-brand-muted">{s.label}</span>
            <span className="text-brand-text font-medium ml-auto">{s.pct.toFixed(1)}%</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function toUsdcEquiv(amount: bigint, rateBps: number): bigint {
  return (amount * BigInt(rateBps)) / 10_000n;
}

export default function PortfolioPage() {
  const { wallet } = useStore();
  const t = useTranslations('Portfolio');
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
          const [totals, rawPos, rateBps, waitEstimate, forecast, tranche] = await Promise.all([
            getPoolTokenTotals(token),
            getInvestorPosition(wallet.address!, token),
            getExchangeRate(token).catch(() => 10_000),
            estimateWithdrawalWait(wallet.address!, token).catch(() => null),
            getLiquidityForecast(token, 30).catch(() => []),
            fetchTranchePositions(wallet.address!, token),
          ]);
          const position: PortfolioSnapshot | null = rawPos
            ? { totalDeposited: rawPos.deposited, available: rawPos.available, deployed: rawPos.deployed, earned: rawPos.earned, depositCount: rawPos.depositCount }
            : null;
          return { token, totals, position, waitEstimate, forecast, rateBps, tranche };
        }),
      );
      setRows(rowData);
      setLastRefresh(new Date());
      if (rowData.length > 1) {
        const initial: Record<string, boolean> = {};
        rowData.forEach((r) => { initial[r.token] = true; });
        setCollapsed((prev) => ({ ...initial, ...prev }));
      }
    } catch (e) {
      console.error('[Portfolio] Load error:', e);
      setError(t('toastFailedLoad'));
    } finally {
      setLoading(false);
    }
  }, [wallet.address, wallet.connected, t]);

  useEffect(() => { load(); }, [load]);

  async function signAndSubmit(xdrPromise: Promise<string>): Promise<void> {
    const xdr = await xdrPromise;
    const freighter = await import('@stellar/freighter-api');
    const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, { networkPassphrase: NETWORK, address: wallet.address! });
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
      toast.success(t('toastWithdrawalSubmitted', { amount: formatUSDC(shares) }));
      setRequestAmounts((p) => ({ ...p, [token]: '' }));
      await load();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('toastFailedWithdrawal'));
    } finally {
      setQueueActionLoading((p) => ({ ...p, [token]: false }));
    }
  }

  async function handleCancelWithdrawal(token: string) {
    if (!wallet.address) return;
    setQueueActionLoading((p) => ({ ...p, [token]: true }));
    try {
      await signAndSubmit(buildCancelWithdrawalRequestTx(wallet.address, token));
      toast.success(t('toastWithdrawalCancelled'));
      await load();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('toastFailedCancel'));
    } finally {
      setQueueActionLoading((p) => ({ ...p, [token]: false }));
    }
  }

  async function handleDrainQueue(token: string) {
    if (!wallet.address) return;
    setQueueActionLoading((p) => ({ ...p, [token]: true }));
    try {
      await signAndSubmit(buildDrainWithdrawalQueueTx(wallet.address, token));
      toast.success(t('toastDrainSubmitted'));
      await load();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('toastFailedDrain'));
    } finally {
      setQueueActionLoading((p) => ({ ...p, [token]: false }));
    }
  }

  if (!wallet.connected) {
    return (
      <div className="min-h-screen pt-24 flex items-center justify-center">
        <div className="text-center space-y-3">
          <p className="text-brand-muted text-lg">{t('connectWallet')}</p>
        </div>
      </div>
    );
  }

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
  const utilisation = totalPoolDeposited > 0n ? Number(totalPoolDeployed) / Number(totalPoolDeposited) : 0;

  const pieSlices = totalUsdcDeposited > 0n
    ? usdcRows.filter((r) => r.usdcDeposited > 0n).map((r) => ({
        label: stablecoinLabel(r.token),
        pct: Number((r.usdcDeposited * 10_000n) / totalUsdcDeposited) / 100,
        color: '',
      }))
    : [];

  const DEFAULT_APY_BPS = 800;
  const weightedApy = totalUsdcDeposited > 0n
    ? usdcRows.reduce((acc, r) => {
        const weight = totalUsdcDeposited > 0n ? Number(r.usdcDeposited) / Number(totalUsdcDeposited) : 0;
        return acc + DEFAULT_APY_BPS * weight;
      }, 0)
    : DEFAULT_APY_BPS;

  return (
    <div className="min-h-screen pt-24 pb-16 px-4 max-w-7xl mx-auto">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 mb-8">
        <div>
          <h1 className="text-3xl font-bold text-brand-text">{t('title')}</h1>
          <p className="text-brand-muted mt-1 text-sm">
            {lastRefresh ? t('lastUpdated', { time: lastRefresh.toLocaleTimeString() }) : t('loadingPositions')}
          </p>
        </div>
        <button onClick={load} disabled={loading} className="flex items-center gap-2 px-4 py-2 bg-brand-card border border-brand-border rounded-xl text-sm text-brand-text hover:bg-brand-border transition-colors disabled:opacity-50">
          {loading ? <span className="w-4 h-4 border-2 border-brand-gold border-t-transparent rounded-full animate-spin" /> : null}
          {t('refresh')}
        </button>
      </div>

      {error && (
        <div role="alert" className="mb-6 flex items-center justify-between bg-red-900/30 border border-red-800/50 text-red-400 rounded-xl px-4 py-3 text-sm">
          <span>{error}</span>
          <button onClick={load} className="underline ml-4 shrink-0">{t('retry')}</button>
        </div>
      )}

      {loading && rows.length === 0 && (
        <div className="space-y-6">
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
            <StatCardSkeleton /><StatCardSkeleton /><StatCardSkeleton /><StatCardSkeleton />
          </div>
          <div className="bg-brand-card border border-brand-border rounded-2xl p-6 animate-pulse">
            <Skeleton className="h-5 w-40 mb-4" />
            <div className="space-y-1">
              <div className="flex justify-between text-xs">
                <Skeleton className="h-3 w-24" /><Skeleton className="h-3 w-8" />
              </div>
              <div className="h-2 rounded-full bg-brand-border overflow-hidden">
                <div className="h-full rounded-full bg-brand-gold/40" style={{ width: '60%' }} />
              </div>
            </div>
            <div className="grid grid-cols-2 sm:grid-cols-4 gap-4 mt-6 text-sm">
              {[1, 2, 3, 4].map((i) => (
                <div key={i}><Skeleton className="h-3 w-20 mb-1" /><Skeleton className="h-5 w-24" /></div>
              ))}
            </div>
          </div>
          <TokenPositionSkeleton />
        </div>
      )}

      {!loading && rows.length === 0 && !error && (
        <div className="text-center py-24 text-brand-muted">{t('noTokens')}</div>
      )}

      {rows.length > 0 && (
        <>
          {/* Total Portfolio Value */}
          <div className="bg-brand-card border border-brand-border rounded-2xl p-6 mb-6">
            <div className="flex flex-col sm:flex-row sm:items-start justify-between gap-6">
              <div className="flex-1">
                <p className="text-brand-muted text-sm mb-1">{t('totalPortfolioValue')}</p>
                <p className="text-4xl font-bold text-brand-text">{formatUSDC(totalUsdcDeposited)}</p>
                <p className="text-xs text-brand-muted mt-1">
                  {t('usdcEquivalent')}
                  <span className="ml-1 cursor-help" title={t('usdcEquivalentHint')}>ⓘ</span>
                </p>
                <div className="grid grid-cols-2 sm:grid-cols-4 gap-4 mt-4 text-sm">
                  <div>
                    <p className="text-brand-muted text-xs">{t('totalDeposited')}</p>
                    <p className="text-brand-text font-medium">{formatUSDC(totalUsdcDeposited)}</p>
                  </div>
                  <div>
                    <p className="text-brand-muted text-xs">{t('totalDeployed')}</p>
                    <p className="text-brand-text font-medium">{formatUSDC(totalUsdcDeployed)}</p>
                  </div>
                  <div>
                    <p className="text-brand-muted text-xs">{t('totalEarned')}</p>
                    <p className="text-brand-gold font-medium">{formatUSDC(totalUsdcEarned)}</p>
                  </div>
                  <div>
                    <p className="text-brand-muted text-xs">{t('weightedApy')}</p>
                    <p className="text-green-400 font-medium">{(weightedApy / 100).toFixed(2)}%</p>
                  </div>
                </div>
              </div>
              {pieSlices.length > 0 && (
                <div className="shrink-0">
                  <p className="text-brand-muted text-xs mb-3">{t('tokenAllocation')}</p>
                  <AllocationPie slices={pieSlices} />
                </div>
              )}
            </div>
          </div>

          {/* Summary stats */}
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-8">
            <StatCard label={t('availableLiquidity')} value={formatUSDC(totalUsdcAvailable)} sub={t('withdrawableNow')} />
            <StatCard label={t('deployedCapital')} value={formatUSDC(totalUsdcDeployed)} sub={t('fundingActiveInvoices')} />
            <StatCard label={t('yieldEarned')} value={formatUSDC(totalUsdcEarned)} sub={t('cumulativeInterest')} highlight />
            <StatCard label={t('weightedApy')} value={`${(weightedApy / 100).toFixed(2)}%`} sub={t('acrossAllPositions')} />
          </div>

          {/* Pool utilisation */}
          <div className="bg-brand-card border border-brand-border rounded-2xl p-6 mb-8">
            <h2 className="text-brand-text font-semibold mb-4">{t('poolUtilisation')}</h2>
            <UtilisationBar utilisation={utilisation} />
            <div className="grid grid-cols-2 sm:grid-cols-4 gap-4 mt-6 text-sm">
              <div>
                <p className="text-brand-muted">{t('totalPoolDeposits')}</p>
                <p className="text-brand-text font-medium">{formatUSDC(totalPoolDeposited)}</p>
              </div>
              <div>
                <p className="text-brand-muted">{t('totalDeployed')}</p>
                <p className="text-brand-text font-medium">{formatUSDC(totalPoolDeployed)}</p>
              </div>
              <div>
                <p className="text-brand-muted">{t('totalPaidOut')}</p>
                <p className="text-brand-text font-medium">{formatUSDC(rows.reduce((a, r) => a + r.totals.totalPaidOut, 0n))}</p>
              </div>
              <div>
                <p className="text-brand-muted">{t('feeRevenue')}</p>
                <p className="text-brand-gold font-medium">{formatUSDC(rows.reduce((a, r) => a + r.totals.totalFeeRevenue, 0n))}</p>
              </div>
            </div>
          </div>

          {/* Per-token positions */}
          <div className="space-y-4">
            <h2 className="text-white font-semibold text-lg">{t('tokenPositions')}</h2>
            {rows.map(({ token, position, totals, waitEstimate, forecast, rateBps, tranche }) => {
              const isCollapsed = collapsed[token] ?? false;
              const usdcDeposited = position ? toUsdcEquiv(position.totalDeposited, rateBps) : 0n;
              const dueDate = waitEstimate?.nearestInvoiceDueDate
                ? new Date(waitEstimate.nearestInvoiceDueDate * 1000).toLocaleDateString()
                : t('noActiveDueDate');
              const isQueued = Boolean(waitEstimate && waitEstimate.queuePosition > 0);
              const actionLoading = queueActionLoading[token] ?? false;
              return (
                <div key={token} className="bg-brand-card border border-brand-border rounded-2xl p-6">
                  <button
                    className="w-full flex items-center justify-between mb-4"
                    onClick={() => rows.length > 1 && setCollapsed((p) => ({ ...p, [token]: !p[token] }))}
                  >
                    <div className="flex items-center gap-3">
                      <h3 className="text-brand-text font-medium text-base">{stablecoinLabel(token)}</h3>
                      {rateBps !== 10_000 && (
                        <span className="text-xs text-brand-muted">≈ {formatUSDC(usdcDeposited)} USDC</span>
                      )}
                      {position ? (
                        <span className="text-xs px-2 py-1 rounded-lg bg-green-900/30 border border-green-800/50 text-green-400">{t('activePosition')}</span>
                      ) : (
                        <span className="text-xs px-2 py-1 rounded-lg bg-brand-border text-brand-muted">{t('noPosition')}</span>
                      )}
                    </div>
                    {rows.length > 1 && <span className="text-brand-muted text-sm">{isCollapsed ? '▸' : '▾'}</span>}
                  </button>

                  {!isCollapsed && (
                    <>
                      {position ? (
                        <div className="grid grid-cols-2 sm:grid-cols-4 gap-4 text-sm">
                          <div>
                            <p className="text-brand-muted">{t('deposited')}</p>
                            <p className="text-brand-text font-medium">{formatUSDC(position.totalDeposited)}</p>
                          </div>
                          <div>
                            <p className="text-brand-muted">{t('available')}</p>
                            <p className="text-brand-text font-medium">{formatUSDC(position.available)}</p>
                          </div>
                          <div>
                            <p className="text-brand-muted">{t('deployed')}</p>
                            <p className="text-brand-text font-medium">{formatUSDC(position.deployed)}</p>
                          </div>
                          <div>
                            <p className="text-brand-muted">{t('earned')}</p>
                            <p className="text-brand-gold font-medium">{formatUSDC(position.earned)}</p>
                          </div>
                        </div>
                      ) : (
                        <p className="text-brand-muted text-sm">{t('noTokenPosition', { token: stablecoinLabel(token) })}</p>
                      )}

                      <div className="mt-4 pt-4 border-t border-brand-border">
                        <UtilisationBar utilisation={totals.totalDeposited > 0n ? Number(totals.totalDeployed) / Number(totals.totalDeposited) : 0} />
                      </div>

                      {isQueued && waitEstimate && (
                        <div className="mt-4 rounded-xl border border-brand-border bg-brand-navy/40 p-4">
                          <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                            <div>
                              <p className="text-brand-text font-medium">{t('withdrawalQueue')}</p>
                              <p className="text-brand-muted text-xs mt-1">
                                {t('queuePosition', { pos: waitEstimate.queuePosition, capital: formatUSDC(waitEstimate.capitalAhead) })}
                              </p>
                              <p className="text-brand-muted text-xs mt-1">
                                {t('estimatedWait', { wait: formatWaitEstimate(waitEstimate.estimatedWaitSecs) })}
                              </p>
                            </div>
                            <div className="text-left sm:text-right">
                              <p className="text-brand-muted text-xs">{t('nearestInvoiceDue')}</p>
                              <p className="text-brand-text text-sm font-medium">{dueDate}</p>
                            </div>
                          </div>
                          <div className="flex flex-wrap gap-2 mt-3">
                            <button onClick={() => handleCancelWithdrawal(token)} disabled={actionLoading} className="px-3 py-1.5 text-xs rounded-lg border border-brand-border text-brand-text hover:bg-brand-border transition-colors disabled:opacity-50">
                              {t('cancelRequest')}
                            </button>
                            <button onClick={() => handleDrainQueue(token)} disabled={actionLoading} className="px-3 py-1.5 text-xs rounded-lg border border-brand-border text-brand-gold hover:bg-brand-border transition-colors disabled:opacity-50" title="Anyone can trigger a drain attempt against current liquidity">
                              {t('tryDrainNow')}
                            </button>
                          </div>
                        </div>
                      )}

                      {!isQueued && position && position.available > 0n && (
                        <div className="mt-4 rounded-xl border border-brand-border bg-brand-navy/40 p-4">
                          <p className="text-brand-text font-medium mb-2">{t('requestWithdrawal')}</p>
                          <p className="text-brand-muted text-xs mb-3">{t('requestWithdrawalDesc')}</p>
                          <div className="flex flex-col sm:flex-row gap-2">
                            <input
                              type="number"
                              min="0"
                              step="any"
                              placeholder={t('amountPlaceholder')}
                              value={requestAmounts[token] ?? ''}
                              onChange={(e) => setRequestAmounts((p) => ({ ...p, [token]: e.target.value }))}
                              className="flex-1 bg-brand-dark border border-brand-border rounded-lg px-3 py-1.5 text-sm text-brand-text focus:outline-none focus:border-brand-gold"
                            />
                            <button onClick={() => handleRequestWithdrawal(token)} disabled={actionLoading || !requestAmounts[token]} className="px-4 py-1.5 text-xs rounded-lg bg-brand-gold text-black font-medium hover:opacity-90 transition-opacity disabled:opacity-50">
                              {actionLoading ? t('loadingPositions') : t('submitRequest')}
                            </button>
                          </div>
                        </div>
                      )}

                      {forecast.length > 0 && (
                        <div className="mt-4">
                          <LiquidityForecastChart data={forecast} title="30-Day Liquidity Forecast" queuedDemand={waitEstimate?.capitalAhead} />
                        </div>
                      )}

                      {tranche && (
                        <div className="mt-4 pt-4 border-t border-brand-border">
                          <h4 className="text-white font-medium text-sm mb-3">Tranche Positions</h4>
                          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                            <div className="bg-blue-900/20 border border-blue-800/30 rounded-lg p-3">
                              <div className="flex items-center justify-between mb-2">
                                <span className="text-blue-400 text-xs font-medium">Senior Tranche</span>
                                <span className="text-xs px-2 py-0.5 rounded bg-blue-900/50 text-blue-300">Lower Risk</span>
                              </div>
                              <div className="space-y-1 text-xs">
                                <div className="flex justify-between"><span className="text-brand-muted">{t('deposited')}</span><span className="text-white">{formatUSDC(tranche.senior.deposited)}</span></div>
                                <div className="flex justify-between"><span className="text-brand-muted">{t('earned')}</span><span className="text-green-400">{formatUSDC(tranche.senior.earned)}</span></div>
                                <div className="flex justify-between"><span className="text-brand-muted">Losses</span><span className="text-white">{formatUSDC(tranche.senior.losses)}</span></div>
                                <div className="flex justify-between"><span className="text-brand-muted">Trailing APY</span><span className="text-white">{tranche.seniorApyPct.toFixed(1)}%</span></div>
                              </div>
                            </div>
                            <div className="bg-purple-900/20 border border-purple-800/30 rounded-lg p-3">
                              <div className="flex items-center justify-between mb-2">
                                <span className="text-purple-400 text-xs font-medium">Junior Tranche</span>
                                <span className="text-xs px-2 py-0.5 rounded bg-purple-900/50 text-purple-300">Higher Risk</span>
                              </div>
                              <div className="space-y-1 text-xs">
                                <div className="flex justify-between"><span className="text-brand-muted">{t('deposited')}</span><span className="text-white">{formatUSDC(tranche.junior.deposited)}</span></div>
                                <div className="flex justify-between"><span className="text-brand-muted">{t('earned')}</span><span className="text-green-400">{formatUSDC(tranche.junior.earned)}</span></div>
                                <div className="flex justify-between"><span className="text-brand-muted">Losses</span><span className="text-red-400">{formatUSDC(tranche.junior.losses)}</span></div>
                                <div className="flex justify-between"><span className="text-brand-muted">Trailing APY</span><span className="text-white">{tranche.juniorApyPct.toFixed(1)}%</span></div>
                              </div>
                            </div>
                          </div>
                          <div className="mt-3 text-center">
                            <a href="/invest/tranches" className="text-xs text-brand-gold hover:underline">Manage tranche investments →</a>
                          </div>
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
