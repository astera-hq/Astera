'use client';

import { formatDate } from '@/lib/stellar';
import type { TransactionStep } from '@/hooks/useInvoiceDetail';
import type { InvoiceStatus } from '@/lib/types';

/** Invoice status timeline. */
interface TimelineProps {
  statusSteps: TransactionStep[];
  status: InvoiceStatus;
}

export default function Timeline({ statusSteps, status }: TimelineProps) {
  return (
    <div className="p-6 bg-brand-card border border-brand-border rounded-2xl mb-6">
      <div className="flex items-center justify-between gap-4 mb-6">
        <h2 className="text-lg font-semibold">Timeline</h2>
        <span className={`text-xs px-2.5 py-1 rounded-full badge-${status.toLowerCase()}`}>
          {status}
        </span>
      </div>
      <div className="space-y-4">
        {statusSteps.map((step, i) => (
          <div key={step.label} className="flex items-center gap-4">
            <div
              className={`w-8 h-8 rounded-full flex items-center justify-center flex-shrink-0 text-xs font-bold ${
                step.done ? 'bg-brand-gold text-brand-dark' : 'bg-brand-border text-brand-muted'
              }`}
            >
              {step.done ? '✓' : i + 1}
            </div>
            <div className="flex-1 flex justify-between">
              <span className={step.done ? 'text-white font-medium' : 'text-brand-muted'}>
                {step.label}
              </span>
              {step.done && step.ts > 0 && (
                <span className="text-brand-muted text-sm">{formatDate(step.ts)}</span>
              )}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
