'use client';

/**
 * @fileoverview Scenario Modeler — investor returns projection tool (#289)
 * @description Lets investors model best/base/worst-case outcomes across five
 *   parameters: deposit, duration, yield rate, default rate, and collateral
 *   recovery. All maths is client-side; no contract calls needed.
 *
 *   Drop into the /invest page beneath the APYCalculator:
 *   ```tsx
 *   <ScenarioModeler yieldBps={poolConfig?.yield_bps ?? null} />
 *   ```
 */

import { useMemo, useState } from 'react';
import {
  computeAllScenarios,
  breakevenUtilization,
  formatApyPercent,
  type ScenarioResult,
} from '@/lib/apy';
import { toStroops, formatUSDC } from '@/lib/stellar';

// ─── Types ────────────────────────────────────────────────────────────────────

export interface ScenarioModelerProps {
  /** Live yield_bps from pool contract. null while loading. */
  yieldBps: number | null;
  /** Show loading state while pool config is being fetched. */
  loading?: boolean;
  className?: string;
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

function pct(n: number) {
  return `${(n * 100).toFixed(0)}%`;
}

function signedUSDC(stroops: bigint): string {
  const formatted = formatUSDC(stroops < 0n ? -stroops : stroops);
  return stroops < 0n ? `−${formatted}` : `+${formatted}`;
}

function signedPct(n: number): string {
  const abs = Math.abs(n).toFixed(2);
  return n < 0 ? `−${abs}%` : `+${abs}%`;
}

// ─── Slider ───────────────────────────────────────────────────────────────────

interface SliderProps {
  label: string;
  hint?: string;
  min: number;
  max: number;
  step: number;
  value: number;
  onChange: (v: number) => void;
  format: (v: number) => string;
  disabled?: boolean;
  accent?: string;
}

function Slider({
  label,
  hint,
  min,
  max,
  step,
  value,
  onChange,
  format,
  disabled,
  accent = '#C9A84C',
}: SliderProps) {
  const pctFill = ((value - min) / (max - min)) * 100;

  return (
    <div className={`space-y-1.5 ${disabled ? 'opacity-40' : ''}`}>
      <div className="flex items-baseline justify-between">
        <label className="text-xs font-semibold tracking-wide text-slate-400 uppercase">
          {label}
        </label>
        <span className="text-sm font-bold tabular-nums" style={{ color: accent }}>
          {format(value)}
        </span>
      </div>
      {hint && <p className="text-[11px] text-slate-600">{hint}</p>}
      <div className="relative h-1.5 rounded-full bg-slate-800">
        <div
          className="absolute inset-y-0 left-0 rounded-full transition-all duration-100"
          style={{ width: `${pctFill}%`, background: accent }}
        />
        <input
          type="range"
          aria-label={label || 'Scenario slider'}
          min={min}
          max={max}
          step={step}
          value={value}
          onChange={(e) => onChange(Number(e.target.value))}
          disabled={disabled}
          className="absolute inset-0 w-full h-full opacity-0 cursor-pointer disabled:cursor-not-allowed"
        />
      </div>
    </div>
  );
}

// ─── Scenario bar ─────────────────────────────────────────────────────────────

const SCENARIO_PALETTE: Record<
  ScenarioResult['label'],
  { bar: string; badge: string; text: string }
> = {
  'Best Case': {
    bar: 'bg-emerald-500',
    badge: 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20',
    text: 'text-emerald-400',
  },
  'Base Case': {
    bar: 'bg-amber-500',
    badge: 'bg-amber-500/10  text-amber-400  border-amber-500/20',
    text: 'text-amber-400',
  },
  'Worst Case': {
    bar: 'bg-rose-500',
    badge: 'bg-rose-500/10   text-rose-400   border-rose-500/20',
    text: 'text-rose-500',
  },
};

interface ScenarioRowProps {
  scenario: ScenarioResult;
  maxAbsReturn: number;
  principalStroops: bigint;
}

function ScenarioBar({ scenario, maxAbsReturn, principalStroops }: ScenarioRowProps) {
  const palette = SCENARIO_PALETTE[scenario.label];
  const absReturn = Math.abs(Number(scenario.netReturnStroops));
  const barWidth = maxAbsReturn > 0 ? (absReturn / maxAbsReturn) * 100 : 0;
  const isNegative = scenario.netReturnStroops < 0n;

  return (
    <div className="rounded-2xl border border-slate-800 bg-slate-900/50 p-5 space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <span
          className={`text-[11px] font-bold uppercase tracking-widest px-2.5 py-1 rounded-full border ${palette.badge}`}
        >
          {scenario.label}
        </span>
        <div className="text-right">
          <p
            className={`text-xl font-black tabular-nums ${isNegative ? 'text-rose-400' : palette.text}`}
          >
            {signedUSDC(scenario.netReturnStroops)}
          </p>
          <p className="text-[11px] text-slate-500 tabular-nums">
            {signedPct(scenario.netReturnPct)} annualised
          </p>
        </div>
      </div>

      {/* Bar */}
      <div className="h-2 rounded-full bg-slate-800 overflow-hidden">
        <div
          className={`h-full rounded-full transition-all duration-500 ${isNegative ? 'bg-rose-500' : palette.bar}`}
          style={{ width: `${barWidth}%` }}
        />
      </div>

      {/* Breakdown */}
      <div className="grid grid-cols-3 gap-2 text-center">
        {[
          { l: 'Gross yield', v: signedUSDC(scenario.grossYieldStroops), dim: false },
          {
            l: 'Default loss',
            v:
              scenario.defaultLossStroops > 0n
                ? `−${formatUSDC(scenario.defaultLossStroops)}`
                : '$0.00',
            dim: scenario.defaultLossStroops === 0n,
          },
          { l: 'Utilization', v: pct(scenario.utilizationRate), dim: false },
        ].map(({ l, v, dim }) => (
          <div key={l} className="rounded-xl bg-slate-800/60 px-2 py-2">
            <p className="text-[10px] text-slate-500 mb-0.5">{l}</p>
            <p
              className={`text-xs font-bold tabular-nums ${dim ? 'text-slate-500' : 'text-slate-200'}`}
            >
              {v}
            </p>
          </div>
        ))}
      </div>
    </div>
  );
}

// ─── Main component ───────────────────────────────────────────────────────────

const SAVINGS_RATE_PCT = 5; // reference savings account rate for breakeven

export function ScenarioModeler({
  yieldBps,
  loading = false,
  className = '',
}: ScenarioModelerProps) {
  // ── Inputs ──────────────────────────────────────────────────────────────────
  const [depositUSDC, setDepositUSDC] = useState(10_000);
  const [lockDays, setLockDays] = useState(90);
  const [yieldOverride, setYieldOverride] = useState<number | null>(null);
  const [utilizationRate, setUtilizationRate] = useState(0.8);
  const [defaultRate, setDefaultRate] = useState(0.02);
  const [collateralRecovery, setCollateralRecovery] = useState(0.7);
  const [useOverride, setUseOverride] = useState(false);

  const effectiveYieldBps =
    useOverride && yieldOverride !== null ? yieldOverride : (yieldBps ?? 800);

  const principalStroops = toStroops(depositUSDC);

  // ── Scenarios ───────────────────────────────────────────────────────────────
  const [best, base, worst] = useMemo(
    () =>
      computeAllScenarios({
        principalStroops,
        yieldBps: effectiveYieldBps,
        lockDays,
        utilizationRate,
        defaultRate,
        collateralRecoveryRate: collateralRecovery,
      }),
    [
      principalStroops,
      effectiveYieldBps,
      lockDays,
      utilizationRate,
      defaultRate,
      collateralRecovery,
    ],
  );

  const maxAbsReturn = Math.max(
    Math.abs(Number(best.netReturnStroops)),
    Math.abs(Number(base.netReturnStroops)),
    Math.abs(Number(worst.netReturnStroops)),
    1,
  );

  // ── Breakeven ───────────────────────────────────────────────────────────────
  const breakeven = useMemo(
    () =>
      breakevenUtilization(
        {
          principalStroops,
          yieldBps: effectiveYieldBps,
          lockDays,
          defaultRate,
          collateralRecoveryRate: collateralRecovery,
        },
        SAVINGS_RATE_PCT,
      ),
    [principalStroops, effectiveYieldBps, lockDays, defaultRate, collateralRecovery],
  );

  const hasValidRate = yieldBps !== null && yieldBps >= 0;
  const isReady = !loading && (hasValidRate || useOverride);

  // ── Render ──────────────────────────────────────────────────────────────────
  return (
    <div
      className={`rounded-2xl border border-slate-800 bg-slate-900 overflow-hidden ${className}`}
    >
      {/* ── Header ── */}
      <div className="px-6 py-5 border-b border-slate-800 flex items-start justify-between gap-4">
        <div>
          <h2 className="text-base font-bold text-white tracking-tight">Scenario Modeler</h2>
          <p className="text-xs text-slate-500 mt-0.5">
            Model best / base / worst-case outcomes across five parameters. Live rate:{' '}
            <span className="text-amber-400 font-semibold">
              {loading
                ? 'loading…'
                : hasValidRate
                  ? `${formatApyPercent(yieldBps!)}% APY`
                  : 'unavailable'}
            </span>
          </p>
        </div>
        <label className="flex items-center gap-2 cursor-pointer shrink-0 mt-0.5">
          <span className="text-[11px] text-slate-500">Override rate</span>
          <button
            role="switch"
            aria-checked={useOverride}
            onClick={() => setUseOverride((v) => !v)}
            className={`relative w-9 h-5 rounded-full transition-colors duration-200 ${useOverride ? 'bg-amber-500' : 'bg-slate-700'}`}
          >
            <span
              className={`absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full shadow transition-transform duration-200 ${useOverride ? 'translate-x-4' : ''}`}
            />
          </button>
        </label>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-[320px_1fr] divide-y lg:divide-y-0 lg:divide-x divide-slate-800">
        {/* ── Controls ── */}
        <div className="p-6 space-y-6">
          {/* Deposit amount — numeric input + slider */}
          <div className="space-y-1.5">
            <div className="flex items-baseline justify-between">
              <label className="text-xs font-semibold tracking-wide text-slate-400 uppercase">
                Deposit Amount
              </label>
              <span className="text-sm font-bold text-amber-400 tabular-nums">
                ${depositUSDC.toLocaleString()}
              </span>
            </div>
            <div className="relative">
              <span className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-500 text-sm">
                $
              </span>
              <input
                type="number"
                aria-label="Deposit Amount"
                min={1_000}
                max={1_000_000}
                step={1_000}
                value={depositUSDC}
                onChange={(e) =>
                  setDepositUSDC(Math.max(1_000, Math.min(1_000_000, Number(e.target.value))))
                }
                className="w-full bg-slate-800 border border-slate-700 rounded-xl pl-7 pr-4 py-2.5 text-white text-sm focus:outline-none focus:border-amber-500 tabular-nums"
              />
            </div>
            <Slider
              label="Deposit Amount"
              min={1_000}
              max={1_000_000}
              step={1_000}
              value={depositUSDC}
              onChange={setDepositUSDC}
              format={(v) => `$${v.toLocaleString()}`}
            />
          </div>

          <Slider
            label="Investment Duration"
            hint="Days you plan to keep funds in the pool"
            min={30}
            max={365}
            step={1}
            value={lockDays}
            onChange={setLockDays}
            format={(v) => `${v} days`}
          />

          {useOverride && (
            <Slider
              label="Yield Rate Override"
              hint="Overrides the live pool rate for modelling"
              min={100}
              max={3_000}
              step={50}
              value={yieldOverride ?? effectiveYieldBps}
              onChange={(v) => setYieldOverride(v)}
              format={(v) => `${(v / 100).toFixed(2)}% APY`}
              accent="#F59E0B"
            />
          )}

          <div className="border-t border-slate-800 pt-5 space-y-5">
            <p className="text-[11px] font-bold uppercase tracking-widest text-slate-600">
              Risk Assumptions
            </p>

            <Slider
              label="Pool Utilization"
              hint="% of pool capital actively deployed to invoices"
              min={0}
              max={1}
              step={0.01}
              value={utilizationRate}
              onChange={setUtilizationRate}
              format={pct}
              accent="#60A5FA"
            />

            <Slider
              label="Default Rate"
              hint="% of deployed capital expected to default"
              min={0}
              max={0.2}
              step={0.005}
              value={defaultRate}
              onChange={setDefaultRate}
              format={(v) => `${(v * 100).toFixed(1)}%`}
              accent="#F87171"
            />

            <Slider
              label="Collateral Recovery"
              hint="% recovered from defaulted invoices via collateral"
              min={0.3}
              max={1}
              step={0.05}
              value={collateralRecovery}
              onChange={setCollateralRecovery}
              format={pct}
              accent="#A78BFA"
            />
          </div>
        </div>

        {/* ── Results ── */}
        <div className="p-6 space-y-4">
          {!isReady ? (
            <div className="h-full flex items-center justify-center text-slate-600 text-sm">
              {loading ? 'Loading pool rate…' : 'Enable rate override to begin modeling.'}
            </div>
          ) : (
            <>
              {/* Scenario bars */}
              {[best, base, worst].map((s) => (
                <ScenarioBar
                  key={s.label}
                  scenario={s}
                  maxAbsReturn={maxAbsReturn}
                  principalStroops={principalStroops}
                />
              ))}

              {/* Breakeven callout */}
              <div className="rounded-2xl border border-slate-800 bg-slate-900/50 p-5">
                <p className="text-[11px] font-bold uppercase tracking-widest text-slate-600 mb-2">
                  Breakeven vs {SAVINGS_RATE_PCT}% savings account
                </p>
                {breakeven === null ? (
                  <p className="text-sm text-rose-400 font-semibold">
                    ✗ Not achievable under current default assumptions. Reduce the default rate or
                    improve collateral recovery.
                  </p>
                ) : (
                  <p className="text-sm text-slate-300">
                    You need{' '}
                    <span className="font-black text-amber-400 text-base">
                      &gt;{pct(breakeven)}
                    </span>{' '}
                    pool utilization to outperform a {SAVINGS_RATE_PCT}% savings account over{' '}
                    {lockDays} days.
                    {utilizationRate >= breakeven ? (
                      <span className="text-emerald-400 font-semibold">
                        {' '}
                        ✓ Your base case clears this threshold.
                      </span>
                    ) : (
                      <span className="text-rose-400 font-semibold">
                        {' '}
                        Your base case is below this threshold.
                      </span>
                    )}
                  </p>
                )}
              </div>

              {/* Disclaimer */}
              <p className="text-[11px] text-slate-600 leading-relaxed border-t border-slate-800 pt-4">
                <strong className="text-slate-500">Disclaimer:</strong> This tool is for
                illustrative purposes only. Projections are based on simplified linear accrual and
                do not account for compounding, liquidity constraints, timing risk, or regulatory
                factors. Past performance is not indicative of future results. Nothing here
                constitutes financial advice.
              </p>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
