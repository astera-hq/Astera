'use client';

import { truncateAddress } from '@/lib/stellar';
import type { InvoiceEvent } from '@/hooks/useInvoiceDetail';

/** Parsed on-chain event timeline for the invoice. */
interface InvoiceHistoryProps {
  history: InvoiceEvent[];
  historyLoading: boolean;
  historyError: string | null;
}

export default function InvoiceHistory({
  history,
  historyLoading,
  historyError,
}: InvoiceHistoryProps) {
  if (historyLoading) {
    return (
      <div className="p-6 bg-brand-card border border-brand-border rounded-2xl mb-6">
        <div className="h-5 bg-brand-border rounded w-40 mb-4 animate-pulse" />
        <div className="space-y-3">
          {[1, 2, 3].map((n) => (
            <div key={n} className="h-14 bg-brand-dark rounded-xl animate-pulse" />
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="p-6 bg-brand-card border border-brand-border rounded-2xl mb-6">
      <div className="flex items-center justify-between gap-4 mb-4">
        <h2 className="text-lg font-semibold">Transaction History</h2>
        {historyError && <span className="text-xs text-brand-muted">{historyError}</span>}
      </div>
      {history.length === 0 ? (
        <p className="text-sm text-brand-muted">No related transactions found.</p>
      ) : (
        <div className="space-y-3">
          {history.map((event) => (
            <div
              key={`${event.kind}-${event.ledger}-${event.txHash}`}
              className="p-4 rounded-xl border border-brand-border bg-brand-dark/60"
            >
              <div className="flex items-start justify-between gap-4">
                <div>
                  <p className="font-medium text-white">{event.label}</p>
                  <p className="text-sm text-brand-muted mt-1">{event.detail}</p>
                </div>
                {event.txHash && (
                  <a
                    href={`https://stellar.expert/explorer/testnet/tx/${event.txHash}`}
                    target="_blank"
                    rel="noreferrer"
                    className="text-xs text-brand-gold hover:underline break-all"
                  >
                    {truncateAddress(event.txHash)}
                  </a>
                )}
              </div>
              {event.timestamp && (
                <p className="text-xs text-brand-muted mt-2">
                  {new Date(event.timestamp).toLocaleString('en-US', {
                    year: 'numeric',
                    month: 'short',
                    day: 'numeric',
                    hour: '2-digit',
                    minute: '2-digit',
                  })}
                </p>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
