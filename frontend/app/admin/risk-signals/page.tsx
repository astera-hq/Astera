'use client';

import { useCallback, useEffect, useState } from 'react';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { Skeleton } from '@/components/Skeleton';
import { parseStellarAddress } from '@/lib/types';
import type { RiskSignal } from '@/lib/types';
import {
  buildSubmitRiskSignalTx,
  submitTx,
  getContractErrorMessage,
} from '@/lib/contracts';

const INDEXER_URL = process.env.NEXT_PUBLIC_INDEXER_URL || 'http://localhost:3001';

export default function RiskSignalsAdminPage() {
  const { wallet } = useStore();
  const [signal, setSignal] = useState<RiskSignal | null>(null);
  const [loading, setLoading] = useState(false);
  const [txLoading, setTxLoading] = useState(false);

  const [lookupAddress, setLookupAddress] = useState('');

  const [smeAddress, setSmeAddress] = useState('');
  const [debtorConcentration, setDebtorConcentration] = useState('');
  const [invoiceSizeRisk, setInvoiceSizeRisk] = useState('');

  async function signAndSubmit(xdr: string) {
    const freighter = await import('@stellar/freighter-api');
    const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
      networkPassphrase: 'Test SDF Network ; September 2015',
      address: wallet.address!,
    });
    if (signError) throw new Error(signError.message || 'Signing rejected.');
    await submitTx(signedTxXdr);
  }

  const fetchRiskSignal = useCallback(async (sme: string) => {
    setLoading(true);
    setSignal(null);
    try {
      const res = await fetch(`${INDEXER_URL}/credit-score/${sme}/risk-signals`);
      if (!res.ok) {
        if (res.status === 404) {
          toast.error('No risk signal found for this SME.');
          return;
        }
        throw new Error(`Indexer returned ${res.status}`);
      }
      const data = await res.json();
      setSignal(data);
    } catch (e) {
      console.error(e);
      toast.error('Failed to fetch risk signal from indexer.');
    } finally {
      setLoading(false);
    }
  }, []);

  async function handleLookup(e: React.FormEvent) {
    e.preventDefault();
    const address = lookupAddress.trim();
    if (!address) {
      toast.error('Enter an SME address.');
      return;
    }
    try {
      const resolved = parseStellarAddress(address);
      await fetchRiskSignal(resolved);
    } catch {
      toast.error('Invalid Stellar address.');
    }
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!wallet.address) return;

    const debtorBps = Number(debtorConcentration);
    const invoiceBps = Number(invoiceSizeRisk);

    if (!Number.isFinite(debtorBps) || debtorBps < 0 || debtorBps > 10_000) {
      toast.error('Debtor concentration must be between 0 and 10000 bps.');
      return;
    }
    if (!Number.isFinite(invoiceBps) || invoiceBps < 0 || invoiceBps > 10_000) {
      toast.error('Invoice size risk must be between 0 and 10000 bps.');
      return;
    }

    setTxLoading(true);
    try {
      const admin = parseStellarAddress(wallet.address);
      const sme = parseStellarAddress(smeAddress.trim());
      const xdr = await buildSubmitRiskSignalTx({
        admin,
        sme,
        debtorConcentrationBps: debtorBps,
        invoiceSizeRiskBps: invoiceBps,
      });
      await signAndSubmit(xdr);
      toast.success(`Risk signal submitted for ${sme.slice(0, 8)}…`);
      setSmeAddress('');
      setDebtorConcentration('');
      setInvoiceSizeRisk('');
      // Refresh the lookup if it matches
      await fetchRiskSignal(sme);
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : 'Transaction failed.';
      toast.error(getContractErrorMessage(message));
    } finally {
      setTxLoading(false);
    }
  }

  return (
    <div className="max-w-5xl space-y-8">
      <div>
        <h1 className="text-3xl font-bold mb-2">Risk Signals</h1>
        <p className="text-brand-muted text-sm">
          View and submit SME risk signals used for credit score adjustment. Risk signals measure
          debtor concentration and invoice-size risk, and are factored into the final blended score.
        </p>
      </div>

      {/* Lookup form */}
      <div className="p-6 bg-brand-card border border-brand-border rounded-2xl space-y-4">
        <h2 className="font-semibold">Look Up Risk Signal</h2>
        <form onSubmit={handleLookup} className="flex gap-3">
          <input
            type="text"
            placeholder="SME address or Stellar address…"
            value={lookupAddress}
            onChange={(e) => setLookupAddress(e.target.value)}
            className="flex-1 bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold font-mono text-sm"
          />
          <button
            type="submit"
            disabled={loading}
            className="px-5 py-2.5 bg-brand-dark border border-brand-border rounded-xl text-sm font-semibold hover:bg-brand-border transition-colors disabled:opacity-50"
          >
            {loading ? 'Loading…' : 'Look Up'}
          </button>
        </form>

        {signal && (
          <div className="border-t border-brand-border pt-4 space-y-3 text-sm">
            <div className="grid grid-cols-2 sm:grid-cols-4 gap-4">
              <div className="p-4 bg-brand-dark rounded-xl">
                <p className="text-xs text-brand-muted mb-1">Debtor Concentration</p>
                <p className="text-lg font-bold">{(signal.debtorConcentrationBps / 100).toFixed(2)}%</p>
                <p className="text-xs text-brand-muted">{signal.debtorConcentrationBps} bps</p>
              </div>
              <div className="p-4 bg-brand-dark rounded-xl">
                <p className="text-xs text-brand-muted mb-1">Invoice Size Risk</p>
                <p className="text-lg font-bold">{(signal.invoiceSizeRiskBps / 100).toFixed(2)}%</p>
                <p className="text-xs text-brand-muted">{signal.invoiceSizeRiskBps} bps</p>
              </div>
              <div className="p-4 bg-brand-dark rounded-xl">
                <p className="text-xs text-brand-muted mb-1">Total Volume</p>
                <p className="text-lg font-bold">{Number(signal.totalVolume).toLocaleString()}</p>
              </div>
              <div className="p-4 bg-brand-dark rounded-xl">
                <p className="text-xs text-brand-muted mb-1">Last Updated</p>
                <p className="text-lg font-bold">
                  {new Date(signal.updatedAt * 1000).toLocaleDateString()}
                </p>
              </div>
            </div>
          </div>
        )}
      </div>

      {/* Submit form */}
      <div className="p-6 bg-brand-card border border-brand-border rounded-2xl space-y-4">
        <h2 className="font-semibold">Submit Risk Signal</h2>
        <p className="text-brand-muted text-sm">
          Manually submit a risk signal for an SME. Values are in basis points (0–10000, where 10000 = 100%).
        </p>
        <form onSubmit={handleSubmit} className="grid grid-cols-1 sm:grid-cols-4 gap-3">
          <input
            type="text"
            placeholder="SME address"
            value={smeAddress}
            onChange={(e) => setSmeAddress(e.target.value)}
            required
            className="sm:col-span-4 bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold font-mono text-sm"
          />
          <div>
            <label className="block text-xs text-brand-muted mb-1">Debtor concentration (bps)</label>
            <input
              type="number"
              min={0}
              max={10000}
              placeholder="0–10000"
              value={debtorConcentration}
              onChange={(e) => setDebtorConcentration(e.target.value)}
              className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm"
            />
          </div>
          <div>
            <label className="block text-xs text-brand-muted mb-1">Invoice size risk (bps)</label>
            <input
              type="number"
              min={0}
              max={10000}
              placeholder="0–10000"
              value={invoiceSizeRisk}
              onChange={(e) => setInvoiceSizeRisk(e.target.value)}
              className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm"
            />
          </div>
          <button
            type="submit"
            disabled={txLoading}
            className="sm:col-span-2 py-2.5 bg-brand-gold text-brand-dark rounded-xl text-sm font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50"
          >
            {txLoading ? 'Processing…' : 'Submit Risk Signal'}
          </button>
        </form>
      </div>
    </div>
  );
}
