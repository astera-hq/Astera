'use client';

// #1043: juror dashboard for the structured multi-party dispute arbitration
// subsystem — stake/deregister as a juror, and commit/reveal votes on cases
// this wallet was selected for. Modeled on app/admin/oracles/page.tsx's
// structure (local fetch-on-mount state, a `signAndSubmit` XDR helper).

import { useEffect, useState } from 'react';
import toast from 'react-hot-toast';
import { useTranslations } from 'next-intl';
import { useStore } from '@/lib/store';
import { Skeleton } from '@/components/Skeleton';
import { parseStellarAddress } from '@/lib/types';
import type { JurorInfo, DisputeCase, JurorVoteStatus } from '@/lib/types';
import { NETWORK } from '@/lib/stellar';
import {
  getJurorInfo,
  getJurorCases,
  getArbitrationCase,
  getJurorVoteStatus,
  buildRegisterJurorTx,
  buildDeregisterJurorTx,
  buildCommitVoteTx,
  buildRevealVoteTx,
  computeArbitrationCommitHash,
  generateArbitrationSalt,
  submitTx,
} from '@/lib/contracts';

const STATUS_STYLES: Record<DisputeCase['status'], string> = {
  EvidenceWindow: 'bg-blue-500/20 text-blue-400 border-blue-500/30',
  CommitReveal: 'bg-amber-500/20 text-amber-400 border-amber-500/30',
  Resolved: 'bg-green-500/20 text-green-400 border-green-500/30',
  NoQuorumEscalated: 'bg-red-500/20 text-red-400 border-red-500/30',
};

function saltStorageKey(caseId: number, juror: string): string {
  return `astera:arbitration:salt:${caseId}:${juror}`;
}

function getStoredSalt(caseId: number, juror: string): string | null {
  if (typeof window === 'undefined') return null;
  return (
    window.localStorage.getItem(saltStorageKey(caseId, juror)) ||
    window.sessionStorage.getItem(saltStorageKey(caseId, juror))
  );
}

function saveStoredSalt(caseId: number, juror: string, data: string): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(saltStorageKey(caseId, juror), data);
  } catch {}
  try {
    window.sessionStorage.setItem(saltStorageKey(caseId, juror), data);
  } catch {}
}

function formatStake(amount: bigint): string {
  return (Number(amount) / 10_000_000).toLocaleString();
}

export default function ArbitrationJurorsPage() {
  const t = useTranslations('arbitration.jurors');
  const { wallet } = useStore();
  const [info, setInfo] = useState<JurorInfo | null>(null);
  const [cases, setCases] = useState<DisputeCase[]>([]);
  const [votes, setVotes] = useState<Record<number, JurorVoteStatus | null>>({});
  const [loading, setLoading] = useState(true);
  const [notDeployed, setNotDeployed] = useState(false);
  const [txLoading, setTxLoading] = useState(false);
  const [stakeInput, setStakeInput] = useState('1000');
  const [voteChoice, setVoteChoice] = useState<Record<number, boolean>>({});

  async function load() {
    if (!wallet.address) return;
    setLoading(true);
    try {
      const address = parseStellarAddress(wallet.address);
      const [jurorInfo, caseIds] = await Promise.all([
        getJurorInfo(address),
        getJurorCases(address),
      ]);
      setInfo(jurorInfo);
      const loadedCases = await Promise.all(caseIds.map((id) => getArbitrationCase(id)));
      const active = loadedCases.filter((c): c is DisputeCase => c !== null);
      setCases(active);
      const voteEntries = await Promise.all(
        active.map(async (c) => [c.id, await getJurorVoteStatus(c.id, address)] as const),
      );
      setVotes(Object.fromEntries(voteEntries));
    } catch (e) {
      // Most likely NEXT_PUBLIC_ARBITRATION_CONTRACT_ID is unset — this page
      // is only meaningful once the #1043 arbitration contract is deployed.
      console.error(e);
      setNotDeployed(true);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wallet.address]);

  async function signAndSubmit(xdr: string) {
    const freighter = await import('@stellar/freighter-api');
    const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
      networkPassphrase: NETWORK,
      address: wallet.address!,
    });
    if (signError) throw new Error(signError.message || 'Signing rejected.');
    await submitTx(signedTxXdr);
  }

  async function handleRegister() {
    if (!wallet.address) return;
    const stakeAmount = BigInt(Math.round(Number(stakeInput) * 10_000_000));
    if (!Number.isFinite(Number(stakeInput)) || stakeAmount <= 0n) {
      toast.error(t('errors.invalidStake'));
      return;
    }
    setTxLoading(true);
    try {
      const operator = parseStellarAddress(wallet.address);
      const xdr = await buildRegisterJurorTx({ operator, stakeAmount });
      await signAndSubmit(xdr);
      toast.success(t('toasts.registered'));
      await load();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('errors.transactionFailed'));
    } finally {
      setTxLoading(false);
    }
  }

  async function handleDeregister() {
    if (!wallet.address) return;
    setTxLoading(true);
    try {
      const operator = parseStellarAddress(wallet.address);
      const xdr = await buildDeregisterJurorTx({ operator });
      await signAndSubmit(xdr);
      toast.success(
        info?.deregisterRequestedAt
          ? t('toasts.stakeWithdrawn')
          : t('toasts.deregistrationRequested'),
      );
      await load();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('errors.transactionFailed'));
    } finally {
      setTxLoading(false);
    }
  }

  async function handleCommit(caseId: number) {
    if (!wallet.address) return;
    const choice = voteChoice[caseId];
    if (choice === undefined) {
      toast.error(t('errors.pickSide'));
      return;
    }
    setTxLoading(true);
    try {
      const juror = parseStellarAddress(wallet.address);
      const salt = generateArbitrationSalt();
      // Persisted in localStorage + sessionStorage backup so this browser can reveal it later
      saveStoredSalt(caseId, juror, JSON.stringify({ vote: choice, salt: Array.from(salt) }));
      const commitHash = await computeArbitrationCommitHash(choice, salt);
      const xdr = await buildCommitVoteTx({ juror, caseId, commitHash });
      await signAndSubmit(xdr);
      toast.success(t('toasts.voteCommitted'));
      await load();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('errors.transactionFailed'));
    } finally {
      setTxLoading(false);
    }
  }

  async function handleReveal(caseId: number) {
    if (!wallet.address) return;
    const juror = parseStellarAddress(wallet.address);
    const stored = getStoredSalt(caseId, juror);
    if (!stored) {
      toast.error(t('errors.noStoredSalt'));
      return;
    }
    setTxLoading(true);
    try {
      const { vote, salt } = JSON.parse(stored) as { vote: boolean; salt: number[] };
      const xdr = await buildRevealVoteTx({
        juror,
        caseId,
        voteChoice: vote,
        salt: new Uint8Array(salt),
      });
      await signAndSubmit(xdr);
      toast.success(t('toasts.voteRevealed'));
      await load();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('errors.transactionFailed'));
    } finally {
      setTxLoading(false);
    }
  }

  if (!wallet.address) {
    return (
      <div className="max-w-4xl">
        <h1 className="text-3xl font-bold mb-2">{t('title')}</h1>
        <p className="text-brand-muted text-sm">{t('connectWallet')}</p>
      </div>
    );
  }

  if (notDeployed && !loading) {
    return (
      <div className="max-w-4xl">
        <h1 className="text-3xl font-bold mb-2">{t('title')}</h1>
        <p className="text-brand-muted text-sm">{t('notDeployed')}</p>
      </div>
    );
  }

  const now = Math.floor(Date.now() / 1000);

  return (
    <div className="max-w-5xl space-y-8">
      <div>
        <h1 className="text-3xl font-bold mb-2">{t('title')}</h1>
        <p className="text-brand-muted text-sm">{t('subtitle')}</p>
      </div>

      <div className="p-6 bg-brand-card border border-brand-border rounded-2xl space-y-4">
        <h2 className="text-xl font-bold">{t('status.heading')}</h2>
        {loading ? (
          <Skeleton className="h-20 w-full rounded-xl" />
        ) : info ? (
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
            <div>
              <p className="text-brand-muted text-xs">{t('status.stake')}</p>
              <p className="font-semibold">{formatStake(info.stakeAmount)}</p>
            </div>
            <div>
              <p className="text-brand-muted text-xs">{t('status.casesServed')}</p>
              <p className="font-semibold">{info.casesServed}</p>
            </div>
            <div>
              <p className="text-brand-muted text-xs">{t('status.timesSlashed')}</p>
              <p className={info.timesSlashed > 0 ? 'font-semibold text-red-400' : 'font-semibold'}>
                {info.timesSlashed}
              </p>
            </div>
            <div>
              <p className="text-brand-muted text-xs">{t('status.status')}</p>
              <p className="font-semibold">
                {info.deregisterRequestedAt
                  ? t('status.deregistering')
                  : info.isActive
                    ? t('status.active')
                    : t('status.inactive')}
              </p>
            </div>
          </div>
        ) : (
          <p className="text-sm text-brand-muted">{t('status.notRegistered')}</p>
        )}

        <div className="flex gap-3 pt-2">
          {!info || info.deregisterRequestedAt === null ? (
            <>
              <input
                type="number"
                min={0}
                value={stakeInput}
                onChange={(e) => setStakeInput(e.target.value)}
                placeholder={t('stakeAmountPlaceholder')}
                className="flex-1 bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm"
              />
              <button
                onClick={handleRegister}
                disabled={txLoading || !!info}
                className="px-5 py-2.5 bg-brand-gold text-brand-dark rounded-xl text-sm font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50"
              >
                {info ? t('registered') : t('register')}
              </button>
            </>
          ) : null}
          {info && (
            <button
              onClick={handleDeregister}
              disabled={txLoading}
              className="px-5 py-2.5 bg-red-500/20 text-red-400 border border-red-500/30 rounded-xl text-sm font-semibold hover:bg-red-500/30 transition-colors disabled:opacity-50"
            >
              {info.deregisterRequestedAt ? t('withdrawStake') : t('deregister')}
            </button>
          )}
        </div>
      </div>

      <div className="space-y-4">
        <h2 className="text-xl font-bold">{t('cases.heading')}</h2>
        {loading ? (
          <Skeleton className="h-32 w-full rounded-2xl" />
        ) : cases.length === 0 ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">
            {t('cases.empty')}
          </div>
        ) : (
          <div className="space-y-4">
            {cases.map((c) => {
              const vote = votes[c.id];
              const inCommitPhase = c.status === 'CommitReveal' && now <= c.commitDeadline;
              const inRevealPhase =
                c.status === 'CommitReveal' && now > c.commitDeadline && now <= c.revealDeadline;
              return (
                <div
                  key={c.id}
                  className="p-6 bg-brand-card border border-brand-border rounded-2xl space-y-4"
                >
                  <div className="flex items-center justify-between">
                    <div>
                      <p className="text-sm text-brand-muted">
                        {t('cases.caseNumber', { id: c.id })}
                      </p>
                      <p className="font-semibold">
                        {t('cases.invoiceNumber', { id: c.invoiceId })}
                      </p>
                    </div>
                    <span
                      className={`px-3 py-1 rounded-full text-xs font-semibold border ${STATUS_STYLES[c.status]}`}
                    >
                      {c.status}
                    </span>
                  </div>

                  {inCommitPhase && !vote?.hasCommitted && (
                    <div className="space-y-3 pt-3 border-t border-brand-border">
                      <p className="text-sm text-brand-muted">{t('cases.commitPrompt')}</p>
                      <div className="flex gap-3">
                        <button
                          onClick={() => setVoteChoice((v) => ({ ...v, [c.id]: true }))}
                          className={`flex-1 py-2.5 rounded-xl text-sm font-semibold border transition-colors ${
                            voteChoice[c.id] === true
                              ? 'bg-green-500/30 text-green-300 border-green-500/50'
                              : 'bg-green-500/10 text-green-400 border-green-500/20 hover:bg-green-500/20'
                          }`}
                        >
                          {t('cases.favorDebtor')}
                        </button>
                        <button
                          onClick={() => setVoteChoice((v) => ({ ...v, [c.id]: false }))}
                          className={`flex-1 py-2.5 rounded-xl text-sm font-semibold border transition-colors ${
                            voteChoice[c.id] === false
                              ? 'bg-red-500/30 text-red-300 border-red-500/50'
                              : 'bg-red-500/10 text-red-400 border-red-500/20 hover:bg-red-500/20'
                          }`}
                        >
                          {t('cases.favorSme')}
                        </button>
                      </div>
                      <button
                        onClick={() => handleCommit(c.id)}
                        disabled={txLoading}
                        className="w-full py-2.5 bg-brand-gold text-brand-dark rounded-xl text-sm font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50"
                      >
                        {t('cases.commitVote')}
                      </button>
                    </div>
                  )}

                  {vote?.hasCommitted && vote.revealedVote === null && (
                    <div className="pt-3 border-t border-brand-border">
                      {inRevealPhase ? (
                        <button
                          onClick={() => handleReveal(c.id)}
                          disabled={txLoading}
                          className="w-full py-2.5 bg-brand-gold text-brand-dark rounded-xl text-sm font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50"
                        >
                          {t('cases.revealVote')}
                        </button>
                      ) : (
                        <p className="text-sm text-brand-muted">{t('cases.awaitingReveal')}</p>
                      )}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
