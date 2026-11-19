'use client';

import type { Dispatch, SetStateAction } from 'react';
import ConfirmActionModal from '@/components/ConfirmActionModal';
import ArbitrationPanel from './ArbitrationPanel';
import type {
  DisputeCase,
  DisputeRecord,
  EvidenceEntry,
  InvoiceStatus,
} from '@/lib/types';

/** All dispute-related UI: status banner, arbitration, default + pre-funding disputes. */
interface DisputePanelProps {
  invoiceId: number;
  status: InvoiceStatus;
  isOwner: boolean;
  walletAddress: string | null;
  disputeRecord: DisputeRecord | null;
  arbitrationCase: DisputeCase | null;
  arbitrationEvidence: EvidenceEntry[];
  actionLoading: boolean;
  defaultDisputeModalOpen: boolean;
  setDefaultDisputeModalOpen: Dispatch<SetStateAction<boolean>>;
  defaultDisputeEvidence: string;
  setDefaultDisputeEvidence: Dispatch<SetStateAction<string>>;
  defaultDisputeRespondent: string;
  setDefaultDisputeRespondent: Dispatch<SetStateAction<string>>;
  onRaiseDefaultDispute: () => void;
  disputeModalOpen: boolean;
  setDisputeModalOpen: Dispatch<SetStateAction<boolean>>;
  disputeReason: string;
  setDisputeReason: Dispatch<SetStateAction<string>>;
  onDispute: () => void;
  newEvidenceHash: string;
  setNewEvidenceHash: Dispatch<SetStateAction<string>>;
  onSubmitEvidence: () => void;
}

export default function DisputePanel({
  invoiceId,
  status,
  isOwner,
  walletAddress,
  disputeRecord,
  arbitrationCase,
  arbitrationEvidence,
  actionLoading,
  defaultDisputeModalOpen,
  setDefaultDisputeModalOpen,
  defaultDisputeEvidence,
  setDefaultDisputeEvidence,
  defaultDisputeRespondent,
  setDefaultDisputeRespondent,
  onRaiseDefaultDispute,
  disputeModalOpen,
  setDisputeModalOpen,
  disputeReason,
  setDisputeReason,
  onDispute,
  newEvidenceHash,
  setNewEvidenceHash,
  onSubmitEvidence,
}: DisputePanelProps) {
  return (
    <>
      {status === 'Disputed' && (
        <div className="p-4 bg-red-900/20 border border-red-800/50 rounded-xl">
          <div className="flex items-center gap-2 text-red-400 font-medium mb-2">
            <svg
              className="w-5 h-5"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z"
              />
            </svg>
            This invoice is under dispute review
          </div>
          <p className="text-sm text-brand-muted">
            Our team will review your dispute within 3-5 business days. You will be notified once
            the issue is resolved.
          </p>

          {arbitrationCase && (
            <ArbitrationPanel
              arbitrationCase={arbitrationCase}
              arbitrationEvidence={arbitrationEvidence}
              newEvidenceHash={newEvidenceHash}
              setNewEvidenceHash={setNewEvidenceHash}
              walletAddress={walletAddress}
              actionLoading={actionLoading}
              onSubmitEvidence={onSubmitEvidence}
            />
          )}
          {!arbitrationCase && disputeRecord && disputeRecord.outcome === 'Pending' && (
            <p className="mt-2 text-xs text-brand-muted">
              This dispute is below the arbitration value threshold and stays on the standard admin
              review path.
            </p>
          )}
        </div>
      )}

      {isOwner && status === 'Defaulted' && (
        <>
          <button
            onClick={() => setDefaultDisputeModalOpen(true)}
            className="w-full px-5 py-3 border border-red-700/50 text-red-400 font-semibold rounded-xl hover:bg-red-900/20 transition-colors"
          >
            Dispute Default
          </button>
          {defaultDisputeModalOpen && (
            <ConfirmActionModal
              title={`Dispute Default for Invoice #${invoiceId}`}
              description="Disputing a default flag files a claim (e.g. non-delivery, wrongful default). Invoices above the configured value threshold are routed to staked-juror arbitration instead of a unilateral admin decision."
              confirmLabel="Raise Dispute"
              onConfirm={() => void onRaiseDefaultDispute()}
              onCancel={() => {
                setDefaultDisputeModalOpen(false);
                setDefaultDisputeEvidence('');
                setDefaultDisputeRespondent('');
              }}
              variant="destructive"
              isOpen={defaultDisputeModalOpen}
            >
              <div className="px-6 pt-4 space-y-3">
                <div>
                  <label
                    htmlFor="default-dispute-evidence"
                    className="block text-xs font-medium text-brand-muted mb-2"
                  >
                    Evidence hash
                  </label>
                  <textarea
                    id="default-dispute-evidence"
                    value={defaultDisputeEvidence}
                    onChange={(e) => setDefaultDisputeEvidence(e.target.value)}
                    placeholder="Hash/URI of your supporting evidence..."
                    rows={3}
                    className="w-full px-4 py-2.5 rounded-xl border border-brand-border bg-brand-dark text-white placeholder-brand-muted/50 focus:border-brand-gold focus:outline-none focus:ring-2 focus:ring-brand-gold/40 resize-none"
                    disabled={actionLoading}
                  />
                </div>
                <div>
                  <label
                    htmlFor="default-dispute-respondent"
                    className="block text-xs font-medium text-brand-muted mb-2"
                  >
                    Respondent address
                  </label>
                  <input
                    id="default-dispute-respondent"
                    type="text"
                    value={defaultDisputeRespondent}
                    onChange={(e) => setDefaultDisputeRespondent(e.target.value)}
                    placeholder="G..."
                    className="w-full px-4 py-2.5 rounded-xl border border-brand-border bg-brand-dark text-white placeholder-brand-muted/50 focus:border-brand-gold focus:outline-none focus:ring-2 focus:ring-brand-gold/40"
                    disabled={actionLoading}
                  />
                  <p className="mt-1.5 text-xs text-brand-muted">
                    Only used if this dispute is routed to arbitration — stands in for the
                    debtor&apos;s side of the case (see FAQ for why).
                  </p>
                </div>
              </div>
            </ConfirmActionModal>
          )}
        </>
      )}

      {isOwner &&
        (status === 'Verified' || status === 'Funded' || status === 'AwaitingVerification') && (
          <button
            onClick={() => setDisputeModalOpen(true)}
            className="w-full px-5 py-3 border border-red-700/50 text-red-400 font-semibold rounded-xl hover:bg-red-900/20 transition-colors"
          >
            Raise Dispute
          </button>
        )}

      {disputeModalOpen && (
        <ConfirmActionModal
          title={`Raise Dispute for Invoice #${invoiceId}`}
          description="Disputing an invoice will flag it for manual review. This action cannot be undone. Please provide a clear reason for the dispute."
          confirmLabel="Confirm Dispute"
          onConfirm={() => void onDispute()}
          onCancel={() => {
            setDisputeModalOpen(false);
            setDisputeReason('');
          }}
          variant="destructive"
          isOpen={disputeModalOpen}
        >
          <div className="px-6 pt-4">
            <label
              htmlFor="dispute-reason"
              className="block text-xs font-medium text-brand-muted mb-2"
            >
              Dispute Reason
            </label>
            <textarea
              id="dispute-reason"
              value={disputeReason}
              onChange={(e) => setDisputeReason(e.target.value)}
              placeholder="Describe why you are disputing this invoice..."
              rows={4}
              className="w-full px-4 py-2.5 rounded-xl border border-brand-border bg-brand-dark text-white placeholder-brand-muted/50 focus:border-brand-gold focus:outline-none focus:ring-2 focus:ring-brand-gold/40 resize-none"
              disabled={actionLoading}
            />
            <p className="mt-1.5 text-xs text-brand-muted">
              Provide a clear explanation to help the review team understand your dispute.
            </p>
          </div>
        </ConfirmActionModal>
      )}
    </>
  );
}
