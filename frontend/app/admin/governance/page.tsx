'use client';

import { useEffect, useState, useCallback } from 'react';
import { useTranslations } from 'next-intl';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { GovernanceClient, ProposalStatus, ProposalCategory, GovernanceAction } from '@astera/sdk';
import { truncateAddress, formatDate } from '@/lib/stellar';

interface Proposal {
  id: bigint;
  proposer: string;
  description: string;
  target_contract: string;
  action: GovernanceAction;
  votes_for: bigint;
  votes_against: bigint;
  status: ProposalStatus;
  created_at: number;
  voting_ends_at: number;
  execution_delay: number;
  snapshot_supply: bigint;
  passed_at: number;
  category: ProposalCategory;
  quorum_bps: number;
  pass_bps: number;
}

export default function AdminGovernancePage() {
  const t = useTranslations('Admin.governance');
  const { wallet } = useStore();
  const [proposals, setProposals] = useState<Proposal[]>([]);
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState<bigint | null>(null);
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [selectedProposal, setSelectedProposal] = useState<Proposal | null>(null);

  const loadProposals = useCallback(async () => {
    setLoading(true);
    try {
      // TODO: Initialize GovernanceClient with actual config
      // const governanceClient = new GovernanceClient({
      //   contractId: process.env.NEXT_PUBLIC_GOVERNANCE_CONTRACT_ID!,
      //   rpcUrl: process.env.NEXT_PUBLIC_STELLAR_RPC_URL!,
      //   network: process.env.NEXT_PUBLIC_STELLAR_NETWORK!,
      //   signer: wallet.signer,
      // });
      // const allProposals = await governanceClient.getAllProposals();
      // setProposals(allProposals);
      
      // Mock data for now
      setProposals([]);
    } catch (e) {
      toast.error(t('loadFailed'));
      console.error(e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadProposals();
  }, [loadProposals]);

  async function handleVote(proposalId: bigint, voteFor: boolean) {
    if (!wallet.address) {
      toast.error(t('connectWallet'));
      return;
    }

    setActionLoading(proposalId);
    try {
      // TODO: Implement voting
      // const governanceClient = new GovernanceClient({ ... });
      // await governanceClient.vote({
      //   proposalId,
      //   voteFor,
      //   shares: BigInt(1000), // Get actual share balance
      // }, wallet.address);
      
      toast.success(t('voteSuccess'));
      await loadProposals();
    } catch (e) {
      toast.error(t('voteFailed'));
      console.error(e);
    } finally {
      setActionLoading(null);
    }
  }

  async function handleExecute(proposalId: bigint) {
    if (!wallet.address) {
      toast.error(t('connectWallet'));
      return;
    }

    setActionLoading(proposalId);
    try {
      // TODO: Implement execution
      // const governanceClient = new GovernanceClient({ ... });
      // await governanceClient.executeProposal({ proposalId }, wallet.address);
      
      toast.success(t('executeSuccess'));
      await loadProposals();
    } catch (e) {
      toast.error(t('executeFailed'));
      console.error(e);
    } finally {
      setActionLoading(null);
    }
  }

  function getStatusColor(status: ProposalStatus): string {
    switch (status) {
      case ProposalStatus.Active:
        return 'bg-blue-900/40 text-blue-400 border-blue-800/50';
      case ProposalStatus.Passed:
        return 'bg-green-900/40 text-green-400 border-green-800/50';
      case ProposalStatus.Rejected:
        return 'bg-red-900/40 text-red-400 border-red-800/50';
      case ProposalStatus.Executed:
        return 'bg-purple-900/40 text-purple-400 border-purple-800/50';
      case ProposalStatus.Cancelled:
        return 'bg-gray-900/40 text-gray-400 border-gray-800/50';
      case ProposalStatus.Expired:
        return 'bg-yellow-900/40 text-yellow-400 border-yellow-800/50';
      default:
        return 'bg-gray-900/40 text-gray-400 border-gray-800/50';
    }
  }

  function getStatusLabel(status: ProposalStatus): string {
    switch (status) {
      case ProposalStatus.Active:
        return t('statusActive');
      case ProposalStatus.Passed:
        return t('statusPassed');
      case ProposalStatus.Rejected:
        return t('statusRejected');
      case ProposalStatus.Executed:
        return t('statusExecuted');
      case ProposalStatus.Cancelled:
        return t('statusCancelled');
      case ProposalStatus.Expired:
        return t('statusExpired');
      default:
        return status.toString();
    }
  }

  function getCategoryLabel(category: ProposalCategory): string {
    switch (category) {
      case ProposalCategory.ParameterChange:
        return t('categoryParameterChange');
      case ProposalCategory.Treasury:
        return t('categoryTreasury');
      case ProposalCategory.Critical:
        return t('categoryCritical');
      default:
        return category.toString();
    }
  }

  function getActionDescription(action: GovernanceAction): string {
    switch (action.type) {
      case 'SetPoolYield':
        return t('actionSetPoolYield', { value: action.value });
      case 'SetPoolTreasury':
        return t('actionSetPoolTreasury', { address: truncateAddress(action.value) });
      case 'SetInvoiceGracePeriod':
        return t('actionSetInvoiceGracePeriod', { days: action.value });
      case 'SetOracleRegistryInvoiceContract':
        return t('actionSetOracleRegistryInvoiceContract', { address: truncateAddress(action.value) });
      case 'SetComplianceRescreeningInterval':
        return t('actionSetComplianceRescreeningInterval', { secs: action.value });
      default:
        return action.type;
    }
  }

  function calculateQuorumMet(proposal: Proposal): boolean {
    const totalVotes = proposal.votes_for + proposal.votes_against;
    const quorumThreshold = (proposal.snapshot_supply * BigInt(proposal.quorum_bps)) / BigInt(10000);
    return totalVotes >= quorumThreshold;
  }

  function calculatePassThreshold(proposal: Proposal): boolean {
    const totalVotes = proposal.votes_for + proposal.votes_against;
    if (totalVotes === BigInt(0)) return false;
    const passThreshold = (proposal.votes_for * BigInt(10000)) / totalVotes;
    return passThreshold >= BigInt(proposal.pass_bps);
  }

  return (
    <div className="space-y-8">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold mb-2">{t('title')}</h1>
          <p className="text-brand-muted text-sm">{t('description')}</p>
        </div>
        <button
          onClick={() => setShowCreateModal(true)}
          className="px-4 py-2 bg-brand-gold text-brand-dark font-bold rounded-lg hover:bg-brand-gold/90 transition-colors"
        >
          {t('createProposal')}
        </button>
      </div>

      {/* Stats Cards */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <div className="bg-brand-card border border-brand-border rounded-xl p-4">
          <div className="text-2xl font-bold text-brand-gold">{proposals.length}</div>
          <div className="text-sm text-brand-muted">{t('totalProposals')}</div>
        </div>
        <div className="bg-brand-card border border-brand-border rounded-xl p-4">
          <div className="text-2xl font-bold text-blue-400">
            {proposals.filter((p) => p.status === ProposalStatus.Active).length}
          </div>
          <div className="text-sm text-brand-muted">{t('activeProposals')}</div>
        </div>
        <div className="bg-brand-card border border-brand-border rounded-xl p-4">
          <div className="text-2xl font-bold text-green-400">
            {proposals.filter((p) => p.status === ProposalStatus.Passed).length}
          </div>
          <div className="text-sm text-brand-muted">{t('passedProposals')}</div>
        </div>
        <div className="bg-brand-card border border-brand-border rounded-xl p-4">
          <div className="text-2xl font-bold text-purple-400">
            {proposals.filter((p) => p.status === ProposalStatus.Executed).length}
          </div>
          <div className="text-sm text-brand-muted">{t('executedProposals')}</div>
        </div>
      </div>

      {/* Proposals Table */}
      <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden shadow-sm">
        <div className="px-6 py-4 bg-brand-dark/30 border-b border-brand-border">
          <h2 className="text-sm font-bold uppercase tracking-widest text-brand-muted">
            {t('proposals')}
          </h2>
        </div>

        {loading ? (
          <div className="p-6 text-center text-brand-muted">{t('loading')}</div>
        ) : proposals.length === 0 ? (
          <div className="p-6 text-center text-brand-muted">{t('noProposals')}</div>
        ) : (
          <div className="divide-y divide-brand-border">
            {proposals.map((proposal) => (
              <div key={proposal.id.toString()} className="p-6 hover:bg-brand-dark/30 transition-colors">
                <div className="flex items-start justify-between gap-4">
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-3 mb-2">
                      <span className={`px-2 py-0.5 text-[10px] font-bold rounded border ${getStatusColor(proposal.status)}`}>
                        {getStatusLabel(proposal.status)}
                      </span>
                      <span className="px-2 py-0.5 bg-brand-dark/50 text-brand-muted text-[10px] font-medium rounded">
                        {getCategoryLabel(proposal.category)}
                      </span>
                      <span className="text-xs text-brand-muted">#{proposal.id}</span>
                    </div>
                    <h3 className="font-semibold mb-1">{proposal.description}</h3>
                    <p className="text-sm text-brand-muted mb-2">{getActionDescription(proposal.action)}</p>
                    <div className="flex items-center gap-4 text-xs text-brand-muted">
                      <span>{t('proposer')}: {truncateAddress(proposal.proposer)}</span>
                      <span>{t('created')}: {formatDate(proposal.created_at)}</span>
                      <span>{t('target')}: {truncateAddress(proposal.target_contract)}</span>
                    </div>
                  </div>

                  <div className="flex flex-col items-end gap-2 min-w-[200px]">
                    <div className="w-full">
                      <div className="flex justify-between text-xs mb-1">
                        <span className="text-green-400">{t('for')}: {(Number(proposal.votes_for) / 1e6).toFixed(2)}M</span>
                        <span className="text-red-400">{t('against')}: {(Number(proposal.votes_against) / 1e6).toFixed(2)}M</span>
                      </div>
                      <div className="h-2 bg-brand-dark/50 rounded-full overflow-hidden flex">
                        <div
                          className="bg-green-500 transition-all"
                          style={{
                            width: `${Number(proposal.votes_for * BigInt(10000) / (proposal.votes_for + proposal.votes_against || BigInt(1))) / 100}%`,
                          }}
                        />
                        <div
                          className="bg-red-500 transition-all"
                          style={{
                            width: `${Number(proposal.votes_against * BigInt(10000) / (proposal.votes_for + proposal.votes_against || BigInt(1))) / 100}%`,
                          }}
                        />
                      </div>
                    </div>

                    <div className="flex gap-2 text-xs">
                      <span className={calculateQuorumMet(proposal) ? 'text-green-400' : 'text-red-400'}>
                        {t('quorum')}: {calculateQuorumMet(proposal) ? '✓' : '✗'}
                      </span>
                      <span className={calculatePassThreshold(proposal) ? 'text-green-400' : 'text-red-400'}>
                        {t('pass')}: {calculatePassThreshold(proposal) ? '✓' : '✗'}
                      </span>
                    </div>

                    {proposal.status === ProposalStatus.Active && (
                      <div className="flex gap-2">
                        <button
                          onClick={() => handleVote(proposal.id, true)}
                          disabled={actionLoading === proposal.id}
                          className="px-3 py-1 bg-green-600 hover:bg-green-700 text-white text-xs rounded transition-colors disabled:opacity-50"
                        >
                          {t('voteFor')}
                        </button>
                        <button
                          onClick={() => handleVote(proposal.id, false)}
                          disabled={actionLoading === proposal.id}
                          className="px-3 py-1 bg-red-600 hover:bg-red-700 text-white text-xs rounded transition-colors disabled:opacity-50"
                        >
                          {t('voteAgainst')}
                        </button>
                      </div>
                    )}

                    {proposal.status === ProposalStatus.Passed && (
                      <button
                        onClick={() => handleExecute(proposal.id)}
                        disabled={actionLoading === proposal.id}
                        className="px-3 py-1 bg-brand-gold hover:bg-brand-gold/90 text-brand-dark text-xs font-bold rounded transition-colors disabled:opacity-50"
                      >
                        {t('execute')}
                      </button>
                    )}
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Create Proposal Modal */}
      {showCreateModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
          <div className="bg-brand-card border border-brand-border rounded-2xl max-w-2xl w-full max-h-[90vh] overflow-y-auto">
            <div className="p-6 border-b border-brand-border flex items-center justify-between">
              <h2 className="text-xl font-bold">{t('createProposalTitle')}</h2>
              <button
                onClick={() => setShowCreateModal(false)}
                className="text-brand-muted hover:text-white"
              >
                ✕
              </button>
            </div>
            <div className="p-6">
              <p className="text-brand-muted text-sm mb-4">
                {t('createProposalDesc')}
              </p>
              <div className="bg-brand-dark/30 border border-brand-border rounded-lg p-4">
                <p className="text-sm text-brand-muted">
                  {t('proposalCreationNotImplemented')}
                </p>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
