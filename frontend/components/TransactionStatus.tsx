'use client';

import { useStore, type TrackedTransaction } from '@/lib/store';
import { ExplorerLink } from './ExplorerLink';

const STATUS_CONFIG: Record<
  TrackedTransaction['status'],
  { bg: string; dot: string; label: string }
> = {
  pending: {
    bg: 'bg-blue-500/10 border-blue-500/30',
    dot: 'bg-blue-400 animate-pulse',
    label: 'Pending',
  },
  confirmed: {
    bg: 'bg-green-500/10 border-green-500/30',
    dot: 'bg-green-400',
    label: 'Confirmed',
  },
  failed: {
    bg: 'bg-red-500/10 border-red-500/30',
    dot: 'bg-red-400',
    label: 'Failed',
  },
};

export function TransactionStatusPanel() {
  const { trackedTransactions, removeTrackedTransaction } = useStore();

  if (trackedTransactions.length === 0) return null;

  return (
    <div className="fixed bottom-4 right-4 z-[250] w-80 space-y-2">
      {trackedTransactions.map((tx) => {
        const config = STATUS_CONFIG[tx.status];
        return (
          <div
            key={tx.hash}
            className={`p-3 rounded-xl border backdrop-blur-sm ${config.bg} transition-all`}
          >
            <div className="flex items-start justify-between gap-2">
              <div className="flex items-center gap-2 min-w-0">
                <span className={`h-2 w-2 rounded-full shrink-0 ${config.dot}`} />
                <span className="text-xs font-semibold text-white truncate">{tx.label}</span>
                <span className="text-[10px] text-brand-muted">{config.label}</span>
              </div>
              <button
                onClick={() => removeTrackedTransaction(tx.hash)}
                className="text-brand-muted hover:text-white text-xs shrink-0"
                aria-label="Dismiss"
              >
                ×
              </button>
            </div>
            {tx.hash && (
              <div className="mt-1.5">
                <ExplorerLink type="transaction" id={tx.hash} className="text-[10px]" />
              </div>
            )}
            {tx.error && <p className="mt-1 text-[10px] text-red-400 truncate">{tx.error}</p>}
          </div>
        );
      })}
    </div>
  );
}
