'use client';

import { useEffect, useState, useMemo, useCallback } from 'react';
import Link from 'next/link';
import toast from 'react-hot-toast';
import { Skeleton } from '@/components/Skeleton';
import { getMultipleInvoices, getInvoiceCount } from '@/lib/contracts';
import { formatUSDC, formatDate } from '@/lib/stellar';
import type { Invoice } from '@/lib/types';

const AGING_BUCKETS = [
  { key: 'critical', label: '90+ Days Overdue', minDays: 90, color: 'text-red-400 border-red-500/30 bg-red-500/10' },
  { key: 'severe', label: '61–90 Days Overdue', minDays: 61, color: 'text-orange-400 border-orange-500/30 bg-orange-500/10' },
  { key: 'moderate', label: '31–60 Days Overdue', minDays: 31, color: 'text-yellow-400 border-yellow-500/30 bg-yellow-500/10' },
  { key: 'mild', label: '1–30 Days Overdue', minDays: 1, color: 'text-yellow-300 border-yellow-400/30 bg-yellow-400/5' },
] as const;

const AT_RISK_DAYS = 7;

type AgingBucket = typeof AGING_BUCKETS[number]['key'];

interface AgingGroup {
  bucket: AgingBucket;
  label: string;
  color: string;
  invoices: Invoice[];
  totalAmount: bigint;
}

export default function AdminAgingPage() {
  const [invoices, setInvoices] = useState<Invoice[]>([]);
  const [loading, setLoading] = useState(true);
  const [expandedBucket, setExpandedBucket] = useState<AgingBucket | null>(null);

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
      toast.error('Failed to load invoices for aging report.');
      console.error(e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const nowSecs = Math.floor(Date.now() / 1000);
  const oneDaySecs = 86400;

  const { overdueBuckets, atRiskInvoices } = useMemo(() => {
    const buckets: AgingGroup[] = AGING_BUCKETS.map((b) => ({
      bucket: b.key,
      label: b.label,
      color: b.color,
      invoices: [] as Invoice[],
      totalAmount: 0n,
    }));

    const atRisk: Invoice[] = [];
    const bucketDefs = AGING_BUCKETS;

    for (const inv of invoices) {
      if (inv.status !== 'Funded') continue;

      const overdueDays = Math.floor((nowSecs - inv.dueDate) / oneDaySecs);

      if (overdueDays > 0) {
        for (let i = 0; i < bucketDefs.length; i++) {
          const def = bucketDefs[i]!;
          const nextDef = i > 0 ? bucketDefs[i - 1]! : null;
          const upperBound = nextDef ? nextDef.minDays - 1 : Infinity;
          if (overdueDays >= def.minDays && overdueDays <= upperBound) {
            buckets[i]!.invoices.push(inv);
            buckets[i]!.totalAmount += BigInt(inv.amount ?? 0);
            break;
          }
        }
      } else if (overdueDays <= 0 && Math.abs(overdueDays) <= AT_RISK_DAYS) {
        atRisk.push(inv);
      }
    }

    return { overdueBuckets: buckets, atRiskInvoices: atRisk };
  }, [invoices, nowSecs]);

  const totalOverdueAmount = overdueBuckets.reduce((sum, b) => sum + b.totalAmount, 0n);
  const totalOverdueCount = overdueBuckets.reduce((sum, b) => sum + b.invoices.length, 0);

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
      <div>
        <h1 className="text-3xl font-bold mb-2">Invoice Aging Report</h1>
        <p className="text-brand-muted text-sm">
          Overdue and at-risk invoices grouped by aging period.
        </p>
      </div>

      {/* Summary Cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
        <SummaryCard
          label="Total Overdue"
          value={totalOverdueCount.toString()}
          description={`${formatUSDC(totalOverdueAmount)} in overdue invoices`}
          trend={totalOverdueCount > 0 ? 'danger' : 'success'}
        />
        <SummaryCard
          label="Critical (90+ days)"
          value={overdueBuckets[0]!.invoices.length.toString()}
          description={formatUSDC(overdueBuckets[0]!.totalAmount)}
          trend={overdueBuckets[0]!.invoices.length > 0 ? 'danger' : undefined}
        />
        <SummaryCard
          label="At Risk (due within 7d)"
          value={atRiskInvoices.length.toString()}
          description={`${formatUSDC(atRiskInvoices.reduce((s, i) => s + BigInt(i.amount ?? 0), 0n))}`}
          trend={atRiskInvoices.length > 0 ? 'primary' : undefined}
        />
        <SummaryCard
          label="Total Funded Invoices"
          value={invoices.filter((i) => i.status === 'Funded').length.toString()}
          description="Currently active funded invoices"
        />
      </div>

      {/* At Risk Section */}
      {atRiskInvoices.length > 0 && (
        <section>
          <h2 className="text-xs font-bold text-brand-muted uppercase tracking-widest mb-4">
            At-Risk Invoices (due within {AT_RISK_DAYS} days)
          </h2>
          <div className="bg-brand-card border border-yellow-500/30 rounded-2xl overflow-hidden">
            <InvoiceTable invoices={atRiskInvoices} nowSecs={nowSecs} />
          </div>
        </section>
      )}

      {/* Aging Buckets */}
      <section>
        <h2 className="text-xs font-bold text-brand-muted uppercase tracking-widest mb-4">
          Overdue Invoices by Aging Period
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
                    {bucket.invoices.length} invoice{bucket.invoices.length !== 1 ? 's' : ''}
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
          {totalOverdueCount === 0 && (
            <div className="p-12 bg-brand-card border border-brand-border rounded-2xl text-center">
              <div className="text-3xl mb-3">✓</div>
              <p className="text-brand-muted font-medium">No overdue invoices.</p>
              <p className="text-xs text-brand-muted mt-1">
                All funded invoices are current on their payments.
              </p>
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
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-left text-sm">
        <thead>
          <tr className="border-b border-brand-border bg-brand-dark/50">
            <th className="px-6 py-4 font-semibold text-brand-muted uppercase tracking-wider">ID</th>
            <th className="px-6 py-4 font-semibold text-brand-muted uppercase tracking-wider">
              Debtor
            </th>
            <th className="px-6 py-4 font-semibold text-brand-muted uppercase tracking-wider">
              Amount
            </th>
            <th className="px-6 py-4 font-semibold text-brand-muted uppercase tracking-wider">
              Due Date
            </th>
            <th className="px-6 py-4 font-semibold text-brand-muted uppercase tracking-wider">
              Days Overdue
            </th>
            <th className="px-6 py-4 font-semibold text-brand-muted uppercase tracking-wider">
              Action
            </th>
          </tr>
        </thead>
        <tbody className="divide-y divide-brand-border">
          {invoices.map((inv) => {
            const overdueDays = Math.floor(
              (nowSecs - inv.dueDate) / 86400,
            );
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
                    {overdueDays}d
                  </span>
                </td>
                <td className="px-6 py-4">
                  <Link
                    href={`/invoice/${inv.id}`}
                    className="text-brand-gold hover:underline text-xs font-medium"
                  >
                    View →
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
