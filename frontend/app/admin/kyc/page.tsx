'use client';

import { useEffect, useState } from 'react';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { Skeleton } from '@/components/Skeleton';
import { parseStellarAddress } from '@/lib/types';
import {
  getKycRequired,
  getInvestorKyc,
  buildSetKycRequiredTx,
  buildSetInvestorKycTx,
  submitTx,
  fetchKycInvestors,
  KycInvestor,
} from '@/lib/contracts';

export default function AdminKycPage() {
  const { wallet } = useStore();
  const [kycRequired, setKycRequired] = useState(false);
  const [loading, setLoading] = useState(true);
  const [txLoading, setTxLoading] = useState(false);

  const [pendingInvestors, setPendingInvestors] = useState<KycInvestor[]>([]);
  const [approvedInvestors, setApprovedInvestors] = useState<KycInvestor[]>([]);

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
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    loadKycData();
  }, []);

  async function signAndSubmit(xdr: string) {
    const freighter = await import('@stellar/freighter-api');
    const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
      networkPassphrase: 'Test SDF Network ; September 2015',
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
      toast.success(`KYC requirement ${!kycRequired ? 'enabled' : 'disabled'}.`);
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Transaction failed.');
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
        `Investor ${investor.slice(0, 8)}… has been ${approve ? 'approved' : 'revoked'}.`,
      );
      // Refresh the lists
      await loadKycData();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Transaction failed.');
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
      setLookupAddressError(e instanceof Error ? e.message : 'Invalid Stellar address.');
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
      setManageAddressError(e instanceof Error ? e.message : 'Invalid Stellar address.');
    }
  }

  return (
    <div className="max-w-4xl space-y-8">
      <div>
        <h1 className="text-3xl font-bold mb-2">KYC / Investor Whitelist</h1>
        <p className="text-brand-muted text-sm">
          Control investor eligibility for pool deposits. When KYC is required, only approved
          addresses may deposit.
        </p>
      </div>

      {/* KYC toggle */}
      <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
        <div className="flex items-center justify-between">
          <div>
            <p className="font-semibold">Global KYC Requirement</p>
            <p className="text-xs text-brand-muted mt-0.5">
              {loading ? (
                <Skeleton className="h-4 w-24 inline-block" />
              ) : kycRequired ? (
                'Currently REQUIRED: Unapproved investors cannot deposit.'
              ) : (
                'Currently NOT REQUIRED: All investors may deposit freely.'
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
            {txLoading ? 'Processing…' : kycRequired ? 'Disable KYC' : 'Enable KYC'}
          </button>
        </div>
      </div>

      {/* Pending KYC Requests */}
      <div className="space-y-4">
        <h2 className="text-xl font-bold">Pending KYC Requests</h2>
        {loading ? (
          <Skeleton className="h-32 w-full rounded-2xl" />
        ) : pendingInvestors.length === 0 ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">
            No pending KYC requests found.
          </div>
        ) : (
          <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden">
            <table className="w-full text-left text-sm">
              <thead className="bg-brand-dark border-b border-brand-border text-brand-muted">
                <tr>
                  <th className="px-6 py-4 font-medium">Wallet Address</th>
                  <th className="px-6 py-4 font-medium">Deposited Amount</th>
                  <th className="px-6 py-4 font-medium">First Seen</th>
                  <th className="px-6 py-4 font-medium text-right">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-brand-border">
                {pendingInvestors.map((inv) => (
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
                        Approve
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Approved Investors */}
      <div className="space-y-4">
        <h2 className="text-xl font-bold">Approved Investors</h2>
        {loading ? (
          <Skeleton className="h-32 w-full rounded-2xl" />
        ) : approvedInvestors.length === 0 ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">
            No approved investors found.
          </div>
        ) : (
          <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden">
            <table className="w-full text-left text-sm">
              <thead className="bg-brand-dark border-b border-brand-border text-brand-muted">
                <tr>
                  <th className="px-6 py-4 font-medium">Wallet Address</th>
                  <th className="px-6 py-4 font-medium">Deposited Amount</th>
                  <th className="px-6 py-4 font-medium">First Seen</th>
                  <th className="px-6 py-4 font-medium text-right">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-brand-border">
                {approvedInvestors.map((inv) => (
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
                        Revoke
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {/* Approve / Revoke investor manual */}
        <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
          <h2 className="font-semibold mb-4">Manual Approve / Revoke</h2>
          <form onSubmit={handleManageKyc} className="space-y-4">
            <div>
              <label className="block text-sm text-brand-muted mb-1">Investor Address</label>
              <input
                type="text"
                value={manageAddress}
                onChange={(e) => {
                  setManageAddress(e.target.value);
                  setManageAddressError(null);
                }}
                placeholder="G..."
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
                Approve
              </button>
              <button
                type="submit"
                onClick={() => setManageApproved(false)}
                disabled={txLoading}
                className="flex-1 py-3 bg-red-500/20 text-red-400 border border-red-500/30 rounded-xl text-sm font-semibold hover:bg-red-500/30 transition-colors disabled:opacity-50"
              >
                Revoke
              </button>
            </div>
          </form>
        </div>

        {/* Lookup investor KYC status */}
        <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
          <h2 className="font-semibold mb-4">Check Investor Status</h2>
          <form onSubmit={handleLookup} className="flex gap-3">
            <input
              type="text"
              value={lookupAddress}
              onChange={(e) => {
                setLookupAddress(e.target.value);
                setLookupAddressError(null);
              }}
              placeholder="G..."
              required
              className="flex-1 bg-brand-dark border border-brand-border rounded-xl px-4 py-3 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold font-mono text-sm w-full"
            />
            <button
              type="submit"
              disabled={lookupLoading}
              className="px-5 py-3 bg-brand-gold text-brand-dark rounded-xl text-sm font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50"
            >
              {lookupLoading ? '…' : 'Check'}
            </button>
          </form>
          {lookupAddressError ? (
            <p className="mt-3 text-sm text-red-400">{lookupAddressError}</p>
          ) : null}
          {lookupResult !== null && (
            <p
              className={`mt-3 text-sm font-medium ${lookupResult ? 'text-green-400' : 'text-red-400'}`}
            >
              {lookupResult ? 'Approved' : 'Not approved'}
            </p>
          )}
        </div>
      </div>

      <div className="p-4 bg-brand-dark border border-brand-border rounded-xl text-xs text-brand-muted space-y-1">
        <p>• When KYC is disabled, all investors may deposit freely.</p>
        <p>• Approvals are stored on-chain; revocation takes effect immediately.</p>
        <p>• Existing positions are not affected by KYC status changes.</p>
      </div>
    </div>
  );
}
