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

type ModalAction = 'approve' | 'dispute' | 'verify' | null;

interface ModalState {
  isOpen: boolean;
  invoice: Invoice | null;
  action: ModalAction;
}

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

  const hasMore = scannedCount < totalCount;

  const fetchBatch = useCallback(async (startId: number, batchSize: number) => {
    const endId = Math.max(1, startId - batchSize + 1);
    const ids = Array.from({ length: startId - endId + 1 }, (_, i) => startId - i);

    const fetched = await getMultipleInvoices(ids);

    return fetched.filter((inv) => inv.status === 'Pending');
  }, []);

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

      const pending = await fetchBatch(count, PAGE_SIZE);
      setInvoices(pending);
      setScannedCount(Math.min(PAGE_SIZE, count));
    } catch (e) {
      toast.error('Failed to load pending invoices.');
      console.error(e);
    } finally {
      setLoading(false);
    }
  }, [fetchBatch]);

  const loadMore = useCallback(async () => {
    if (loadingMore || !hasMore) return;
    setLoadingMore(true);
    try {
      const nextStartId = totalCount - scannedCount;
      if (nextStartId < 1) return;

      const pending = await fetchBatch(nextStartId, PAGE_SIZE);
      setInvoices((prev) => [...prev, ...pending]);
      setScannedCount((prev) => Math.min(prev + PAGE_SIZE, totalCount));
    } catch (e) {
      console.error('Failed to load more invoices:', e);
    } finally {
      setLoadingMore(false);
    }
  }, [loadingMore, hasMore, totalCount, scannedCount, fetchBatch]);

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

  function openApproveModal(invoice: Invoice) {
    setModalState({ isOpen: true, invoice, action: 'approve' });
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

  const modalConfig: Record<
    NonNullable<ModalAction>,
    {
      title: (id: number) => string;
      description: (inv: Invoice) => string;
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
      <div>
        <h1 className="text-3xl font-bold mb-2">Pending Invoices</h1>
        <p className="text-brand-muted text-sm">
          Review and approve new invoice applications for co-funding.
        </p>
      </div>

      <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden">
        <div className="overflow-x-auto">
          <table className="w-full text-left text-sm">
            <thead>
              <tr className="border-b border-brand-border bg-brand-dark/50">
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
                <TableRowSkeleton colSpan={5} />
              ) : invoices.length === 0 ? (
                <tr>
                  <td colSpan={5} className="px-6 py-12 text-center text-brand-muted italic">
                    No pending invoice applications found.
                  </td>
                </tr>
              ) : (
                invoices.map((inv) => (
                  <tr key={inv.id} className="hover:bg-brand-dark/30 transition-colors">
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
    </div>
  );
}
