'use client';

import { useEffect, useState, useMemo, useCallback, useRef } from 'react';
import Link from 'next/link';
import toast from 'react-hot-toast';
import { usePathname, useRouter } from 'next/navigation';
import { useStore } from '@/lib/store';
import InvoiceCard from '@/components/InvoiceCard';
import { StatCardSkeleton, InvoiceCardSkeleton } from '@/components/Skeleton';
import CreditScore from '@/components/CreditScore';
import OnboardingModal, { isFirstTimeUser } from '@/components/OnboardingModal';
import {
  getMultipleInvoices,
  getInvoiceCount,
  getInvoiceMetadata,
  getFundedInvoice,
} from '@/lib/contracts';
import { formatUSDC } from '@/lib/stellar';
import type { Invoice, InvoiceMetadata } from '@/lib/types';
import { filterInvoicesByStatuses } from '@/lib/dashboardFilters';
import { useTranslations } from 'next-intl';

type DashboardRow = { invoice: Invoice; metadata: InvoiceMetadata };

type StatusFilter = Invoice['status'];
type SortOption =
  | 'created-desc'
  | 'created-asc'
  | 'amount-desc'
  | 'amount-asc'
  | 'due-asc'
  | 'due-desc';

/** Number of invoices to load per page */
const PAGE_SIZE = 20;

export default function DashboardPage() {
  const t = useTranslations('Dashboard');
  const router = useRouter();
  const pathname = usePathname();
  const { wallet } = useStore();
  const [invoices, setInvoices] = useState<DashboardRow[]>([]);
  const [committedMap, setCommittedMap] = useState<Record<number, bigint>>({});
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [showOnboarding, setShowOnboarding] = useState(false);

  const [search, setSearch] = useState('');
  const [debouncedSearch, setDebouncedSearch] = useState('');
  const [statusFilters, setStatusFilters] = useState<StatusFilter[]>([]);
  const [sort, setSort] = useState<SortOption>('created-desc');
  const [hydrated, setHydrated] = useState(false);

  const STATUS_TABS: StatusFilter[] = [
    'Pending',
    'AwaitingVerification',
    'Verified',
    'Disputed',
    'Funded',
    'Paid',
    'Defaulted',
  ];

  const SORT_OPTIONS: { value: SortOption; label: string }[] = [
    { value: 'created-desc', label: t('sort.createdDesc') },
    { value: 'created-asc', label: t('sort.createdAsc') },
    { value: 'amount-desc', label: t('sort.amountDesc') },
    { value: 'amount-asc', label: t('sort.amountAsc') },
    { value: 'due-asc', label: t('sort.dueAsc') },
    { value: 'due-desc', label: t('sort.dueDesc') },
  ];

  /** Total number of on-chain invoices (not just the user's) */
  const [totalOnChainCount, setTotalOnChainCount] = useState(0);
  /** How many on-chain invoices we have already scanned */
  const [scannedCount, setScannedCount] = useState(0);
  /** Whether all on-chain invoices have been scanned */
  const hasMore = scannedCount < totalOnChainCount;

  /** Ref used to preserve scroll position when loading more */
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setHydrated(true);
  }, []);

  useEffect(() => {
    if (!hydrated) return;

    const params = new URLSearchParams(window.location.search);
    const q = params.get('q') ?? '';
    const status = params.get('status');
    const initialStatuses = status
      ? status
          .split(',')
          .filter((value): value is StatusFilter => STATUS_TABS.includes(value as StatusFilter))
      : [];
    const initialSort = params.get('sort');
    const initialSortValue = SORT_OPTIONS.some((opt) => opt.value === initialSort)
      ? (initialSort as SortOption)
      : 'created-desc';

    setSearch(q);
    setDebouncedSearch(q);
    setStatusFilters(initialStatuses);
    setSort(initialSortValue);
  }, [hydrated]);

  useEffect(() => {
    const handle = window.setTimeout(() => {
      setDebouncedSearch(search);
    }, 300);
    return () => window.clearTimeout(handle);
  }, [search]);

  useEffect(() => {
    if (!hydrated) return;

    const params = new URLSearchParams();
    if (debouncedSearch.trim()) params.set('q', debouncedSearch.trim());
    if (statusFilters.length > 0) params.set('status', statusFilters.join(','));
    if (sort !== 'created-desc') params.set('sort', sort);

    const query = params.toString();
    router.replace(query ? `${pathname}?${query}` : pathname, { scroll: false });
  }, [debouncedSearch, hydrated, pathname, router, sort, statusFilters]);

  // Check if user is first-time visitor
  useEffect(() => {
    if (isFirstTimeUser()) {
      setShowOnboarding(true);
    }
  }, []);

  /**
   * Fetch a batch of invoices starting from `startId` down to 1 (newest first).
   * Returns the user's invoices found in this batch and the co-funding map entries.
   */
  const fetchBatch = useCallback(
    async (startId: number, batchSize: number) => {
      const endId = Math.max(1, startId - batchSize + 1);
      const ids = Array.from({ length: startId - endId + 1 }, (_, i) => startId - i);

      const fetched = await getMultipleInvoices(ids);

      const mine = fetched
        .map((invoice, index) => ({ id: ids[index], invoice }))
        .filter((row) => row.invoice.owner === wallet.address);
      const rows: DashboardRow[] = await Promise.all(
        mine.map(async ({ id, invoice }) => ({
          invoice,
          metadata: await getInvoiceMetadata(id),
        })),
      );

      // Fetch co-funding progress for pending invoices in this batch
      const committed: Record<number, bigint> = {};
      await Promise.all(
        rows
          .filter((row) => row.invoice.status === 'Pending')
          .map(async (row) => {
            try {
              const record = await getFundedInvoice(row.invoice.id);
              if (record) committed[row.invoice.id] = record.committed;
            } catch {
              // Not registered for co-funding yet
            }
          }),
      );

      return { rows, committed, scannedUpTo: endId - 1 };
    },
    [wallet.address],
  );

  /** Initial load — fetches the first PAGE_SIZE invoices (from newest) */
  const loadInvoices = useCallback(async () => {
    setLoading(true);
    try {
      const count = await getInvoiceCount();
      setTotalOnChainCount(count);

      if (count === 0) {
        setInvoices([]);
        setCommittedMap({});
        setScannedCount(0);
        return;
      }

      const { rows, committed, scannedUpTo } = await fetchBatch(count, PAGE_SIZE);
      setInvoices(rows);
      setCommittedMap(committed);
      setScannedCount(count - Math.max(scannedUpTo, 0));
    } catch (e) {
      toast.error('Failed to load invoices. Make sure contracts are deployed.');
      console.error(e);
    } finally {
      setLoading(false);
    }
  }, [fetchBatch]);

  /** Load the next page of invoices */
  const loadMore = useCallback(async () => {
    if (loadingMore || !hasMore) return;

    // Save scroll position
    const scrollY = window.scrollY;

    setLoadingMore(true);
    try {
      const nextStartId = totalOnChainCount - scannedCount;
      if (nextStartId < 1) return;

      const { rows, committed } = await fetchBatch(nextStartId, PAGE_SIZE);
      setInvoices((prev) => [...prev, ...rows]);
      setCommittedMap((prev) => ({ ...prev, ...committed }));
      setScannedCount((prev) => Math.min(prev + PAGE_SIZE, totalOnChainCount));

      // Restore scroll position after DOM update
      requestAnimationFrame(() => {
        window.scrollTo(0, scrollY);
      });
    } catch (e) {
      console.error('Failed to load more invoices:', e);
    } finally {
      setLoadingMore(false);
    }
  }, [loadingMore, hasMore, totalOnChainCount, scannedCount, fetchBatch]);

  useEffect(() => {
    if (!wallet.connected) {
      setLoading(false);
      return;
    }
    loadInvoices();
  }, [wallet.connected, wallet.address, loadInvoices]);

  const stats = {
    total: invoices.length,
    pending: invoices.filter((row) => row.invoice.status === 'Pending').length,
    funded: invoices.filter((row) => row.invoice.status === 'Funded').length,
    paid: invoices.filter((row) => row.invoice.status === 'Paid').length,
    defaulted: invoices.filter((row) => row.invoice.status === 'Defaulted').length,
    totalVolume: invoices.reduce((acc, row) => acc + row.invoice.amount, 0n),
  };

  const filtered = useMemo(() => {
    let result = [...invoices];

    if (debouncedSearch.trim()) {
      const q = debouncedSearch.trim().toLowerCase();
      result = result.filter(
        (row) =>
          row.metadata.debtor.toLowerCase().includes(q) ||
          row.metadata.description.toLowerCase().includes(q) ||
          row.metadata.name.toLowerCase().includes(q),
      );
    }

    result = filterInvoicesByStatuses(result, statusFilters);

    switch (sort) {
      case 'created-desc':
        result.sort((a, b) => b.invoice.createdAt - a.invoice.createdAt);
        break;
      case 'created-asc':
        result.sort((a, b) => a.invoice.createdAt - b.invoice.createdAt);
        break;
      case 'amount-desc':
        result.sort((a, b) =>
          b.metadata.amount > a.metadata.amount
            ? 1
            : b.metadata.amount < a.metadata.amount
              ? -1
              : 0,
        );
        break;
      case 'amount-asc':
        result.sort((a, b) =>
          a.metadata.amount > b.metadata.amount
            ? 1
            : a.metadata.amount < b.metadata.amount
              ? -1
              : 0,
        );
        break;
      case 'due-asc':
        result.sort((a, b) => a.metadata.dueDate - b.metadata.dueDate);
        break;
      case 'due-desc':
        result.sort((a, b) => b.metadata.dueDate - a.metadata.dueDate);
        break;
    }

    return result;
  }, [debouncedSearch, invoices, sort, statusFilters]);

  const isFiltered = debouncedSearch.trim() !== '' || statusFilters.length > 0;

  return (
    <div className="min-h-screen pt-24 pb-16 px-6">
      <div className="max-w-6xl mx-auto">
        {/* Header */}
        <div className="flex items-center justify-between mb-8">
          <div>
            <h1 className="text-3xl font-bold mb-1">{t('title')}</h1>
            <p className="text-brand-muted">{t('description')}</p>
          </div>
          <div className="flex items-center gap-3">
            <button
              onClick={() => setShowOnboarding(true)}
              className="px-4 py-2 text-brand-muted hover:text-white transition-colors text-sm"
            >
              {t('help')}
            </button>
            {wallet.connected && (
              <Link
                href="/invoice/new"
                className="px-5 py-2.5 bg-brand-gold text-brand-dark font-semibold rounded-xl hover:bg-brand-amber transition-colors"
              >
                {t('newInvoice')}
              </Link>
            )}
          </div>
        </div>

        {!wallet.connected ? (
          <div className="flex flex-col items-center justify-center py-32 text-center">
            <div className="text-4xl mb-4">◈</div>
            <h2 className="text-xl font-semibold mb-2">{t('connectWallet')}</h2>
            <p className="text-brand-muted">{t('connectWalletDesc')}</p>
          </div>
        ) : (
          <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
            {/* Left column */}
            <div className="lg:col-span-2 space-y-6">
              {/* Quick stats */}
              <div className="grid grid-cols-2 sm:grid-cols-4 gap-4">
                {[
                  { label: t('stats.totalVolume'), value: formatUSDC(stats.totalVolume), highlight: true },
                  { label: t('stats.pending'), value: stats.pending.toString() },
                  { label: t('stats.funded'), value: stats.funded.toString() },
                  { label: t('stats.paid'), value: stats.paid.toString() },
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
              <div ref={listRef}>
                <h2 className="text-lg font-semibold mb-4">{t('yourInvoices')}</h2>

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
                    placeholder={t('searchPlaceholder')}
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
                    <button
                      onClick={() => setStatusFilters([])}
                      className={`px-3 py-1 rounded-lg text-xs font-medium transition-colors ${
                        statusFilters.length === 0
                          ? 'bg-brand-gold text-brand-dark'
                          : 'text-brand-muted hover:text-white bg-brand-card border border-brand-border'
                      }`}
                    >
                      {t('status.all')}
                    </button>
                    {STATUS_TABS.map((tab) => (
                      <button
                        key={tab}
                        onClick={() =>
                          setStatusFilters((prev) =>
                            prev.includes(tab) ? prev.filter((item) => item !== tab) : [...prev, tab],
                          )
                        }
                        className={`px-3 py-1 rounded-lg text-xs font-medium transition-colors ${
                          statusFilters.includes(tab)
                            ? 'bg-brand-gold text-brand-dark'
                            : 'text-brand-muted hover:text-white bg-brand-card border border-brand-border'
                        }`}
                      >
                        {t(`status.${tab.toLowerCase()}`)}
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
                {statusFilters.length > 0 && (
                  <div className="flex items-center gap-2 flex-wrap mb-4">
                    {statusFilters.map((status) => (
                      <button
                        key={status}
                        onClick={() =>
                          setStatusFilters((prev) => prev.filter((item) => item !== status))
                        }
                        className="px-2.5 py-1 rounded-full text-xs bg-brand-card border border-brand-border text-white hover:border-brand-gold/60"
                      >
                        {t(`status.${status.toLowerCase()}`)} ✕
                      </button>
                    ))}
                  </div>
                )}

                {loading ? (
                  <div className="space-y-4">
                    {[1, 2, 3].map((n) => (
                      <InvoiceCardSkeleton key={n} />
                    ))}
                  </div>
                ) : invoices.length === 0 ? (
                  <div className="p-12 bg-brand-card border border-brand-border rounded-2xl text-center">
                    <p className="text-brand-muted mb-4">{t('noInvoices')}</p>
                    <Link
                      href="/invoice/new"
                      className="text-brand-gold hover:underline text-sm font-medium"
                    >
                      {t('createFirst')}
                    </Link>
                  </div>
                ) : filtered.length === 0 ? (
                  <div className="p-12 bg-brand-card border border-brand-border rounded-2xl text-center">
                    <p className="text-brand-muted mb-3">{t('noMatch')}</p>
                    {isFiltered && (
                      <button
                        onClick={() => {
                          setSearch('');
                          setDebouncedSearch('');
                          setStatusFilters([]);
                        }}
                        className="text-brand-gold hover:underline text-sm font-medium"
                      >
                        {t('clearFilters')}
                      </button>
                    )}
                  </div>
                ) : (
                  <>
                    <div className="space-y-4">
                      {filtered.map((inv) => (
                        <InvoiceCard
                          key={inv.invoice.id}
                          id={inv.invoice.id}
                          metadata={inv.metadata}
                          fundedAmount={committedMap[inv.invoice.id]}
                        />
                      ))}
                    </div>

                    {/* Load More / Pagination Controls */}
                    {hasMore && (
                      <div className="mt-6 text-center">
                        <button
                          onClick={loadMore}
                          disabled={loadingMore}
                          className="px-6 py-2.5 bg-brand-card border border-brand-border rounded-xl text-sm font-medium text-white hover:border-brand-gold/50 transition-colors disabled:opacity-50"
                        >
                          {loadingMore ? (
                            <span className="flex items-center justify-center gap-2">
                              <span className="w-4 h-4 border-2 border-brand-gold border-t-transparent rounded-full animate-spin" />
                              {t('loadingMore')}
                            </span>
                          ) : (
                            t('loadMore')
                          )}
                        </button>
                        <p className="text-xs text-brand-muted mt-2">
                          {t('showing', { count: invoices.length })}
                          {totalOnChainCount > 0 &&
                            ` · Scanned ${scannedCount} of ${totalOnChainCount} on-chain`}
                        </p>
                      </div>
                    )}

                    {!hasMore && invoices.length > 0 && (
                      <p className="text-xs text-brand-muted text-center mt-4">
                        {t('allLoaded', { count: invoices.length })}
                      </p>
                    )}
                  </>
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
