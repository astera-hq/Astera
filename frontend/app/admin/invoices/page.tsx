'use client';

import { useEffect, useState, useCallback } from 'react';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { TableRowSkeleton } from '@/components/Skeleton';
import ConfirmActionModal from '@/components/ConfirmActionModal';
import {
  getMultipleInvoices,
  getInvoiceCount,
  getPoolTokenTotals,
  buildInitCoFundingTx,
  submitTx,
} from '@/lib/contracts';
import { formatUSDC, truncateAddress, formatDate } from '@/lib/stellar';
import type { Invoice } from '@/lib/types';

/** Number of invoices to scan per batch */
const PAGE_SIZE = 20;

type InvoiceTab = 'pending' | 'funding';
type ModalAction = 'approve' | 'dispute' | 'verify' | null;

interface ModalState {
  isOpen: boolean;
  invoice: Invoice | null;
  action: ModalAction;
}

type BatchResult = {
  status: 'pending' | 'success' | 'failed';
  message?: string;
};

export default function AdminInvoicesPage() {
  const { wallet } = useStore();
  const [invoices, setInvoices] = useState<Invoice[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [actionLoading, setActionLoading] = useState<number | null>(null);
  const [totalCount, setTotalCount] = useState(0);
  const [availableLiquidity, setAvailableLiquidity] = useState<bigint | null>(null);
  const [scannedCount, setScannedCount] = useState(0);
  const [modalState, setModalState] = useState<ModalState>({
    isOpen: false,
    invoice: null,
    action: null,
  });
  const [activeTab, setActiveTab] = useState<InvoiceTab>('funding');
  const [selectedInvoiceIds, setSelectedInvoiceIds] = useState<number[]>([]);
  const [batchConfirmOpen, setBatchConfirmOpen] = useState(false);
  const [batchRunning, setBatchRunning] = useState(false);
  const [batchProgress, setBatchProgress] = useState<{
    current: number;
    total: number;
    finished: boolean;
  } | null>(null);
  const [batchResults, setBatchResults] = useState<Record<number, BatchResult>>({});
  const [batchInvoices, setBatchInvoices] = useState<Invoice[]>([]);

  const hasMore = scannedCount < totalCount;
  const currentStatusFilter = activeTab === 'funding' ? 'Verified' : 'Pending';

  const fetchBatch = useCallback(
    async (startId: number, batchSize: number, status: Invoice['status']) => {
      if (startId < 1) return [] as Invoice[];

      const endId = Math.max(1, startId - batchSize + 1);
      const ids = Array.from({ length: startId - endId + 1 }, (_, i) => startId - i);

      const fetched = await getMultipleInvoices(ids);
      return fetched.filter((inv) => inv.status === status);
    },
    [],
  );

  const loadInvoices = useCallback(async () => {
    setLoading(true);
    try {
      const count = await getInvoiceCount();
      setTotalCount(count);

      if (count === 0) {
        setInvoices([]);
        setScannedCount(0);
        return;
      }

      const pending = await fetchBatch(count, PAGE_SIZE, currentStatusFilter);
      setInvoices(pending);
      setScannedCount(Math.min(PAGE_SIZE, count));
    } catch (e) {
      toast.error(`Failed to load ${currentStatusFilter.toLowerCase()} invoices.`);
      console.error(e);
    } finally {
      setLoading(false);
    }
  }, [fetchBatch, currentStatusFilter]);

  const loadMore = useCallback(async () => {
    if (loadingMore || !hasMore) return;
    setLoadingMore(true);
    try {
      const nextStartId = totalCount - scannedCount;
      if (nextStartId < 1) return;

      const pending = await fetchBatch(nextStartId, PAGE_SIZE, currentStatusFilter);
      setInvoices((prev) => [...prev, ...pending]);
      setScannedCount((prev) => Math.min(prev + PAGE_SIZE, totalCount));
    } catch (e) {
      console.error('Failed to load more invoices:', e);
    } finally {
      setLoadingMore(false);
    }
  }, [loadingMore, hasMore, totalCount, scannedCount, fetchBatch, currentStatusFilter]);

  useEffect(() => {
    loadInvoices();
  }, [loadInvoices]);

  useEffect(() => {
    const token = process.env.NEXT_PUBLIC_USDC_TOKEN_ID;
    if (!token) return;
    getPoolTokenTotals(token)
      .then((totals) => setAvailableLiquidity(totals.totalDeposited - totals.totalDeployed))
      .catch(() => setAvailableLiquidity(null));
  }, []);

  useEffect(() => {
    setSelectedInvoiceIds([]);
    setBatchResults({});
    setBatchProgress(null);
    setBatchInvoices([]);
  }, [activeTab]);

  function openApproveModal(invoice: Invoice) {
    setModalState({ isOpen: true, invoice, action: 'approve' });
  }

  function toggleInvoiceSelection(invoiceId: number) {
    setSelectedInvoiceIds((prev) =>
      prev.includes(invoiceId) ? prev.filter((id) => id !== invoiceId) : [...prev, invoiceId],
    );
  }

  function toggleSelectAll() {
    if (selectedInvoiceIds.length === invoices.length) {
      setSelectedInvoiceIds([]);
      return;
    }

    setSelectedInvoiceIds(invoices.map((inv) => inv.id));
  }

  async function handleBatchFunding() {
    if (!wallet.address) {
      toast.error('Admin wallet not connected.');
      return;
    }

    const invoicesToFund = invoices.filter((inv) => selectedInvoiceIds.includes(inv.id));
    if (invoicesToFund.length === 0) return;

    setBatchInvoices(invoicesToFund);
    setBatchRunning(true);
    setBatchResults({});
    setBatchProgress({ current: 0, total: invoicesToFund.length, finished: false });

    const results: Record<number, BatchResult> = {};

    try {
      for (let index = 0; index < invoicesToFund.length; index += 1) {
        const invoice = invoicesToFund[index];
        setBatchProgress({ current: index + 1, total: invoicesToFund.length, finished: false });

        try {
          const xdr = await buildInitCoFundingTx({
            admin: wallet.address,
            invoiceId: invoice.id,
            principal: invoice.amount,
            sme: invoice.owner,
            dueDate: invoice.dueDate,
            token: invoice.poolContract,
          });

          const freighter = await import('@stellar/freighter-api');
          const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
            networkPassphrase: 'Test SDF Network ; September 2015',
            address: wallet.address,
          });

          if (signError) throw new Error(signError.message || 'Signing rejected.');

          await submitTx(signedTxXdr);
          results[invoice.id] = { status: 'success' };
        } catch (error: unknown) {
          const message = error instanceof Error ? error.message : 'Unknown error';
          results[invoice.id] = { status: 'failed', message };
        }

        setBatchResults({ ...results });
      }
    } finally {
      setBatchRunning(false);
      setBatchProgress((prev) =>
        prev
          ? { ...prev, finished: true }
          : { current: invoicesToFund.length, total: invoicesToFund.length, finished: true },
      );
      await loadInvoices();
    }
  }

  async function handleApprove() {
    const invoice = modalState.invoice;
    if (!invoice || !wallet.address) return;

    setModalState({ isOpen: false, invoice: null, action: null });
    setActionLoading(invoice.id);

    try {
      const xdr = await buildInitCoFundingTx({
        admin: wallet.address,
        invoiceId: invoice.id,
        principal: invoice.amount,
        sme: invoice.owner,
        dueDate: invoice.dueDate,
        token: invoice.poolContract,
      });

      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
        networkPassphrase: 'Test SDF Network ; September 2015',
        address: wallet.address,
      });

      if (signError) throw new Error(signError.message || 'Signing rejected.');

      await submitTx(signedTxXdr);
      toast.success(`Invoice #${invoice.id} has been approved for co-funding.`);
      await loadInvoices();
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Failed to approve invoice.';
      toast.error(msg);
      console.error(e);
    } finally {
      setActionLoading(null);
    }
  }

  const selectedInvoices = invoices.filter((invoice) => selectedInvoiceIds.includes(invoice.id));
  const selectedTotal = selectedInvoices.reduce((total, invoice) => total + invoice.amount, 0n);
  const batchDisplayInvoices = batchInvoices.length > 0 ? batchInvoices : selectedInvoices;
  const poolHasLiquidity = availableLiquidity !== null && availableLiquidity >= selectedTotal;
  const canStartBatch =
    selectedInvoiceIds.length > 0 && availableLiquidity !== null && poolHasLiquidity;
  const batchSummary = Object.values(batchResults).reduce(
    (summary, result) => {
      if (result.status === 'success') summary.success += 1;
      if (result.status === 'failed') summary.failed += 1;
      return summary;
    },
    { success: 0, failed: 0 },
  );

  const modalConfig: Record<
    NonNullable<ModalAction>,
    {
      title: string;
      description: string;
      confirmPhrase?: string;
      variant: 'default' | 'destructive';
      confirmLabel: string;
    }
  > = {
    approve: {
      title: (id: number) => `Approve Invoice #${id}`,
      description: (inv: Invoice) =>
        `Approve and fund Invoice #${inv.id} for ${formatUSDC(inv.amount)}. This will initiate co-funding from the liquidity pool.`,
      variant: 'default',
      confirmLabel: 'Approve & Fund',
    },
    dispute: {
      title: (id: number) => `Mark Invoice #${id} as Disputed`,
      description: (inv: Invoice) =>
        `Mark Invoice #${inv.id} as disputed. This will pause all funding activities and flag the invoice for manual review.`,
      variant: 'destructive',
      confirmPhrase: 'DISPUTE',
      confirmLabel: 'Mark as Disputed',
    },
    verify: {
      title: (id: number) => `Verify Invoice #${id}`,
      description: (inv: Invoice) =>
        `Verify Invoice #${inv.id} as authentic. This confirms the invoice details and enables funding.`,
      variant: 'default',
      confirmLabel: 'Verify Invoice',
    },
  };

  const currentConfig = modalState.action ? modalConfig[modalState.action] : null;

  return (
    <div className="space-y-8">
      <div className="space-y-4">
        <div>
          <h1 className="text-3xl font-bold mb-2">
            {activeTab === 'funding' ? 'Verified Invoices Queue' : 'Pending Invoices'}
          </h1>
          <p className="text-brand-muted text-sm">
            {activeTab === 'funding'
              ? 'Select verified invoices ready for pool deployment and fund them in one flow.'
              : 'Review and approve new invoice applications for co-funding.'}
          </p>
        </div>

        <div className="flex flex-wrap gap-2">
          <button
            type="button"
            onClick={() => setActiveTab('funding')}
            className={`px-4 py-2 text-sm font-medium rounded-full transition-colors ${
              activeTab === 'funding'
                ? 'bg-brand-gold text-brand-dark'
                : 'bg-brand-card border border-brand-border text-brand-muted hover:text-white'
            }`}
          >
            Funding Queue
          </button>
          <button
            type="button"
            onClick={() => setActiveTab('pending')}
            className={`px-4 py-2 text-sm font-medium rounded-full transition-colors ${
              activeTab === 'pending'
                ? 'bg-brand-gold text-brand-dark'
                : 'bg-brand-card border border-brand-border text-brand-muted hover:text-white'
            }`}
          >
            Pending Invoices
          </button>
        </div>
      </div>

      {activeTab === 'funding' && selectedInvoiceIds.length > 0 && (
        <div className="flex flex-col gap-3 rounded-2xl border border-brand-border bg-brand-dark/40 p-4 text-sm text-white">
          <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
            <div className="font-medium">
              Deploying {formatUSDC(selectedTotal)}{' '}
              {process.env.NEXT_PUBLIC_USDC_TOKEN_ID ? 'USDC' : ''}
            </div>
            <div className="flex flex-wrap gap-2 items-center">
              <button
                type="button"
                onClick={() => setBatchConfirmOpen(true)}
                disabled={!canStartBatch || batchRunning}
                className="px-4 py-2 bg-brand-gold text-brand-dark rounded-lg text-xs font-semibold transition-colors disabled:opacity-50"
              >
                Fund Selected ({selectedInvoiceIds.length})
              </button>
              <span className="text-brand-muted">
                Pool has {availableLiquidity !== null ? formatUSDC(availableLiquidity) : 'unknown'}{' '}
                available —{' '}
                {availableLiquidity === null
                  ? 'unable to verify'
                  : poolHasLiquidity
                    ? 'sufficient'
                    : 'insufficient'}
              </span>
            </div>
          </div>
          {availableLiquidity !== null && !poolHasLiquidity && (
            <div className="text-yellow-300">
              Not enough pool liquidity for the selected invoices.
            </div>
          )}
        </div>
      )}

      <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden">
        <div className="overflow-x-auto">
          <table className="w-full text-left text-sm">
            <thead>
              <tr className="border-b border-brand-border bg-brand-dark/50">
                {activeTab === 'funding' && (
                  <th className="px-6 py-4 font-semibold text-brand-muted uppercase tracking-wider">
                    <input
                      type="checkbox"
                      checked={invoices.length > 0 && selectedInvoiceIds.length === invoices.length}
                      onChange={toggleSelectAll}
                      aria-label="Select all invoices"
                      className="h-4 w-4 text-brand-gold accent-brand-gold"
                    />
                  </th>
                )}
                <th className="px-6 py-4 font-semibold text-brand-muted uppercase tracking-wider">
                  ID
                </th>
                <th className="px-6 py-4 font-semibold text-brand-muted uppercase tracking-wider">
                  Applicant
                </th>
                <th className="px-6 py-4 font-semibold text-brand-muted uppercase tracking-wider">
                  Amount
                </th>
                <th className="px-6 py-4 font-semibold text-brand-muted uppercase tracking-wider">
                  Due Date
                </th>
                <th className="px-6 py-4 font-semibold text-brand-muted uppercase tracking-wider">
                  Action
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-brand-border">
              {loading ? (
                <TableRowSkeleton colSpan={activeTab === 'funding' ? 6 : 5} />
              ) : invoices.length === 0 ? (
                <tr>
                  <td
                    colSpan={activeTab === 'funding' ? 6 : 5}
                    className="px-6 py-12 text-center text-brand-muted italic"
                  >
                    {activeTab === 'funding'
                      ? 'No verified invoices are ready for funding.'
                      : 'No pending invoice applications found.'}
                  </td>
                </tr>
              ) : (
                invoices.map((inv) => (
                  <tr key={inv.id} className="hover:bg-brand-dark/30 transition-colors">
                    {activeTab === 'funding' && (
                      <td className="px-6 py-4">
                        <input
                          type="checkbox"
                          checked={selectedInvoiceIds.includes(inv.id)}
                          onChange={() => toggleInvoiceSelection(inv.id)}
                          className="h-4 w-4 text-brand-gold accent-brand-gold"
                          aria-label={`Select invoice ${inv.id}`}
                        />
                      </td>
                    )}
                    <td className="px-6 py-4 font-mono">#{inv.id}</td>
                    <td className="px-6 py-4">
                      <div className="flex flex-col">
                        <span className="font-medium text-white">{inv.debtor}</span>
                        <span className="text-xs text-brand-muted">
                          {truncateAddress(inv.owner)}
                        </span>
                      </div>
                    </td>
                    <td className="px-6 py-4 font-bold text-white whitespace-nowrap">
                      {formatUSDC(inv.amount)}
                      {availableLiquidity !== null && availableLiquidity < inv.amount && (
                        <span className="ml-2 inline-flex items-center rounded-full bg-yellow-500/20 px-2 py-0.5 text-[10px] font-semibold text-yellow-300">
                          Low Liquidity
                        </span>
                      )}
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap">
                      <div className="flex flex-col">
                        <span>{formatDate(inv.dueDate)}</span>
                        <span className="text-xs text-brand-muted">
                          {Math.ceil((inv.dueDate * 1000 - Date.now()) / 86400000)} days remaining
                        </span>
                      </div>
                    </td>
                    <td className="px-6 py-4">
                      <button
                        onClick={() => openApproveModal(inv)}
                        disabled={actionLoading !== null}
                        className="px-4 py-2 bg-brand-gold text-brand-dark text-xs font-bold rounded-lg hover:bg-brand-amber transition-colors disabled:opacity-50 whitespace-nowrap"
                      >
                        {actionLoading === inv.id ? 'Processing...' : 'Approve & Fund'}
                      </button>
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </div>

      {/* Load More */}
      {hasMore && (
        <div className="text-center">
          <button
            onClick={loadMore}
            disabled={loadingMore}
            className="px-6 py-2.5 bg-brand-card border border-brand-border rounded-xl text-sm font-medium text-white hover:border-brand-gold/50 transition-colors disabled:opacity-50"
          >
            {loadingMore ? (
              <span className="flex items-center justify-center gap-2">
                <span className="w-4 h-4 border-2 border-brand-gold border-t-transparent rounded-full animate-spin" />
                Loading more...
              </span>
            ) : (
              'Load more invoices'
            )}
          </button>
          <p className="text-xs text-brand-muted mt-2">
            Scanned {scannedCount} of {totalCount} on-chain invoices
          </p>
        </div>
      )}

      {batchProgress && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm px-4 py-6">
          <div className="w-full max-w-3xl overflow-hidden rounded-3xl border border-brand-border bg-brand-card shadow-2xl">
            <div className="border-b border-brand-border px-6 py-5">
              <h2 className="text-xl font-semibold text-white">Batch funding status</h2>
              <p className="mt-1 text-sm text-brand-muted">
                {batchRunning
                  ? `Funding invoice ${batchProgress.current} of ${batchProgress.total}...`
                  : `Batch complete: ${batchSummary.success} funded, ${batchSummary.failed} failed.`}
              </p>
            </div>

            <div className="max-h-[60vh] overflow-y-auto px-6 py-4">
              <div className="space-y-3">
                {batchDisplayInvoices.map((invoice) => {
                  const result = batchResults[invoice.id];
                  return (
                    <div
                      key={invoice.id}
                      className="rounded-2xl border border-brand-border bg-brand-dark/50 p-4"
                    >
                      <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
                        <div>
                          <p className="text-sm text-brand-muted">Invoice #{invoice.id}</p>
                          <p className="text-sm text-white">
                            {formatUSDC(invoice.amount)} scheduled for funding
                          </p>
                        </div>
                        <div className="flex items-center gap-2 text-sm">
                          {result?.status === 'success' ? (
                            <span className="text-green-300">Success ✓</span>
                          ) : result?.status === 'failed' ? (
                            <span className="text-red-300">Failed ✗</span>
                          ) : (
                            <span className="text-brand-muted">Pending…</span>
                          )}
                        </div>
                      </div>
                      {result?.message && (
                        <p className="mt-2 text-xs text-red-400">{result.message}</p>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>

            <div className="flex items-center justify-end gap-3 border-t border-brand-border px-6 py-4">
              <button
                type="button"
                disabled={batchRunning}
                onClick={() => {
                  if (batchRunning) return;
                  setBatchProgress(null);
                  setBatchResults({});
                  setBatchInvoices([]);
                  setSelectedInvoiceIds([]);
                }}
                className="rounded-xl bg-brand-dark px-4 py-2 text-sm text-white hover:bg-brand-border transition-colors disabled:opacity-50"
              >
                Close
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Confirmation Modal */}
      {modalState.isOpen && modalState.invoice && currentConfig && (
        <ConfirmActionModal
          title={currentConfig.title(modalState.invoice.id)}
          description={currentConfig.description(modalState.invoice)}
          confirmPhrase={currentConfig.confirmPhrase}
          onConfirm={handleApprove}
          onCancel={() => setModalState({ isOpen: false, invoice: null, action: null })}
          variant={currentConfig.variant}
          isOpen={modalState.isOpen}
          confirmLabel={currentConfig.confirmLabel}
        />
      )}

      {batchConfirmOpen && (
        <ConfirmActionModal
          title={`Fund ${selectedInvoiceIds.length} selected invoices`}
          description={`This will initiate funding for ${selectedInvoiceIds.length} verified invoice(s). Freighter will prompt once for each invoice in the batch.`}
          onConfirm={() => {
            setBatchConfirmOpen(false);
            handleBatchFunding();
          }}
          onCancel={() => setBatchConfirmOpen(false)}
          variant="default"
          isOpen={batchConfirmOpen}
          confirmLabel="Fund selected"
        />
      )}
    </div>
  );
}
