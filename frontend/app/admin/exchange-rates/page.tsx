'use client';

import { useEffect, useState } from 'react';
import { useStore } from '@/lib/store';
import {
  getAcceptedTokens,
  getExchangeRate,
  buildSetExchangeRateTx,
  submitTx,
} from '@/lib/contracts';
import { stablecoinLabel } from '@/lib/stellar';

export default function AdminExchangeRatesPage() {
  const { wallet } = useStore();
  const [tokens, setTokens] = useState<string[]>([]);
  const [rates, setRates] = useState<Record<string, number>>({});
  const [loading, setLoading] = useState(true);
  const [txLoading, setTxLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const [selectedToken, setSelectedToken] = useState('');
  const [newRatePct, setNewRatePct] = useState('');

  useEffect(() => {
    async function load() {
      setLoading(true);
      try {
        const accepted = await getAcceptedTokens();
        setTokens(accepted);
        if (accepted.length > 0) setSelectedToken(accepted[0]);
        const rateMap: Record<string, number> = {};
        await Promise.all(
          accepted.map(async (t) => {
            rateMap[t] = await getExchangeRate(t);
          }),
        );
        setRates(rateMap);
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

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!wallet.address || !selectedToken || !newRatePct) return;

    const bps = Math.round(parseFloat(newRatePct) * 100);
    if (isNaN(bps) || bps <= 0) {
      setError('Rate must be a positive number (e.g. 100 for 1:1 with USD).');
      return;
    }

    setTxLoading(true);
    setError(null);
    setSuccess(null);
    try {
      const xdr = await buildSetExchangeRateTx(wallet.address, selectedToken, bps);
      await signAndSubmit(xdr);
      setRates((prev) => ({ ...prev, [selectedToken]: bps }));
      setSuccess(
        `Exchange rate for ${stablecoinLabel(selectedToken)} set to ${newRatePct}% of USD (${bps} bps).`,
      );
      setNewRatePct('');
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Transaction failed.');
    } finally {
      setTxLoading(false);
    }
  }

  return (
    <div className="max-w-2xl space-y-8">
      <div>
        <h1 className="text-3xl font-bold mb-2">Exchange Rates</h1>
        <p className="text-brand-muted text-sm">
          Set USD-denominated exchange rates for each accepted stablecoin (in basis points). Used
          for normalised pool reporting across currencies.
        </p>
      </div>

      {/* Current rates */}
      <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
        <h2 className="font-semibold mb-4">Current Rates</h2>
        {loading ? (
          <div className="h-20 animate-pulse bg-brand-dark rounded-xl" />
        ) : tokens.length === 0 ? (
          <p className="text-brand-muted text-sm">No tokens configured.</p>
        ) : (
          <div className="space-y-2">
            {tokens.map((t) => (
              <div
                key={t}
                className="flex items-center justify-between p-3 bg-brand-dark rounded-xl border border-brand-border"
              >
                <span className="font-medium">{stablecoinLabel(t)}</span>
                <span className="text-brand-gold font-semibold text-sm">
                  {rates[t] !== undefined ? `${(rates[t] / 100).toFixed(4)}x USD` : '—'}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Update rate form */}
      <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
        <h2 className="font-semibold mb-4">Update Exchange Rate</h2>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm text-brand-muted mb-1">Token</label>
            <select
              value={selectedToken}
              onChange={(e) => setSelectedToken(e.target.value)}
              disabled={tokens.length === 0}
              className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-3 text-white focus:outline-none focus:border-brand-gold"
            >
              {tokens.map((t) => (
                <option key={t} value={t}>
                  {stablecoinLabel(t)}
                </option>
              ))}
            </select>
          </div>
          <div>
            <label className="block text-sm text-brand-muted mb-1">
              Rate (% of USD, e.g. 100 = 1:1, 108 = 1.08 USD)
            </label>
            <input
              type="number"
              step="0.01"
              min="0.01"
              value={newRatePct}
              onChange={(e) => setNewRatePct(e.target.value)}
              placeholder="e.g. 100"
              required
              className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-3 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold"
            />
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

          <button
            type="submit"
            disabled={txLoading || tokens.length === 0}
            className="w-full py-3 bg-brand-gold text-brand-dark font-semibold rounded-xl hover:bg-brand-amber transition-colors disabled:opacity-50"
          >
            {txLoading ? 'Processing…' : 'Set Exchange Rate'}
          </button>
        </form>
      </div>

      <div className="p-4 bg-brand-dark border border-brand-border rounded-xl text-xs text-brand-muted space-y-1">
        <p>• Default rate is 10000 bps (100% of USD = 1:1).</p>
        <p>• EURC at 1.08 USD would be entered as 108 (= 10800 bps internally).</p>
        <p>• Rates are used for display/reporting only; pool accounting stays in native token units.</p>
      </div>
    </div>
  );
}
