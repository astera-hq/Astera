'use client';

import { useMemo, useState } from 'react';
import { formatApyPercent, projectedInterestStroops } from '@/lib/apy';
import { formatUSDC, toStroops } from '@/lib/stellar';

export type APYCalculatorProps = {
  /** Live `yield_bps` from the pool contract (`get_config`). */
  yieldBps: number | null;
  /** While pool config is loading. */
  loading?: boolean;
  className?: string;
};

const DEFAULT_LOCK_DAYS = '30';

/**
 * Real-time projection of simple interest over a chosen horizon using the pool’s yield rate.
 */
export function APYCalculator({ yieldBps, loading = false, className = '' }: APYCalculatorProps) {
  const [depositInput, setDepositInput] = useState('');
  const [lockDaysInput, setLockDaysInput] = useState(DEFAULT_LOCK_DAYS);

  const { interestStroops, totalStroops } = useMemo(() => {
    if (yieldBps === null || yieldBps < 0) {
      return { interestStroops: 0n, totalStroops: 0n };
    }

    const deposit = parseFloat(depositInput);
    if (!Number.isFinite(deposit) || deposit <= 0) {
      return { interestStroops: 0n, totalStroops: 0n };
    }

    const days = parseInt(lockDaysInput, 10);
    if (!Number.isFinite(days) || days < 1) {
      return { interestStroops: 0n, totalStroops: 0n };
    }

    const principalStroops = toStroops(deposit);
    const interestStroops = projectedInterestStroops(principalStroops, yieldBps, days);
    return {
      interestStroops,
      totalStroops: principalStroops + interestStroops,
    };
  }, [depositInput, lockDaysInput, yieldBps]);

  const hasValidPoolRate = yieldBps !== null && yieldBps >= 0;

  return (
    <div className={`p-6 bg-brand-card border border-brand-border rounded-2xl ${className}`.trim()}>
      <h2 className="text-lg font-semibold mb-1">Earnings calculator</h2>
      <p className="text-xs text-brand-muted mb-4">
        Model projected returns using the pool&apos;s current rate (
        {loading
          ? 'loading…'
          : hasValidPoolRate
            ? `${formatApyPercent(yieldBps!)}% APY`
            : 'rate unavailable'}
        ).
      </p>

      <div className="space-y-4">
        <div>
          <label className="block text-sm text-brand-muted mb-2">Deposit amount (USDC)</label>
          <div className="relative">
            <input
              type="number"
              min="0"
              step="0.01"
              placeholder="0.00"
              value={depositInput}
              onChange={(e) => setDepositInput(e.target.value)}
              disabled={loading || !hasValidPoolRate}
              className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-3 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-lg disabled:opacity-50"
            />
            <span className="absolute right-4 top-1/2 -translate-y-1/2 text-brand-muted text-sm font-medium">
              USDC
            </span>
          </div>
        </div>

        <div>
          <label className="block text-sm text-brand-muted mb-2">Lock period (days)</label>
          <input
            type="number"
            min="1"
            step="1"
            value={lockDaysInput}
            onChange={(e) => setLockDaysInput(e.target.value)}
            disabled={loading || !hasValidPoolRate}
            className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-3 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-lg disabled:opacity-50"
          />
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-2">
          <div className="p-4 bg-brand-dark border border-brand-border rounded-xl">
            <p className="text-xs text-brand-muted mb-1">Projected interest</p>
            <p className="text-xl font-semibold text-brand-gold">
              {loading || !hasValidPoolRate
                ? '—'
                : depositInput && parseFloat(depositInput) > 0
                  ? formatUSDC(interestStroops)
                  : formatUSDC(0n)}
            </p>
          </div>
          <div className="p-4 bg-brand-dark border border-brand-border rounded-xl">
            <p className="text-xs text-brand-muted mb-1">Total at maturity</p>
            <p className="text-xl font-semibold text-white">
              {loading || !hasValidPoolRate
                ? '—'
                : depositInput && parseFloat(depositInput) > 0
                  ? formatUSDC(totalStroops)
                  : formatUSDC(0n)}
            </p>
          </div>
        </div>
      </div>

      <p className="mt-4 text-xs text-brand-muted leading-relaxed border-t border-brand-border pt-4">
        <strong className="text-brand-muted">Disclaimer:</strong> This projection uses the
        pool&apos;s configured yield rate and assumes continuous linear accrual like on-chain
        invoice interest. Actual returns depend on invoice repayment timing, utilization, and pool
        parameters — nothing is guaranteed.
      </p>
    </div>
  );
}
