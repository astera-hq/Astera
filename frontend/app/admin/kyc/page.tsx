'use client';

import { useEffect, useState } from 'react';
import { useStore } from '@/lib/store';
import {
  getKycRequired,
  getInvestorKyc,
  buildSetKycRequiredTx,
  buildSetInvestorKycTx,
  submitTx,
} from '@/lib/contracts';

export default function AdminKycPage() {
  const { wallet } = useStore();
  const [kycRequired, setKycRequired] = useState(false);
  const [loading, setLoading] = useState(true);
  const [txLoading, setTxLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  const [lookupAddress, setLookupAddress] = useState('');
  const [lookupResult, setLookupResult] = useState<boolean | null>(null);
  const [lookupLoading, setLookupLoading] = useState(false);

  const [manageAddress, setManageAddress] = useState('');
  const [manageApproved, setManageApproved] = useState(true);

  useEffect(() => {
    async function load() {
      setLoading(true);
      try {
        const required = await getKycRequired();
        setKycRequired(required);
      } catch (e) {
        console.error(e);
      } finally {
        setLoading(false);
      }
    }
    load();
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
    setError(null);
    setSuccess(null);
    try {
      const xdr = await buildSetKycRequiredTx(wallet.address, !kycRequired);
      await signAndSubmit(xdr);
      setKycRequired((prev) => !prev);
      setSuccess(`KYC requirement ${!kycRequired ? 'enabled' : 'disabled'}.`);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Transaction failed.');
    } finally {
      setTxLoading(false);
    }
  }

  async function handleLookup(e: React.FormEvent) {
    e.preventDefault();
    if (!lookupAddress) return;
    setLookupLoading(true);
    setLookupResult(null);
    try {
      const approved = await getInvestorKyc(lookupAddress);
      setLookupResult(approved);
    } catch (e) {
      console.error(e);
      setLookupResult(null);
    } finally {
      setLookupLoading(false);
    }
  }

  async function handleManageKyc(e: React.FormEvent) {
    e.preventDefault();
    if (!wallet.address || !manageAddress) return;
    setTxLoading(true);
    setError(null);
    setSuccess(null);
    try {
      const xdr = await buildSetInvestorKycTx(wallet.address, manageAddress, manageApproved);
      await signAndSubmit(xdr);
      setSuccess(
        `Investor ${manageAddress.slice(0, 8)}… has been ${manageApproved ? 'approved' : 'revoked'}.`,
      );
      setManageAddress('');
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Transaction failed.');
    } finally {
      setTxLoading(false);
    }
  }

  return (
    <div className="max-w-2xl space-y-8">
      <div>
        <h1 className="text-3xl font-bold mb-2">KYC / Investor Whitelist</h1>
        <p className="text-brand-muted text-sm">
          Control investor eligibility for pool deposits. When KYC is required, only approved
          addresses may deposit.
        </p>
      </div>

      {/* KYC toggle */}
      <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
        <div className="flex items-center justify-between mb-4">
          <div>
            <p className="font-semibold">KYC Requirement</p>
            <p className="text-xs text-brand-muted mt-0.5">
              {loading ? '...' : kycRequired ? 'Currently required' : 'Currently not required'}
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

        {error && (
          <div className="p-3 bg-red-900/20 border border-red-800/50 rounded-xl text-red-400 text-sm">
            {error}
          </div>
        )}
        {success && (
          <div className="p-3 bg-green-900/20 border border-green-800/50 rounded-xl text-green-400 text-sm">
            {success}
          </div>
        )}
      </div>

      {/* Approve / Revoke investor */}
      <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
        <h2 className="font-semibold mb-4">Approve / Revoke Investor</h2>
        <form onSubmit={handleManageKyc} className="space-y-4">
          <div>
            <label className="block text-sm text-brand-muted mb-1">Investor Address</label>
            <input
              type="text"
              value={manageAddress}
              onChange={(e) => setManageAddress(e.target.value)}
              placeholder="G..."
              required
              className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-3 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold font-mono text-sm"
            />
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
            onChange={(e) => setLookupAddress(e.target.value)}
            placeholder="G..."
            required
            className="flex-1 bg-brand-dark border border-brand-border rounded-xl px-4 py-3 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold font-mono text-sm"
          />
          <button
            type="submit"
            disabled={lookupLoading}
            className="px-5 py-3 bg-brand-gold text-brand-dark rounded-xl text-sm font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50"
          >
            {lookupLoading ? '…' : 'Check'}
          </button>
        </form>
        {lookupResult !== null && (
          <p
            className={`mt-3 text-sm font-medium ${lookupResult ? 'text-green-400' : 'text-red-400'}`}
          >
            {lookupResult ? 'Approved' : 'Not approved'}
          </p>
        )}
      </div>

      <div className="p-4 bg-brand-dark border border-brand-border rounded-xl text-xs text-brand-muted space-y-1">
        <p>• When KYC is disabled, all investors may deposit freely.</p>
        <p>• Approvals are stored on-chain; revocation takes effect immediately.</p>
        <p>• Existing positions are not affected by KYC status changes.</p>
      </div>
    </div>
  );
}
