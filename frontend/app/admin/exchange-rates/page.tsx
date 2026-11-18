'use client';

import { useEffect, useState } from 'react';
import { useTranslations } from 'next-intl';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { Skeleton } from '@/components/Skeleton';
import ConfirmActionModal from '@/components/ConfirmActionModal';
import {
  getAcceptedTokens,
  getExchangeRate,
  buildSetExchangeRateTx,
  submitTx,
} from '@/lib/contracts';
import { NETWORK, stablecoinLabel } from '@/lib/stellar';

export default function AdminExchangeRatesPage() {
  const t = useTranslations('Admin.exchangeRates');
  const { wallet } = useStore();
  const [tokens, setTokens] = useState<string[]>([]);
  const [rates, setRates] = useState<Record<string, number>>({});
  const [loading, setLoading] = useState(true);
  const [txLoading, setTxLoading] = useState(false);
  const [selectedToken, setSelectedToken] = useState('');
  const [newRatePct, setNewRatePct] = useState('');
  const [showReviewModal, setShowReviewModal] = useState(false);
  const [pendingRate, setPendingRate] = useState<{
    token: string;
    pct: string;
    bps: number;
  } | null>(null);

  useEffect(() => {
    async function load() {
      setLoading(true);
      try {
        const accepted = await getAcceptedTokens();
        setTokens(accepted);
        if (accepted.length > 0) setSelectedToken(accepted[0]!);
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
      networkPassphrase: NETWORK,
      address: wallet.address!,
    });
    if (signError) throw new Error(signError.message || 'Signing rejected.');
    await submitTx(signedTxXdr);
  }

  function openReviewModal() {
    if (!wallet.address || !selectedToken || !newRatePct) return;

    const bps = Math.round(parseFloat(newRatePct) * 100);
    if (isNaN(bps) || bps <= 0) {
      toast.error(t('rateMustBePositive'));
      return;
    }

    setPendingRate({ token: selectedToken, pct: newRatePct, bps });
    setShowReviewModal(true);
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    openReviewModal();
  }

  async function confirmAndSubmit() {
    if (!wallet.address || !pendingRate) return;

    const { token, pct, bps } = pendingRate;
    setShowReviewModal(false);
    setTxLoading(true);
    try {
      const xdr = await buildSetExchangeRateTx(wallet.address, token, bps);
      await signAndSubmit(xdr);
      setRates((prev) => ({ ...prev, [token]: bps }));
      toast.success(t('success', { token: stablecoinLabel(token), pct, bps: bps.toString() }));
      setNewRatePct('');
      setPendingRate(null);
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('transactionFailed'));
    } finally {
      setTxLoading(false);
    }
  }

  return (
    <div className="max-w-2xl space-y-8">
      <div>
        <h1 className="text-3xl font-bold mb-2">{t('title')}</h1>
        <p className="text-brand-muted text-sm">{t('description')}</p>
      </div>

      {/* Current rates */}
      <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
        <h2 className="font-semibold mb-4">{t('currentRates')}</h2>
        {loading ? (
          <div className="space-y-3">
            <Skeleton className="h-12 w-full" />
            <Skeleton className="h-12 w-full" />
            <Skeleton className="h-12 w-full" />
          </div>
        ) : tokens.length === 0 ? (
          <p className="text-brand-muted text-sm">{t('noTokens')}</p>
        ) : (
          <div className="space-y-2">
            {tokens.map((tok) => (
              <div
                key={tok}
                className="flex items-center justify-between p-3 bg-brand-dark rounded-xl border border-brand-border"
              >
                <span className="font-medium">{stablecoinLabel(tok)}</span>
                <span className="text-brand-gold font-semibold text-sm">
                  {rates[tok] !== undefined
                    ? t('rateDisplay', { rate: (rates[tok] / 100).toFixed(4) })
                    : t('rateUnknown')}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Update rate form */}
      <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
        <h2 className="font-semibold mb-4">{t('updateRate')}</h2>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm text-brand-muted mb-1">{t('token')}</label>
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
            <label className="block text-sm text-brand-muted mb-1">{t('rateLabel')}</label>
            <input
              type="number"
              step="0.01"
              min="0.01"
              value={newRatePct}
              onChange={(e) => setNewRatePct(e.target.value)}
              placeholder={t('ratePlaceholder')}
              required
              className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-3 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold"
            />
          </div>

          <button
            type="submit"
            disabled={txLoading || tokens.length === 0}
            className="w-full py-3 bg-brand-gold text-brand-dark font-semibold rounded-xl hover:bg-brand-amber transition-colors disabled:opacity-50"
          >
            {txLoading ? t('processing') : t('setRate')}
          </button>
        </form>
      </div>

      <div className="p-4 bg-brand-dark border border-brand-border rounded-xl text-xs text-brand-muted space-y-1">
        <p>• {t('note1')}</p>
        <p>• {t('note2')}</p>
        <p>• {t('note3')}</p>
      </div>

      <ConfirmActionModal
        title={t('reviewTitle')}
        description={t('reviewDesc', {
          token: stablecoinLabel(pendingRate?.token ?? selectedToken),
        })}
        isOpen={showReviewModal}
        onConfirm={() => void confirmAndSubmit()}
        onCancel={() => {
          setShowReviewModal(false);
          setPendingRate(null);
        }}
        confirmLabel={t('confirmSubmit')}
        cancelLabel={t('back')}
      >
        {pendingRate && (
          <div className="space-y-3 rounded-xl border border-brand-border bg-brand-dark p-4 text-sm text-brand-muted">
            <div className="flex items-center justify-between">
              <span>{t('token')}</span>
              <span className="font-medium text-white">{stablecoinLabel(pendingRate.token)}</span>
            </div>
            <div className="flex items-center justify-between">
              <span>{t('rateEntered')}</span>
              <span className="font-medium text-white">
                {t('rateEnteredValue', { pct: pendingRate.pct })}
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span>{t('internalBps')}</span>
              <span className="font-medium text-white">
                {t('internalBpsValue', { bps: pendingRate.bps })}
              </span>
            </div>
          </div>
        )}
      </ConfirmActionModal>
    </div>
  );
}
