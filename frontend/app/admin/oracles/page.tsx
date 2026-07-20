'use client';

import { useEffect, useState } from 'react';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { Skeleton } from '@/components/Skeleton';
import ConfirmActionModal from '@/components/ConfirmActionModal';
import { parseStellarAddress } from '@/lib/types';
import type { OracleInfo, OracleRegistryConfig, VerificationRound } from '@/lib/types';
import {
  getRegistryConfig,
  listActiveOracles,
  getOracleInfo,
  getVerificationRound,
  buildAdminResolveRoundTx,
  buildSlashOracleTx,
  submitTx,
} from '@/lib/contracts';

function formatStake(amount: bigint): string {
  return (Number(amount) / 10_000_000).toLocaleString();
}

const STATUS_STYLES: Record<VerificationRound['status'], string> = {
  Open: 'bg-blue-500/20 text-blue-400 border-blue-500/30',
  ConsensusApproved: 'bg-green-500/20 text-green-400 border-green-500/30',
  ConsensusRejected: 'bg-red-500/20 text-red-400 border-red-500/30',
  Expired: 'bg-amber-500/20 text-amber-400 border-amber-500/30',
};

export default function AdminOraclesPage() {
  const { wallet } = useStore();
  const [config, setConfig] = useState<OracleRegistryConfig | null>(null);
  const [oracles, setOracles] = useState<OracleInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [notDeployed, setNotDeployed] = useState(false);
  const [txLoading, setTxLoading] = useState(false);

  const [slashTarget, setSlashTarget] = useState<OracleInfo | null>(null);
  const [slashBps, setSlashBps] = useState('1000');

  const [lookupId, setLookupId] = useState('');
  const [round, setRound] = useState<VerificationRound | null | 'not_found'>(null);
  const [roundLoading, setRoundLoading] = useState(false);
  const [resolveReason, setResolveReason] = useState('');

  async function loadRegistry() {
    setLoading(true);
    try {
      const [cfg, addresses] = await Promise.all([getRegistryConfig(), listActiveOracles()]);
      setConfig(cfg);
      const infos = await Promise.all(addresses.map((addr) => getOracleInfo(addr)));
      setOracles(infos.filter((o): o is OracleInfo => o !== null));
    } catch (e) {
      // Most likely NEXT_PUBLIC_ORACLE_REGISTRY_CONTRACT_ID is unset — this
      // page is only meaningful once the #861 registry has been deployed.
      console.error(e);
      setNotDeployed(true);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    loadRegistry();
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

  async function handleSlash() {
    if (!wallet.address || !slashTarget) return;
    const bps = Number(slashBps);
    if (!Number.isFinite(bps) || bps <= 0 || bps > 10_000) {
      toast.error('Slash bps must be between 1 and 10000.');
      return;
    }
    setTxLoading(true);
    try {
      const admin = parseStellarAddress(wallet.address);
      const operator = parseStellarAddress(slashTarget.address);
      const xdr = await buildSlashOracleTx({ admin, operator, bps });
      await signAndSubmit(xdr);
      toast.success(`Slashed ${operator.slice(0, 8)}… by ${(bps / 100).toFixed(2)}%.`);
      setSlashTarget(null);
      await loadRegistry();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Transaction failed.');
    } finally {
      setTxLoading(false);
    }
  }

  async function handleLookupRound(e: React.FormEvent) {
    e.preventDefault();
    const id = Number(lookupId);
    if (!Number.isFinite(id) || id < 0) {
      toast.error('Enter a valid invoice ID.');
      return;
    }
    setRoundLoading(true);
    setRound(null);
    try {
      const r = await getVerificationRound(id);
      setRound(r ?? 'not_found');
    } catch (e) {
      console.error(e);
      setRound('not_found');
    } finally {
      setRoundLoading(false);
    }
  }

  async function handleResolve(approved: boolean) {
    if (!wallet.address || typeof round !== 'object' || round === null) return;
    setTxLoading(true);
    try {
      const admin = parseStellarAddress(wallet.address);
      const xdr = await buildAdminResolveRoundTx({
        admin,
        invoiceId: round.invoiceId,
        approved,
        reason: resolveReason || 'Manual admin fallback resolution',
      });
      await signAndSubmit(xdr);
      toast.success(
        `Round for invoice #${round.invoiceId} resolved as ${approved ? 'Approved' : 'Rejected'}.`,
      );
      const refreshed = await getVerificationRound(round.invoiceId);
      setRound(refreshed ?? 'not_found');
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Transaction failed.');
    } finally {
      setTxLoading(false);
    }
  }

  if (notDeployed && !loading) {
    return (
      <div className="max-w-4xl">
        <h1 className="text-3xl font-bold mb-2">Oracle Network</h1>
        <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-sm text-brand-muted">
          The N-of-M staked oracle consensus registry isn&apos;t configured
          (<code className="font-mono text-xs">NEXT_PUBLIC_ORACLE_REGISTRY_CONTRACT_ID</code> is
          unset), or the deployed contract could not be reached. This page becomes active once a{' '}
          <code className="font-mono text-xs">oracle_registry</code> contract is deployed and the
          invoice contract&apos;s <code className="font-mono text-xs">set_oracle_registry</code> has
          been called.
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-5xl space-y-8">
      <div>
        <h1 className="text-3xl font-bold mb-2">Oracle Network</h1>
        <p className="text-brand-muted text-sm">
          Registered oracles vote on invoice verification with weight proportional to their staked
          amount. Once approving or rejecting weight crosses the quorum threshold, the invoice
          contract is updated automatically.
        </p>
      </div>

      {/* Registry config summary */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        {[
          { label: 'Min Stake', value: loading ? null : `${formatStake(config!.minStake)}` },
          {
            label: 'Quorum',
            value: loading ? null : `${(config!.quorumBps / 100).toFixed(1)}%`,
          },
          { label: 'Required Votes', value: loading ? null : config!.requiredVotes },
          {
            label: 'Round Duration',
            value: loading ? null : `${Math.round(config!.roundDurationSecs / 3600)}h`,
          },
        ].map((stat) => (
          <div key={stat.label} className="p-4 bg-brand-card border border-brand-border rounded-2xl">
            <p className="text-xs text-brand-muted mb-1">{stat.label}</p>
            <p className="text-xl font-bold">
              {stat.value === null ? <Skeleton className="h-6 w-16" /> : stat.value}
            </p>
          </div>
        ))}
      </div>

      {/* Active oracles */}
      <div className="space-y-4">
        <h2 className="text-xl font-bold">Registered Oracles</h2>
        {loading ? (
          <Skeleton className="h-32 w-full rounded-2xl" />
        ) : oracles.length === 0 ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">
            No active oracles registered yet.
          </div>
        ) : (
          <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden">
            <table className="w-full text-left text-sm">
              <thead className="bg-brand-dark border-b border-brand-border text-brand-muted">
                <tr>
                  <th className="px-6 py-4 font-medium">Address</th>
                  <th className="px-6 py-4 font-medium">Stake</th>
                  <th className="px-6 py-4 font-medium">Verifications</th>
                  <th className="px-6 py-4 font-medium">Slashes</th>
                  <th className="px-6 py-4 font-medium text-right">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-brand-border">
                {oracles.map((oracle) => (
                  <tr key={oracle.address} className="hover:bg-brand-dark/50 transition-colors">
                    <td className="px-6 py-4 font-mono text-xs">{oracle.address}</td>
                    <td className="px-6 py-4">{formatStake(oracle.stakeAmount)}</td>
                    <td className="px-6 py-4">{oracle.totalVerifications}</td>
                    <td className="px-6 py-4">
                      <span className={oracle.totalSlashes > 0 ? 'text-red-400' : ''}>
                        {oracle.totalSlashes}
                      </span>
                    </td>
                    <td className="px-6 py-4 text-right">
                      <button
                        onClick={() => {
                          setSlashTarget(oracle);
                          setSlashBps('1000');
                        }}
                        disabled={txLoading}
                        className="px-4 py-2 bg-red-500/20 text-red-400 border border-red-500/30 rounded-lg text-xs font-semibold hover:bg-red-500/30 transition-colors disabled:opacity-50"
                      >
                        Slash
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Verification round lookup / admin fallback */}
      <div className="space-y-4">
        <h2 className="text-xl font-bold">Verification Round Lookup</h2>
        <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
          <form onSubmit={handleLookupRound} className="flex gap-3 mb-4">
            <input
              type="number"
              min={0}
              value={lookupId}
              onChange={(e) => setLookupId(e.target.value)}
              placeholder="Invoice ID"
              required
              className="flex-1 bg-brand-dark border border-brand-border rounded-xl px-4 py-3 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm"
            />
            <button
              type="submit"
              disabled={roundLoading}
              className="px-5 py-3 bg-brand-gold text-brand-dark rounded-xl text-sm font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50"
            >
              {roundLoading ? '…' : 'Look up'}
            </button>
          </form>

          {round === 'not_found' && (
            <p className="text-sm text-brand-muted">No verification round found for that invoice.</p>
          )}

          {round && typeof round === 'object' && (
            <div className="space-y-3">
              <div className="flex items-center gap-3">
                <span
                  className={`px-3 py-1 rounded-full text-xs font-semibold border ${STATUS_STYLES[round.status]}`}
                >
                  {round.status}
                </span>
                <span className="text-sm text-brand-muted">
                  {round.totalRegisteredOracles} oracles eligible · quorum{' '}
                  {(round.quorumBps / 100).toFixed(1)}%
                </span>
              </div>
              <div className="grid grid-cols-2 gap-4 text-sm">
                <div>
                  <p className="text-brand-muted text-xs">Weight For</p>
                  <p className="font-semibold text-green-400">{formatStake(round.weightFor)}</p>
                </div>
                <div>
                  <p className="text-brand-muted text-xs">Weight Against</p>
                  <p className="font-semibold text-red-400">{formatStake(round.weightAgainst)}</p>
                </div>
                <div>
                  <p className="text-brand-muted text-xs">Opened</p>
                  <p>{new Date(round.openedAt * 1000).toLocaleString()}</p>
                </div>
                <div>
                  <p className="text-brand-muted text-xs">Deadline</p>
                  <p>{new Date(round.deadline * 1000).toLocaleString()}</p>
                </div>
              </div>

              {round.status === 'Expired' && (
                <div className="pt-4 border-t border-brand-border space-y-3">
                  <p className="text-sm text-amber-400">
                    Oracle participation never reached quorum before the deadline. As admin you can
                    resolve this round directly so the invoice isn&apos;t permanently stuck.
                  </p>
                  <input
                    type="text"
                    value={resolveReason}
                    onChange={(e) => setResolveReason(e.target.value)}
                    placeholder="Resolution reason (optional)"
                    className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm"
                  />
                  <div className="flex gap-3">
                    <button
                      onClick={() => handleResolve(true)}
                      disabled={txLoading}
                      className="flex-1 py-2.5 bg-green-500/20 text-green-400 border border-green-500/30 rounded-xl text-sm font-semibold hover:bg-green-500/30 transition-colors disabled:opacity-50"
                    >
                      Approve
                    </button>
                    <button
                      onClick={() => handleResolve(false)}
                      disabled={txLoading}
                      className="flex-1 py-2.5 bg-red-500/20 text-red-400 border border-red-500/30 rounded-xl text-sm font-semibold hover:bg-red-500/30 transition-colors disabled:opacity-50"
                    >
                      Reject
                    </button>
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      </div>

      <div className="p-4 bg-brand-dark border border-brand-border rounded-xl text-xs text-brand-muted space-y-1">
        <p>• Votes are weighted by each oracle&apos;s staked amount, not a flat 1-address-1-vote.</p>
        <p>• Slashing permanently reduces an oracle&apos;s withdrawable stake — this cannot be undone.</p>
        <p>• Admin fallback resolution is only available once a round has expired without reaching quorum.</p>
      </div>

      <ConfirmActionModal
        title={slashTarget ? `Slash ${slashTarget.address.slice(0, 12)}…` : ''}
        description="This permanently reduces this oracle's withdrawable stake. Use for a proven-bad verdict, paired with the invoice dispute-resolution flow."
        confirmPhrase="CONFIRM SLASH"
        onConfirm={handleSlash}
        onCancel={() => setSlashTarget(null)}
        variant="destructive"
        isOpen={slashTarget !== null}
        confirmLabel="Slash Oracle"
      >
        <div className="mt-2">
          <label className="block text-sm text-brand-muted mb-1">Slash amount (basis points, 1-10000)</label>
          <input
            type="number"
            min={1}
            max={10000}
            value={slashBps}
            onChange={(e) => setSlashBps(e.target.value)}
            className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white focus:outline-none focus:border-brand-gold text-sm"
          />
        </div>
      </ConfirmActionModal>
    </div>
  );
}
