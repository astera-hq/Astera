'use client';

import { useCallback, useEffect, useState } from 'react';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { Skeleton } from '@/components/Skeleton';
import { parseStellarAddress } from '@/lib/types';
import type { CoFundingRound } from '@/lib/types';
import { toStroops, formatUSDC } from '@/lib/stellar';
import {
  listCoFundingRounds,
  getCoFundingRound,
  getInvestorCoFundPositions,
  buildCommitToInvoiceTx,
  buildWithdrawCoFundingCommitmentTx,
  buildTransferCoFundShareTx,
  submitTx,
} from '@/lib/contracts';

const STATUS_STYLES: Record<CoFundingRound['status'], string> = {
  Open: 'bg-blue-500/20 text-blue-400 border-blue-500/30',
  Filled: 'bg-green-500/20 text-green-400 border-green-500/30',
  Cancelled: 'bg-brand-dark text-brand-muted border-brand-border',
  Expired: 'bg-amber-500/20 text-amber-400 border-amber-500/30',
};

export default function CoFundingPage() {
  const { wallet } = useStore();
  const [rounds, setRounds] = useState<CoFundingRound[]>([]);
  const [loading, setLoading] = useState(true);
  const [txLoading, setTxLoading] = useState(false);

  const [commitAmounts, setCommitAmounts] = useState<Record<number, string>>({});

  const [positions, setPositions] = useState<Array<{ invoiceId: number; bps: number }>>([]);
  const [positionsLoading, setPositionsLoading] = useState(false);

  const [transferTarget, setTransferTarget] = useState<number | null>(null);
  const [transferTo, setTransferTo] = useState('');
  const [transferBps, setTransferBps] = useState('10000');

  const loadRounds = useCallback(async () => {
    setLoading(true);
    try {
      const ids = await listCoFundingRounds();
      const loaded = await Promise.all(ids.map((id) => getCoFundingRound(id)));
      setRounds(loaded.filter((r): r is CoFundingRound => r !== null));
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  }, []);

  const loadPositions = useCallback(async () => {
    if (!wallet.address) {
      setPositions([]);
      return;
    }
    setPositionsLoading(true);
    try {
      const pos = await getInvestorCoFundPositions(wallet.address);
      setPositions(pos);
    } catch (e) {
      console.error(e);
    } finally {
      setPositionsLoading(false);
    }
  }, [wallet.address]);

  useEffect(() => {
    loadRounds();
  }, [loadRounds]);

  useEffect(() => {
    loadPositions();
  }, [loadPositions]);

  async function signAndSubmit(xdr: string) {
    const freighter = await import('@stellar/freighter-api');
    const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
      networkPassphrase: 'Test SDF Network ; September 2015',
      address: wallet.address!,
    });
    if (signError) throw new Error(signError.message || 'Signing rejected.');
    await submitTx(signedTxXdr);
  }

  async function handleCommit(round: CoFundingRound) {
    if (!wallet.address) return;
    const raw = commitAmounts[round.invoiceId];
    const amountNum = Number(raw);
    if (!raw || !Number.isFinite(amountNum) || amountNum <= 0) {
      toast.error('Enter a valid commit amount.');
      return;
    }
    setTxLoading(true);
    try {
      const investor = parseStellarAddress(wallet.address);
      const xdr = await buildCommitToInvoiceTx({
        investor,
        invoiceId: round.invoiceId,
        amount: toStroops(amountNum),
      });
      await signAndSubmit(xdr);
      toast.success(`Committed to invoice #${round.invoiceId}.`);
      setCommitAmounts((prev) => ({ ...prev, [round.invoiceId]: '' }));
      await Promise.all([loadRounds(), loadPositions()]);
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Transaction failed.');
    } finally {
      setTxLoading(false);
    }
  }

  async function handleWithdrawCommitment(invoiceId: number) {
    if (!wallet.address) return;
    setTxLoading(true);
    try {
      const investor = parseStellarAddress(wallet.address);
      const xdr = await buildWithdrawCoFundingCommitmentTx({ investor, invoiceId });
      await signAndSubmit(xdr);
      toast.success(`Withdrew commitment from invoice #${invoiceId}.`);
      await Promise.all([loadRounds(), loadPositions()]);
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Transaction failed.');
    } finally {
      setTxLoading(false);
    }
  }

  async function handleTransfer(e: React.FormEvent) {
    e.preventDefault();
    if (!wallet.address || transferTarget === null) return;
    const bps = Number(transferBps);
    if (!Number.isFinite(bps) || bps <= 0 || bps > 10_000) {
      toast.error('Bps must be between 1 and 10000.');
      return;
    }
    setTxLoading(true);
    try {
      const from = parseStellarAddress(wallet.address);
      const to = parseStellarAddress(transferTo.trim());
      const round = rounds.find((r) => r.invoiceId === transferTarget);
      if (!round) throw new Error('Round not found.');
      const xdr = await buildTransferCoFundShareTx({
        from,
        invoiceId: transferTarget,
        token: round.token,
        to,
        bps,
      });
      await signAndSubmit(xdr);
      toast.success(`Transferred ${(bps / 100).toFixed(2)}% of invoice #${transferTarget}.`);
      setTransferTarget(null);
      setTransferTo('');
      await loadPositions();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Transaction failed.');
    } finally {
      setTxLoading(false);
    }
  }

  const openRounds = rounds.filter((r) => r.status === 'Open');

  return (
    <div className="max-w-5xl space-y-8">
      <div>
        <h1 className="text-3xl font-bold mb-2">Co-Funding Rounds</h1>
        <p className="text-brand-muted text-sm">
          Commit capital toward a specific invoice alongside other investors. Every co-funder
          ranks pari passu and owns a proportional slice of that invoice&apos;s principal and
          interest — separate from the general pool position, and tradeable once the round is
          filled.
        </p>
      </div>

      {/* Open rounds */}
      <div className="space-y-4">
        <h2 className="text-xl font-bold">Open Rounds</h2>
        {loading ? (
          <Skeleton className="h-40 w-full rounded-2xl" />
        ) : openRounds.length === 0 ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">
            No open co-funding rounds right now.
          </div>
        ) : (
          <div className="space-y-4">
            {openRounds.map((round) => {
              const pct =
                round.targetPrincipal > 0n
                  ? Number((round.committedPrincipal * 10_000n) / round.targetPrincipal) / 100
                  : 0;
              return (
                <div
                  key={round.invoiceId}
                  className="p-6 bg-brand-card border border-brand-border rounded-2xl space-y-4"
                >
                  <div className="flex items-center justify-between">
                    <div>
                      <p className="font-semibold">Invoice #{round.invoiceId}</p>
                      <p className="text-xs text-brand-muted mt-0.5">
                        Deadline: {new Date(round.fundingDeadline * 1000).toLocaleString()}
                      </p>
                    </div>
                    <span
                      className={`px-3 py-1 rounded-full text-xs font-semibold border ${STATUS_STYLES[round.status]}`}
                    >
                      {round.status}
                    </span>
                  </div>

                  <div>
                    <div className="flex justify-between text-xs text-brand-muted mb-1">
                      <span>
                        {formatUSDC(round.committedPrincipal)} / {formatUSDC(round.targetPrincipal)}
                      </span>
                      <span>{pct.toFixed(1)}%</span>
                    </div>
                    <div className="h-2 bg-brand-dark rounded-full overflow-hidden">
                      <div
                        className="h-full bg-brand-gold transition-all"
                        style={{ width: `${Math.min(pct, 100)}%` }}
                      />
                    </div>
                  </div>

                  <form
                    onSubmit={(e) => {
                      e.preventDefault();
                      handleCommit(round);
                    }}
                    className="flex gap-3"
                  >
                    <input
                      type="number"
                      min="0"
                      step="0.01"
                      placeholder="Amount (USDC)"
                      value={commitAmounts[round.invoiceId] ?? ''}
                      onChange={(e) =>
                        setCommitAmounts((prev) => ({
                          ...prev,
                          [round.invoiceId]: e.target.value,
                        }))
                      }
                      disabled={!wallet.connected || txLoading}
                      className="flex-1 bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm disabled:opacity-50"
                    />
                    <button
                      type="submit"
                      disabled={!wallet.connected || txLoading}
                      className="px-5 py-2.5 bg-brand-gold text-brand-dark rounded-xl text-sm font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50"
                    >
                      Commit
                    </button>
                  </form>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* My positions */}
      <div className="space-y-4">
        <h2 className="text-xl font-bold">My Co-Funding Positions</h2>
        {!wallet.connected ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">
            Connect your wallet to see your positions.
          </div>
        ) : positionsLoading ? (
          <Skeleton className="h-24 w-full rounded-2xl" />
        ) : positions.length === 0 ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">
            You have no co-funding positions yet.
          </div>
        ) : (
          <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden">
            <table className="w-full text-left text-sm">
              <thead className="bg-brand-dark border-b border-brand-border text-brand-muted">
                <tr>
                  <th className="px-6 py-4 font-medium">Invoice</th>
                  <th className="px-6 py-4 font-medium">Share</th>
                  <th className="px-6 py-4 font-medium">Round Status</th>
                  <th className="px-6 py-4 font-medium text-right">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-brand-border">
                {positions.map((pos) => {
                  const round = rounds.find((r) => r.invoiceId === pos.invoiceId);
                  return (
                    <tr key={pos.invoiceId} className="hover:bg-brand-dark/50 transition-colors">
                      <td className="px-6 py-4">#{pos.invoiceId}</td>
                      <td className="px-6 py-4">{(pos.bps / 100).toFixed(2)}%</td>
                      <td className="px-6 py-4">
                        {round ? (
                          <span
                            className={`px-2 py-0.5 rounded-full text-xs font-semibold border ${STATUS_STYLES[round.status]}`}
                          >
                            {round.status}
                          </span>
                        ) : (
                          <span className="text-brand-muted text-xs">—</span>
                        )}
                      </td>
                      <td className="px-6 py-4 text-right space-x-2">
                        {round?.status === 'Open' && (
                          <button
                            onClick={() => handleWithdrawCommitment(pos.invoiceId)}
                            disabled={txLoading}
                            className="px-3 py-1.5 bg-red-500/20 text-red-400 border border-red-500/30 rounded-lg text-xs font-semibold hover:bg-red-500/30 transition-colors disabled:opacity-50"
                          >
                            Withdraw
                          </button>
                        )}
                        {round?.status === 'Filled' && (
                          <button
                            onClick={() => {
                              setTransferTarget(pos.invoiceId);
                              setTransferBps(String(pos.bps));
                            }}
                            disabled={txLoading}
                            className="px-3 py-1.5 bg-brand-dark border border-brand-border rounded-lg text-xs font-semibold hover:bg-brand-border transition-colors disabled:opacity-50"
                          >
                            Transfer
                          </button>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Transfer form */}
      {transferTarget !== null && (
        <div className="p-6 bg-brand-card border border-brand-border rounded-2xl space-y-4">
          <div className="flex items-center justify-between">
            <h3 className="font-semibold">Transfer share in invoice #{transferTarget}</h3>
            <button
              onClick={() => setTransferTarget(null)}
              className="text-brand-muted hover:text-white text-sm"
            >
              Cancel
            </button>
          </div>
          <form onSubmit={handleTransfer} className="space-y-3">
            <div>
              <label className="block text-sm text-brand-muted mb-1">Recipient Address</label>
              <input
                type="text"
                value={transferTo}
                onChange={(e) => setTransferTo(e.target.value)}
                placeholder="G..."
                required
                className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold font-mono text-sm"
              />
            </div>
            <div>
              <label className="block text-sm text-brand-muted mb-1">
                Amount (basis points, 1-10000)
              </label>
              <input
                type="number"
                min={1}
                max={10000}
                value={transferBps}
                onChange={(e) => setTransferBps(e.target.value)}
                className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white focus:outline-none focus:border-brand-gold text-sm"
              />
            </div>
            <button
              type="submit"
              disabled={txLoading}
              className="w-full py-2.5 bg-brand-gold text-brand-dark rounded-xl text-sm font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50"
            >
              {txLoading ? 'Processing…' : 'Transfer Share'}
            </button>
          </form>
        </div>
      )}

      <div className="p-4 bg-brand-dark border border-brand-border rounded-xl text-xs text-brand-muted space-y-1">
        <p>• Commits above the remaining target are automatically clamped, not rejected.</p>
        <p>• Withdrawing a commitment returns 100% of your committed principal.</p>
        <p>• Shares can only be transferred once a round is Filled.</p>
      </div>
    </div>
  );
}
