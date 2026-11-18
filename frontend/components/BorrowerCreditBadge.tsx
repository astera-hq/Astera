'use client';

import useSWR from 'swr';
import { getFullCreditScore } from '@/lib/contracts';

function band(score: number) {
  if (score >= 800) return ['Excellent', 'text-emerald-400'] as const;
  if (score >= 700) return ['Good', 'text-blue-400'] as const;
  if (score >= 600) return ['Fair', 'text-yellow-400'] as const;
  return ['Poor', 'text-red-400'] as const;
}

export default function BorrowerCreditBadge({ borrower }: { borrower: string }) {
  const { data, isLoading } = useSWR(['credit-score', borrower], ([, address]) =>
    getFullCreditScore(address),
  );
  if (isLoading)
    return (
      <div className="rounded-xl border border-brand-border bg-brand-card p-4 text-sm text-brand-muted">
        Loading borrower credit score…
      </div>
    );
  if (!data || data.totalInvoices === 0)
    return (
      <div className="rounded-xl border border-brand-border bg-brand-card p-4">
        <h3 className="font-semibold">Borrower credit</h3>
        <p className="mt-1 text-sm text-brand-muted">No credit history yet.</p>
      </div>
    );
  const headline = data.finalScore ?? data.blendedScore ?? data.score;
  const [label, color] = band(headline);
  return (
    <div className="rounded-xl border border-brand-border bg-brand-card p-4">
      <div className="flex items-baseline justify-between">
        <h3 className="font-semibold">Borrower credit</h3>
        <span className={`font-medium ${color}`}>{label}</span>
      </div>
      <p className="mt-2 text-2xl font-bold">
        {headline}
        <span className="text-sm text-brand-muted"> / 850</span>
      </p>
      {(data.riskAdjustmentPts !== 0 || data.trendAdjustmentPts !== 0) && (
        <p className="mt-1 text-xs text-brand-muted">
          {data.riskAdjustmentPts !== 0 &&
            `Risk adjustment: ${data.riskAdjustmentPts > 0 ? '+' : ''}${data.riskAdjustmentPts} pts`}
          {data.riskAdjustmentPts !== 0 && data.trendAdjustmentPts !== 0 && ' · '}
          {data.trendAdjustmentPts !== 0 &&
            `Trend: ${data.trendAdjustmentPts > 0 ? '+' : ''}${data.trendAdjustmentPts} pts`}
        </p>
      )}
      <p className="mt-2 text-sm text-brand-muted">
        Payment history: {data.paidOnTime} on-time, {data.paidLate} late · average{' '}
        {data.averagePaymentDays} days
      </p>
      <p className="text-sm text-brand-muted">
        Default history: {data.defaulted} · {data.totalInvoices} invoices
      </p>
    </div>
  );
}
