'use client';

/**
 * @fileoverview Storage Health monitoring panel — Admin UI (#290)
 * @description Surfaces StorageStats from the invoice contract and provides
 *   a "Run Cleanup" action that calls cleanup_expired_storage() with terminal
 *   invoice IDs. Estimates monthly storage cost in XLM and USDC equivalent.
 */

import { useState, useEffect, useCallback } from 'react';
import { useTranslations } from 'next-intl';
import GlossaryTerm from '@/components/GlossaryTerm';

// ─── Types ────────────────────────────────────────────────────────────────────

interface StorageStats {
  total_invoices: bigint;
  active_invoices: bigint;
  cleaned_invoices: bigint;
}

interface StorageHealth {
  stats: StorageStats;
  estimated_cost_stroops: bigint;
  cleanable_ids: bigint[];
  last_fetched: Date;
}

// ─── Constants ────────────────────────────────────────────────────────────────

const STROOPS_PER_XLM = 10_000_000n;
// XLM/USDC rate — configurable via NEXT_PUBLIC_XLM_USDC_RATE env var.
// Falls back to pool's exchange rate if available, else '--'.
const XLM_USDC_RATE: number | null = (() => {
  const env = process.env.NEXT_PUBLIC_XLM_USDC_RATE;
  if (env) {
    const parsed = parseFloat(env);
    if (!isNaN(parsed) && parsed > 0) return parsed;
  }
  return null;
})();
const LEDGERS_PER_MONTH = 518_400n;
const STROOPS_PER_LEDGER_PER_ENTRY = 1n;

function stroopsToXlm(stroops: bigint): number {
  return Number(stroops) / Number(STROOPS_PER_XLM);
}

function xlmToUsdc(xlm: number): number | null {
  return XLM_USDC_RATE !== null ? xlm * XLM_USDC_RATE : null;
}

// ─── Stat card ────────────────────────────────────────────────────────────────

interface StatCardProps {
  label: string;
  value: string;
  sub?: string;
  accent?: 'green' | 'amber' | 'red' | 'blue';
}

function StatCard({ label, value, sub, accent = 'blue' }: StatCardProps) {
  const accentMap = {
    green: 'text-emerald-400 border-emerald-500/30 bg-emerald-500/5',
    amber: 'text-amber-400 border-amber-500/30 bg-amber-500/5',
    red: 'text-rose-400 border-rose-500/30 bg-rose-500/5',
    blue: 'text-sky-400 border-sky-500/30 bg-sky-500/5',
  };

  return (
    <div
      className={`rounded-2xl border p-5 flex flex-col gap-1 transition-all duration-300 ${accentMap[accent]}`}
    >
      <span className="text-[11px] font-bold uppercase tracking-widest text-slate-400">
        {label}
      </span>
      <span className="text-3xl font-black font-mono tracking-tight mt-1">{value}</span>
      {sub && <span className="text-xs text-slate-500 mt-0.5 font-mono">{sub}</span>}
    </div>
  );
}

// ─── Suggestion banner ────────────────────────────────────────────────────────

interface SuggestionBannerProps {
  cleanableCount: number;
  estimatedSavingsXlm: number;
  onCleanup: () => void;
  loading: boolean;
}

function SuggestionBanner({
  cleanableCount,
  estimatedSavingsXlm,
  onCleanup,
  loading,
}: SuggestionBannerProps) {
  const t = useTranslations('Admin.monitoring');
  if (cleanableCount === 0) return null;

  return (
    <div className="flex items-center justify-between gap-4 rounded-2xl border border-amber-500/30 bg-amber-500/5 px-5 py-4">
      <div className="flex items-center gap-3">
        <span className="text-lg">🧹</span>
        <p className="text-sm text-amber-300">
          {t('cleanableBanner', { count: cleanableCount, xlm: estimatedSavingsXlm.toFixed(4) })}
        </p>
      </div>
      <button
        onClick={onCleanup}
        disabled={loading}
        className="shrink-0 rounded-xl border border-amber-500/40 bg-amber-500/10 px-4 py-2 text-sm font-semibold text-amber-300 hover:bg-amber-500/20 disabled:opacity-50 disabled:cursor-not-allowed transition-all duration-200"
      >
        {loading ? t('cleaning') : t('runCleanup')}
      </button>
    </div>
  );
}

// ─── Cleanup result ───────────────────────────────────────────────────────────

interface CleanupResult {
  removed: number;
  timestamp: Date;
}

function CleanupResultToast({ result }: { result: CleanupResult | null }) {
  const t = useTranslations('Admin.monitoring');
  if (!result) return null;

  return (
    <div className="flex items-center gap-3 rounded-xl border border-emerald-500/30 bg-emerald-500/5 px-4 py-3 text-sm text-emerald-300">
      <span>✓</span>
      <span>
        {t('cleanupResult', {
          count: result.removed,
          y: result.removed !== 1 ? t('entryPlural') : t('entrySingular'),
          time: result.timestamp.toLocaleTimeString(),
        })}
      </span>
    </div>
  );
}

// ─── Main page component ──────────────────────────────────────────────────────

interface ServiceHealthState {
  status: 'ok' | 'degraded' | 'down';
  checks: Record<string, { name: string; status: 'ok' | 'degraded' | 'down'; detail?: string }>;
}

export default function StorageMonitoringPage() {
  const t = useTranslations('Admin.monitoring');
  const [health, setHealth] = useState<StorageHealth | null>(null);
  const [services, setServices] = useState<ServiceHealthState | null>(null);
  const [loading, setLoading] = useState(true);
  const [cleanupLoading, setCleanupLoading] = useState(false);
  const [cleanupResult, setCleanupResult] = useState<CleanupResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  // ── Data fetching ──────────────────────────────────────────────────────────

  const fetchHealth = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const apiHealthRes = await fetch('/api/health').catch(() => null);
      if (apiHealthRes && apiHealthRes.ok) {
        const data = await apiHealthRes.json();
        setServices(data);
      }

      await new Promise((r) => setTimeout(r, 400));

      const mockStats: StorageStats = {
        total_invoices: 247n,
        active_invoices: 89n,
        cleaned_invoices: 23n,
      };

      const estimatedCost =
        mockStats.active_invoices * STROOPS_PER_LEDGER_PER_ENTRY * LEDGERS_PER_MONTH;

      const cleanableIds: bigint[] = Array.from({ length: 12 }, (_, i) => BigInt(i + 100));

      setHealth({
        stats: mockStats,
        estimated_cost_stroops: estimatedCost,
        cleanable_ids: cleanableIds,
        last_fetched: new Date(),
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : t('fetchError'));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchHealth();
  }, [fetchHealth]);

  // ── Cleanup action ─────────────────────────────────────────────────────────

  const handleCleanup = useCallback(async () => {
    if (!health || health.cleanable_ids.length === 0) return;
    setCleanupLoading(true);
    setCleanupResult(null);

    try {
      // Batch into groups of 50 (MAX_CLEANUP_BATCH)
      const BATCH = 50;
      let totalRemoved = 0;

      for (let i = 0; i < health.cleanable_ids.length; i += BATCH) {
        const batch = health.cleanable_ids.slice(i, i + BATCH);

        // ── Replace with your actual contract client call ────────────────
        // const removed = await invoiceContractClient.cleanup_expired_storage({
        //   caller: adminAddress,
        //   ids: batch,
        // });
        // totalRemoved += removed;

        // Mock: simulate removal
        await new Promise((r) => setTimeout(r, 400));
        totalRemoved += batch.length;
      }

      setCleanupResult({ removed: totalRemoved, timestamp: new Date() });
      // Refresh stats after cleanup
      await fetchHealth();
    } catch (err) {
      setError(err instanceof Error ? err.message : t('cleanupFailed'));
    } finally {
      setCleanupLoading(false);
    }
  }, [health, fetchHealth]);

  // ── Derived values ─────────────────────────────────────────────────────────

  const costXlm = health ? stroopsToXlm(health.estimated_cost_stroops) : 0;
  const costUsdc = xlmToUsdc(costXlm);

  const savingsXlm = health
    ? stroopsToXlm(
        BigInt(health.cleanable_ids.length) * STROOPS_PER_LEDGER_PER_ENTRY * LEDGERS_PER_MONTH,
      )
    : 0;

  const utilizationPct =
    health && health.stats.total_invoices > 0n
      ? Math.round(
          (Number(health.stats.active_invoices) / Number(health.stats.total_invoices)) * 100,
        )
      : 0;

  // ── Render ─────────────────────────────────────────────────────────────────

  return (
    <div className="min-h-screen bg-slate-950 text-slate-100 font-sans">
      {/* ── Page header ── */}
      <div className="border-b border-slate-800/60 bg-slate-900/40 backdrop-blur-sm sticky top-0 z-10">
        <div className="max-w-5xl mx-auto px-6 py-4 flex items-center justify-between">
          <div>
            <p className="text-[10px] font-bold uppercase tracking-[0.2em] text-slate-500">
              {t('asteraAdmin')}
            </p>
            <h1 className="text-lg font-black tracking-tight text-slate-100">{t('title')}</h1>
          </div>

          <div className="flex items-center gap-3">
            {health && (
              <span className="text-[11px] text-slate-500 font-mono">
                {t('updated', { time: health.last_fetched.toLocaleTimeString() })}
              </span>
            )}
            <button
              onClick={fetchHealth}
              disabled={loading}
              className="rounded-xl border border-slate-700 bg-slate-800 px-4 py-2 text-sm font-semibold text-slate-300 hover:bg-slate-700 disabled:opacity-50 transition-all duration-200"
            >
              {loading ? t('refreshing') : t('refresh')}
            </button>
          </div>
        </div>
      </div>

      <div className="max-w-5xl mx-auto px-6 py-8 space-y-6">
        {/* ── Error state ── */}
        {error && (
          <div className="rounded-xl border border-rose-500/30 bg-rose-500/5 px-5 py-4 text-sm text-rose-300">
            ⚠ {error}
          </div>
        )}

        {/* ── Cleanup suggestion ── */}
        {health && (
          <SuggestionBanner
            cleanableCount={health.cleanable_ids.length}
            estimatedSavingsXlm={savingsXlm}
            onCleanup={handleCleanup}
            loading={cleanupLoading}
          />
        )}

        {/* ── Cleanup result toast ── */}
        <CleanupResultToast result={cleanupResult} />

        {/* ── Off-chain service liveness ── */}
        <div className="rounded-2xl border border-slate-800 bg-slate-900/30 p-6 space-y-4">
          <div className="flex items-center justify-between">
            <h2 className="text-[11px] font-black uppercase tracking-widest text-slate-400">
              {t('servicesHealth')}
            </h2>
            {services && (
              <span
                className={`text-xs font-mono font-semibold px-2.5 py-1 rounded-full border ${
                  services.status === 'ok'
                    ? 'text-emerald-400 border-emerald-500/30 bg-emerald-500/10'
                    : services.status === 'degraded'
                      ? 'text-amber-400 border-amber-500/30 bg-amber-500/10'
                      : 'text-rose-400 border-rose-500/30 bg-rose-500/10'
                }`}
              >
                {t('overall', { status: services.status.toUpperCase() })}
              </span>
            )}
          </div>

          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <StatCard
              label={t('stellarRpc')}
              value={services?.checks?.stellar_rpc?.status.toUpperCase() ?? t('unknown')}
              sub={services?.checks?.stellar_rpc?.detail ?? t('checking')}
              accent={
                services?.checks?.stellar_rpc?.status === 'ok'
                  ? 'green'
                  : services?.checks?.stellar_rpc?.status === 'degraded'
                    ? 'amber'
                    : 'red'
              }
            />
            <StatCard
              label={t('oracleService')}
              value={services?.checks?.oracle_service?.status.toUpperCase() ?? t('unknown')}
              sub={services?.checks?.oracle_service?.detail ?? t('checking')}
              accent={
                services?.checks?.oracle_service?.status === 'ok'
                  ? 'green'
                  : services?.checks?.oracle_service?.status === 'degraded'
                    ? 'amber'
                    : 'red'
              }
            />
            <StatCard
              label={t('complianceService')}
              value={services?.checks?.compliance_service?.status.toUpperCase() ?? t('unknown')}
              sub={services?.checks?.compliance_service?.detail ?? t('checking')}
              accent={
                services?.checks?.compliance_service?.status === 'ok'
                  ? 'green'
                  : services?.checks?.compliance_service?.status === 'degraded'
                    ? 'amber'
                    : 'red'
              }
            />
          </div>
        </div>

        {/* ── Stat grid ── */}
        {loading && !health ? (
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            {Array.from({ length: 4 }).map((_, i) => (
              <div
                key={i}
                className="rounded-2xl border border-slate-800 bg-slate-900/40 p-5 h-28 animate-pulse"
              />
            ))}
          </div>
        ) : health ? (
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            <StatCard
              label={t('activeEntries')}
              value={health.stats.active_invoices.toLocaleString()}
              sub={t('ofTotal', { pct: utilizationPct })}
              accent="blue"
            />
            <StatCard
              label={t('totalCreated')}
              value={health.stats.total_invoices.toLocaleString()}
              accent="blue"
            />
            <StatCard
              label={t('cleanedEntries')}
              value={health.stats.cleaned_invoices.toLocaleString()}
              sub={t('cumulative')}
              accent="green"
            />
            <StatCard
              label={t('cleanableNow')}
              value={health.cleanable_ids.length.toLocaleString()}
              sub={t('terminalNotRemoved')}
              accent={health.cleanable_ids.length > 0 ? 'amber' : 'green'}
            />
          </div>
        ) : null}

        {/* ── Cost estimate ── */}
        {health && (
          <div className="rounded-2xl border border-slate-800 bg-slate-900/30 p-6 space-y-4">
            <h2 className="text-[11px] font-black uppercase tracking-widest text-slate-400">
              {t('estimatedCost')}
            </h2>

            <div className="flex flex-wrap items-end gap-6">
              <div>
                <p className="text-4xl font-black font-mono text-sky-400 tracking-tight">
                  {costXlm.toFixed(4)}
                  <span className="text-lg text-slate-500 ml-2">{t('xlm')}</span>
                </p>
                <p className="text-sm text-slate-500 mt-1 font-mono">
                  {costUsdc !== null
                    ? t('usdcEstimate', { cost: costUsdc.toFixed(4) })
                    : t('usdcEstimateUnknown')}
                </p>
              </div>

              <div className="text-xs text-slate-600 font-mono leading-relaxed">
                <p>
                  {t('activeEntriesCount', {
                    count: health.stats.active_invoices.toLocaleString(),
                  })}
                </p>
                <p>{t('stroopPerLedger')}</p>
                <p>{t('ledgersPerMonth', { count: LEDGERS_PER_MONTH.toLocaleString() })}</p>
                <p className="text-slate-500 mt-1">
                  {t('stroopsPerXlm', { count: STROOPS_PER_XLM.toLocaleString() })}
                </p>
              </div>
            </div>

            <p className="text-[11px] text-slate-600">
              {t('costDisclaimer', {
                rate: XLM_USDC_RATE !== null ? `$${XLM_USDC_RATE.toFixed(2)}` : '--',
              })}
            </p>
          </div>
        )}

        {/* ── Manual cleanup ── */}
        {health && (
          <div className="rounded-2xl border border-slate-800 bg-slate-900/30 p-6 space-y-4">
            <div className="flex items-center justify-between">
              <h2 className="text-[11px] font-black uppercase tracking-widest text-slate-400">
                {t('manualCleanup')}
              </h2>
              {health.cleanable_ids.length > 0 && (
                <span className="text-[11px] font-mono text-slate-500">
                  {t('cleanupBatches', { count: Math.ceil(health.cleanable_ids.length / 50) })}
                </span>
              )}
            </div>

            <p className="text-sm text-slate-400 leading-relaxed">{t('cleanupDescription')}</p>

            <div className="flex flex-wrap gap-3">
              <button
                onClick={handleCleanup}
                disabled={cleanupLoading || health.cleanable_ids.length === 0}
                className="inline-flex items-center gap-2 rounded-xl bg-sky-600 hover:bg-sky-500 disabled:bg-slate-700 disabled:text-slate-500 px-5 py-2.5 text-sm font-bold text-white transition-all duration-200 disabled:cursor-not-allowed"
              >
                {cleanupLoading ? (
                  <>
                    <span className="inline-block w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin" />
                    {t('cleaning')}
                  </>
                ) : (
                  <>
                    {t('runCleanup')}
                    {health.cleanable_ids.length > 0 && ` (${health.cleanable_ids.length})`}
                  </>
                )}
              </button>

              {health.cleanable_ids.length === 0 && (
                <span className="self-center text-sm text-emerald-400 font-mono">
                  {t('storageClean')}
                </span>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
