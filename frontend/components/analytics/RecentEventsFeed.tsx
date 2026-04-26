'use client';

import type { ContractEvent } from '@/lib/monitoring';

interface RecentEventsFeedProps {
  events: ContractEvent[];
  isLoading: boolean;
}

export function RecentEventsFeed({ events, isLoading }: RecentEventsFeedProps) {
  if (isLoading) {
    return (
      <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
        <div className="bg-brand-dark/50 animate-pulse rounded h-6 w-40 mb-4" />
        <div className="space-y-3">
          {Array.from({ length: 5 }).map((_, i) => (
            <div key={i} className="flex items-start gap-3">
              <div className="bg-brand-dark/50 animate-pulse rounded h-5 w-20 shrink-0" />
              <div className="flex-1">
                <div className="bg-brand-dark/50 animate-pulse rounded h-4 w-40 mb-1" />
                <div className="bg-brand-dark/50 animate-pulse rounded h-3 w-64" />
              </div>
            </div>
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
      <h3 className="text-lg font-bold text-white mb-4 flex items-center gap-2">
        <span className="w-2 h-6 bg-amber-500 rounded-full" />
        Recent On-Chain Events
      </h3>

      {events.length === 0 ? (
        <div className="text-center py-8 text-brand-muted">
          <p className="text-sm">No recent events detected.</p>
        </div>
      ) : (
        <div className="space-y-2 max-h-96 overflow-y-auto custom-scrollbar">
          {events.map((event) => {
            const [contractType, eventType] = event.topic;
            const isInvoice = contractType === 'INVOICE';

            return (
              <div
                key={event.id}
                className="flex items-start gap-3 p-3 bg-brand-dark/50 rounded-xl hover:bg-white/5 transition-colors"
              >
                <span
                  className={`text-[10px] font-bold px-2 py-0.5 rounded-full shrink-0 ${
                    isInvoice
                      ? eventType === 'default'
                        ? 'bg-red-500/20 text-red-500'
                        : 'bg-indigo-500/20 text-indigo-500'
                      : eventType === 'funded' || eventType === 'paid'
                        ? 'bg-green-500/20 text-green-500'
                        : 'bg-brand-gold/20 text-brand-gold'
                  }`}
                >
                  {String(eventType).toUpperCase()}
                </span>
                <div className="flex-1 min-w-0">
                  <p className="text-xs text-white font-medium">
                    {isInvoice ? 'Invoice Contract' : 'Pool Contract'}
                  </p>
                  <p className="text-[10px] text-brand-muted truncate">
                    {typeof event.value === 'object' && event.value !== null
                      ? JSON.stringify(event.value, (_, v) =>
                          typeof v === 'bigint' ? v.toString() : v,
                        ).slice(0, 80)
                      : String(event.value).slice(0, 80)}
                  </p>
                </div>
                <span className="text-[10px] text-brand-muted font-mono shrink-0">
                  #{event.ledger}
                </span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
