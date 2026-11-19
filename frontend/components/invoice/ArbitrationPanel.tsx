'use client';

import type { Dispatch, SetStateAction } from 'react';
import type { DisputeCase, EvidenceEntry } from '@/lib/types';

/** Arbitration case details + evidence submission (evidence-window phase). */
interface ArbitrationPanelProps {
  arbitrationCase: DisputeCase;
  arbitrationEvidence: EvidenceEntry[];
  newEvidenceHash: string;
  setNewEvidenceHash: Dispatch<SetStateAction<string>>;
  walletAddress: string | null;
  actionLoading: boolean;
  onSubmitEvidence: () => void;
}

export default function ArbitrationPanel({
  arbitrationCase,
  arbitrationEvidence,
  newEvidenceHash,
  setNewEvidenceHash,
  walletAddress,
  actionLoading,
  onSubmitEvidence,
}: ArbitrationPanelProps) {
  return (
    <div className="mt-4 pt-4 border-t border-red-800/50 space-y-3">
      <div className="flex items-center justify-between text-sm">
        <span className="text-brand-muted">Arbitration case #{arbitrationCase.id}</span>
        <span className="px-2.5 py-1 rounded-full text-xs font-semibold bg-brand-dark border border-brand-border">
          {arbitrationCase.status}
        </span>
      </div>
      {arbitrationEvidence.length > 0 && (
        <ul className="text-xs text-brand-muted space-y-1">
          {arbitrationEvidence.map((e, i) => (
            <li key={i} className="font-mono truncate">
              [{e.party}] {e.evidenceHash}
            </li>
          ))}
        </ul>
      )}
      {arbitrationCase.status === 'EvidenceWindow' &&
        walletAddress &&
        (walletAddress === arbitrationCase.claimant ||
          walletAddress === arbitrationCase.respondent) && (
          <div className="flex gap-2">
            <input
              type="text"
              value={newEvidenceHash}
              onChange={(e) => setNewEvidenceHash(e.target.value)}
              placeholder="Evidence hash / URI"
              className="flex-1 bg-brand-dark border border-brand-border rounded-lg px-3 py-2 text-xs text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold"
            />
            <button
              onClick={onSubmitEvidence}
              disabled={actionLoading || !newEvidenceHash.trim()}
              className="px-3 py-2 bg-brand-gold text-brand-dark rounded-lg text-xs font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50"
            >
              Submit
            </button>
          </div>
        )}
    </div>
  );
}
