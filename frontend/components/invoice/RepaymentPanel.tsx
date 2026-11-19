'use client';

import type { Dispatch, SetStateAction } from 'react';
import { formatUSDC } from '@/lib/stellar';
import EstimatedFee from '@/components/EstimatedFee';
import type { SimulationState } from '@/hooks/useTransactionSimulation';
import type { FundedInvoice, InvoiceStatus } from '@/lib/types';

/** Repayment form, estimated fee and the "fully repaid" notice. */
interface RepaymentPanelProps {
  isOwner: boolean;
  status: InvoiceStatus;
  fundedInvoice: FundedInvoice | null;
  remainingDue: bigint;
  fullyRepaid: boolean;
  actionLoading: boolean;
  repayAmount: string;
  setRepayAmount: Dispatch<SetStateAction<string>>;
  repaySimulation: SimulationState;
  onRepay: () => void;
}

export default function RepaymentPanel({
  isOwner,
  status,
  fundedInvoice,
  remainingDue,
  fullyRepaid,
  actionLoading,
  repayAmount,
  setRepayAmount,
  repaySimulation,
  onRepay,
}: RepaymentPanelProps) {
  return (
    <>
      {isOwner && status === 'Funded' && fundedInvoice && !fullyRepaid && (
        <div className="p-4 bg-brand-card border border-brand-border rounded-2xl space-y-3">
          <label className="block text-sm text-brand-muted mb-1">Repayment Amount (USDC)</label>
          <input
            type="text"
            value={repayAmount}
            onChange={(e) => setRepayAmount(e.target.value)}
            placeholder={`Max: ${formatUSDC(remainingDue)}`}
            disabled={actionLoading}
            className="w-full px-4 py-2 bg-brand-dark border border-brand-border rounded-lg text-white placeholder-brand-muted focus:border-brand-gold focus:outline-none disabled:opacity-60"
          />
          <EstimatedFee simulation={repaySimulation} />

          <button
            onClick={onRepay}
            disabled={actionLoading || !repayAmount || repaySimulation.status === 'loading'}
            className="w-full px-5 py-3 bg-brand-gold text-brand-dark font-semibold rounded-xl hover:bg-brand-amber transition-colors disabled:opacity-60"
          >
            {actionLoading
              ? 'Processing payment...'
              : repayAmount
                ? `Pay ${formatUSDC(BigInt(repayAmount))}`
                : 'Pay full amount'}
          </button>
        </div>
      )}

      {isOwner && status === 'Funded' && fundedInvoice && fullyRepaid && (
        <div className="p-4 bg-green-900/20 border border-green-800/50 rounded-xl text-sm text-green-400 text-center">
          Invoice fully repaid ✓
        </div>
      )}
    </>
  );
}
