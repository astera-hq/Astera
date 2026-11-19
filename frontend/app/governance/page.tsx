'use client';

import { useCallback, useEffect, useMemo, useState } from 'react';
import type { FormEvent } from 'react';
import { useTranslations } from 'next-intl';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { GOVERNANCE_CONTRACT_ID, formatDate, truncateAddress } from '@/lib/stellar';
import {
  getGovernanceConfig,
  getShareBalance,
  listGovernanceProposals,
  getGovernanceClient,
  sdkSigner,
} from '@/lib/sdk';
import { GovernanceAction, ProposalCategory, type Signer } from '@astera/sdk';
import type { GovernanceConfig, GovernanceProposal, ProposalStatus } from '@/lib/types';

const PROPOSALS_PAGE_SIZE = 10;

type StatusFilter = 'All' | ProposalStatus;

const STATUS_FILTERS: StatusFilter[] = [
  'All',
  'Active',
  'Passed',
  'Rejected',
  'Executed',
  'Expired',
];

export default function GovernancePage() {
  const t = useTranslations('Governance');
  const { wallet } = useStore();
  const [proposals, setProposals] = useState<GovernanceProposal[]>([]);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [votingPower, setVotingPower] = useState<bigint | null>(null);
  const [governanceConfig, setGovernanceConfig] = useState<GovernanceConfig | null>(null);
  const [formOpen, setFormOpen] = useState(true);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [description, setDescription] = useState('');
  const [targetContract, setTargetContract] = useState('');
  const [functionName, setFunctionName] = useState('');
  const [calldata, setCalldata] = useState('');
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('All');
  const [visibleCount, setVisibleCount] = useState(PROPOSALS_PAGE_SIZE);

  const sortedProposals = useMemo(() => [...proposals].sort((a, b) => b.id - a.id), [proposals]);

  const filteredProposals = useMemo(
    () =>
      statusFilter === 'All'
        ? sortedProposals
        : sortedProposals.filter((proposal) => proposal.status === statusFilter),
    [sortedProposals, statusFilter],
  );

  const visibleProposals = useMemo(
    () => filteredProposals.slice(0, visibleCount),
    [filteredProposals, visibleCount],
  );

  const hasMoreProposals = visibleCount < filteredProposals.length;

  const loadProposals = useCallback(async () => {
    if (!GOVERNANCE_CONTRACT_ID) {
      setRefreshing(false);
      return;
    }
    setRefreshing(true);
    try {
      const items = await listGovernanceProposals();
      setProposals(items);
      if (selectedId === null && items.length > 0) {
        setSelectedId(items[0]!.id);
      }
    } catch (error) {
      console.error('[Governance] Failed to load proposals:', error);
      toast.error(t('toast.loadFailed'));
    } finally {
      setRefreshing(false);
    }
  }, [selectedId, t]);

  const loadVotingPower = useCallback(async () => {
    if (!wallet.address || !GOVERNANCE_CONTRACT_ID) return;

    let config = governanceConfig;
    if (!config) {
      config = await getGovernanceConfig();
      if (config) setGovernanceConfig(config);
    }

    if (config) {
      const balance = await getShareBalance(config.shareToken, wallet.address);
      setVotingPower(balance);
    }
  }, [wallet.address, governanceConfig]);

  useEffect(() => {
    if (!GOVERNANCE_CONTRACT_ID) {
      setLoading(false);
      return;
    }
    setLoading(true);
    Promise.all([loadProposals(), loadVotingPower()]).finally(() => setLoading(false));
    const interval = setInterval(loadProposals, 30_000);
    return () => clearInterval(interval);
  }, [loadProposals, loadVotingPower]);

  useEffect(() => {
    setVisibleCount(PROPOSALS_PAGE_SIZE);
  }, [statusFilter]);

  async function submitGovernanceTx(
    run: (signer: Signer, caller: string) => Promise<unknown>,
  ) {
    if (!wallet.connected || !wallet.address) {
      toast.error(t('toast.connectFirst'));
      return;
    }

    setSubmitting(true);
    try {
      // SDK clients build, sign and submit themselves; adapt Freighter to the
      // SDK Signer contract via sdkSigner.
      await run(await sdkSigner(wallet.address), wallet.address);
      toast.success(t('toast.submitted'));
      await loadProposals();
    } catch (error) {
      const message = error instanceof Error ? error.message : t('toast.failed');
      toast.error(message);
    } finally {
      setSubmitting(false);
    }
  }

  async function handleCreateProposal(e: FormEvent) {
    e.preventDefault();
    if (!wallet.address) return;
    await submitGovernanceTx((signer, caller) =>
      getGovernanceClient().createProposal(
        {
          description,
          targetContract,
          // The form's function name / calldata map onto the SDK's typed
          // GovernanceAction union; raw calldata is carried in `value`.
          action: { type: functionName, value: calldata } as unknown as GovernanceAction,
          category: ProposalCategory.ParameterChange,
        },
        caller,
        signer,
      ),
    );
    setDescription('');
    setTargetContract('');
    setFunctionName('');
    setCalldata('');
  }

  const selectedProposal =
    proposals.find((proposal) => proposal.id === selectedId) ??
    visibleProposals.find((proposal) => proposal.id === selectedId) ??
    null;

  return (
    <div className="min-h-screen pt-24 pb-16 px-4 sm:px-6">
      <div className="max-w-7xl mx-auto space-y-8">
        <div className="flex flex-col lg:flex-row lg:items-end lg:justify-between gap-4">
          <div className="space-y-2">
            <p className="text-xs uppercase tracking-[0.3em] text-brand-gold">{t('eyebrow')}</p>
            <h1 className="text-4xl font-bold text-white">{t('title')}</h1>
            <p className="text-brand-muted max-w-2xl">{t('intro')}</p>
          </div>
          <div className="flex items-center gap-3">
            {wallet.connected && votingPower !== null && (
              <div className="px-3 py-1.5 rounded-lg border border-brand-border bg-brand-dark/50 text-xs text-brand-muted">
                {t('votingPower')}{' '}
                <span className="text-white font-semibold">{votingPower.toString()}</span>
                {governanceConfig && votingPower < governanceConfig.minShareBalance && (
                  <span className="text-yellow-400 ml-1">
                    {t('minRequired', { min: governanceConfig.minShareBalance.toString() })}
                  </span>
                )}
              </div>
            )}
            <button
              onClick={() => void loadProposals()}
              className="px-4 py-2 rounded-xl border border-brand-border text-sm text-brand-muted hover:text-white hover:border-brand-gold/40"
            >
              {refreshing ? t('refreshing') : t('refresh')}
            </button>
            <button
              onClick={() => setFormOpen((value) => !value)}
              className="px-4 py-2 rounded-xl bg-brand-gold text-brand-dark text-sm font-semibold"
            >
              {formOpen ? t('hideForm') : t('newProposal')}
            </button>
          </div>
        </div>

        {!GOVERNANCE_CONTRACT_ID && (
          <div className="rounded-2xl border border-yellow-700/40 bg-yellow-950/20 p-4 text-sm text-yellow-200">
            {t('notConfigured')}
          </div>
        )}

        {formOpen && (
          <form
            onSubmit={handleCreateProposal}
            className="rounded-3xl border border-brand-border bg-brand-card p-6 space-y-4"
          >
            <div className="flex items-center justify-between gap-4">
              <div>
                <h2 className="text-xl font-bold text-white">{t('form.title')}</h2>
                <p className="text-sm text-brand-muted">{t('form.intro')}</p>
              </div>
              <span className="text-[10px] uppercase tracking-widest text-brand-muted">
                {t('form.shareWeighted')}
              </span>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <label className="space-y-2">
                <span className="text-xs uppercase tracking-widest text-brand-muted">
                  {t('form.targetContract')}
                </span>
                <input
                  value={targetContract}
                  onChange={(e) => setTargetContract(e.target.value)}
                  placeholder="C..."
                  className="w-full rounded-xl border border-brand-border bg-brand-dark px-4 py-3 text-white focus:border-brand-gold focus:outline-none"
                  disabled={!wallet.connected || !GOVERNANCE_CONTRACT_ID || submitting}
                />
              </label>
              <label className="space-y-2">
                <span className="text-xs uppercase tracking-widest text-brand-muted">
                  {t('form.functionName')}
                </span>
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
              <span className="text-xs uppercase tracking-widest text-brand-muted">
                {t('form.description')}
              </span>
              <input
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder={t('form.descriptionPlaceholder')}
                className="w-full rounded-xl border border-brand-border bg-brand-dark px-4 py-3 text-white focus:border-brand-gold focus:outline-none"
                disabled={!wallet.connected || !GOVERNANCE_CONTRACT_ID || submitting}
              />
            </label>

            <label className="space-y-2 block">
              <span className="text-xs uppercase tracking-widest text-brand-muted">
                {t('form.calldata')}
              </span>
              <textarea
                value={calldata}
                onChange={(e) => setCalldata(e.target.value)}
                placeholder={'{"yield_bps":850}'}
                className="w-full min-h-[110px] rounded-xl border border-brand-border bg-brand-dark px-4 py-3 text-white focus:border-brand-gold focus:outline-none"
                disabled={!wallet.connected || !GOVERNANCE_CONTRACT_ID || submitting}
              />
            </label>

            <button
              type="submit"
              disabled={
                !wallet.connected ||
                !GOVERNANCE_CONTRACT_ID ||
                submitting ||
                (governanceConfig !== null &&
                  votingPower !== null &&
                  votingPower < governanceConfig.minShareBalance)
              }
              className="rounded-xl bg-brand-gold px-5 py-3 text-sm font-semibold text-brand-dark disabled:opacity-50"
              title={
                governanceConfig !== null &&
                votingPower !== null &&
                votingPower < governanceConfig.minShareBalance
                  ? t('form.insufficientTitle', { min: governanceConfig.minShareBalance.toString() })
                  : undefined
              }
            >
              {submitting
                ? t('form.submitting')
                : governanceConfig !== null &&
                    votingPower !== null &&
                    votingPower < governanceConfig.minShareBalance
                  ? t('form.insufficient')
                  : t('form.create')}
            </button>
          </form>
        )}

        <div className="grid grid-cols-1 xl:grid-cols-[1.4fr_0.9fr] gap-6">
          <section className="rounded-3xl border border-brand-border bg-brand-card p-6">
            <div className="flex flex-col gap-4 mb-4">
              <div className="flex items-center justify-between gap-4">
                <div>
                  <h2 className="text-xl font-bold text-white">{t('history.title')}</h2>
                  <p className="text-sm text-brand-muted">
                    {loading
                      ? t('history.loading')
                      : statusFilter === 'All'
                        ? t('history.total', {
                            count: proposals.length,
                            visible: visibleProposals.length,
                          })
                        : t('history.filtered', {
                            count: filteredProposals.length,
                            status: statusFilter.toLowerCase(),
                            visible: visibleProposals.length,
                          })}
                  </p>
                </div>
                {selectedProposal && (
                  <span className="text-xs uppercase tracking-widest text-brand-gold">
                    {t('history.selected', { id: selectedProposal.id })}
                  </span>
                )}
              </div>

              <div className="flex flex-wrap gap-2">
                {STATUS_FILTERS.map((filter) => (
                  <button
                    key={filter}
                    type="button"
                    onClick={() => setStatusFilter(filter)}
                    className={`px-3 py-1.5 rounded-lg text-xs font-medium border transition-colors ${
                      statusFilter === filter
                        ? 'border-brand-gold/50 bg-brand-gold/10 text-brand-gold'
                        : 'border-brand-border text-brand-muted hover:text-white hover:border-brand-gold/30'
                    }`}
                  >
                    {t(`filters.${filter.toLowerCase()}`)}
                  </button>
                ))}
              </div>
            </div>

            {proposals.length === 0 ? (
              <div className="rounded-2xl border border-dashed border-brand-border p-8 text-center text-sm text-brand-muted">
                {t('empty.none')}
              </div>
            ) : filteredProposals.length === 0 ? (
              <div className="rounded-2xl border border-dashed border-brand-border p-8 text-center text-sm text-brand-muted">
                {t('empty.filtered', { status: statusFilter.toLowerCase() })}
              </div>
            ) : (
              <div className="space-y-3">
                {visibleProposals.map((proposal) => (
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
                      <h3 className="text-white font-semibold">
                        #{proposal.id} {proposal.description}
                      </h3>
                      <span className="text-[10px] uppercase tracking-widest text-brand-muted">
                        {proposal.status}
                      </span>
                    </div>
                    <div className="mt-2 grid grid-cols-2 gap-2 text-xs text-brand-muted">
                      <span>{t('proposal.proposer', { address: truncateAddress(proposal.proposer) })}</span>
                      <span>{t('proposal.ends', { date: formatDate(proposal.votingEndsAt) })}</span>
                      <span>{t('proposal.for', { count: proposal.votesFor.toString() })}</span>
                      <span>{t('proposal.against', { count: proposal.votesAgainst.toString() })}</span>
                    </div>
                  </button>
                ))}

                {hasMoreProposals && (
                  <button
                    type="button"
                    onClick={() => setVisibleCount((count) => count + PROPOSALS_PAGE_SIZE)}
                    className="w-full rounded-2xl border border-brand-border px-4 py-3 text-sm font-medium text-brand-muted hover:text-white hover:border-brand-gold/40 transition-colors"
                  >
                    {t('loadMore', { remaining: filteredProposals.length - visibleCount })}
                  </button>
                )}
              </div>
            )}
          </section>

          <aside className="space-y-6">
            <div className="rounded-3xl border border-brand-border bg-brand-card p-6">
              <h2 className="text-xl font-bold text-white mb-2">{t('vote.title')}</h2>
              {selectedProposal ? (
                <div className="space-y-4">
                  <p className="text-sm text-brand-muted">
                    {t('vote.power')}{' '}
                    <strong className="text-white">
                      {votingPower !== null ? votingPower.toString() : '—'}
                    </strong>
                  </p>
                  <div className="rounded-2xl border border-brand-border bg-brand-dark/50 p-4 text-sm text-brand-muted">
                    <p>{t('vote.target', { address: truncateAddress(selectedProposal.targetContract) })}</p>
                    <p>{t('vote.function', { name: selectedProposal.functionName })}</p>
                    <p className="mt-1 break-words">{selectedProposal.calldata}</p>
                  </div>
                  <div className="grid grid-cols-2 gap-3">
                    <button
                      onClick={() =>
                        void submitGovernanceTx((signer) =>
                          getGovernanceClient().vote(
                            {
                              proposalId: BigInt(selectedProposal.id),
                              voteFor: true,
                              shares: 0n,
                            },
                            wallet.address ?? '',
                            signer,
                          ),
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
                      {t('vote.for')}
                    </button>
                    <button
                      onClick={() =>
                        void submitGovernanceTx((signer) =>
                          getGovernanceClient().vote(
                            {
                              proposalId: BigInt(selectedProposal.id),
                              voteFor: false,
                              shares: 0n,
                            },
                            wallet.address ?? '',
                            signer,
                          ),
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
                      {t('vote.against')}
                    </button>
                  </div>
                </div>
              ) : (
                <p className="text-sm text-brand-muted">{t('vote.select')}</p>
              )}
            </div>

            <div className="rounded-3xl border border-brand-border bg-brand-card p-6">
              <h2 className="text-xl font-bold text-white mb-2">{t('execute.title')}</h2>
              <p className="text-sm text-brand-muted mb-4">{t('execute.intro')}</p>
              <button
                onClick={() =>
                  selectedProposal &&
                  void submitGovernanceTx((signer) =>
                    getGovernanceClient().executeProposal(
                      { proposalId: BigInt(selectedProposal.id) },
                      wallet.address ?? '',
                      signer,
                    ),
                  )
                }
                disabled={
                  !selectedProposal ||
                  !wallet.connected ||
                  !GOVERNANCE_CONTRACT_ID ||
                  submitting ||
                  selectedProposal?.status !== 'Passed'
                }
                className="w-full rounded-xl border border-brand-border px-4 py-3 text-sm font-semibold text-white disabled:opacity-50"
              >
                {t('execute.button')}
              </button>
            </div>
          </aside>
        </div>
      </div>
    </div>
  );
}
