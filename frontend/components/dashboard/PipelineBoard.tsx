import { useMemo } from 'react';
import { useTranslations } from 'next-intl';
import Link from 'next/link';
import PipelineColumn from '@/components/dashboard/PipelineColumn';
import {
  PIPELINE_COLUMN_DEFINITIONS,
  isAttentionInvoice,
  type DashboardRowLike,
} from '@/lib/dashboardPipeline';
import { formatUSDC } from '@/lib/stellar';

type Props = {
  rows: DashboardRowLike[];
};

export default function PipelineBoard({ rows }: Props) {
  const t = useTranslations('Dashboard');

  const nowUnix = Math.floor(Date.now() / 1000);

  const columnData = useMemo(
    () =>
      PIPELINE_COLUMN_DEFINITIONS.map((column) => ({
        ...column,
        rows: rows.filter((row) => column.statuses.includes(row.invoice.status)),
      })),
    [rows],
  );

  const attentionRows = useMemo(
    () => rows.filter((row) => isAttentionInvoice(row, nowUnix)),
    [rows, nowUnix],
  );

  const attentionTotal = attentionRows.reduce((acc, row) => acc + row.metadata.amount, 0n);

  return (
    <div className="space-y-4">
      <div className="overflow-x-auto pb-2">
        <div className="flex min-w-max gap-4">
          {columnData.map((column) => (
            <PipelineColumn
              key={column.key}
              title={t(column.labelKey)}
              rows={column.rows}
            />
          ))}
        </div>
      </div>

      <section className="rounded-2xl border border-brand-border bg-brand-card p-4">
        <div className="mb-3 flex items-center justify-between gap-2">
          <h3 className="text-sm font-semibold text-white">{t('pipeline.needsAttention')}</h3>
          <span className="rounded-full bg-brand-dark px-2 py-0.5 text-xs text-brand-muted">
            {attentionRows.length}
          </span>
        </div>
        <p className="mb-3 text-xs text-brand-muted">{formatUSDC(attentionTotal)}</p>

        {attentionRows.length === 0 ? (
          <p className="text-sm text-brand-muted">{t('pipeline.noAttention')}</p>
        ) : (
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            {attentionRows.map((row) => (
              <Link
                key={row.invoice.id}
                href={`/invoice/${row.invoice.id}`}
                className="rounded-xl border border-brand-border bg-brand-dark p-3 transition-colors hover:border-brand-gold/50"
              >
                <p className="text-xs text-brand-muted">#{row.invoice.id}</p>
                <p className="mt-1 line-clamp-1 text-sm font-medium text-white">{row.metadata.name}</p>
                <p className="mt-1 text-xs text-red-400">{row.invoice.status}</p>
              </Link>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}

