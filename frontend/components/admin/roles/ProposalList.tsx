'use client';

import { Skeleton } from '@/components/Skeleton';
import { ROLE_LABELS } from '@/lib/types';
import type { MultiSigConfig, Proposal, Role } from '@/lib/types';
import ApprovalActions from './ApprovalActions';
import { summarizeAction } from './proposalPayload';

export interface ProposalListProps {
  proposals: Array<{ id: number; proposal: Proposal }>;
  loading: boolean;
  walletAddress: string | null;
  isSigner: Record<Role, boolean>;
  roleConfigs: Record<Role, MultiSigConfig | null> | null;
  txLoading: boolean;
  onApprove: (id: number) => void;
  onReject: (id: number) => void;
  onRevoke: (id: number) => void;
  onExecute: (id: number) => void;
}

export default function ProposalList({
  proposals,
  loading,
  walletAddress,
  isSigner,
  roleConfigs,
  txLoading,
  onApprove,
  onReject,
  onRevoke,
  onExecute,
}: ProposalListProps) {
  return (
    <div className="space-y-4">
      <h2 className="text-xl font-bold">Proposals</h2>
      {loading ? (
        <Skeleton className="h-32 w-full rounded-2xl" />
      ) : proposals.length === 0 ? (
        <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">
          No proposals yet.
        </div>
      ) : (
        <div className="space-y-3">
          {proposals.map(({ id, proposal }) => {
            const address = walletAddress;
            const canApprove =
              isSigner[proposal.role] &&
              proposal.status === 'Pending' &&
              !!address &&
              !proposal.approvals.some((a) => a === address);
            const canRevoke =
              !!address &&
              proposal.approvals.some((a) => a === address) &&
              (proposal.status === 'Pending' || proposal.status === 'Approved');
            const canReject = isSigner[proposal.role] && proposal.status === 'Pending';
            const canExecute = proposal.status === 'Approved';
            const config = roleConfigs?.[proposal.role] ?? null;

            return (
              <div
                key={id}
                className="p-5 bg-brand-card border border-brand-border rounded-2xl space-y-2"
              >
                <div className="flex items-center justify-between flex-wrap gap-2">
                  <div>
                    <span className="text-xs text-brand-muted">
                      #{id} · {ROLE_LABELS[proposal.role]}
                    </span>
                    <p className="font-medium text-sm">{summarizeAction(proposal.action)}</p>
                  </div>
                  <span
                    className={`text-xs px-2 py-0.5 rounded-full ${
                      proposal.status === 'Executed'
                        ? 'bg-green-500/20 text-green-400'
                        : proposal.status === 'Rejected'
                          ? 'bg-red-500/20 text-red-400'
                          : proposal.status === 'Approved'
                            ? 'bg-brand-gold/20 text-brand-gold'
                            : 'bg-brand-border text-brand-muted'
                    }`}
                  >
                    {proposal.status}
                  </span>
                </div>
                <p className="text-xs text-brand-muted">
                  Approvals: {proposal.approvals.length} / {config?.threshold ?? '?'} · Proposer{' '}
                  {proposal.proposer.slice(0, 8)}… · Expires{' '}
                  {new Date(proposal.expiresAt * 1000).toLocaleString()}
                </p>
                <ApprovalActions
                  proposalId={id}
                  canApprove={canApprove}
                  canReject={canReject}
                  canRevoke={canRevoke}
                  canExecute={canExecute}
                  txLoading={txLoading}
                  onApprove={onApprove}
                  onReject={onReject}
                  onRevoke={onRevoke}
                  onExecute={onExecute}
                />
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
