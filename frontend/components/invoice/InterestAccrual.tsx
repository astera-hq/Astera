'use client';

import { formatUSDC } from '@/lib/stellar';
import type { PoolConfig, FundedInvoice } from '@/lib/types';

/** Interest accrual / repayment summary. Returns null when data is missing. */
interface InterestAccrualProps {
  fundedInvoice: FundedInvoice | null;
  poolConfig: PoolConfig | null;
  projectedInterest: bigint;
  accruedInterest: bigint;
  interestProgress: number;
  remainingDue: bigint;
}

export default function InterestAccrual({
  fundedInvoice,
  poolConfig,
  projectedInterest,
  accruedInterest,
  interestProgress,
  remainingDue,
}: InterestAccrualProps) {
  if (!fundedInvoice || !poolConfig) return null;

  return (
    <div className="p-6 bg-brand-card border border-brand-border rounded-2xl mb-6">
      <h2 className="text-lg font-semibold mb-4">Interest Accrual</h2>
      <div className="space-y-3 text-sm">
        <div className="flex items-center justify-between">
          <span className="text-brand-muted">Accrued interest</span>
          <span className="font-medium">{formatUSDC(accruedInterest)}</span>
        </div>
        <div className="flex items-center justify-between">
          <span className="text-brand-muted">Projected interest to due date</span>
          <span className="font-medium">{formatUSDC(projectedInterest)}</span>
        </div>
        <div className="flex items-center justify-between">
          <span className="text-brand-muted">Estimated total due</span>
          <span className="font-semibold">
            {formatUSDC(
              fundedInvoice.principal + projectedInterest + fundedInvoice.factoringFee,
            )}
          </span>
        </div>
        <div className="flex items-center justify-between">
          <span className="text-brand-muted">Already repaid</span>
          <span className="font-medium text-green-400">
            {formatUSDC(fundedInvoice.repaidAmount)}
          </span>
        </div>
        <div className="flex items-center justify-between">
          <span className="text-brand-muted font-semibold">Remaining due</span>
          <span className="font-semibold text-brand-gold">{formatUSDC(remainingDue)}</span>
        </div>
        <div className="h-2 bg-brand-border rounded-full overflow-hidden">
          <div
            className="h-full bg-brand-gold rounded-full transition-all"
            style={{ width: `${interestProgress}%` }}
          />
        </div>
        <p className="text-xs text-brand-muted">
          Estimated against {poolConfig.yieldBps / 100}% APY over the remaining term.
        </p>
      </div>
    </div>
  );
}
