'use client';

import InsuranceCoverageBadge from '@/components/InsuranceCoverageBadge';
import type { FullCreditScore, FundedInvoice } from '@/lib/types';

/** Borrower credit score card and insurance coverage badge. */
interface CreditScorePanelProps {
  creditScore: FullCreditScore | null;
  fundedInvoice: FundedInvoice | null;
}

export default function CreditScorePanel({ creditScore, fundedInvoice }: CreditScorePanelProps) {
  return (
    <>
      {creditScore && (
        <div className="p-6 bg-brand-card border border-brand-border rounded-2xl mb-6">
          <div className="flex items-center justify-between gap-4">
            <div>
              <h2 className="text-lg font-semibold mb-1">Borrower Credit Score</h2>
              <p className="text-xs text-brand-muted">
                {creditScore.totalInvoices > 0
                  ? `${creditScore.paidOnTime}/${creditScore.totalInvoices} invoices paid on time`
                  : 'No repayment history yet'}
              </p>
            </div>
            <div className="text-right">
              <div className="text-3xl font-bold gradient-text">{creditScore.blendedScore}</div>
              <span className="text-xs text-brand-muted">
                {creditScore.totalInvoices === 0
                  ? 'No history'
                  : creditScore.blendedScore >= 750
                    ? 'Excellent'
                    : creditScore.blendedScore >= 650
                      ? 'Good'
                      : creditScore.blendedScore >= 550
                        ? 'Fair'
                        : 'Building'}
              </span>
            </div>
          </div>
        </div>
      )}

      {fundedInvoice && (
        <div className="mb-6">
          <InsuranceCoverageBadge fundedInvoice={fundedInvoice} />
        </div>
      )}
    </>
  );
}
