import type { InvoiceMetadata } from '@/lib/types';
import { formatUSDC, formatDate, daysUntil } from '@/lib/stellar';
import Link from 'next/link';
import { Skeleton } from '@/components/Skeleton';

interface Props {
  id: number;
  metadata: InvoiceMetadata;
  /** Amount committed toward this invoice so far (only relevant for Pending invoices) */
  fundedAmount?: bigint;
}

const statusLabel: Record<string, string> = {
  Pending: 'Pending',
  Funded: 'Funded',
  Paid: 'Paid',
  Defaulted: 'Defaulted',
  Expired: 'Expired',
  Disputed: 'Disputed',
  Cancelled: 'Cancelled',
};

const statusClass: Record<string, string> = {
  Pending:
    'bg-slate-100 text-slate-700 border-slate-300 dark:bg-slate-900/40 dark:text-slate-400 dark:border-slate-800/50',
  Funded:
    'bg-blue-100 text-blue-700 border-blue-300 dark:bg-blue-900/40 dark:text-blue-400 dark:border-blue-800/50',
  Paid: 'bg-green-100 text-green-700 border-green-300 dark:bg-green-900/40 dark:text-green-400 dark:border-green-800/50',
  Defaulted:
    'bg-red-100 text-red-700 border-red-300 dark:bg-red-900/40 dark:text-red-400 dark:border-red-800/50',
  Expired:
    'bg-orange-100 text-orange-700 border-orange-300 dark:bg-orange-900/40 dark:text-orange-400 dark:border-orange-800/50',
  Disputed:
    'bg-yellow-100 text-yellow-700 border-yellow-300 dark:bg-yellow-900/40 dark:text-yellow-400 dark:border-yellow-800/50',
  Cancelled:
    'bg-slate-100 text-slate-500 border-slate-300 line-through dark:bg-slate-900/40 dark:text-slate-500 dark:border-slate-800/50',
};

export default function InvoiceCard({ id, metadata, fundedAmount }: Props) {
  const days = daysUntil(metadata.dueDate);
  const isOverdue = days < 0;

  const showProgress =
    metadata.status === 'Pending' && fundedAmount !== undefined && metadata.amount > 0n;

  const fundedPercent = showProgress
    ? Number((fundedAmount! * 10_000n) / metadata.amount) / 100
    : 0;

  return (
    <Link
      href={`/invoice/${id}`}
      className="block p-5 bg-[var(--card)] border border-[var(--border)] rounded-2xl hover:border-brand-gold/30 transition-colors group"
    >
      <div className="flex items-start justify-between gap-3 mb-4">
        <div className="flex gap-3 min-w-0 flex-1">
          {metadata.image ? (
            // eslint-disable-next-line @next/next/no-img-element
            <img
              src={metadata.image}
              alt=""
              className="w-12 h-12 rounded-xl object-cover border border-[var(--border)] flex-shrink-0 bg-[var(--bg)]"
            />
          ) : null}
          <div className="min-w-0">
            <p className="text-xs text-[var(--muted)] mb-1">
              {metadata.symbol} · #{id}
            </p>
            <h3 className="font-semibold text-lg group-hover:text-brand-gold transition-colors line-clamp-2 text-[var(--text-primary)]">
              {metadata.name}
            </h3>
            <p className="text-sm text-[var(--muted)] truncate mt-0.5">{metadata.debtor}</p>
          </div>
        </div>
        <span
          className={`text-xs font-medium px-2.5 py-1 rounded-full flex-shrink-0 border ${
            statusClass[metadata.status] ??
            'bg-slate-100 text-slate-700 border-slate-300 dark:bg-slate-900/40 dark:text-slate-400 dark:border-slate-800/50'
          }`}
        >
          {statusLabel[metadata.status] ?? metadata.status}
        </span>
      </div>

      <div className="text-2xl font-bold mb-4 text-[var(--text-primary)]">{formatUSDC(metadata.amount)}</div>

      <div className="flex items-center justify-between text-sm text-[var(--muted)]">
        <div>
          Due <span className="text-[var(--text-primary)]">{formatDate(metadata.dueDate)}</span>
        </div>
        <div
          className={
            isOverdue
              ? 'text-red-400'
              : days <= 7
                ? 'text-orange-400'
                : days <= 30
                  ? 'text-yellow-400'
                  : 'text-[var(--muted)]'
          }
        >
          {isOverdue ? `${Math.abs(days)}d overdue` : `${days}d left`}
        </div>
      </div>

      {showProgress && (
        <div className="mt-4 border-t border-[var(--border)] pt-4">
          <div className="flex items-center justify-between text-xs text-[var(--muted)] mb-1.5">
            <span>Co-funding progress</span>
            <span className="text-[var(--text-primary)] font-medium">{fundedPercent.toFixed(1)}%</span>
          </div>
          <div className="h-1.5 bg-[var(--border)] rounded-full overflow-hidden">
            <div
              className="h-full bg-brand-gold rounded-full transition-all duration-300"
              style={{ width: `${Math.min(100, fundedPercent)}%` }}
            />
          </div>
          <div className="flex items-center justify-between text-xs mt-1.5">
            <span className="text-[var(--muted)]">{formatUSDC(fundedAmount!)} committed</span>
            <span className="text-[var(--muted)]">
              {formatUSDC(metadata.amount - fundedAmount!)} remaining
            </span>
          </div>
        </div>
      )}

      {metadata.description && (
        <p className="mt-3 text-xs text-[var(--muted)] line-clamp-2 border-t border-[var(--border)] pt-3">
          {metadata.description}
        </p>
      )}
    </Link>
  );
}

export function InvoiceCardSkeleton() {
  return (
    <div className="p-5 bg-brand-card border border-brand-border rounded-2xl animate-pulse">
      <div className="flex items-start justify-between gap-3 mb-4">
        <div className="flex gap-3 min-w-0 flex-1">
          <Skeleton className="w-12 h-12 rounded-xl flex-shrink-0" />
          <div className="min-w-0 flex-1">
            <Skeleton className="h-3 w-24 mb-2" />
            <Skeleton className="h-5 w-48 mb-1" />
            <Skeleton className="h-4 w-32" />
          </div>
        </div>
        <Skeleton className="h-6 w-16 rounded-full flex-shrink-0" />
      </div>

      <Skeleton className="h-8 w-32 mb-4" />

      <div className="flex items-center justify-between text-sm">
        <div className="flex items-center gap-2">
          <Skeleton className="h-4 w-10" />
          <Skeleton className="h-4 w-28" />
        </div>
        <Skeleton className="h-4 w-20" />
      </div>
    </div>
  );
}
