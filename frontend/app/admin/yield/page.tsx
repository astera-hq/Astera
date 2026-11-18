'use client';

import { useEffect, useMemo, useState } from 'react';
import { useTranslations } from 'next-intl';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { Skeleton } from '@/components/Skeleton';
import { RateCurveChart } from '@/components/analytics';
import { validateRateModelConfig } from '@/lib/rate-model';
import { NETWORK, stablecoinLabel } from '@/lib/stellar';
import type { RateModelConfig } from '@/lib/types';
import {
  getPoolConfig,
  buildSetYieldTx,
  buildSetFactoringFeeTx,
  submitTx,
  getAcceptedTokens,
  getRateModelConfig,
  buildProposeRateModelTx,
} from '@/lib/contracts';

export default function AdminYieldPage() {
  const t = useTranslations('Admin.yield');
  const { wallet, poolConfig, setPoolConfig } = useStore();
  const [newYield, setNewYield] = useState('');
  const [newFactoringFee, setNewFactoringFee] = useState('');
  const [loading, setLoading] = useState(false);
  const [txLoading, setTxLoading] = useState(false);

  // #863: rate-model curve editor state (values stored as percent strings,
  // converted to bps on submit).
  const [rateTokens, setRateTokens] = useState<string[]>([]);
  const [rateToken, setRateToken] = useState('');
  const [currentModel, setCurrentModel] = useState<RateModelConfig | null>(null);
  const [baseRate, setBaseRate] = useState('2');
  const [kinkUtil, setKinkUtil] = useState('80');
  const [slope1, setSlope1] = useState('6');
  const [slope2, setSlope2] = useState('24');
  const [maxRate, setMaxRate] = useState('50');

  useEffect(() => {
    async function load() {
      setLoading(true);
      try {
        const config = await getPoolConfig();
        setPoolConfig(config);
        setNewYield((config.yieldBps / 100).toString());
        setNewFactoringFee((config.factoringFeeBps / 100).toString());
        const tokens = await getAcceptedTokens();
        setRateTokens(tokens);
        if (tokens.length > 0) setRateToken((prev) => prev || tokens[0] || '');
      } catch (e) {
        console.error(e);
      } finally {
        setLoading(false);
      }
    }
    load();
  }, [setPoolConfig]);

  // Load the selected token's current curve (if any) into the editor.
  useEffect(() => {
    if (!rateToken) return;
    let cancelled = false;
    getRateModelConfig(rateToken)
      .then((model) => {
        if (cancelled) return;
        setCurrentModel(model);
        if (model) {
          setBaseRate((model.baseRateBps / 100).toString());
          setKinkUtil((model.optimalUtilizationBps / 100).toString());
          setSlope1((model.slope1Bps / 100).toString());
          setSlope2((model.slope2Bps / 100).toString());
          setMaxRate((model.maxRateBps / 100).toString());
        }
      })
      .catch(() => {
        if (!cancelled) setCurrentModel(null);
      });
    return () => {
      cancelled = true;
    };
  }, [rateToken]);

  // Live preview of the draft curve before submitting the timelocked proposal.
  const draftModel = useMemo<RateModelConfig>(
    () => ({
      baseRateBps: Math.round(parseFloat(baseRate || '0') * 100),
      optimalUtilizationBps: Math.round(parseFloat(kinkUtil || '0') * 100),
      slope1Bps: Math.round(parseFloat(slope1 || '0') * 100),
      slope2Bps: Math.round(parseFloat(slope2 || '0') * 100),
      maxRateBps: Math.round(parseFloat(maxRate || '0') * 100),
    }),
    [baseRate, kinkUtil, slope1, slope2, maxRate],
  );
  const draftError = useMemo(
    () =>
      validateRateModelConfig(
        draftModel,
        t as (key: string, values?: Record<string, unknown>) => string,
      ),
    [draftModel, t],
  );

  if (loading) {
    return (
      <div className="space-y-6">
        <Skeleton className="h-10 w-48 rounded-lg" />
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
          <Skeleton className="h-48 rounded-2xl" />
          <Skeleton className="h-48 rounded-2xl" />
        </div>
      </div>
    );
  }

  async function signAndSubmit(xdr: string) {
    const freighter = await import('@stellar/freighter-api');
    const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
      networkPassphrase: NETWORK,
      address: wallet.address!,
    });

    if (signError) throw new Error(signError.message || 'Signing rejected.');
    await submitTx(signedTxXdr);
  }

  async function handleYieldSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!wallet.address) return;

    const bps = Math.round(parseFloat(newYield) * 100);
    if (isNaN(bps) || bps < 0 || bps > 5000) {
      toast.error(t('yieldRangeError'));
      return;
    }

    setTxLoading(true);

    try {
      const xdr = await buildSetYieldTx(wallet.address, bps);
      await signAndSubmit(xdr);
      toast.success(t('yieldUpdated', { yield: newYield, bps: bps.toString() }));

      const updatedConfig = await getPoolConfig();
      setPoolConfig(updatedConfig);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : t('yieldUpdateFailed');
      toast.error(msg);
      console.error(e);
    } finally {
      setTxLoading(false);
    }
  }

  async function handleFactoringFeeSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!wallet.address) return;

    const bps = Math.round(parseFloat(newFactoringFee) * 100);
    if (isNaN(bps) || bps < 0 || bps > 10000) {
      toast.error(t('feeRangeError'));
      return;
    }

    setTxLoading(true);

    try {
      const xdr = await buildSetFactoringFeeTx(wallet.address, bps);
      await signAndSubmit(xdr);
      toast.success(t('feeUpdated', { fee: newFactoringFee, bps: bps.toString() }));

      const updatedConfig = await getPoolConfig();
      setPoolConfig(updatedConfig);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : t('feeUpdateFailed');
      toast.error(msg);
      console.error(e);
    } finally {
      setTxLoading(false);
    }
  }

  // #863: propose new curve parameters — takes effect only after the
  // on-chain timelock (48h default) via execute_rate_model_change.
  async function handleRateModelSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!wallet.address || !rateToken) return;

    if (draftError) {
      toast.error(draftError);
      return;
    }

    setTxLoading(true);

    try {
      const xdr = await buildProposeRateModelTx(wallet.address, rateToken, draftModel);
      await signAndSubmit(xdr);
      toast.success(t('rateModelProposed', { token: stablecoinLabel(rateToken) }));

      const model = await getRateModelConfig(rateToken);
      setCurrentModel(model);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : t('rateModelProposalFailed');
      toast.error(msg);
      console.error(e);
    } finally {
      setTxLoading(false);
    }
  }

  return (
    <div className="max-w-4xl space-y-8">
      <div>
        <h1 className="text-3xl font-bold mb-2">{t('title')}</h1>
        <p className="text-brand-muted text-sm">{t('description')}</p>
      </div>

      <div className="p-8 bg-brand-card border border-brand-border rounded-2xl shadow-sm">
        <label className="block text-sm font-semibold text-brand-muted mb-6 uppercase tracking-wider">
          {t('currentConfig')}
        </label>

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 mb-8">
          <div className="p-4 bg-brand-dark rounded-xl border border-brand-border">
            <p className="text-xs text-brand-muted mb-1">{t('currentYield')}</p>
            <p className="text-2xl font-bold text-white">
              {loading ? '...' : ((poolConfig?.yieldBps ?? 0) / 100).toFixed(2)}%
            </p>
          </div>
          <div className="p-4 bg-brand-dark rounded-xl border border-brand-border">
            <p className="text-xs text-brand-muted mb-1">{t('currentFactoringFee')}</p>
            <p className="text-2xl font-bold text-brand-gold">
              {loading ? '...' : ((poolConfig?.factoringFeeBps ?? 0) / 100).toFixed(2)}%
            </p>
          </div>
        </div>

        <form onSubmit={handleYieldSubmit} className="space-y-6 pt-6 border-t border-brand-border">
          <div>
            <label className="block text-sm font-medium text-white mb-2">{t('newYieldRate')}</label>
            <div className="relative">
              <input
                type="number"
                step="0.01"
                min="0"
                max="50"
                value={newYield}
                onChange={(e) => setNewYield(e.target.value)}
                className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-3 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-lg"
                placeholder={t('yieldPlaceholder')}
                required
              />
              <span className="absolute right-4 top-1/2 -translate-y-1/2 text-brand-muted font-bold">
                %
              </span>
            </div>
            <p className="mt-2 text-xs text-brand-muted">{t('yieldExample')}</p>
          </div>

          <button
            type="submit"
            disabled={txLoading || loading}
            className="w-full py-4 bg-brand-gold text-brand-dark font-bold rounded-xl hover:bg-brand-amber transition-all shadow-lg active:scale-[0.98] disabled:opacity-50"
          >
            {txLoading ? t('updatingRate') : t('updateYield')}
          </button>
        </form>

        <form
          onSubmit={handleFactoringFeeSubmit}
          className="space-y-6 pt-6 mt-6 border-t border-brand-border"
        >
          <div>
            <label className="block text-sm font-medium text-white mb-2">
              {t('newFactoringFee')}
            </label>
            <div className="relative">
              <input
                type="number"
                step="0.01"
                min="0"
                max="100"
                value={newFactoringFee}
                onChange={(e) => setNewFactoringFee(e.target.value)}
                className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-3 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-lg"
                placeholder={t('feePlaceholder')}
                required
              />
              <span className="absolute right-4 top-1/2 -translate-y-1/2 text-brand-muted font-bold">
                %
              </span>
            </div>
            <p className="mt-2 text-xs text-brand-muted">{t('factoringFeeDesc')}</p>
          </div>

          <button
            type="submit"
            disabled={txLoading || loading}
            className="w-full py-4 bg-white text-brand-dark font-bold rounded-xl hover:bg-stone-200 transition-all shadow-lg active:scale-[0.98] disabled:opacity-50"
          >
            {txLoading ? t('updatingFee') : t('updateFactoringFee')}
          </button>
        </form>
      </div>

      {/* #863: utilization-driven rate model editor */}
      <div className="p-8 bg-brand-card border border-brand-border rounded-2xl shadow-sm">
        <div className="mb-6">
          <h2 className="text-xl font-bold text-white mb-1">{t('rateCurve')}</h2>
          <p className="text-brand-muted text-sm">{t('rateCurveDesc')}</p>
        </div>

        {rateTokens.length === 0 ? (
          <p className="text-brand-muted text-sm">{t('noTokens')}</p>
        ) : (
          <form onSubmit={handleRateModelSubmit} className="space-y-6">
            <div>
              <label className="block text-sm font-medium text-white mb-2">{t('token')}</label>
              <select
                value={rateToken}
                onChange={(e) => setRateToken(e.target.value)}
                className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-3 text-white focus:outline-none focus:border-brand-gold"
              >
                {rateTokens.map((tok) => (
                  <option key={tok} value={tok}>
                    {stablecoinLabel(tok)}
                  </option>
                ))}
              </select>
              <p className="mt-2 text-xs text-brand-muted">
                {currentModel ? t('hasCurve') : t('noCurve')}
              </p>
            </div>

            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
              {(
                [
                  [t('baseRate'), baseRate, setBaseRate, t('baseRateHint')],
                  [t('kinkUtil'), kinkUtil, setKinkUtil, t('kinkUtilHint')],
                  [t('slope1'), slope1, setSlope1, t('slope1Hint')],
                  [t('slope2'), slope2, setSlope2, t('slope2Hint')],
                  [t('maxRate'), maxRate, setMaxRate, t('maxRateHint')],
                ] as const
              ).map(([label, value, setter, hint]) => (
                <div key={label}>
                  <label className="block text-sm font-medium text-white mb-2">{label}</label>
                  <input
                    type="number"
                    step="0.01"
                    min="0"
                    value={value}
                    onChange={(e) => setter(e.target.value)}
                    className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-3 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold"
                    required
                  />
                  <p className="mt-1 text-xs text-brand-muted">{hint}</p>
                </div>
              ))}
            </div>

            {draftError ? (
              <p className="text-sm text-red-400">{draftError}</p>
            ) : (
              <RateCurveChart config={draftModel} title={t('proposalPreview')} />
            )}

            <button
              type="submit"
              disabled={txLoading || loading || !rateToken || Boolean(draftError)}
              className="w-full py-4 bg-brand-gold text-brand-dark font-bold rounded-xl hover:bg-brand-amber transition-all shadow-lg active:scale-[0.98] disabled:opacity-50"
            >
              {txLoading ? t('submittingProposal') : t('proposeChange')}
            </button>
            <p className="text-xs text-brand-muted">{t('proposalNote')}</p>
          </form>
        )}
      </div>

      <div className="p-6 bg-brand-dark border border-brand-border rounded-2xl text-xs text-brand-muted space-y-2">
        <p className="font-bold text-white mb-1 uppercase tracking-tighter">
          {t('safetyControls')}
        </p>
        <p>• {t('safety1')}</p>
        <p>• {t('safety2')}</p>
        <p>• {t('safety3')}</p>
        <p>• {t('safety4')}</p>
        <p>• {t('safety5')}</p>
        <p>• {t('safety6')}</p>
      </div>
    </div>
  );
}
