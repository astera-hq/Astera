'use client';

export interface ApprovalActionsProps {
  proposalId: number;
  canApprove: boolean;
  canReject: boolean;
  canRevoke: boolean;
  canExecute: boolean;
  txLoading: boolean;
  onApprove: (id: number) => void;
  onReject: (id: number) => void;
  onRevoke: (id: number) => void;
  onExecute: (id: number) => void;
}

/** Approve / Execute / Revoke / Reject buttons for a single proposal. */
export default function ApprovalActions({
  proposalId,
  canApprove,
  canReject,
  canRevoke,
  canExecute,
  txLoading,
  onApprove,
  onReject,
  onRevoke,
  onExecute,
}: ApprovalActionsProps) {
  return (
    <div className="flex gap-2 pt-1">
      {canApprove && (
        <button
          onClick={() => onApprove(proposalId)}
          disabled={txLoading}
          className="px-3 py-1.5 bg-green-500/20 text-green-400 border border-green-500/30 rounded-lg text-xs font-semibold hover:bg-green-500/30 transition-colors disabled:opacity-50"
        >
          Approve
        </button>
      )}
      {canExecute && (
        <button
          onClick={() => onExecute(proposalId)}
          disabled={txLoading}
          className="px-3 py-1.5 bg-brand-gold/20 text-brand-gold border border-brand-gold/30 rounded-lg text-xs font-semibold hover:bg-brand-gold/30 transition-colors disabled:opacity-50"
        >
          Execute
        </button>
      )}
      {canRevoke && (
        <button
          onClick={() => onRevoke(proposalId)}
          disabled={txLoading}
          className="px-3 py-1.5 bg-brand-dark border border-brand-border rounded-lg text-xs font-semibold hover:bg-brand-border transition-colors disabled:opacity-50"
        >
          Revoke Approval
        </button>
      )}
      {canReject && (
        <button
          onClick={() => onReject(proposalId)}
          disabled={txLoading}
          className="px-3 py-1.5 bg-red-500/20 text-red-400 border border-red-500/30 rounded-lg text-xs font-semibold hover:bg-red-500/30 transition-colors disabled:opacity-50"
        >
          Reject
        </button>
      )}
    </div>
  );
}
