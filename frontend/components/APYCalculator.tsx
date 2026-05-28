// Astera/frontend/components/APYCalculator.tsx

'use client';

import { useMemo, useState } from 'react';
import { usePoolConfig } from '@/lib/cache';
import { formatApyPercent, projectedInterestStroops } from '@/lib/apy';
import { formatUSDC, toStroops } from '@/lib/stellar';
import { Skeleton } from '@/components/Skeleton';

const DEFAULT_LOCK_DAYS = '30';
const DEFAULT_YIELD_BPS = 800;

/**
 * Real-time projection of simple interest over a chosen horizon using the pool's yield rate.
 *
 * Fetches the live `yield_bps` from the pool contract’s `get_config()` on mount
 * (via SWR-backed `usePoolConfig`). Falls back to {@link DEFAULT_YIELD_BPS} when
 * the contract read fails, and shows a loading skeleton during initial fetch.
 */
export function APYCalculator({ className = '' }: { className?: string }) {
  const [depositInput, setDepositInput] = useState('');
  const [lockDaysInput, setLockDaysInput] = useState(DEFAULT_LOCK_DAYS);
  const { data: poolConfig, isLoading } = usePoolConfig();

  const yieldBps = poolConfig?.yieldBps ?? DEFAULT_YIELD_BPS;
  const hasValidPoolRate = yieldBps >= 0;
  const isFallbackRate = !poolConfig;

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

  if (isLoading) {
    return <APYCalculatorSkeleton className={className} />;
  }

  return (
    <div className={`p-6 bg-brand-card border border-brand-border rounded-2xl ${className}`.trim()}>
      <h2 className="text-lg font-semibold mb-1">Earnings calculator</h2>
      <p className="text-xs text-brand-muted mb-4">
        Model projected returns using the pool&apos;s current rate (
        {hasValidPoolRate ? `${formatApyPercent(yieldBps!)}% APY` : 'rate unavailable'}
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
              disabled={!hasValidPoolRate}
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
            disabled={!hasValidPoolRate}
            className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-3 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-lg disabled:opacity-50"
          />
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-2">
          <div className="p-4 bg-brand-dark border border-brand-border rounded-xl">
            <p className="text-xs text-brand-muted mb-1">Projected interest</p>
            <p className="text-xl font-semibold text-brand-gold">
              {!hasValidPoolRate
                ? '---'
                : depositInput && parseFloat(depositInput) > 0
                  ? formatUSDC(interestStroops)
                  : formatUSDC(0n)}
            </p>
          </div>
          <div className="p-4 bg-brand-dark border border-brand-border rounded-xl">
            <p className="text-xs text-brand-muted mb-1">Total at maturity</p>
            <p className="text-xl font-semibold text-white">
              {!hasValidPoolRate
                ? '---'
                : depositInput && parseFloat(depositInput) > 0
                  ? formatUSDC(totalStroops)
                  : formatUSDC(0n)}
            </p>
          </div>
        </div>
      </div>

      {isFallbackRate && (
        <p className="mt-4 text-xs text-yellow-300">
          Failed to load live pool configuration. Using fallback APY of{' '}
          {formatApyPercent(DEFAULT_YIELD_BPS)}%.
        </p>
      )}

      <p className="mt-4 text-xs text-brand-muted leading-relaxed border-t border-brand-border pt-4">
        <strong className="text-brand-muted">Disclaimer:</strong> This projection uses the
        pool&apos;s configured yield rate and assumes continuous linear accrual like on-chain
        invoice interest. Actual returns depend on invoice repayment timing, utilization, and pool
        parameters -- nothing is guaranteed.
      </p>
    </div>
  );
}

export function APYCalculatorSkeleton({ className = '' }: { className?: string }) {
  return (
    <div
      className={`p-6 bg-brand-card border border-brand-border rounded-2xl ${className}`.trim()}
      role="status"
      aria-label="Loading earnings calculator"
    >
      <Skeleton className="h-6 w-44 mb-2" />
      <Skeleton className="h-3 w-72 mb-6" />

      <div className="space-y-4">
        <div>
          <Skeleton className="h-4 w-36 mb-3" />
          <Skeleton className="h-12 w-full rounded-xl" />
        </div>
        <div>
          <Skeleton className="h-4 w-28 mb-3" />
          <Skeleton className="h-12 w-full rounded-xl" />
        </div>
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-2">
          <div className="p-4 bg-brand-dark border border-brand-border rounded-xl">
            <Skeleton className="h-3 w-28 mb-2" />
            <Skeleton className="h-6 w-32" />
          </div>
          <div className="p-4 bg-brand-dark border border-brand-border rounded-xl">
            <Skeleton className="h-3 w-32 mb-2" />
            <Skeleton className="h-6 w-32" />
          </div>
        </div>
      </div>

      <Skeleton className="h-4 w-full mt-6" />
      <Skeleton className="h-3 w-3/5 mt-2" />
    </div>
  );
}
