'use client';

import { useEffect, useMemo, useState } from 'react';
import { useTranslations } from 'next-intl';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { Skeleton } from '@/components/Skeleton';
import { parseStellarAddress } from '@/lib/types';
import { NETWORK } from '@/lib/stellar';
import {
  getKycRequired,
  getInvestorKyc,
  buildSetKycRequiredTx,
  buildSetInvestorKycTx,
  submitTx,
  fetchKycInvestors,
  KycInvestor,
} from '@/lib/contracts';

const PAGE_SIZE = 20;

export default function AdminKycPage() {
  const t = useTranslations('Admin.kyc');
  const { wallet } = useStore();
  const [kycRequired, setKycRequired] = useState(false);
  const [loading, setLoading] = useState(true);
  const [txLoading, setTxLoading] = useState(false);

  const [pendingInvestors, setPendingInvestors] = useState<KycInvestor[]>([]);
  const [approvedInvestors, setApprovedInvestors] = useState<KycInvestor[]>([]);
  const [pendingPage, setPendingPage] = useState(1);
  const [approvedPage, setApprovedPage] = useState(1);

  // Manual fallback state
  const [lookupAddress, setLookupAddress] = useState('');
  const [lookupAddressError, setLookupAddressError] = useState<string | null>(null);
  const [lookupResult, setLookupResult] = useState<boolean | null>(null);
  const [lookupLoading, setLookupLoading] = useState(false);
  const [manageAddress, setManageAddress] = useState('');
  const [manageAddressError, setManageAddressError] = useState<string | null>(null);
  const [manageApproved, setManageApproved] = useState(true);

  async function loadKycData() {
    setLoading(true);
    try {
      const required = await getKycRequired();
      setKycRequired(required);

      const { pending, approved } = await fetchKycInvestors();
      setPendingInvestors(pending);
      setApprovedInvestors(approved);
      setPendingPage(1);
      setApprovedPage(1);
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    loadKycData();
  }, []);

  const pendingPageCount = Math.max(1, Math.ceil(pendingInvestors.length / PAGE_SIZE));
  const approvedPageCount = Math.max(1, Math.ceil(approvedInvestors.length / PAGE_SIZE));

  const paginatedPendingInvestors = useMemo(
    () => pendingInvestors.slice((pendingPage - 1) * PAGE_SIZE, pendingPage * PAGE_SIZE),
    [pendingInvestors, pendingPage],
  );
  const paginatedApprovedInvestors = useMemo(
    () => approvedInvestors.slice((approvedPage - 1) * PAGE_SIZE, approvedPage * PAGE_SIZE),
    [approvedInvestors, approvedPage],
  );

  useEffect(() => {
    if (pendingPage > pendingPageCount) setPendingPage(pendingPageCount);
  }, [pendingPage, pendingPageCount]);

  useEffect(() => {
    if (approvedPage > approvedPageCount) setApprovedPage(approvedPageCount);
  }, [approvedPage, approvedPageCount]);

  async function signAndSubmit(xdr: string) {
    const freighter = await import('@stellar/freighter-api');
    const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
      networkPassphrase: NETWORK,
      address: wallet.address!,
    });
    if (signError) throw new Error(signError.message || 'Signing rejected.');
    await submitTx(signedTxXdr);
  }

  async function handleToggleKyc() {
    if (!wallet.address) return;
    setTxLoading(true);
    try {
      const admin = parseStellarAddress(wallet.address);
      const xdr = await buildSetKycRequiredTx(admin, !kycRequired);
      await signAndSubmit(xdr);
      setKycRequired((prev) => !prev);
      toast.success(t('kycEnabled', { status: !kycRequired ? t('enabled') : t('disabled') }));
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('transactionFailed'));
    } finally {
      setTxLoading(false);
    }
  }

  async function handleAction(address: string, approve: boolean) {
    if (!wallet.address) return;
    setTxLoading(true);
    try {
      const admin = parseStellarAddress(wallet.address);
      const investor = parseStellarAddress(address);
      const xdr = await buildSetInvestorKycTx(admin, investor, approve);
      await signAndSubmit(xdr);
      toast.success(
        approve
          ? t('investorApproved', { address: investor.slice(0, 8) })
          : t('investorRevoked', { address: investor.slice(0, 8) }),
      );
      await loadKycData();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('transactionFailed'));
    } finally {
      setTxLoading(false);
    }
  }

  async function handleLookup(e: React.FormEvent) {
    e.preventDefault();
    if (!lookupAddress) return;
    setLookupLoading(true);
    setLookupResult(null);
    setLookupAddressError(null);
    try {
      const investor = parseStellarAddress(lookupAddress.trim());
      const approved = await getInvestorKyc(investor);
      setLookupResult(approved);
    } catch (e) {
      setLookupAddressError(e instanceof Error ? e.message : t('invalidAddress'));
      setLookupResult(null);
    } finally {
      setLookupLoading(false);
    }
  }

  async function handleManageKyc(e: React.FormEvent) {
    e.preventDefault();
    if (!manageAddress) return;
    setManageAddressError(null);
    try {
      const investor = parseStellarAddress(manageAddress.trim());
      await handleAction(investor, manageApproved);
      setManageAddress('');
    } catch (e) {
      setManageAddressError(e instanceof Error ? e.message : t('invalidAddress'));
    }
  }

  return (
    <div className="max-w-4xl space-y-8">
      <div>
        <h1 className="text-3xl font-bold mb-2">{t('title')}</h1>
        <p className="text-brand-muted text-sm">{t('description')}</p>
      </div>

      {/* KYC toggle */}
      <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
        <div className="flex items-center justify-between">
          <div>
            <p className="font-semibold">{t('globalKyc')}</p>
            <p className="text-xs text-brand-muted mt-0.5">
              {loading ? (
                <Skeleton className="h-4 w-24 inline-block" />
              ) : kycRequired ? (
                t('kycRequired')
              ) : (
                t('kycNotRequired')
              )}
            </p>
          </div>
          <button
            onClick={handleToggleKyc}
            disabled={txLoading || loading}
            className={`px-5 py-2.5 rounded-xl text-sm font-semibold transition-colors disabled:opacity-50 ${
              kycRequired
                ? 'bg-red-500/20 text-red-400 border border-red-500/30 hover:bg-red-500/30'
                : 'bg-green-500/20 text-green-400 border border-green-500/30 hover:bg-green-500/30'
            }`}
          >
            {txLoading ? t('processing') : kycRequired ? t('disableKyc') : t('enableKyc')}
          </button>
        </div>
      </div>

      {/* Pending KYC Requests */}
      <div className="space-y-4">
        <h2 className="text-xl font-bold">{t('pendingTitle')}</h2>
        {loading ? (
          <Skeleton className="h-32 w-full rounded-2xl" />
        ) : pendingInvestors.length === 0 ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">
            {t('noPending')}
          </div>
        ) : (
          <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden">
            <div className="overflow-x-auto">
              <table className="w-full text-left text-sm">
                <thead className="bg-brand-dark border-b border-brand-border text-brand-muted">
                  <tr>
                    <th className="px-6 py-4 font-medium">{t('walletAddress')}</th>
                    <th className="px-6 py-4 font-medium">{t('depositedAmount')}</th>
                    <th className="px-6 py-4 font-medium">{t('firstSeen')}</th>
                    <th className="px-6 py-4 font-medium text-right">{t('actions')}</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-brand-border">
                  {paginatedPendingInvestors.map((inv) => (
                    <tr key={inv.address} className="hover:bg-brand-dark/50 transition-colors">
                      <td className="px-6 py-4 font-mono text-xs">{inv.address}</td>
                      <td className="px-6 py-4">
                        {(Number(inv.totalDeposited) / 10_000_000).toLocaleString()} USDC
                      </td>
                      <td className="px-6 py-4 text-brand-muted">
                        {new Date(inv.firstSeenAt).toLocaleDateString()}
                      </td>
                      <td className="px-6 py-4 text-right">
                        <button
                          onClick={() => handleAction(inv.address, true)}
                          disabled={txLoading}
                          className="px-4 py-2 bg-green-500/20 text-green-400 border border-green-500/30 rounded-lg text-xs font-semibold hover:bg-green-500/30 transition-colors disabled:opacity-50"
                        >
                          {t('approve')}
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <Pagination
              page={pendingPage}
              pageCount={pendingPageCount}
              totalItems={pendingInvestors.length}
              onPageChange={setPendingPage}
            />
          </div>
        )}
      </div>

      {/* Approved Investors */}
      <div className="space-y-4">
        <h2 className="text-xl font-bold">{t('approvedTitle')}</h2>
        {loading ? (
          <Skeleton className="h-32 w-full rounded-2xl" />
        ) : approvedInvestors.length === 0 ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">
            {t('noApproved')}
          </div>
        ) : (
          <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden">
            <div className="overflow-x-auto">
              <table className="w-full text-left text-sm">
                <thead className="bg-brand-dark border-b border-brand-border text-brand-muted">
                  <tr>
                    <th className="px-6 py-4 font-medium">{t('walletAddress')}</th>
                    <th className="px-6 py-4 font-medium">{t('depositedAmount')}</th>
                    <th className="px-6 py-4 font-medium">{t('firstSeen')}</th>
                    <th className="px-6 py-4 font-medium text-right">{t('actions')}</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-brand-border">
                  {paginatedApprovedInvestors.map((inv) => (
                    <tr key={inv.address} className="hover:bg-brand-dark/50 transition-colors">
                      <td className="px-6 py-4 font-mono text-xs">{inv.address}</td>
                      <td className="px-6 py-4">
                        {(Number(inv.totalDeposited) / 10_000_000).toLocaleString()} USDC
                      </td>
                      <td className="px-6 py-4 text-brand-muted">
                        {new Date(inv.firstSeenAt).toLocaleDateString()}
                      </td>
                      <td className="px-6 py-4 text-right">
                        <button
                          onClick={() => handleAction(inv.address, false)}
                          disabled={txLoading}
                          className="px-4 py-2 bg-red-500/20 text-red-400 border border-red-500/30 rounded-lg text-xs font-semibold hover:bg-red-500/30 transition-colors disabled:opacity-50"
                        >
                          {t('revoke')}
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <Pagination
              page={approvedPage}
              pageCount={approvedPageCount}
              totalItems={approvedInvestors.length}
              onPageChange={setApprovedPage}
            />
          </div>
        )}
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {/* Approve / Revoke investor manual */}
        <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
          <h2 className="font-semibold mb-4">{t('manualApproveRevoke')}</h2>
          <form onSubmit={handleManageKyc} className="space-y-4">
            <div>
              <label className="block text-sm text-brand-muted mb-1">{t('investorAddress')}</label>
              <input
                type="text"
                value={manageAddress}
                onChange={(e) => {
                  setManageAddress(e.target.value);
                  setManageAddressError(null);
                }}
                placeholder={t('addressPlaceholder')}
                required
                className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-3 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold font-mono text-sm"
              />
              {manageAddressError ? (
                <p className="mt-2 text-sm text-red-400">{manageAddressError}</p>
              ) : null}
            </div>
            <div className="flex gap-3">
              <button
                type="submit"
                onClick={() => setManageApproved(true)}
                disabled={txLoading}
                className="flex-1 py-3 bg-green-500/20 text-green-400 border border-green-500/30 rounded-xl text-sm font-semibold hover:bg-green-500/30 transition-colors disabled:opacity-50"
              >
                {t('approve')}
              </button>
              <button
                type="submit"
                onClick={() => setManageApproved(false)}
                disabled={txLoading}
                className="flex-1 py-3 bg-red-500/20 text-red-400 border border-red-500/30 rounded-xl text-sm font-semibold hover:bg-red-500/30 transition-colors disabled:opacity-50"
              >
                {t('revoke')}
              </button>
            </div>
          </form>
        </div>

        {/* Lookup investor KYC status */}
        <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
          <h2 className="font-semibold mb-4">{t('checkStatus')}</h2>
          <form onSubmit={handleLookup} className="flex gap-3">
            <input
              type="text"
              value={lookupAddress}
              onChange={(e) => {
                setLookupAddress(e.target.value);
                setLookupAddressError(null);
              }}
              placeholder={t('addressPlaceholder')}
              required
              className="flex-1 bg-brand-dark border border-brand-border rounded-xl px-4 py-3 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold font-mono text-sm w-full"
            />
            <button
              type="submit"
              disabled={lookupLoading}
              className="px-5 py-3 bg-brand-gold text-brand-dark rounded-xl text-sm font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50"
            >
              {lookupLoading ? '…' : t('check')}
            </button>
          </form>
          {lookupAddressError ? (
            <p className="mt-3 text-sm text-red-400">{lookupAddressError}</p>
          ) : null}
          {lookupResult !== null && (
            <p
              className={`mt-3 text-sm font-medium ${lookupResult ? 'text-green-400' : 'text-red-400'}`}
            >
              {lookupResult ? t('lookupResult') : t('lookupResultNot')}
            </p>
          )}
        </div>
      </div>

      <div className="p-4 bg-brand-dark border border-brand-border rounded-xl text-xs text-brand-muted space-y-1">
        <p>• {t('note1')}</p>
        <p>• {t('note2')}</p>
        <p>• {t('note3')}</p>
      </div>
    </div>
  );
}

function Pagination({
  page,
  pageCount,
  totalItems,
  onPageChange,
}: {
  page: number;
  pageCount: number;
  totalItems: number;
  onPageChange: (page: number) => void;
}) {
  const t = useTranslations('Admin.kyc');
  return (
    <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3 border-t border-brand-border px-6 py-4 text-xs text-brand-muted">
      <span>
        {t('page', {
          page: page.toString(),
          pageCount: pageCount.toString(),
          count: totalItems.toString(),
        })}
      </span>
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={() => onPageChange(page - 1)}
          disabled={page === 1}
          className="rounded-lg border border-brand-border px-3 py-1.5 text-xs font-semibold text-white transition-colors hover:bg-brand-dark disabled:cursor-not-allowed disabled:opacity-40"
        >
          {t('previous')}
        </button>
        <button
          type="button"
          onClick={() => onPageChange(page + 1)}
          disabled={page === pageCount}
          className="rounded-lg border border-brand-border px-3 py-1.5 text-xs font-semibold text-white transition-colors hover:bg-brand-dark disabled:cursor-not-allowed disabled:opacity-40"
        >
          {t('next')}
        </button>
      </div>
    </div>
  );
}
