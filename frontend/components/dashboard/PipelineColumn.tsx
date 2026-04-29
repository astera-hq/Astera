import Link from 'next/link';
import { formatUSDC, formatDate } from '@/lib/stellar';
import type { DashboardRowLike } from '@/lib/dashboardPipeline';

type Props = {
  title: string;
  rows: DashboardRowLike[];
};

export default function PipelineColumn({ title, rows }: Props) {
  const totalAmount = rows.reduce((acc, row) => acc + row.metadata.amount, 0n);

  return (
    <section className="w-[280px] sm:w-[300px] shrink-0 rounded-2xl border border-brand-border bg-brand-card p-4">
      <div className="mb-4">
        <div className="flex items-center justify-between gap-2">
          <h3 className="text-sm font-semibold text-white">{title}</h3>
          <span className="rounded-full bg-brand-dark px-2 py-0.5 text-xs text-brand-muted">
            {rows.length}
          </span>
        </div>
        <p className="mt-1 text-xs text-brand-muted">{formatUSDC(totalAmount)}</p>
      </div>

      <div className="space-y-3">
        {rows.map((row) => (
          <Link
            key={row.invoice.id}
            href={`/invoice/${row.invoice.id}`}
            className="block rounded-xl border border-brand-border bg-brand-dark p-3 transition-colors hover:border-brand-gold/50"
          >
            <p className="text-xs text-brand-muted">#{row.invoice.id}</p>
            <p className="mt-1 line-clamp-1 text-sm font-medium text-white">{row.metadata.name}</p>
            <p className="mt-1 line-clamp-1 text-xs text-brand-muted">{row.metadata.debtor}</p>
            <div className="mt-2 flex items-center justify-between gap-2 text-xs">
              <span className="font-medium text-brand-gold">{formatUSDC(row.metadata.amount)}</span>
              <span className="text-brand-muted">{formatDate(row.metadata.dueDate)}</span>
            </div>
          </Link>
        ))}
      </div>
    </section>
  );
}

