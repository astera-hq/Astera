'use client';

import { useEffect, useState, useCallback } from 'react';
import { useTranslations } from 'next-intl';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { TableRowSkeleton } from '@/components/Skeleton';
import ConfirmActionModal from '@/components/ConfirmActionModal';
import { getInvoice, getInvoiceCount, buildMarkDefaultedTx, submitTx } from '@/lib/contracts';
import { NETWORK, formatUSDC, truncateAddress, formatDate } from '@/lib/stellar';
import type { Invoice } from '@/lib/types';

export default function AdminDefaultsPage() {
  const t = useTranslations('Admin.defaults');
  const { wallet } = useStore();
  const [invoices, setInvoices] = useState<Invoice[]>([]);
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState<number | null>(null);
  const [modalState, setModalState] = useState<{
    isOpen: boolean;
    invoice: Invoice | null;
  }>({ isOpen: false, invoice: null });

  const loadOverdueInvoices = useCallback(async () => {
    setLoading(true);
    try {
      const count = await getInvoiceCount();
      const all: Invoice[] = [];
      const now = Math.floor(Date.now() / 1000);

      for (let i = 1; i <= count; i++) {
        const inv = await getInvoice(i);
        // Overdue criteria: Status is 'Funded' and current time > dueDate
        if (inv.status === 'Funded' && now > inv.dueDate) {
          all.push(inv);
        }
      }
      setInvoices(all);
    } catch (e) {
      toast.error(t('loadFailed'));
      console.error(e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadOverdueInvoices();
  }, [loadOverdueInvoices]);

  function openMarkDefaultModal(invoice: Invoice) {
    setModalState({ isOpen: true, invoice });
  }

  async function handleMarkDefault() {
    const invoice = modalState.invoice;
    if (!invoice || !wallet.address) return;

    setModalState({ isOpen: false, invoice: null });
    setActionLoading(invoice.id);

    try {
      const xdr = await buildMarkDefaultedTx(wallet.address, invoice.id);

      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
        networkPassphrase: NETWORK,
        address: wallet.address,
      });

      if (signError) throw new Error(signError.message || 'Signing rejected.');

      await submitTx(signedTxXdr);
      toast.success(t('defaultedInvoice', { id: invoice.id }));
      await loadOverdueInvoices();
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : t('defaultingFailed');
      toast.error(msg);
      console.error(e);
    } finally {
      setActionLoading(null);
    }
  }

  return (
    <div className="space-y-8">
      <div>
        <h1 className="text-3xl font-bold mb-2">{t('title')}</h1>
        <p className="text-brand-muted text-sm">{t('description')}</p>
      </div>

      <div className="grid grid-cols-1 gap-6">
        <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden shadow-sm">
          <div className="px-6 py-4 bg-brand-dark/30 border-b border-brand-border flex items-center justify-between">
            <h2 className="text-sm font-bold uppercase tracking-widest text-brand-muted">
              {t('actionRequired')}
            </h2>
            <span className="px-2 py-0.5 bg-red-900/40 text-red-400 text-[10px] font-bold rounded border border-red-800/50">
              {t('overdue', { count: invoices.length })}
            </span>
          </div>

          <div className="overflow-x-auto">
            <table className="w-full text-left text-sm">
              <thead>
                <tr className="border-b border-brand-border bg-brand-dark/10">
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
                  <th className="px-6 py-4 font-semibold text-brand-muted uppercase tracking-wider text-right">
                    {t('tableAction')}
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-brand-border">
                {loading ? (
                  <TableRowSkeleton colSpan={5} />
                ) : invoices.length === 0 ? (
                  <tr>
                    <td colSpan={5} className="px-6 py-12 text-center text-brand-muted italic">
                      {t('noOverdue')}
                    </td>
                  </tr>
                ) : (
                  invoices.map((inv) => (
                    <tr key={inv.id} className="hover:bg-brand-dark/20 transition-colors group">
                      <td className="px-6 py-4 font-mono font-bold text-brand-gold">#{inv.id}</td>
                      <td className="px-6 py-4">
                        <div className="flex flex-col">
                          <span className="font-semibold text-white">{inv.debtor}</span>
                          <span className="text-xs text-brand-muted">
                            {truncateAddress(inv.owner)}
                          </span>
                        </div>
                      </td>
                      <td className="px-6 py-4 font-mono font-bold text-white whitespace-nowrap">
                        {formatUSDC(inv.amount)}
                      </td>
                      <td className="px-6 py-4 whitespace-nowrap">
                        <div className="flex flex-col text-red-400 font-medium">
                          <span>{formatDate(inv.dueDate)}</span>
                          <span className="text-[10px] uppercase font-bold tracking-tighter opacity-70">
                            {t('daysLate', {
                              days: Math.ceil((Date.now() - inv.dueDate * 1000) / 86400000),
                            })}
                          </span>
                        </div>
                      </td>
                      <td className="px-6 py-4 text-right">
                        <button
                          onClick={() => openMarkDefaultModal(inv)}
                          disabled={actionLoading !== null}
                          className="px-4 py-2 bg-red-900/30 text-red-500 border border-red-800/30 text-xs font-bold rounded-lg hover:bg-red-900/50 transition-all active:scale-[0.98] disabled:opacity-50 whitespace-nowrap"
                        >
                          {actionLoading === inv.id ? t('processing') : t('markDefault')}
                        </button>
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      <div className="p-6 bg-brand-dark border border-brand-border rounded-2xl text-xs text-brand-muted space-y-3">
        <div className="flex items-center gap-2 text-red-400 font-bold uppercase tracking-widest mb-1">
          <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
            <path
              fillRule="evenodd"
              d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z"
              clipRule="evenodd"
            />
          </svg>
          {t('policyTitle')}
        </div>
        <p dangerouslySetInnerHTML={{ __html: t('policyLine1') }} />
        <p dangerouslySetInnerHTML={{ __html: t('policyLine2') }} />
        <p>{t('policyLine3')}</p>
        <p className="text-brand-muted italic mt-2">{t('policyNote')}</p>
      </div>

      {/* Confirmation Modal for Mark Default */}
      <ConfirmActionModal
        title={t('modalTitle', { id: modalState.invoice?.id ?? '' })}
        description={
          modalState.invoice
            ? t('modalDesc', {
                id: modalState.invoice.id,
                amount: formatUSDC(modalState.invoice.amount),
              })
            : ''
        }
        confirmPhrase={t('confirmPhrase')}
        onConfirm={handleMarkDefault}
        onCancel={() => setModalState({ isOpen: false, invoice: null })}
        variant="destructive"
        isOpen={modalState.isOpen}
        confirmLabel={t('confirmLabel')}
      />
    </div>
  );
}
