'use client';

import { useCallback, useEffect, useState } from 'react';
import type { FormEvent } from 'react';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import {
  GOVERNANCE_CONTRACT_ID,
  formatDate,
  truncateAddress,
} from '@/lib/stellar';
import {
  listGovernanceProposals,
  buildCreateProposalTx,
  buildVoteProposalTx,
  buildExecuteProposalTx,
  submitTx,
} from '@/lib/contracts';
import type { GovernanceProposal } from '@/lib/types';

export default function GovernancePage() {
  const { wallet } = useStore();
  const [proposals, setProposals] = useState<GovernanceProposal[]>([]);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [formOpen, setFormOpen] = useState(true);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [description, setDescription] = useState('');
  const [targetContract, setTargetContract] = useState('');
  const [functionName, setFunctionName] = useState('');
  const [calldata, setCalldata] = useState('');

  const loadProposals = useCallback(async () => {
    setRefreshing(true);
    try {
      const items = await listGovernanceProposals();
      setProposals(items);
      if (selectedId === null && items.length > 0) {
        setSelectedId(items[0].id);
      }
    } catch (error) {
      console.error('[Governance] Failed to load proposals:', error);
      toast.error('Unable to load governance proposals.');
    } finally {
      setRefreshing(false);
    }
  }, [selectedId]);

  useEffect(() => {
    setLoading(true);
    loadProposals().finally(() => setLoading(false));
    const interval = setInterval(loadProposals, 30_000);
    return () => clearInterval(interval);
  }, [loadProposals]);

  async function submitGovernanceTx(buildTx: () => Promise<string>) {
    if (!wallet.connected || !wallet.address) {
      toast.error('Connect a wallet first.');
      return;
    }

    setSubmitting(true);
    try {
      const xdr = await buildTx();
      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error } = await freighter.signTransaction(xdr, {
        networkPassphrase: 'Test SDF Network ; September 2015',
        address: wallet.address,
      });
      if (error) throw new Error(error.message);
      await submitTx(signedTxXdr);
      toast.success('Governance transaction submitted.');
      await loadProposals();
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Transaction failed.';
      toast.error(message);
    } finally {
      setSubmitting(false);
    }
  }

  async function handleCreateProposal(e: FormEvent) {
    e.preventDefault();
    if (!wallet.address) return;
    await submitGovernanceTx(() =>
      buildCreateProposalTx({
        proposer: wallet.address!,
        description,
        targetContract,
        functionName,
        calldata,
      }),
    );
    setDescription('');
    setTargetContract('');
    setFunctionName('');
    setCalldata('');
  }

  const selectedProposal = proposals.find((proposal) => proposal.id === selectedId) ?? null;

  return (
    <div className="min-h-screen pt-24 pb-16 px-4 sm:px-6">
      <div className="max-w-7xl mx-auto space-y-8">
        <div className="flex flex-col lg:flex-row lg:items-end lg:justify-between gap-4">
          <div className="space-y-2">
            <p className="text-xs uppercase tracking-[0.3em] text-brand-gold">Governance</p>
            <h1 className="text-4xl font-bold text-white">Proposal voting and history</h1>
            <p className="text-brand-muted max-w-2xl">
              Share holders can create proposals, vote by balance weight, and review the full
              execution history from the same interface.
            </p>
          </div>
          <div className="flex items-center gap-3">
            <button
              onClick={() => void loadProposals()}
              className="px-4 py-2 rounded-xl border border-brand-border text-sm text-brand-muted hover:text-white hover:border-brand-gold/40"
            >
              {refreshing ? 'Refreshing...' : 'Refresh'}
            </button>
            <button
              onClick={() => setFormOpen((value) => !value)}
              className="px-4 py-2 rounded-xl bg-brand-gold text-brand-dark text-sm font-semibold"
            >
              {formOpen ? 'Hide Form' : 'New Proposal'}
            </button>
          </div>
        </div>

        {!GOVERNANCE_CONTRACT_ID && (
          <div className="rounded-2xl border border-yellow-700/40 bg-yellow-950/20 p-4 text-sm text-yellow-200">
            Governance contract ID is not configured yet. The page is ready, but voting and proposal
            creation stay disabled until the contract is deployed.
          </div>
        )}

        {formOpen && (
          <form
            onSubmit={handleCreateProposal}
            className="rounded-3xl border border-brand-border bg-brand-card p-6 space-y-4"
          >
            <div className="flex items-center justify-between gap-4">
              <div>
                <h2 className="text-xl font-bold text-white">Create Proposal</h2>
                <p className="text-sm text-brand-muted">
                  Add a protocol change, governance action, or parameter update.
                </p>
              </div>
              <span className="text-[10px] uppercase tracking-widest text-brand-muted">
                Share-weighted
              </span>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <label className="space-y-2">
                <span className="text-xs uppercase tracking-widest text-brand-muted">Target contract</span>
                <input
                  value={targetContract}
                  onChange={(e) => setTargetContract(e.target.value)}
                  placeholder="C..."
                  className="w-full rounded-xl border border-brand-border bg-brand-dark px-4 py-3 text-white focus:border-brand-gold focus:outline-none"
                  disabled={!wallet.connected || !GOVERNANCE_CONTRACT_ID || submitting}
                />
              </label>
              <label className="space-y-2">
                <span className="text-xs uppercase tracking-widest text-brand-muted">Function name</span>
                <input
                  value={functionName}
                  onChange={(e) => setFunctionName(e.target.value)}
                  placeholder="set_yield_rate"
                  className="w-full rounded-xl border border-brand-border bg-brand-dark px-4 py-3 text-white focus:border-brand-gold focus:outline-none"
                  disabled={!wallet.connected || !GOVERNANCE_CONTRACT_ID || submitting}
                />
              </label>
            </div>

            <label className="space-y-2 block">
              <span className="text-xs uppercase tracking-widest text-brand-muted">Description</span>
              <input
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder="Raise the pool yield by 50 bps"
                className="w-full rounded-xl border border-brand-border bg-brand-dark px-4 py-3 text-white focus:border-brand-gold focus:outline-none"
                disabled={!wallet.connected || !GOVERNANCE_CONTRACT_ID || submitting}
              />
            </label>

            <label className="space-y-2 block">
              <span className="text-xs uppercase tracking-widest text-brand-muted">Calldata</span>
              <textarea
                value={calldata}
                onChange={(e) => setCalldata(e.target.value)}
                placeholder='{"yield_bps":850}'
                className="w-full min-h-[110px] rounded-xl border border-brand-border bg-brand-dark px-4 py-3 text-white focus:border-brand-gold focus:outline-none"
                disabled={!wallet.connected || !GOVERNANCE_CONTRACT_ID || submitting}
              />
            </label>

            <button
              type="submit"
              disabled={!wallet.connected || !GOVERNANCE_CONTRACT_ID || submitting}
              className="rounded-xl bg-brand-gold px-5 py-3 text-sm font-semibold text-brand-dark disabled:opacity-50"
            >
              {submitting ? 'Submitting...' : 'Create Proposal'}
            </button>
          </form>
        )}

        <div className="grid grid-cols-1 xl:grid-cols-[1.4fr_0.9fr] gap-6">
          <section className="rounded-3xl border border-brand-border bg-brand-card p-6">
            <div className="flex items-center justify-between gap-4 mb-4">
              <div>
                <h2 className="text-xl font-bold text-white">Proposal History</h2>
                <p className="text-sm text-brand-muted">
                  {loading ? 'Loading proposals...' : `${proposals.length} proposals loaded`}
                </p>
              </div>
              {selectedProposal && (
                <span className="text-xs uppercase tracking-widest text-brand-gold">
                  Selected #{selectedProposal.id}
                </span>
              )}
            </div>

            {proposals.length === 0 ? (
              <div className="rounded-2xl border border-dashed border-brand-border p-8 text-center text-sm text-brand-muted">
                No proposals have been submitted yet.
              </div>
            ) : (
              <div className="space-y-3">
                {proposals.map((proposal) => (
                  <button
                    key={proposal.id}
                    onClick={() => setSelectedId(proposal.id)}
                    className={`w-full rounded-2xl border p-4 text-left transition-colors ${
                      selectedProposal?.id === proposal.id
                        ? 'border-brand-gold/50 bg-brand-gold/10'
                        : 'border-brand-border bg-brand-dark/40 hover:border-brand-gold/30'
                    }`}
                  >
                    <div className="flex items-center justify-between gap-3">
                      <h3 className="text-white font-semibold">#{proposal.id} {proposal.description}</h3>
                      <span className="text-[10px] uppercase tracking-widest text-brand-muted">
                        {proposal.status}
                      </span>
                    </div>
                    <div className="mt-2 grid grid-cols-2 gap-2 text-xs text-brand-muted">
                      <span>Proposer {truncateAddress(proposal.proposer)}</span>
                      <span>Ends {formatDate(proposal.votingEndsAt)}</span>
                      <span>For {proposal.votesFor.toString()}</span>
                      <span>Against {proposal.votesAgainst.toString()}</span>
                    </div>
                  </button>
                ))}
              </div>
            )}
          </section>

          <aside className="space-y-6">
            <div className="rounded-3xl border border-brand-border bg-brand-card p-6">
              <h2 className="text-xl font-bold text-white mb-2">Vote</h2>
              {selectedProposal ? (
                <div className="space-y-4">
                  <p className="text-sm text-brand-muted">
                    Voting power is based on your current share balance.
                  </p>
                  <div className="rounded-2xl border border-brand-border bg-brand-dark/50 p-4 text-sm text-brand-muted">
                    <p>Target: {truncateAddress(selectedProposal.targetContract)}</p>
                    <p>Function: {selectedProposal.functionName}</p>
                    <p className="mt-1 break-words">{selectedProposal.calldata}</p>
                  </div>
                  <div className="grid grid-cols-2 gap-3">
                    <button
                      onClick={() =>
                        void submitGovernanceTx(() =>
                          buildVoteProposalTx({
                            voter: wallet.address ?? '',
                            proposalId: selectedProposal.id,
                            inFavor: true,
                          }),
                        )
                      }
                      disabled={
                        !wallet.connected ||
                        !GOVERNANCE_CONTRACT_ID ||
                        submitting ||
                        selectedProposal.status !== 'Active'
                      }
                      className="rounded-xl bg-green-600 px-4 py-3 text-sm font-semibold text-white disabled:opacity-50"
                    >
                      Vote For
                    </button>
                    <button
                      onClick={() =>
                        void submitGovernanceTx(() =>
                          buildVoteProposalTx({
                            voter: wallet.address ?? '',
                            proposalId: selectedProposal.id,
                            inFavor: false,
                          }),
                        )
                      }
                      disabled={
                        !wallet.connected ||
                        !GOVERNANCE_CONTRACT_ID ||
                        submitting ||
                        selectedProposal.status !== 'Active'
                      }
                      className="rounded-xl bg-red-600 px-4 py-3 text-sm font-semibold text-white disabled:opacity-50"
                    >
                      Vote Against
                    </button>
                  </div>
                </div>
              ) : (
                <p className="text-sm text-brand-muted">Select a proposal to vote.</p>
              )}
            </div>

            <div className="rounded-3xl border border-brand-border bg-brand-card p-6">
              <h2 className="text-xl font-bold text-white mb-2">Execute</h2>
              <p className="text-sm text-brand-muted mb-4">
                Once a proposal is passed and the execution delay has elapsed, any wallet can
                trigger execution.
              </p>
              <button
                onClick={() =>
                  selectedProposal &&
                  void submitGovernanceTx(() => buildExecuteProposalTx(wallet.address ?? '', selectedProposal.id))
                }
                disabled={
                  !selectedProposal || !wallet.connected || !GOVERNANCE_CONTRACT_ID || submitting
                  || selectedProposal?.status !== 'Passed'
                }
                className="w-full rounded-xl border border-brand-border px-4 py-3 text-sm font-semibold text-white disabled:opacity-50"
              >
                Execute Selected
              </button>
            </div>
          </aside>
        </div>
      </div>
    </div>
  );
}
