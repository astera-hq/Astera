'use client';

import { useEffect, useState, useCallback } from 'react';
import type { FormEvent } from 'react';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { ChartSkeleton } from '@/components/Skeleton';
import PoolStats from '@/components/PoolStats';
import { APYCalculator } from '@/components/APYCalculator';
import {
  getPoolConfig,
  getInvestorPosition,
  getAcceptedTokens,
  getPoolTokenTotals,
  getTokenDepositCap,
  buildDepositTx,
  buildWithdrawTx,
  submitTx,
  getKycRequired,
  getInvestorKyc,
} from '@/lib/contracts';
import { toStroops, formatUSDC, stablecoinLabel, USDC_TOKEN_ID } from '@/lib/stellar';
import type { PoolTokenTotals } from '@/lib/types';
import { useTranslations } from 'next-intl';

export default function InvestPage() {
  const t = useTranslations('Invest');
  const { wallet, poolConfig, setPoolConfig, position, setPosition } = useStore();
  const [amount, setAmount] = useState('');
  const [mode, setMode] = useState<'deposit' | 'withdraw'>('deposit');
  const [loading, setLoading] = useState(false);
  const [txLoading, setTxLoading] = useState(false);
  const [txStatus, setTxStatus] = useState<'idle' | 'pending' | 'confirmed' | 'failed'>('idle');
  const [txHash, setTxHash] = useState<string | null>(null);
  const [txError, setTxError] = useState<string | null>(null);

  const [acceptedTokens, setAcceptedTokens] = useState<string[]>([]);
  const [selectedToken, setSelectedToken] = useState<string>('');
  const [tokenTotals, setTokenTotals] = useState<PoolTokenTotals | null>(null);
  const [tokenDepositCap, setTokenDepositCap] = useState<bigint>(0n);

  // #109: KYC status
  const [kycRequired, setKycRequired] = useState(false);
  const [kycApproved, setKycApproved] = useState(false);

  useEffect(() => {
    loadPool();
  }, []);

  useEffect(() => {
    if (!selectedToken) return;
    loadTokenTotals(selectedToken);
  }, [selectedToken, poolConfig]);

  useEffect(() => {
    if (wallet.connected && wallet.address && selectedToken) {
      loadPosition(wallet.address, selectedToken);
    }
  }, [wallet.address, wallet.connected, selectedToken]);

  useEffect(() => {
    async function loadKyc() {
      try {
        const required = await getKycRequired();
        setKycRequired(required);
        if (required && wallet.address) {
          const approved = await getInvestorKyc(wallet.address);
          setKycApproved(approved);
        } else {
          setKycApproved(true);
        }
      } catch {
        // non-fatal — KYC state is informational
      }
    }
    if (POOL_CONFIGURED) loadKyc();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wallet.address, wallet.connected]);

  function pickDefaultToken(tokens: string[]): string {
    if (tokens.length === 0) return '';
    if (USDC_TOKEN_ID && tokens.includes(USDC_TOKEN_ID)) return USDC_TOKEN_ID;
    return tokens[0];
  }

  async function loadPool() {
    setLoading(true);
    try {
      const [config, tokens] = await Promise.all([getPoolConfig(), getAcceptedTokens()]);
      setPoolConfig(config);
      setAcceptedTokens(tokens);
      setSelectedToken((prev) => {
        if (prev && tokens.includes(prev)) return prev;
        return pickDefaultToken(tokens);
      });
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  }

  async function loadTokenTotals(token: string) {
    if (!POOL_CONFIGURED) return;
    try {
      const [tt, cap] = await Promise.all([getPoolTokenTotals(token), getTokenDepositCap(token)]);
      setTokenTotals(tt);
      setTokenDepositCap(cap);
    } catch {
      setTokenTotals(null);
      setTokenDepositCap(0n);
    }
  }

  async function loadPosition(addr: string, token: string) {
    try {
      const pos = await getInvestorPosition(addr, token);
      setPosition(pos);
    } catch (e) {
      console.error(e);
    }
  }

  const POOL_CONFIGURED = Boolean(process.env.NEXT_PUBLIC_POOL_CONTRACT_ID);
  const remainingTokenCapacity =
    tokenDepositCap > 0n && tokenTotals
      ? tokenDepositCap > tokenTotals.totalDeposited
        ? tokenDepositCap - tokenTotals.totalDeposited
        : 0n
      : null;
  const depositAtCapacity =
    mode === 'deposit' && tokenDepositCap > 0n && tokenTotals
      ? tokenTotals.totalDeposited >= tokenDepositCap
      : false;

  async function submitTransaction() {
    if (!wallet.address || !amount || !selectedToken) return;

    setTxLoading(true);
    setTxStatus('pending');
    setTxHash(null);
    setTxError(null);

    try {
      const stroops = toStroops(parseFloat(amount));

      const xdr =
        mode === 'deposit'
          ? await buildDepositTx(wallet.address, selectedToken, stroops)
          : await buildWithdrawTx(wallet.address, selectedToken, stroops);

      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
        networkPassphrase: 'Test SDF Network ; September 2015',
        address: wallet.address,
      });
      if (signError) throw new Error(signError.message);

      await submitTx(signedTxXdr, (progress) => {
        setTxStatus(progress.status);
        setTxHash(progress.hash);
        setTxError(progress.error ?? null);
      });
      const sym = stablecoinLabel(selectedToken);
      toast.success(
        `${mode === 'deposit' ? 'Deposited' : 'Withdrew'} ${formatUSDC(stroops)} ${sym} successfully.`,
      );
      setTxStatus('confirmed');
      setAmount('');
      await loadPool();
      await loadTokenTotals(selectedToken);
      await loadPosition(wallet.address, selectedToken);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Transaction failed.';
      toast.error(msg);
      setTxStatus('failed');
      setTxError(msg);
    } finally {
      setTxLoading(false);
    }
  }

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    await submitTransaction();
  }

  return (
    <div className="min-h-screen pt-24 pb-16 px-4 sm:px-6">
      <div className="max-w-5xl mx-auto">
        <div className="mb-8">
          <h1 className="text-3xl font-bold mb-1">{t('title')}</h1>
          <p className="text-brand-muted">{t('description')}</p>
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          <div className="space-y-6">
            {loading ? (
              <ChartSkeleton />
            ) : poolConfig ? (
              <PoolStats
                config={poolConfig}
                tokenTotals={tokenTotals}
                tokenLabel={stablecoinLabel(selectedToken || '')}
              />
            ) : (
              <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-brand-muted text-sm">
                {t('poolNotDeployed')}
              </div>
            )}

            {wallet.connected && position && selectedToken && (
              <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
                <h2 className="text-lg font-semibold mb-1">{t('yourPosition')}</h2>
                <p className="text-xs text-brand-muted mb-4">{stablecoinLabel(selectedToken)}</p>
                <div className="space-y-3">
                  {[
                    { label: t('stats.totalDeposited'), value: formatUSDC(position.deposited) },
                    {
                      label: t('stats.availableToWithdraw'),
                      value: formatUSDC(position.available),
                      highlight: true,
                    },
                    { label: t('stats.currentlyDeployed'), value: formatUSDC(position.deployed) },
                    {
                      label: t('stats.totalEarned'),
                      value: formatUSDC(position.earned),
                      highlight: true,
                    },
                  ].map((r) => (
                    <div key={r.label} className="flex justify-between items-center text-sm">
                      <span className="text-brand-muted">{r.label}</span>
                      <span
                        className={`font-semibold ${r.highlight ? 'text-brand-gold' : 'text-white'}`}
                      >
                        {r.value}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>

          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl h-fit">
            {!wallet.connected ? (
              <div className="text-center py-12">
                <p className="text-brand-muted">{t('connectWallet')}</p>
              </div>
            ) : (
              <>
                {/* #109: KYC status banner */}
                {kycRequired && !kycApproved && (
                  <div className="mb-4 p-3 bg-yellow-900/20 border border-yellow-700/40 rounded-xl text-yellow-400 text-xs">
                    {t('kycRequired')}
                  </div>
                )}
                {kycRequired && kycApproved && (
                  <div className="mb-4 p-3 bg-green-900/20 border border-green-700/40 rounded-xl text-green-400 text-xs">
                    {t('kycVerified')}
                  </div>
                )}

                <div className="flex rounded-xl overflow-hidden border border-brand-border mb-6">
                  {(['deposit', 'withdraw'] as const).map((m) => (
                    <button
                      key={m}
                      onClick={() => {
                        setMode(m);
                      }}
                      className={`flex-1 py-2.5 text-sm font-medium capitalize transition-colors ${
                        mode === m
                          ? 'bg-brand-gold text-brand-dark'
                          : 'text-brand-muted hover:text-white'
                      }`}
                    >
                      {t(`modes.${m}`)}
                    </button>
                  ))}
                </div>

                <form onSubmit={handleSubmit} className="space-y-4">
                  <div>
                    <label className="block text-sm text-brand-muted mb-2">{t('stablecoin')}</label>
                    <select
                      value={selectedToken}
                      onChange={(e) => {
                        setSelectedToken(e.target.value);
                      }}
                      disabled={acceptedTokens.length === 0}
                      className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-3 text-white focus:outline-none focus:border-brand-gold"
                    >
                      {acceptedTokens.length === 0 ? (
                        <option value="">{t('noTokens')}</option>
                      ) : (
                        acceptedTokens.map((t) => (
                          <option key={t} value={t}>
                            {stablecoinLabel(t)}
                          </option>
                        ))
                      )}
                    </select>
                  </div>

                  <div>
                    <label className="block text-sm text-brand-muted mb-2">
                      {t('amountLabel', { token: stablecoinLabel(selectedToken) || 'token' })}
                    </label>
                    <div className="relative">
                      <input
                        type="number"
                        min="1"
                        step="0.01"
                        placeholder="0.00"
                        value={amount}
                        onChange={(e) => setAmount(e.target.value)}
                        className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-3 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-lg"
                        required
                      />
                    </div>
                    {mode === 'withdraw' && position && (
                      <p className="text-xs text-brand-muted mt-1">
                        {t('available', {
                          amount: formatUSDC(position.available),
                          token: stablecoinLabel(selectedToken),
                        })}
                      </p>
                    )}
                  </div>

                  {txStatus !== 'idle' && (
                    <div
                      className={`p-4 rounded-xl border text-sm space-y-2 ${
                        txStatus === 'confirmed'
                          ? 'bg-green-900/20 border-green-800/50 text-green-300'
                          : txStatus === 'failed'
                            ? 'bg-red-900/20 border-red-800/50 text-red-300'
                            : 'bg-blue-900/20 border-blue-800/50 text-blue-300'
                      }`}
                    >
                      <div className="flex items-center justify-between gap-3">
                        <span className="font-medium capitalize">{txStatus}</span>
                        {txHash && (
                          <a
                            href={`https://stellar.expert/explorer/testnet/tx/${txHash}`}
                            target="_blank"
                            rel="noreferrer"
                            className="text-xs underline underline-offset-2"
                          >
                            View on explorer
                          </a>
                        )}
                      </div>
                      {txHash && <p className="font-mono text-xs break-all">{txHash}</p>}
                      {txError && txStatus === 'failed' && <p>{txError}</p>}
                    </div>
                  )}

                  <button
                    type="submit"
                    disabled={
                      txLoading ||
                      !amount ||
                      !selectedToken ||
                      (mode === 'deposit' && kycRequired && !kycApproved) ||
                      depositAtCapacity
                    }
                    className="w-full py-3 bg-brand-gold text-brand-dark font-semibold rounded-xl hover:bg-brand-amber transition-colors disabled:opacity-60 capitalize"
                  >
                    {txLoading
                      ? t('processing')
                      : `${t(`modes.${mode}`)} ${stablecoinLabel(selectedToken)}`}
                  </button>
                  {depositAtCapacity && mode === 'deposit' && (
                    <p className="text-xs text-red-300">
                      This token is at its configured deposit cap.
                    </p>
                  )}
                  {txStatus === 'failed' && (
                    <button
                      type="button"
                      onClick={() => void submitTransaction()}
                      disabled={txLoading || !amount || !selectedToken}
                      className="w-full py-3 border border-brand-border text-white font-semibold rounded-xl hover:border-brand-gold/50 transition-colors disabled:opacity-60"
                    >
                      {t('retry')}
                    </button>
                  )}
                </form>

                <div className="mt-6 p-4 bg-brand-dark border border-brand-border rounded-xl text-xs text-brand-muted space-y-1">
                  <p>• {t('notes.stablecoin')}</p>
                  <p>• {t('notes.repayment')}</p>
                  <p>• {t('notes.withdrawal')}</p>
                  {selectedToken && (
                    <p>
                      •{' '}
                      {tokenDepositCap > 0n && remainingTokenCapacity !== null ? (
                        <>
                          {formatUSDC(remainingTokenCapacity)} remaining of{' '}
                          {formatUSDC(tokenDepositCap)} capacity for {stablecoinLabel(selectedToken)}
                        </>
                      ) : (
                        <>No deposit cap configured for {stablecoinLabel(selectedToken)}.</>
                      )}
                    </p>
                  )}
                </div>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
