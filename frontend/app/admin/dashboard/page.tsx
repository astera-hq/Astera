'use client';

/**
 * Admin Dashboard (#242)
 *
 * Four metric rows:
 * 1. Capital Overview — TVL, deployed, utilization, available liquidity
 * 2. Invoice Health  — active, overdue (red alert), defaulted 30d, default rate
 * 3. Investor Activity — active investors, new 7d, pending withdrawals
 * 4. Quick Actions   — pause protocol, links to sub-pages with pending counts
 *
 * Auto-refreshes every 60 seconds.
 */

import { useEffect, useState, useMemo, useCallback } from 'react';
import Link from 'next/link';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { StatCardSkeleton, Skeleton } from '@/components/Skeleton';
import ConfirmActionModal from '@/components/ConfirmActionModal';
import {
  getPoolConfig,
  getInvoiceCount,
  getMultipleInvoices,
  isProtocolPaused,
  buildPauseProtocolTx,
  buildUnpauseProtocolTx,
  submitTx,
} from '@/lib/contracts';
import { formatUSDC } from '@/lib/stellar';
import type { Invoice } from '@/lib/types';

const REFRESH_INTERVAL_MS = 60_000;
const THIRTY_DAYS_SECS = 30 * 24 * 60 * 60;
const SEVEN_DAYS_SECS = 7 * 24 * 60 * 60;
// #695: batch invoice lookups via get_multiple_invoices; ~20 IDs per call
// keeps each simulation under Soroban resource limits while parallelising
// across pools with hundreds of invoices.
const INVOICE_BATCH_SIZE = 20;

function chunk<T>(items: T[], size: number): T[][] {
  const out: T[][] = [];
  for (let i = 0; i < items.length; i += size) {
    out.push(items.slice(i, i + size));
  }
  return out;
}

export default function AdminDashboardPage() {
  const { poolConfig, setPoolConfig, wallet } = useStore();
  const [invoices, setInvoices] = useState<Invoice[]>([]);
  const [loading, setLoading] = useState(true);
  const [lastRefreshed, setLastRefreshed] = useState<Date | null>(null);
  const [protocolPaused, setProtocolPaused] = useState(false);
  const [pauseModalOpen, setPauseModalOpen] = useState(false);
  const [pauseSubmitting, setPauseSubmitting] = useState(false);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const [config, count, paused] = await Promise.all([
        getPoolConfig(),
        getInvoiceCount(),
        isProtocolPaused(),
      ]);
      setPoolConfig(config);
      setProtocolPaused(paused);

      const ids = Array.from({ length: count }, (_, i) => i + 1);
      const batches = await Promise.all(
        chunk(ids, INVOICE_BATCH_SIZE).map((group) => getMultipleInvoices(group)),
      );
      setInvoices(batches.flat());
      setLastRefreshed(new Date());
    } catch (e) {
      toast.error('Failed to load protocol statistics.');
      console.error(e);
    } finally {
      setLoading(false);
    }
  }, [setPoolConfig]);

  async function handlePauseToggleConfirm() {
    if (!wallet.address) return;

    setPauseModalOpen(false);
    setPauseSubmitting(true);

    try {
      const xdr = protocolPaused
        ? await buildUnpauseProtocolTx(wallet.address)
        : await buildPauseProtocolTx(wallet.address);

      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
        networkPassphrase: 'Test SDF Network ; September 2015',
        address: wallet.address,
      });

      if (signError) throw new Error(signError.message || 'Signing rejected.');

      await submitTx(signedTxXdr);
      toast.success(protocolPaused ? 'Protocol unpaused.' : 'Protocol paused.');
      await loadData();
    } catch (e) {
      const message = e instanceof Error ? e.message : 'Failed to update protocol pause state.';
      toast.error(message);
      console.error(e);
    } finally {
      setPauseSubmitting(false);
    }
  }

  useEffect(() => {
    void loadData();
    const id = setInterval(() => void loadData(), REFRESH_INTERVAL_MS);
    return () => clearInterval(id);
  }, [loadData]);

  const stats = useMemo(() => {
    if (!poolConfig) return null;

    const nowSecs = Math.floor(Date.now() / 1000);
    const funded = invoices.filter((i) => i.status !== 'Pending');
    const active = invoices.filter((i) => i.status === 'Funded');
    const overdue = invoices.filter((i) => i.status === 'Funded' && i.dueDate < nowSecs);
    const defaulted = invoices.filter((i) => i.status === 'Defaulted');
    const defaulted30d = defaulted.filter((i) => nowSecs - (i.fundedAt ?? 0) <= THIRTY_DAYS_SECS);
    const defaultRate = funded.length > 0 ? (defaulted.length / funded.length) * 100 : 0;

    // Deployed = sum of funded active invoices
    const deployed = active.reduce((s, i) => s + BigInt(i.amount ?? 0), 0n);

    // Approximation: TVL is the pool's total deposited (not returned by poolConfig yet — 0n fallback)
    const tvl = 0n;
    const utilization = tvl > 0n ? Number((deployed * 100n) / tvl) : 0;
    const available = tvl > deployed ? tvl - deployed : 0n;

    // Investors: addresses with non-zero positions (approximated from invoice owners)
    const allOwners = new Set(invoices.map((i) => i.owner));
    const recentOwners = new Set(
      invoices.filter((i) => nowSecs - (i.createdAt ?? 0) <= SEVEN_DAYS_SECS).map((i) => i.owner),
    );

    return {
      tvl,
      deployed,
      utilization,
      available,
      activeCount: active.length,
      overdueCount: overdue.length,
      defaulted30dCount: defaulted30d.length,
      defaultRate: defaultRate.toFixed(2) + '%',
      activeInvestors: allOwners.size,
      newInvestors7d: recentOwners.size,
      pendingWithdrawals: 0, // populated when withdrawal queue data is available
    };
  }, [poolConfig, invoices]);

  if (loading) {
    return (
      <div className="space-y-6">
        <Skeleton className="h-10 w-48 rounded-lg" />
        {[1, 2, 3, 4].map((row) => (
          <div key={row} className="grid grid-cols-1 md:grid-cols-4 gap-6">
            {[1, 2, 3, 4].map((col) => (
              <StatCardSkeleton key={col} />
            ))}
          </div>
        ))}
      </div>
    );
  }

  if (!stats) return null;

  return (
    <div className="space-y-8">
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-2">
        <div>
          <h1 className="text-3xl font-bold mb-2">Protocol Dashboard</h1>
          <p className="text-brand-muted text-sm">
            Real-time overview of the Astera liquidity pool.
          </p>
        </div>
        {lastRefreshed && (
          <p className="text-xs text-brand-muted">
            Last updated: {lastRefreshed.toLocaleTimeString()} · auto-refreshes every 60s
          </p>
        )}
      </div>

      {/* Row 1: Capital Overview */}
      <Section title="Capital Overview">
        <StatCard
          label="Total Value Locked"
          value={formatUSDC(stats.tvl)}
          description="Sum of all deposited capital"
          trend="primary"
        />
        <StatCard
          label="Deployed Capital"
          value={formatUSDC(stats.deployed)}
          description="Currently in active funded invoices"
        />
        <StatCard
          label="Utilization Rate"
          value={`${stats.utilization}%`}
          description="Deployed ÷ deposited (target 70–90%)"
          trend={stats.utilization >= 70 && stats.utilization <= 90 ? 'success' : 'danger'}
        />
        <StatCard
          label="Available Liquidity"
          value={formatUSDC(stats.available)}
          description="Immediately withdrawable capital"
        />
      </Section>

      {/* Row 2: Invoice Health */}
      <Section title="Invoice Health">
        <StatCard
          label="Active Invoices"
          value={stats.activeCount.toString()}
          description="Currently funded and not yet repaid"
        />
        <StatCard
          label="Overdue Invoices"
          value={stats.overdueCount.toString()}
          description="Past due date (within grace period)"
          trend={stats.overdueCount > 0 ? 'danger' : 'success'}
          badge={stats.overdueCount > 0 ? '⚠' : undefined}
        />
        <StatCard
          label="Defaulted (30d)"
          value={stats.defaulted30dCount.toString()}
          description="Defaults in the last 30 days"
          trend={stats.defaulted30dCount > 0 ? 'danger' : undefined}
        />
        <StatCard
          label="Default Rate"
          value={stats.defaultRate}
          description="Rolling rate across all funded invoices"
          trend={parseFloat(stats.defaultRate) > 5 ? 'danger' : 'success'}
        />
      </Section>

      {/* Row 3: Investor Activity */}
      <Section title="Investor Activity">
        <StatCard
          label="Active Investors"
          value={stats.activeInvestors.toString()}
          description="Addresses with non-zero positions"
        />
        <StatCard
          label="New Investors (7d)"
          value={stats.newInvestors7d.toString()}
          description="First-time depositors this week"
          trend="primary"
        />
        <StatCard
          label="Pending Withdrawals"
          value={stats.pendingWithdrawals.toString()}
          description="Queued withdrawal requests"
          trend={stats.pendingWithdrawals > 0 ? 'danger' : undefined}
        />
      </Section>

      {/* Row 4: Quick Actions */}
      <section>
        <h2 className="text-xs font-bold mb-4 text-brand-muted uppercase tracking-widest">
          Quick Actions
        </h2>
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
          <ActionCard
            href="/admin/kyc"
            label="KYC Approvals"
            count={0}
            description="Review pending KYC applications"
          />
          <ActionCard
            href="/admin/invoices"
            label="Disputed Invoices"
            count={0}
            description="Resolve active disputes"
          />
          <ActionCard
            href="/admin/invoices"
            label="Overdue Invoices"
            count={stats.overdueCount}
            description="Manage past-due invoices"
            alert={stats.overdueCount > 0}
          />
          <ActionCard
            href="/admin/monitoring"
            label="Monitoring"
            count={0}
            description="View contract events and alerts"
          />
        </div>

        <div
          data-testid="protocol-pause-control"
          className={`rounded-2xl border p-5 ${
            protocolPaused ? 'border-red-500/40 bg-red-500/5' : 'border-brand-border bg-brand-card'
          }`}
        >
          <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
            <div>
              <h3 className="font-semibold text-white">Protocol Pause</h3>
              <p className="text-sm text-brand-muted mt-1">
                {protocolPaused
                  ? 'The protocol is paused. State-changing operations are blocked.'
                  : 'The protocol is active. Pause to halt all state-changing operations.'}
              </p>
              <p
                data-testid="protocol-pause-status"
                className={`text-xs font-medium mt-2 ${
                  protocolPaused ? 'text-red-400' : 'text-green-400'
                }`}
              >
                Status: {protocolPaused ? 'Paused' : 'Active'}
              </p>
            </div>
            <button
              type="button"
              data-testid="protocol-pause-toggle"
              onClick={() => setPauseModalOpen(true)}
              disabled={pauseSubmitting || !wallet.address}
              className={`px-4 py-2.5 rounded-xl text-sm font-semibold transition-colors disabled:opacity-50 ${
                protocolPaused
                  ? 'bg-green-600 hover:bg-green-500 text-white'
                  : 'bg-red-600 hover:bg-red-500 text-white'
              }`}
            >
              {pauseSubmitting
                ? 'Updating...'
                : protocolPaused
                  ? 'Unpause Protocol'
                  : 'Pause Protocol'}
            </button>
          </div>
        </div>
      </section>

      <ConfirmActionModal
        title={protocolPaused ? 'Unpause protocol?' : 'Pause protocol?'}
        description={
          protocolPaused
            ? 'This will resume all state-changing protocol operations.'
            : 'This will halt all state-changing protocol operations until unpaused.'
        }
        confirmLabel={protocolPaused ? 'Unpause' : 'Pause'}
        cancelLabel="Cancel"
        variant={protocolPaused ? 'default' : 'destructive'}
        isOpen={pauseModalOpen}
        onConfirm={() => void handlePauseToggleConfirm()}
        onCancel={() => setPauseModalOpen(false)}
      />
    </div>
  );
}

// ── Sub-components ────────────────────────────────────────────────────────────

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section>
      <h2 className="text-xs font-bold text-brand-muted uppercase tracking-widest mb-4">{title}</h2>
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">{children}</div>
    </section>
  );
}

function StatCard({
  label,
  value,
  description,
  trend,
  badge,
}: {
  label: string;
  value: string;
  description: string;
  trend?: 'primary' | 'danger' | 'success';
  badge?: string;
}) {
  return (
    <div className="p-6 bg-brand-card border border-brand-border rounded-2xl shadow-sm hover:border-brand-gold/30 transition-colors relative">
      {badge && (
        <span className="absolute top-3 right-3 bg-red-500 text-white text-xs font-bold px-1.5 py-0.5 rounded-full">
          {badge}
        </span>
      )}
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

function ActionCard({
  href,
  label,
  count,
  description,
  alert,
}: {
  href: string;
  label: string;
  count: number;
  description: string;
  alert?: boolean;
}) {
  return (
    <Link
      href={href}
      className={`block p-5 rounded-2xl border transition-colors hover:border-brand-gold/40 ${
        alert ? 'border-red-500/40 bg-red-500/5' : 'border-brand-border bg-brand-card'
      }`}
    >
      <div className="flex items-center justify-between mb-2">
        <span className="font-semibold text-sm text-white">{label}</span>
        {count > 0 && (
          <span
            className={`text-xs font-bold px-2 py-0.5 rounded-full ${
              alert ? 'bg-red-500 text-white' : 'bg-brand-gold/20 text-brand-gold'
            }`}
          >
            {count}
          </span>
        )}
      </div>
      <p className="text-xs text-brand-muted">{description}</p>
    </Link>
  );
}
