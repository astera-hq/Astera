'use client';

/**
 * #865: admin-facing liquidity forecast dashboard — shows projected available
 * liquidity vs queued withdrawal demand per token, and lets an admin (or anyone,
 * since the entrypoint is permissionless) trigger a drain attempt against current
 * liquidity.
 */

import { useState, useEffect, useCallback } from 'react';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { Skeleton } from '@/components/Skeleton';
import { LiquidityForecastChart } from '@/components/analytics/LiquidityForecastChart';
import {
  getAcceptedTokens,
  getPoolTokenTotals,
  getWithdrawalQueue,
  getLiquidityForecast,
  buildDrainWithdrawalQueueTx,
  submitTx,
} from '@/lib/contracts';
import { formatUSDC, stablecoinLabel } from '@/lib/stellar';
import type { PoolTokenTotals, WithdrawalRequest, LiquidityForecastPoint } from '@/lib/types';

const HORIZON_OPTIONS = [30, 60, 90] as const;

interface TokenLiquidityRow {
  token: string;
  totals: PoolTokenTotals;
  queue: WithdrawalRequest[];
  forecast: LiquidityForecastPoint[];
}

function StatCard({ label, value, sub }: { label: string; value: string; sub?: string }) {
  return (
    <div className="rounded-2xl border border-brand-border bg-brand-card p-5 flex flex-col gap-1">
      <span className="text-[11px] font-bold uppercase tracking-widest text-brand-muted">
        {label}
      </span>
      <span className="text-2xl font-black tracking-tight text-white">{value}</span>
      {sub && <span className="text-xs text-brand-muted mt-0.5">{sub}</span>}
    </div>
  );
}

export default function AdminLiquidityPage() {
  const { wallet } = useStore();
  const [rows, setRows] = useState<TokenLiquidityRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [horizon, setHorizon] = useState<(typeof HORIZON_OPTIONS)[number]>(30);
  const [drainLoading, setDrainLoading] = useState<Record<string, boolean>>({});

  const load = useCallback(
    async (horizonDays: number) => {
      setLoading(true);
      setError(null);
      try {
        const tokens = await getAcceptedTokens();
        const rowData = await Promise.all(
          tokens.map(async (token) => {
            const [totals, queue, forecast] = await Promise.all([
              getPoolTokenTotals(token),
              getWithdrawalQueue(token),
              getLiquidityForecast(token, horizonDays),
            ]);
            return { token, totals, queue, forecast };
          }),
        );
        setRows(rowData);
      } catch (e) {
        console.error('[AdminLiquidity] Load error:', e);
        setError('Failed to load liquidity data. Please try again.');
      } finally {
        setLoading(false);
      }
    },
    [],
  );

  useEffect(() => {
    load(horizon);
  }, [load, horizon]);

  async function handleDrain(token: string) {
    if (!wallet.address) {
      toast.error('Connect a wallet to submit a drain transaction.');
      return;
    }
    setDrainLoading((p) => ({ ...p, [token]: true }));
    try {
      const xdr = await buildDrainWithdrawalQueueTx(wallet.address, token);
      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
        networkPassphrase: 'Test SDF Network ; September 2015',
        address: wallet.address,
      });
      if (signError) throw new Error(signError.message);
      await submitTx(signedTxXdr);
      toast.success(`Drain attempt submitted for ${stablecoinLabel(token)}.`);
      await load(horizon);
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Failed to submit drain transaction.');
    } finally {
      setDrainLoading((p) => ({ ...p, [token]: false }));
    }
  }

  const totalQueuedShares = (queue: WithdrawalRequest[]) =>
    queue.reduce((acc, r) => acc + r.shares, 0n);

  return (
    <div>
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 mb-8">
        <div>
          <h1 className="text-3xl font-bold text-white">Liquidity Forecast</h1>
          <p className="text-brand-muted mt-1 text-sm">
            Projected available liquidity vs queued withdrawal demand, per token.
          </p>
        </div>
        <div className="flex items-center gap-1 bg-brand-card border border-brand-border rounded-xl p-1">
          {HORIZON_OPTIONS.map((h) => (
            <button
              key={h}
              onClick={() => setHorizon(h)}
              className={`px-3 py-1.5 text-xs font-medium rounded-lg transition-colors ${
                horizon === h
                  ? 'bg-brand-gold text-black'
                  : 'text-brand-muted hover:text-white'
              }`}
            >
              {h}d
            </button>
          ))}
        </div>
      </div>

      {error && (
        <div
          role="alert"
          className="mb-6 flex items-center justify-between bg-red-900/30 border border-red-800/50 text-red-400 rounded-xl px-4 py-3 text-sm"
        >
          <span>{error}</span>
          <button onClick={() => load(horizon)} className="underline ml-4 shrink-0">
            Retry
          </button>
        </div>
      )}

      {loading && rows.length === 0 && (
        <div className="space-y-6">
          {[1, 2].map((i) => (
            <div key={i} className="bg-brand-card border border-brand-border rounded-2xl p-6">
              <Skeleton className="h-5 w-32 mb-4" />
              <Skeleton className="h-40 w-full" />
            </div>
          ))}
        </div>
      )}

      {!loading && rows.length === 0 && !error && (
        <div className="text-center py-24 text-brand-muted">No accepted tokens configured.</div>
      )}

      <div className="space-y-8">
        {rows.map(({ token, totals, queue, forecast }) => {
          const queuedShares = totalQueuedShares(queue);
          const availableLiquidity = totals.totalDeposited - totals.totalDeployed;
          return (
            <div key={token} className="space-y-4">
              <h2 className="text-white font-semibold text-lg">{stablecoinLabel(token)}</h2>

              <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
                <StatCard
                  label="Available Liquidity"
                  value={formatUSDC(availableLiquidity > 0n ? availableLiquidity : 0n)}
                />
                <StatCard
                  label="Queue Depth"
                  value={String(queue.length)}
                  sub={queue.length > 0 ? `${formatUSDC(queuedShares)} queued` : undefined}
                />
                <StatCard
                  label="Projected (end of horizon)"
                  value={
                    forecast.length > 0
                      ? formatUSDC(forecast[forecast.length - 1]!.projectedAvailable)
                      : '--'
                  }
                  sub={`${horizon}-day projection`}
                />
              </div>

              <LiquidityForecastChart
                data={forecast}
                queuedDemand={queuedShares}
                title={`${horizon}-Day Liquidity Forecast`}
              />

              {queue.length > 0 && (
                <div className="bg-brand-card border border-brand-border rounded-2xl p-5">
                  <div className="flex items-center justify-between mb-3">
                    <h3 className="text-sm font-semibold text-white">Pending requests</h3>
                    <button
                      onClick={() => handleDrain(token)}
                      disabled={drainLoading[token]}
                      className="px-3 py-1.5 text-xs rounded-lg bg-brand-gold text-black font-medium hover:opacity-90 transition-opacity disabled:opacity-50"
                    >
                      {drainLoading[token] ? 'Submitting…' : 'Try drain now'}
                    </button>
                  </div>
                  <div className="overflow-x-auto">
                    <table className="w-full text-sm">
                      <thead>
                        <tr className="text-left text-brand-muted text-xs uppercase tracking-wide border-b border-brand-border">
                          <th className="py-2 pr-4">#</th>
                          <th className="py-2 pr-4">Investor</th>
                          <th className="py-2 pr-4">Shares</th>
                          <th className="py-2 pr-4">Requested</th>
                        </tr>
                      </thead>
                      <tbody>
                        {queue.map((r, i) => (
                          <tr key={r.requestId} className="border-b border-brand-border/50">
                            <td className="py-2 pr-4 text-brand-muted">{i + 1}</td>
                            <td className="py-2 pr-4 text-white font-mono text-xs">
                              {r.investor.slice(0, 6)}…{r.investor.slice(-4)}
                            </td>
                            <td className="py-2 pr-4 text-white">{formatUSDC(r.shares)}</td>
                            <td className="py-2 pr-4 text-brand-muted">
                              {new Date(r.requestedAt * 1000).toLocaleString()}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
