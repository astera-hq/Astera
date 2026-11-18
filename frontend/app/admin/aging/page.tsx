'use client';

import { useEffect, useState, useMemo, useCallback } from 'react';
import Link from 'next/link';
import { useTranslations } from 'next-intl';
import toast from 'react-hot-toast';
import { Skeleton } from '@/components/Skeleton';
import { getMultipleInvoices, getInvoiceCount } from '@/lib/contracts';
import { formatUSDC, formatDate } from '@/lib/stellar';
import type { Invoice } from '@/lib/types';

function getBucketLabels(t: (key: string) => string) {
  return [
    {
      key: 'critical',
      label: t('bucketCritical'),
      minDays: 90,
      color: 'text-red-400 border-red-500/30 bg-red-500/10',
    },
    {
      key: 'severe',
      label: t('bucketSevere'),
      minDays: 61,
      color: 'text-orange-400 border-orange-500/30 bg-orange-500/10',
    },
    {
      key: 'moderate',
      label: t('bucketModerate'),
      minDays: 31,
      color: 'text-yellow-400 border-yellow-500/30 bg-yellow-500/10',
    },
    {
      key: 'mild',
      label: t('bucketMild'),
      minDays: 1,
      color: 'text-yellow-300 border-yellow-400/30 bg-yellow-400/5',
    },
    {
      key: 'current',
      label: t('bucketCurrent'),
      minDays: 0,
      color: 'text-green-400 border-green-500/30 bg-green-500/10',
    },
  ] as const;
}

const AT_RISK_DAYS = 7;

type AgingBucket = 'critical' | 'severe' | 'moderate' | 'mild' | 'current';

interface AgingGroup {
  bucket: AgingBucket;
  label: string;
  color: string;
  invoices: Invoice[];
  totalAmount: bigint;
}

function exportAgingCSV(
  t: (key: string, values?: Record<string, unknown>) => string,
  overdueBuckets: AgingGroup[],
  atRiskInvoices: Invoice[],
) {
  const header = [
    t('tableId'),
    t('tableDebtor'),
    t('csvOwner'),
    t('tableAmount'),
    t('tableDueDate'),
    t('tableDaysOverdue'),
    t('csvAgingBucket'),
    t('csvStatus'),
  ];

  const rows: (string | number)[][] = [];
  const nowSecs = Math.floor(Date.now() / 1000);
  const oneDaySecs = 86400;

  for (const bucket of overdueBuckets) {
    for (const inv of bucket.invoices) {
      const daysOverdue = Math.floor((nowSecs - inv.dueDate) / oneDaySecs);
      rows.push([
        inv.id,
        inv.debtor,
        inv.owner,
        formatUSDC(inv.amount),
        new Date(inv.dueDate * 1000).toISOString().slice(0, 10),
        daysOverdue,
        bucket.label,
        inv.status,
      ]);
    }
  }

  for (const inv of atRiskInvoices) {
    const daysOverdue = Math.floor((nowSecs - inv.dueDate) / oneDaySecs);
    rows.push([
      inv.id,
      inv.debtor,
      inv.owner,
      formatUSDC(inv.amount),
      new Date(inv.dueDate * 1000).toISOString().slice(0, 10),
      daysOverdue,
      t('statusAtRisk'),
      inv.status,
    ]);
  }

  const csv = [header, ...rows]
    .map((r) => r.map((cell) => `"${String(cell).replace(/"/g, '""')}"`).join(','))
    .join('\n');

  const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `astera-aging-report-${new Date().toISOString().slice(0, 10)}.csv`;
  a.click();
  URL.revokeObjectURL(url);
}

export default function AdminAgingPage() {
  const t = useTranslations('Admin.aging');
  const [invoices, setInvoices] = useState<Invoice[]>([]);
  const [loading, setLoading] = useState(true);
  const [expandedBucket, setExpandedBucket] = useState<AgingBucket | null>(null);
  const AGING_BUCKETS = getBucketLabels(t);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const count = await getInvoiceCount();
      if (count === 0) {
        setInvoices([]);
        return;
      }

      const ids = Array.from({ length: count }, (_, i) => i + 1);
      const all = await getMultipleInvoices(ids);
      setInvoices(all);
    } catch (e) {
      toast.error(t('noFunded'));
      console.error(e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
    const interval = setInterval(loadData, 60_000);
    return () => clearInterval(interval);
  }, [loadData]);

  const nowSecs = Math.floor(Date.now() / 1000);
  const oneDaySecs = 86400;

  const { overdueBuckets, atRiskInvoices } = useMemo(() => {
    const overdueDefs = AGING_BUCKETS.filter((b) => b.minDays >= 1);
    const currentDef = AGING_BUCKETS.find((b) => b.minDays === 0);

    const buckets: AgingGroup[] = AGING_BUCKETS.map((b) => ({
      bucket: b.key,
      label: b.label,
      color: b.color,
      invoices: [] as Invoice[],
      totalAmount: 0n,
    }));

    const atRisk: Invoice[] = [];
    const currentBucket = currentDef ? buckets[AGING_BUCKETS.indexOf(currentDef)] : null;

    for (const inv of invoices) {
      if (inv.status !== 'Funded') continue;

      const overdueDays = Math.floor((nowSecs - inv.dueDate) / oneDaySecs);

      if (overdueDays > 0) {
        for (let i = 0; i < overdueDefs.length; i++) {
          const def = overdueDefs[i]!;
          const nextDef = i > 0 ? overdueDefs[i - 1]! : null;
          const upperBound = nextDef ? nextDef.minDays - 1 : Infinity;
          if (overdueDays >= def.minDays && overdueDays <= upperBound) {
            const bucketIdx = AGING_BUCKETS.indexOf(def);
            buckets[bucketIdx]!.invoices.push(inv);
            buckets[bucketIdx]!.totalAmount += BigInt(inv.amount ?? 0);
            break;
          }
        }
      } else if (overdueDays <= 0 && Math.abs(overdueDays) <= AT_RISK_DAYS) {
        atRisk.push(inv);
      }

      if (overdueDays <= 0 && currentBucket) {
        const bucketIdx = AGING_BUCKETS.indexOf(currentDef!);
        buckets[bucketIdx]!.invoices.push(inv);
        buckets[bucketIdx]!.totalAmount += BigInt(inv.amount ?? 0);
      }
    }

    return { overdueBuckets: buckets, atRiskInvoices: atRisk };
  }, [invoices, nowSecs]);

  const totalOverdueAmount = overdueBuckets.reduce((sum, b) => sum + b.totalAmount, 0n);
  const totalOverdueCount = overdueBuckets.reduce((sum, b) => sum + b.invoices.length, 0);

  const exportCSV = useCallback(() => {
    const headers = [
      t('tableId'),
      t('tableDebtor'),
      t('csvOwner'),
      t('tableAmount'),
      t('tableDueDate'),
      t('csvStatus'),
      t('csvAgingBucket'),
      t('tableDaysOverdue'),
    ];
    const rows: string[][] = [];

    for (const bucket of overdueBuckets) {
      for (const inv of bucket.invoices) {
        const overdueDays = Math.floor((nowSecs - inv.dueDate) / 86400);
        rows.push([
          inv.id.toString(),
          inv.debtor,
          inv.owner,
          formatUSDC(inv.amount),
          formatDate(inv.dueDate),
          inv.status,
          bucket.label,
          overdueDays > 0 ? t('daysSuffix', { days: overdueDays }) : t('daysSuffix', { days: 0 }),
        ]);
      }
    }

    for (const inv of atRiskInvoices) {
      const overdueDays = Math.floor((nowSecs - inv.dueDate) / 86400);
      rows.push([
        inv.id.toString(),
        inv.debtor,
        inv.owner,
        formatUSDC(inv.amount),
        formatDate(inv.dueDate),
        inv.status,
        t('atRiskCsv'),
        t('daysSuffix', { days: overdueDays > 0 ? overdueDays : 0 }),
      ]);
    }

    const csv = [headers, ...rows]
      .map((r) => r.map((cell) => `"${String(cell).replace(/"/g, '""')}"`).join(','))
      .join('\n');

    const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `astera-aging-report-${new Date().toISOString().slice(0, 10)}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  }, [t, overdueBuckets, atRiskInvoices, nowSecs]);

  if (loading) {
    return (
      <div className="space-y-6">
        <Skeleton className="h-10 w-56 rounded-lg" />
        <div className="grid grid-cols-1 md:grid-cols-4 gap-6">
          {[1, 2, 3, 4].map((i) => (
            <Skeleton key={i} className="h-28 rounded-2xl" />
          ))}
        </div>
        <Skeleton className="h-64 rounded-2xl" />
      </div>
    );
  }

  return (
    <div className="space-y-8">
      <div className="flex items-start justify-between">
        <div>
          <h1 className="text-3xl font-bold mb-2">{t('title')}</h1>
          <p className="text-brand-muted text-sm">{t('description')}</p>
        </div>
        <button
          onClick={exportCSV}
          className="px-4 py-2 bg-brand-gold text-black font-semibold rounded-xl hover:brightness-110 transition-all text-sm flex items-center gap-2"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M12 10v6m0 0l-3-3m3 3l3-3m2 8H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
            />
          </svg>
          {t('downloadCsv')}
        </button>
      </div>

      {/* Summary Cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
        <SummaryCard
          label={t('totalOverdue')}
          value={totalOverdueCount.toString()}
          description={t('totalOverdueDesc', { amount: formatUSDC(totalOverdueAmount) })}
          trend={totalOverdueCount > 0 ? 'danger' : 'success'}
        />
        <SummaryCard
          label={t('critical')}
          value={overdueBuckets[0]!.invoices.length.toString()}
          description={formatUSDC(overdueBuckets[0]!.totalAmount)}
          trend={overdueBuckets[0]!.invoices.length > 0 ? 'danger' : undefined}
        />
        <SummaryCard
          label={t('current')}
          value={overdueBuckets[4]!.invoices.length.toString()}
          description={t('currentDesc', { amount: formatUSDC(overdueBuckets[4]!.totalAmount) })}
          trend="success"
        />
        <SummaryCard
          label={t('atRisk')}
          value={atRiskInvoices.length.toString()}
          description={t('atRiskDesc', {
            amount: formatUSDC(atRiskInvoices.reduce((s, i) => s + BigInt(i.amount ?? 0), 0n)),
          })}
          trend={atRiskInvoices.length > 0 ? 'primary' : undefined}
        />
        <SummaryCard
          label={t('totalFunded')}
          value={invoices.filter((i) => i.status === 'Funded').length.toString()}
          description={t('totalFundedDesc')}
        />
      </div>

      {/* At Risk Section */}
      {atRiskInvoices.length > 0 && (
        <section>
          <h2 className="text-xs font-bold text-brand-muted uppercase tracking-widest mb-4">
            {t('atRiskSection', { days: AT_RISK_DAYS })}
          </h2>
          <div className="bg-brand-card border border-yellow-500/30 rounded-2xl overflow-hidden">
            <InvoiceTable invoices={atRiskInvoices} nowSecs={nowSecs} />
          </div>
        </section>
      )}

      {/* Aging Buckets */}
      <section>
        <h2 className="text-xs font-bold text-brand-muted uppercase tracking-widest mb-4">
          {t('overdueSection')}
        </h2>
        <div className="space-y-4">
          {overdueBuckets.map((bucket) => (
            <div
              key={bucket.bucket}
              className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden"
            >
              <button
                onClick={() =>
                  setExpandedBucket(expandedBucket === bucket.bucket ? null : bucket.bucket)
                }
                className="w-full flex items-center justify-between p-5 hover:bg-brand-dark/20 transition-colors"
              >
                <div className="flex items-center gap-3">
                  <div className={`px-3 py-1 rounded-lg text-xs font-semibold ${bucket.color}`}>
                    {bucket.label}
                  </div>
                  <span className="text-sm text-brand-muted">
                    {t('invoiceCount', { count: bucket.invoices.length })}
                  </span>
                </div>
                <div className="flex items-center gap-4">
                  <span className="text-sm font-semibold text-white">
                    {formatUSDC(bucket.totalAmount)}
                  </span>
                  <svg
                    className={`w-4 h-4 text-brand-muted transition-transform ${
                      expandedBucket === bucket.bucket ? 'rotate-180' : ''
                    }`}
                    fill="currentColor"
                    viewBox="0 0 20 20"
                  >
                    <path
                      fillRule="evenodd"
                      d="M5.293 7.293a1 1 0 011.414 0L10 10.586l3.293-3.293a1 1 0 111.414 1.414l-4 4a1 1 0 01-1.414 0l-4-4a1 1 0 010-1.414z"
                      clipRule="evenodd"
                    />
                  </svg>
                </div>
              </button>
              {expandedBucket === bucket.bucket && bucket.invoices.length > 0 && (
                <div className="border-t border-brand-border">
                  <InvoiceTable invoices={bucket.invoices} nowSecs={nowSecs} />
                </div>
              )}
            </div>
          ))}
          {totalOverdueCount === 0 && overdueBuckets[4]!.invoices.length === 0 && (
            <div className="p-12 bg-brand-card border border-brand-border rounded-2xl text-center">
              <div className="text-3xl mb-3">✓</div>
              <p className="text-brand-muted font-medium">{t('noFunded')}</p>
              <p className="text-xs text-brand-muted mt-1">{t('noFundedDesc')}</p>
            </div>
          )}
          {totalOverdueCount === 0 && overdueBuckets[4]!.invoices.length > 0 && (
            <div className="p-12 bg-brand-card border border-brand-border rounded-2xl text-center">
              <div className="text-3xl mb-3">✓</div>
              <p className="text-brand-muted font-medium">{t('noOverdue')}</p>
              <p className="text-xs text-brand-muted mt-1">{t('noOverdueDesc')}</p>
            </div>
          )}
        </div>
      </section>
    </div>
  );
}

function SummaryCard({
  label,
  value,
  description,
  trend,
}: {
  label: string;
  value: string;
  description: string;
  trend?: 'primary' | 'danger' | 'success';
}) {
  return (
    <div className="p-6 bg-brand-card border border-brand-border rounded-2xl shadow-sm">
      <p className="text-xs font-bold text-brand-muted uppercase tracking-wider mb-2">{label}</p>
      <p
        className={`text-3xl font-bold tracking-tight mb-1 ${
          trend === 'primary'
            ? 'gradient-text'
            : trend === 'danger'
              ? 'text-red-500'
              : trend === 'success'
                ? 'text-green-400'
                : 'text-white'
        }`}
      >
        {value}
      </p>
      <p className="text-xs text-brand-muted">{description}</p>
    </div>
  );
}

function InvoiceTable({ invoices, nowSecs }: { invoices: Invoice[]; nowSecs: number }) {
  const t = useTranslations('Admin.aging');
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-left text-sm">
        <thead>
          <tr className="border-b border-brand-border bg-brand-dark/50">
            <th className="px-6 py-4 font-semibold text-brand-muted uppercase tracking-wider">
              {t('tableId')}
            </th>
            <th className="px-6 py-4 font-semibold text-brand-muted uppercase tracking-wider">
              {t('tableDebtor')}
            </th>
            <th className="px-6 py-4 font-semibold text-brand-muted uppercase tracking-wider">
              {t('tableAmount')}
            </th>
            <th className="px-6 py-4 font-semibold text-brand-muted uppercase tracking-wider">
              {t('tableDueDate')}
            </th>
            <th className="px-6 py-4 font-semibold text-brand-muted uppercase tracking-wider">
              {t('tableDaysOverdue')}
            </th>
            <th className="px-6 py-4 font-semibold text-brand-muted uppercase tracking-wider">
              {t('tableAction')}
            </th>
          </tr>
        </thead>
        <tbody className="divide-y divide-brand-border">
          {invoices.map((inv) => {
            const overdueDays = Math.floor((nowSecs - inv.dueDate) / 86400);
            return (
              <tr key={inv.id} className="hover:bg-brand-dark/30 transition-colors">
                <td className="px-6 py-4 font-mono">#{inv.id}</td>
                <td className="px-6 py-4">
                  <div className="flex flex-col">
                    <span className="font-medium text-white">{inv.debtor}</span>
                    <span className="text-xs text-brand-muted">{inv.owner}</span>
                  </div>
                </td>
                <td className="px-6 py-4 font-bold text-white whitespace-nowrap">
                  {formatUSDC(inv.amount)}
                </td>
                <td className="px-6 py-4 whitespace-nowrap">{formatDate(inv.dueDate)}</td>
                <td className="px-6 py-4">
                  <span
                    className={`px-2.5 py-1 rounded-lg text-xs font-semibold ${
                      overdueDays >= 90
                        ? 'bg-red-500/20 text-red-400'
                        : overdueDays >= 61
                          ? 'bg-orange-500/20 text-orange-400'
                          : overdueDays >= 31
                            ? 'bg-yellow-500/20 text-yellow-400'
                            : 'bg-yellow-400/20 text-yellow-300'
                    }`}
                  >
                    {t('daysSuffix', { days: overdueDays })}
                  </span>
                </td>
                <td className="px-6 py-4">
                  <Link
                    href={`/invoice/${inv.id}`}
                    className="text-brand-gold hover:underline text-xs font-medium"
                  >
                    {t('view')}
                  </Link>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
