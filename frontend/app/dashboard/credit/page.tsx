'use client';

import { useCallback, useEffect, useState } from 'react';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { Skeleton } from '@/components/Skeleton';
import { parseStellarAddress } from '@/lib/types';
import type { Attestation, FullCreditScore } from '@/lib/types';
import {
  getFullCreditScore,
  listSmeAttestations,
  simulateScoreWithAttestations,
  buildDisputeAttestationTx,
  submitTx,
  getContractErrorMessage,
} from '@/lib/contracts';

const STATUS_STYLES: Record<Attestation['status'], string> = {
  Active: 'bg-green-500/20 text-green-400 border-green-500/30',
  Disputed: 'bg-amber-500/20 text-amber-400 border-amber-500/30',
  Revoked: 'bg-red-500/20 text-red-400 border-red-500/30',
  Expired: 'bg-brand-dark text-brand-muted border-brand-border',
};

export default function CreditProfilePage() {
  const { wallet } = useStore();
  const [score, setScore] = useState<FullCreditScore | null>(null);
  const [attestations, setAttestations] = useState<Attestation[]>([]);
  const [loading, setLoading] = useState(true);
  const [txLoading, setTxLoading] = useState(false);

  const [disputeTarget, setDisputeTarget] = useState<number | null>(null);
  const [disputeReason, setDisputeReason] = useState('');

  const [simWeightBps, setSimWeightBps] = useState('10000');
  const [simScoreContribution, setSimScoreContribution] = useState('700');
  const [simResult, setSimResult] = useState<number | null>(null);
  const [simLoading, setSimLoading] = useState(false);

  const load = useCallback(async () => {
    if (!wallet.address) {
      setScore(null);
      setAttestations([]);
      setLoading(false);
      return;
    }
    setLoading(true);
    try {
      const [scoreData, attestationData] = await Promise.all([
        getFullCreditScore(wallet.address),
        listSmeAttestations(wallet.address),
      ]);
      setScore(scoreData);
      setAttestations(attestationData);
    } catch (e) {
      console.error(e);
      toast.error('Failed to load credit profile.');
    } finally {
      setLoading(false);
    }
  }, [wallet.address]);

  useEffect(() => {
    load();
  }, [load]);

  async function signAndSubmit(xdr: string) {
    const freighter = await import('@stellar/freighter-api');
    const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
      networkPassphrase: 'Test SDF Network ; September 2015',
      address: wallet.address!,
    });
    if (signError) throw new Error(signError.message || 'Signing rejected.');
    await submitTx(signedTxXdr);
  }

  async function handleSimulate(e: React.FormEvent) {
    e.preventDefault();
    if (!wallet.address) return;
    const weightBps = Number(simWeightBps);
    const scoreContribution = Number(simScoreContribution);
    if (!Number.isFinite(weightBps) || weightBps <= 0 || weightBps > 10_000) {
      toast.error('Weight must be between 1 and 10000 bps.');
      return;
    }
    if (!Number.isFinite(scoreContribution) || scoreContribution < 0 || scoreContribution > 1000) {
      toast.error('Score contribution must be between 0 and 1000.');
      return;
    }
    setSimLoading(true);
    try {
      const result = await simulateScoreWithAttestations(wallet.address, [
        { weightBps, scoreContribution },
      ]);
      setSimResult(result);
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Simulation failed.');
    } finally {
      setSimLoading(false);
    }
  }

  async function handleDispute(e: React.FormEvent) {
    e.preventDefault();
    if (!wallet.address || disputeTarget === null) return;
    if (!disputeReason.trim()) {
      toast.error('Enter a reason for the dispute.');
      return;
    }
    setTxLoading(true);
    try {
      const caller = parseStellarAddress(wallet.address);
      const xdr = await buildDisputeAttestationTx({
        caller,
        attestationId: disputeTarget,
        reasonHash: disputeReason.trim(),
      });
      await signAndSubmit(xdr);
      toast.success(`Disputed attestation #${disputeTarget}.`);
      setDisputeTarget(null);
      setDisputeReason('');
      await load();
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : 'Transaction failed.';
      toast.error(getContractErrorMessage(message));
    } finally {
      setTxLoading(false);
    }
  }

  if (!wallet.connected) {
    return (
      <div className="max-w-3xl">
        <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">
          Connect your wallet to view your credit profile.
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-5xl space-y-8">
      <div>
        <h1 className="text-3xl font-bold mb-2">Credit Profile</h1>
        <p className="text-brand-muted text-sm">
          Your on-chain credit score, blended with any verified external attestations —
          business registry checks, credit bureau data, or other on-chain protocol history.
        </p>
      </div>

      {loading ? (
        <Skeleton className="h-40 w-full rounded-2xl" />
      ) : !score ? (
        <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">
          No credit history yet.
        </div>
      ) : (
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
            <p className="text-xs text-brand-muted mb-1">Blended Score</p>
            <p className="text-3xl font-bold gradient-text">{score.blendedScore}</p>
            <p className="text-xs text-brand-muted mt-1">What lenders see</p>
          </div>
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
            <p className="text-xs text-brand-muted mb-1">Internal Score</p>
            <p className="text-3xl font-bold">{score.score}</p>
            <p className="text-xs text-brand-muted mt-1">From payment history alone</p>
          </div>
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
            <p className="text-xs text-brand-muted mb-1">Payment History</p>
            <p className="text-3xl font-bold">{score.totalInvoices}</p>
            <p className="text-xs text-brand-muted mt-1">
              {score.paidOnTime} on-time · {score.paidLate} late · {score.defaulted} defaulted
            </p>
          </div>
        </div>
      )}

      {/* Attestations */}
      <div className="space-y-4">
        <h2 className="text-xl font-bold">External Attestations</h2>
        {loading ? (
          <Skeleton className="h-32 w-full rounded-2xl" />
        ) : attestations.length === 0 ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">
            No external attestations yet. Ask a registered attestor (business registry, credit
            bureau, or another on-chain protocol) to verify your business.
          </div>
        ) : (
          <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden">
            <table className="w-full text-left text-sm">
              <thead className="bg-brand-dark border-b border-brand-border text-brand-muted">
                <tr>
                  <th className="px-6 py-4 font-medium">Type</th>
                  <th className="px-6 py-4 font-medium">Signal</th>
                  <th className="px-6 py-4 font-medium">Status</th>
                  <th className="px-6 py-4 font-medium">Expires</th>
                  <th className="px-6 py-4 font-medium text-right">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-brand-border">
                {attestations.map((a) => (
                  <tr key={a.id} className="hover:bg-brand-dark/50 transition-colors">
                    <td className="px-6 py-4">{a.attestationType}</td>
                    <td className="px-6 py-4">{a.scoreContribution} / 1000</td>
                    <td className="px-6 py-4">
                      <span
                        className={`px-2 py-0.5 rounded-full text-xs font-semibold border ${STATUS_STYLES[a.status]}`}
                      >
                        {a.status}
                      </span>
                    </td>
                    <td className="px-6 py-4 text-brand-muted">
                      {new Date(a.expiresAt * 1000).toLocaleDateString()}
                    </td>
                    <td className="px-6 py-4 text-right">
                      {a.status === 'Active' && (
                        <button
                          onClick={() => setDisputeTarget(a.id)}
                          disabled={txLoading}
                          className="px-3 py-1.5 bg-red-500/20 text-red-400 border border-red-500/30 rounded-lg text-xs font-semibold hover:bg-red-500/30 transition-colors disabled:opacity-50"
                        >
                          Dispute
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Dispute form */}
      {disputeTarget !== null && (
        <div className="p-6 bg-brand-card border border-brand-border rounded-2xl space-y-4">
          <div className="flex items-center justify-between">
            <h3 className="font-semibold">Dispute attestation #{disputeTarget}</h3>
            <button
              onClick={() => setDisputeTarget(null)}
              className="text-brand-muted hover:text-white text-sm"
            >
              Cancel
            </button>
          </div>
          <form onSubmit={handleDispute} className="space-y-3">
            <div>
              <label className="block text-sm text-brand-muted mb-1">Reason</label>
              <textarea
                value={disputeReason}
                onChange={(e) => setDisputeReason(e.target.value)}
                placeholder="Why is this attestation incorrect?"
                required
                rows={3}
                className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm"
              />
            </div>
            <button
              type="submit"
              disabled={txLoading}
              className="w-full py-2.5 bg-brand-gold text-brand-dark rounded-xl text-sm font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50"
            >
              {txLoading ? 'Processing…' : 'File Dispute'}
            </button>
            <p className="text-xs text-brand-muted">
              Filing a dispute immediately excludes this attestation from your blended score
              until an admin reviews it.
            </p>
          </form>
        </div>
      )}

      {/* What-if simulator */}
      <div className="space-y-4">
        <h2 className="text-xl font-bold">What-If Simulator</h2>
        <p className="text-brand-muted text-sm">
          Preview how a new attestation would change your blended score before requesting one —
          nothing here is submitted on-chain.
        </p>
        <div className="p-6 bg-brand-card border border-brand-border rounded-2xl space-y-4">
          <form onSubmit={handleSimulate} className="grid grid-cols-1 sm:grid-cols-3 gap-3">
            <div>
              <label className="block text-sm text-brand-muted mb-1">Attestor weight (bps)</label>
              <input
                type="number"
                min={1}
                max={10000}
                value={simWeightBps}
                onChange={(e) => setSimWeightBps(e.target.value)}
                className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white focus:outline-none focus:border-brand-gold text-sm"
              />
            </div>
            <div>
              <label className="block text-sm text-brand-muted mb-1">Signal (0-1000)</label>
              <input
                type="number"
                min={0}
                max={1000}
                value={simScoreContribution}
                onChange={(e) => setSimScoreContribution(e.target.value)}
                className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white focus:outline-none focus:border-brand-gold text-sm"
              />
            </div>
            <button
              type="submit"
              disabled={simLoading}
              className="px-5 py-2.5 bg-brand-gold text-brand-dark rounded-xl text-sm font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50 self-end"
            >
              {simLoading ? 'Simulating…' : 'Simulate'}
            </button>
          </form>
          {simResult !== null && (
            <p className="text-sm">
              Your blended score would be <span className="font-bold gradient-text">{simResult}</span>
              {score && (
                <span className="text-brand-muted">
                  {' '}
                  ({simResult >= score.blendedScore ? '+' : ''}
                  {simResult - score.blendedScore} from current)
                </span>
              )}
              .
            </p>
          )}
        </div>
      </div>
    </div>
  );
}
