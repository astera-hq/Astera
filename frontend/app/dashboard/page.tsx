'use client';

import { useEffect, useState, useMemo, useCallback } from 'react';
import Link from 'next/link';
import { useStore } from '@/lib/store';
import InvoiceCard from '@/components/InvoiceCard';
import CreditScore from '@/components/CreditScore';
import OnboardingModal, { isFirstTimeUser } from '@/components/OnboardingModal';
import { getInvoice, getInvoiceCount, getFundedInvoice } from '@/lib/contracts';
import { formatUSDC } from '@/lib/stellar';
import type { Invoice } from '@/lib/types';

type StatusFilter = Invoice['status'] | 'All';
type SortOption = 'newest' | 'oldest' | 'highest' | 'due-soonest';

const STATUS_TABS: StatusFilter[] = ['All', 'Pending', 'Funded', 'Paid', 'Defaulted'];

const SORT_OPTIONS: { value: SortOption; label: string }[] = [
  { value: 'newest', label: 'Newest first' },
  { value: 'oldest', label: 'Oldest first' },
  { value: 'highest', label: 'Highest amount' },
  { value: 'due-soonest', label: 'Due soonest' },
];

export default function DashboardPage() {
  const { wallet } = useStore();
  const [invoices, setInvoices] = useState<Invoice[]>([]);
  const [committedMap, setCommittedMap] = useState<Record<number, bigint>>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showOnboarding, setShowOnboarding] = useState(false);

  const [search, setSearch] = useState('');
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('All');
  const [sort, setSort] = useState<SortOption>('newest');

  // Check if user is first-time visitor
  useEffect(() => {
    if (isFirstTimeUser()) {
      setShowOnboarding(true);
    }
  }, []);

  const loadInvoices = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const count = await getInvoiceCount();
      const all: Invoice[] = [];
      for (let i = 1; i <= count; i++) {
        const inv = await getInvoice(i);
        if (inv.owner === wallet.address) all.push(inv);
      }
      setInvoices(all);

      // Fetch co-funding progress for pending invoices
      const committed: Record<number, bigint> = {};
      await Promise.all(
        all
          .filter((inv) => inv.status === 'Pending')
          .map(async (inv) => {
            try {
              const record = await getFundedInvoice(inv.id);
              if (record) committed[inv.id] = record.committed;
            } catch {
              // Not registered for co-funding yet — leave uncommitted
            }
          }),
      );
      setCommittedMap(committed);
    } catch (e) {
      setError('Failed to load invoices. Make sure contracts are deployed.');
      console.error(e);
    } finally {
      setLoading(false);
    }
  }, [wallet.address]);

  useEffect(() => {
    if (!wallet.connected) {
      setLoading(false);
      return;
    }
    loadInvoices();
  }, [wallet.connected, wallet.address, loadInvoices]);

  const stats = {
    total: invoices.length,
    pending: invoices.filter((i) => i.status === 'Pending').length,
    funded: invoices.filter((i) => i.status === 'Funded').length,
    paid: invoices.filter((i) => i.status === 'Paid').length,
    defaulted: invoices.filter((i) => i.status === 'Defaulted').length,
    totalVolume: invoices.reduce((acc, i) => acc + i.amount, 0n),
  };

  const filtered = useMemo(() => {
    let result = [...invoices];

    if (search.trim()) {
      const q = search.trim().toLowerCase();
      result = result.filter(
        (inv) => inv.debtor.toLowerCase().includes(q) || inv.description.toLowerCase().includes(q),
      );
    }

    if (statusFilter !== 'All') {
      result = result.filter((inv) => inv.status === statusFilter);
    }

    switch (sort) {
      case 'newest':
        result.sort((a, b) => b.createdAt - a.createdAt);
        break;
      case 'oldest':
        result.sort((a, b) => a.createdAt - b.createdAt);
        break;
      case 'highest':
        result.sort((a, b) => (b.amount > a.amount ? 1 : b.amount < a.amount ? -1 : 0));
        break;
      case 'due-soonest':
        result.sort((a, b) => a.dueDate - b.dueDate);
        break;
    }

    return result;
  }, [invoices, search, statusFilter, sort]);

  const isFiltered = search.trim() !== '' || statusFilter !== 'All';

  return (
    <div className="min-h-screen pt-24 pb-16 px-6">
      <div className="max-w-6xl mx-auto">
        {/* Header */}
        <div className="flex items-center justify-between mb-8">
          <div>
            <h1 className="text-3xl font-bold mb-1">SME Dashboard</h1>
            <p className="text-brand-muted">Manage your tokenized invoices</p>
          </div>
          <div className="flex items-center gap-3">
            <button
              onClick={() => setShowOnboarding(true)}
              className="px-4 py-2 text-brand-muted hover:text-white transition-colors text-sm"
            >
              Help
            </button>
            {wallet.connected && (
              <Link
                href="/invoice/new"
                className="px-5 py-2.5 bg-brand-gold text-brand-dark font-semibold rounded-xl hover:bg-brand-amber transition-colors"
              >
                + New Invoice
              </Link>
            )}
          </div>
        </div>

        {!wallet.connected ? (
          <div className="flex flex-col items-center justify-center py-32 text-center">
            <div className="text-4xl mb-4">◈</div>
            <h2 className="text-xl font-semibold mb-2">Connect your wallet</h2>
            <p className="text-brand-muted">Connect Freighter to view and manage your invoices.</p>
          </div>
        ) : (
          <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
            {/* Left column */}
            <div className="lg:col-span-2 space-y-6">
              {/* Quick stats */}
              <div className="grid grid-cols-2 sm:grid-cols-4 gap-4">
                {[
                  { label: 'Total Volume', value: formatUSDC(stats.totalVolume), highlight: true },
                  { label: 'Pending', value: stats.pending.toString() },
                  { label: 'Funded', value: stats.funded.toString() },
                  { label: 'Paid', value: stats.paid.toString() },
                ].map((s) => (
                  <div
                    key={s.label}
                    className="p-4 bg-brand-card border border-brand-border rounded-xl"
                  >
                    <p className="text-xs text-brand-muted mb-1">{s.label}</p>
                    <p className={`text-xl font-bold ${s.highlight ? 'gradient-text' : ''}`}>
                      {s.value}
                    </p>
                  </div>
                ))}
              </div>

              {/* Invoices */}
              <div>
                <h2 className="text-lg font-semibold mb-4">Your Invoices</h2>

                {/* Search */}
                <div className="relative mb-3">
                  <svg
                    className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-brand-muted pointer-events-none"
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth={2}
                      d="M21 21l-4.35-4.35M17 11A6 6 0 1 1 5 11a6 6 0 0 1 12 0z"
                    />
                  </svg>
                  <input
                    type="text"
                    placeholder="Search by debtor or description..."
                    value={search}
                    onChange={(e) => setSearch(e.target.value)}
                    className="w-full bg-brand-dark border border-brand-border rounded-xl pl-9 pr-4 py-2.5 text-sm text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold"
                  />
                  {search && (
                    <button
                      onClick={() => setSearch('')}
                      className="absolute right-3 top-1/2 -translate-y-1/2 text-brand-muted hover:text-white"
                    >
                      ✕
                    </button>
                  )}
                </div>

                {/* Status tabs + Sort */}
                <div className="flex items-center justify-between gap-3 mb-4 flex-wrap">
                  <div className="flex gap-1 flex-wrap">
                    {STATUS_TABS.map((tab) => (
                      <button
                        key={tab}
                        onClick={() => setStatusFilter(tab)}
                        className={`px-3 py-1 rounded-lg text-xs font-medium transition-colors ${
                          statusFilter === tab
                            ? 'bg-brand-gold text-brand-dark'
                            : 'text-brand-muted hover:text-white bg-brand-card border border-brand-border'
                        }`}
                      >
                        {tab}
                      </button>
                    ))}
                  </div>

                  <select
                    value={sort}
                    onChange={(e) => setSort(e.target.value as SortOption)}
                    className="bg-brand-dark border border-brand-border rounded-lg px-3 py-1.5 text-xs text-white focus:outline-none focus:border-brand-gold cursor-pointer"
                  >
                    {SORT_OPTIONS.map((opt) => (
                      <option key={opt.value} value={opt.value}>
                        {opt.label}
                      </option>
                    ))}
                  </select>
                </div>

                {loading ? (
                  <div className="space-y-4">
                    {[1, 2, 3].map((n) => (
                      <div
                        key={n}
                        className="h-32 bg-brand-card border border-brand-border rounded-2xl animate-pulse"
                      />
                    ))}
                  </div>
                ) : error ? (
                  <div className="p-4 bg-red-900/20 border border-red-800/50 rounded-xl text-red-400 text-sm">
                    {error}
                  </div>
                ) : invoices.length === 0 ? (
                  <div className="p-12 bg-brand-card border border-brand-border rounded-2xl text-center">
                    <p className="text-brand-muted mb-4">No invoices yet.</p>
                    <Link
                      href="/invoice/new"
                      className="text-brand-gold hover:underline text-sm font-medium"
                    >
                      Create your first invoice →
                    </Link>
                  </div>
                ) : filtered.length === 0 ? (
                  <div className="p-12 bg-brand-card border border-brand-border rounded-2xl text-center">
                    <p className="text-brand-muted mb-3">No invoices match your filters.</p>
                    {isFiltered && (
                      <button
                        onClick={() => {
                          setSearch('');
                          setStatusFilter('All');
                        }}
                        className="text-brand-gold hover:underline text-sm font-medium"
                      >
                        Clear filters
                      </button>
                    )}
                  </div>
                ) : (
                  <div className="space-y-4">
                    {filtered.map((inv) => (
                      <InvoiceCard key={inv.id} invoice={inv} fundedAmount={committedMap[inv.id]} />
                    ))}
                  </div>
                )}
              </div>
            </div>

            {/* Right column */}
            <div>
              <CreditScore
                paid={stats.paid}
                funded={stats.funded}
                defaulted={stats.defaulted}
                totalVolume={stats.totalVolume}
              />
            </div>
          </div>
        )}
      </div>

      {/* Onboarding Modal */}
      <OnboardingModal isOpen={showOnboarding} onClose={() => setShowOnboarding(false)} />
    </div>
  );
}

// Type definitions
type StatusFilter = 'All' | 'Pending' | 'Funded' | 'Paid' | 'Defaulted';
type SortOption = 'newest' | 'oldest' | 'highest' | 'due-soonest';

// Constants
const STATUS_TABS: StatusFilter[] = ['All', 'Pending', 'Funded', 'Paid', 'Defaulted'];

const SORT_OPTIONS = [
  { value: 'newest', label: 'Newest First' },
  { value: 'oldest', label: 'Oldest First' },
  { value: 'highest', label: 'Highest Amount' },
  { value: 'due-soonest', label: 'Due Soonest' },
];
