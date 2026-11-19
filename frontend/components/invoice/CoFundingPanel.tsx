'use client';

import type { Dispatch, SetStateAction } from 'react';
import { formatUSDC } from '@/lib/stellar';
import WalletConnect from '@/components/WalletConnect';
import type { CoFundingRound } from '@/lib/types';

/** Co-funding commitment panel for open rounds. */
interface CoFundingPanelProps {
  coFundingRound: CoFundingRound | null;
  walletConnected: boolean;
  commitAmount: string;
  setCommitAmount: Dispatch<SetStateAction<string>>;
  commitLoading: boolean;
  onCommit: () => void;
}

export default function CoFundingPanel({
  coFundingRound,
  walletConnected,
  commitAmount,
  setCommitAmount,
  commitLoading,
  onCommit,
}: CoFundingPanelProps) {
  if (!coFundingRound || coFundingRound.status !== 'Open') return null;

  return (
    <div className="p-6 bg-brand-card border border-brand-border rounded-2xl mb-6 space-y-4">
      <div>
        <h2 className="text-lg font-semibold mb-1">Fund This Invoice</h2>
        <p className="text-xs text-brand-muted">
          Join other lenders co-funding this invoice. Committed capital earns a proportional share
          of this invoice&apos;s principal and interest.
        </p>
      </div>

      <div>
        <div className="flex justify-between text-xs text-brand-muted mb-1">
          <span>
            {formatUSDC(coFundingRound.committedPrincipal)} /{' '}
            {formatUSDC(coFundingRound.targetPrincipal)}
          </span>
          <span>
            {coFundingRound.targetPrincipal > 0n
              ? (
                  Number(
                    (coFundingRound.committedPrincipal * 10_000n) /
                      coFundingRound.targetPrincipal,
                  ) / 100
                ).toFixed(1)
              : '0'}
            %
          </span>
        </div>
        <div className="h-2 bg-brand-border rounded-full overflow-hidden">
          <div
            className="h-full bg-brand-gold transition-all"
            style={{
              width: `${
                coFundingRound.targetPrincipal > 0n
                  ? Math.min(
                      100,
                      Number(
                        (coFundingRound.committedPrincipal * 10_000n) /
                          coFundingRound.targetPrincipal,
                      ) / 100,
                    )
                  : 0
              }%`,
            }}
          />
        </div>
      </div>

      {!walletConnected ? (
        <div className="space-y-2">
          <p className="text-sm text-brand-muted">Connect your wallet to fund this invoice.</p>
          <WalletConnect />
        </div>
      ) : (
        <div className="flex gap-3">
          <input
            type="number"
            min="0"
            step="0.01"
            placeholder="Amount (USDC)"
            value={commitAmount}
            onChange={(e) => setCommitAmount(e.target.value)}
            disabled={commitLoading}
            className="flex-1 bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm disabled:opacity-50"
          />
          <button
            onClick={onCommit}
            disabled={commitLoading || !commitAmount}
            className="px-5 py-2.5 bg-brand-gold text-brand-dark rounded-xl text-sm font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50"
          >
            {commitLoading ? 'Funding...' : 'Fund This Invoice'}
          </button>
        </div>
      )}
    </div>
  );
}
