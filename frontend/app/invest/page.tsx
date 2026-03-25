'use client';

import { useEffect, useState } from 'react';
import { useStore } from '@/lib/store';
import PoolStats from '@/components/PoolStats';
import { APYCalculator } from '@/components/APYCalculator';
import {
  getPoolConfig,
  getInvestorPosition,
  buildDepositTx,
  buildWithdrawTx,
  submitTx,
} from '@/lib/contracts';
import { toStroops, formatUSDC } from '@/lib/stellar';

export default function InvestPage() {
  const { wallet, poolConfig, setPoolConfig, position, setPosition } = useStore();
  const [amount, setAmount] = useState('');
  const [mode, setMode] = useState<'deposit' | 'withdraw'>('deposit');
  const [loading, setLoading] = useState(false);
  const [txLoading, setTxLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  useEffect(() => {
    loadPool();
  }, []);

  useEffect(() => {
    if (wallet.connected && wallet.address) {
      loadPosition(wallet.address);
    }
  }, [wallet.address, wallet.connected]);

  async function loadPool() {
    setLoading(true);
    try {
      const config = await getPoolConfig();
      setPoolConfig(config);
    } catch (e) {
      // Pool not deployed yet — show placeholder
      console.error(e);
    } finally {
      setLoading(false);
    }
  }

  async function loadPosition(addr: string) {
    try {
      const pos = await getInvestorPosition(addr);
      setPosition(pos);
    } catch (e) {
      console.error(e);
    }
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!wallet.address || !amount) return;

    setTxLoading(true);
    setError(null);
    setSuccess(null);

    try {
      const stroops = toStroops(parseFloat(amount));

      const xdr =
        mode === 'deposit'
          ? await buildDepositTx(wallet.address, stroops)
          : await buildWithdrawTx(wallet.address, stroops);

      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
        networkPassphrase: 'Test SDF Network ; September 2015',
        address: wallet.address,
      });
      if (signError) throw new Error(signError.message);

      await submitTx(signedTxXdr);
      setSuccess(
        `${mode === 'deposit' ? 'Deposited' : 'Withdrew'} ${formatUSDC(stroops)} successfully.`,
      );
      setAmount('');
      await loadPool();
      await loadPosition(wallet.address);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Transaction failed.';
      setError(msg);
    } finally {
      setTxLoading(false);
    }
  }

  return (
    <div className="min-h-screen pt-24 pb-16 px-6">
      <div className="max-w-5xl mx-auto">
        {/* Header */}
        <div className="mb-8">
          <h1 className="text-3xl font-bold mb-1">Invest</h1>
          <p className="text-brand-muted">
            Deposit USDC into the Astera pool. Earn yield backed by real invoice repayments.
          </p>
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Left: Pool stats */}
          <div className="space-y-6">
            {loading ? (
              <div className="h-64 bg-brand-card border border-brand-border rounded-2xl animate-pulse" />
            ) : poolConfig ? (
              <PoolStats config={poolConfig} />
            ) : (
              <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-brand-muted text-sm">
                Pool not deployed yet. Deploy contracts to see live data.
              </div>
            )}

            <APYCalculator yieldBps={poolConfig?.yieldBps ?? null} loading={loading} />

            {/* Investor position */}
            {wallet.connected && position && (
              <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
                <h2 className="text-lg font-semibold mb-4">Your Position</h2>
                <div className="space-y-3">
                  {[
                    { label: 'Total Deposited', value: formatUSDC(position.deposited) },
                    {
                      label: 'Available to Withdraw',
                      value: formatUSDC(position.available),
                      highlight: true,
                    },
                    { label: 'Currently Deployed', value: formatUSDC(position.deployed) },
                    { label: 'Total Earned', value: formatUSDC(position.earned), highlight: true },
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

          {/* Right: Deposit / Withdraw form */}
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl h-fit">
            {!wallet.connected ? (
              <div className="text-center py-12">
                <p className="text-brand-muted">Connect your wallet to invest.</p>
              </div>
            ) : (
              <>
                <div className="flex rounded-xl overflow-hidden border border-brand-border mb-6">
                  {(['deposit', 'withdraw'] as const).map((m) => (
                    <button
                      key={m}
                      onClick={() => {
                        setMode(m);
                        setError(null);
                        setSuccess(null);
                      }}
                      className={`flex-1 py-2.5 text-sm font-medium capitalize transition-colors ${
                        mode === m
                          ? 'bg-brand-gold text-brand-dark'
                          : 'text-brand-muted hover:text-white'
                      }`}
                    >
                      {m}
                    </button>
                  ))}
                </div>

                <form onSubmit={handleSubmit} className="space-y-4">
                  <div>
                    <label className="block text-sm text-brand-muted mb-2">Amount (USDC)</label>
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
                      <span className="absolute right-4 top-1/2 -translate-y-1/2 text-brand-muted text-sm font-medium">
                        USDC
                      </span>
                    </div>
                    {mode === 'withdraw' && position && (
                      <p className="text-xs text-brand-muted mt-1">
                        Available: {formatUSDC(position.available)}
                      </p>
                    )}
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
                    disabled={txLoading || !amount}
                    className="w-full py-3 bg-brand-gold text-brand-dark font-semibold rounded-xl hover:bg-brand-amber transition-colors disabled:opacity-60 capitalize"
                  >
                    {txLoading ? 'Processing...' : `${mode} USDC`}
                  </button>
                </form>

                <div className="mt-6 p-4 bg-brand-dark border border-brand-border rounded-xl text-xs text-brand-muted space-y-1">
                  <p>• Your USDC is deployed to fund verified SME invoices.</p>
                  <p>• You earn yield when invoices are repaid.</p>
                  <p>• Only undeployed funds can be withdrawn at any time.</p>
                </div>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
