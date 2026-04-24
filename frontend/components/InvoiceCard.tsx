import type { InvoiceMetadata } from '@/lib/types';
import { formatUSDC, formatDate, daysUntil } from '@/lib/stellar';
import Link from 'next/link';

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
      className="block p-5 bg-brand-card border border-brand-border rounded-2xl hover:border-brand-gold/30 transition-colors group"
    >
      <div className="flex items-start justify-between gap-3 mb-4">
        <div className="flex gap-3 min-w-0 flex-1">
          {metadata.image ? (
            // eslint-disable-next-line @next/next/no-img-element
            <img
              src={metadata.image}
              alt=""
              className="w-12 h-12 rounded-xl object-cover border border-brand-border flex-shrink-0 bg-brand-dark"
            />
          ) : null}
          <div className="min-w-0">
            <p className="text-xs text-brand-muted mb-1">
              {metadata.symbol} · #{id}
            </p>
            <h3 className="font-semibold text-lg group-hover:text-brand-gold transition-colors line-clamp-2">
              {metadata.name}
            </h3>
            <p className="text-sm text-brand-muted truncate mt-0.5">{metadata.debtor}</p>
          </div>
        </div>
        <span
          className={`text-xs font-medium px-2.5 py-1 rounded-full flex-shrink-0 badge-${metadata.status.toLowerCase()}`}
        >
          {statusLabel[metadata.status] ?? metadata.status}
        </span>
      </div>

      <div className="text-2xl font-bold mb-4">{formatUSDC(metadata.amount)}</div>

      <div className="flex items-center justify-between text-sm text-brand-muted">
        <div>
          Due <span className="text-white">{formatDate(metadata.dueDate)}</span>
        </div>
        <div
          className={
            isOverdue ? 'text-red-400' : days <= 7 ? 'text-yellow-400' : 'text-brand-muted'
          }
        >
          {isOverdue ? `${Math.abs(days)}d overdue` : `${days}d left`}
        </div>
      </div>

      {showProgress && (
        <div className="mt-4 border-t border-brand-border pt-4">
          <div className="flex items-center justify-between text-xs text-brand-muted mb-1.5">
            <span>Co-funding progress</span>
            <span className="text-white font-medium">{fundedPercent.toFixed(1)}%</span>
          </div>
          <div className="h-1.5 bg-brand-border rounded-full overflow-hidden">
            <div
              className="h-full bg-brand-gold rounded-full transition-all duration-300"
              style={{ width: `${Math.min(100, fundedPercent)}%` }}
            />
          </div>
          <div className="flex items-center justify-between text-xs mt-1.5">
            <span className="text-brand-muted">{formatUSDC(fundedAmount!)} committed</span>
            <span className="text-brand-muted">
              {formatUSDC(metadata.amount - fundedAmount!)} remaining
            </span>
          </div>
        </div>
      )}

      {metadata.description && (
        <p className="mt-3 text-xs text-brand-muted line-clamp-2 border-t border-brand-border pt-3">
          {metadata.description}
        </p>
      )}
    </Link>
  );
}
