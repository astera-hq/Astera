'use client';

import type { Dispatch, SetStateAction } from 'react';
import {
  formatUSDC,
  formatDate,
  fromStroops,
  stablecoinLabel,
  USDC_TOKEN_ID,
} from '@/lib/stellar';
import GlossaryTerm from '@/components/GlossaryTerm';
import type {
  CollateralConfig,
  CollateralDeposit,
  CollateralRiskConfig,
  FundedInvoice,
  InvoiceMetadata,
} from '@/lib/types';

/** Collateral configuration, posting/top-up form and live risk states. */
interface CollateralPanelProps {
  collateralConfig: CollateralConfig | null;
  metadata: InvoiceMetadata;
  collateralDeposit: CollateralDeposit | null;
  fundedInvoice: FundedInvoice | null;
  liveCollateralRatioBps: number | null;
  collateralRiskConfig: CollateralRiskConfig | null;
  atRiskSince: number | null;
  acceptedTokens: string[];
  collateralToken: string;
  setCollateralToken: Dispatch<SetStateAction<string>>;
  crossAssetRequiredAmount: bigint | null;
  crossAssetPriceLoading: boolean;
  collateralAmount: string;
  setCollateralAmount: Dispatch<SetStateAction<string>>;
  collateralLoading: boolean;
  isOwner: boolean;
  fullyRepaid: boolean;
  remainingDue: bigint;
  onOpenDeposit: () => void;
  onDepositCollateral: () => void;
}

export default function CollateralPanel({
  collateralConfig,
  metadata,
  collateralDeposit,
  fundedInvoice,
  liveCollateralRatioBps,
  collateralRiskConfig,
  atRiskSince,
  acceptedTokens,
  collateralToken,
  setCollateralToken,
  crossAssetRequiredAmount,
  crossAssetPriceLoading,
  collateralAmount,
  setCollateralAmount,
  collateralLoading,
  isOwner,
  fullyRepaid,
  remainingDue,
  onOpenDeposit,
  onDepositCollateral,
}: CollateralPanelProps) {
  if (
    !collateralConfig ||
    (metadata.amount < collateralConfig.threshold && !collateralDeposit)
  ) {
    return null;
  }

  return (
    <div className="p-6 bg-brand-card border border-brand-border rounded-2xl mb-6">
      <h2 className="text-lg font-semibold mb-4">Collateral</h2>

      {(() => {
        const requiredAmount =
          (metadata.amount * BigInt(collateralConfig.collateralBps)) / 10_000n;
        const pct = (collateralConfig.collateralBps / 100).toFixed(0);
        const requiredLabel = (
          <>
            Required (<GlossaryTerm id="collateral-ratio">{pct}%</GlossaryTerm>)
          </>
        );

        if (collateralDeposit && collateralDeposit.settled) {
          // #1036: releasedAt/seizedAt are the authoritative signal for
          // which outcome happened — metadata.status === 'Defaulted'
          // only covers the admin default-seizure path, not a
          // price-triggered liquidate_collateral call (which doesn't
          // touch the invoice's own status at all).
          if (collateralDeposit.seizedAt > 0) {
            const wasPriceLiquidated = metadata.status !== 'Defaulted';
            return (
              <div className="p-4 bg-red-900/20 border border-red-800/50 rounded-xl text-sm">
                <p className="font-semibold text-red-400 mb-1">
                  {wasPriceLiquidated ? 'Collateral Liquidated' : 'Collateral Seized'}
                </p>
                <p className="text-brand-muted">
                  {wasPriceLiquidated ? (
                    <>
                      Your collateral of {formatUSDC(collateralDeposit.amount)} was liquidated on{' '}
                      {formatDate(collateralDeposit.seizedAt)} after its oracle-priced value fell
                      below the danger threshold and wasn&apos;t topped up in time.
                    </>
                  ) : (
                    <>
                      Your collateral of {formatUSDC(collateralDeposit.amount)} was seized because
                      this invoice was not repaid. The funds were redistributed to pool investors to
                      offset the default loss.
                    </>
                  )}
                </p>
              </div>
            );
          }
          return (
            <div className="p-4 bg-green-900/20 border border-green-800/50 rounded-xl text-sm text-green-400">
              Collateral of {formatUSDC(collateralDeposit.amount)} was returned to your wallet after
              full repayment. ✓
            </div>
          );
        }

        if (collateralDeposit && !collateralDeposit.settled) {
          const outstanding = fundedInvoice ? remainingDue : metadata.amount;
          const ratio =
            outstanding > 0n
              ? Number((collateralDeposit.amount * 10_000n) / outstanding) / 100
              : 0;
          const targetRatio = collateralConfig.collateralBps / 100;
          const isAtRisk = ratio < targetRatio * 1.2;

          // #1036: on-chain, oracle-priced ratio — the authoritative
          // signal (covers collateral posted in an asset other than the
          // invoice's funding token, unlike the client-side estimate
          // above which assumes a 1:1 same-token comparison). Only
          // available once the invoice is funded and an oracle is
          // configured for both tokens.
          const liveRatioPct =
            liveCollateralRatioBps != null ? liveCollateralRatioBps / 100 : null;
          const dangerPct = collateralRiskConfig
            ? collateralRiskConfig.dangerBps / 100
            : null;
          const onChainAtRisk = atRiskSince != null;
          const topUpDeadline =
            onChainAtRisk && collateralRiskConfig
              ? atRiskSince! + collateralRiskConfig.gracePeriodSecs
              : null;

          return (
            <div className="space-y-3 text-sm">
              {onChainAtRisk && (
                <div className="p-4 bg-red-900/20 border border-red-800/50 rounded-xl">
                  <p className="font-semibold text-red-400 mb-1">
                    ⚠ Collateral at risk of liquidation
                  </p>
                  <p className="text-brand-muted text-xs">
                    The oracle-priced value of your collateral has fallen below the danger
                    threshold{dangerPct != null ? ` (${dangerPct.toFixed(0)}%)` : ''}.
                    {topUpDeadline != null && (
                      <>
                        {' '}
                        Top up before{' '}
                        <span className="text-white">{formatDate(topUpDeadline)}</span> or it may be
                        liquidated by any keeper.
                      </>
                    )}
                  </p>
                </div>
              )}
              <div className="flex items-center justify-between">
                <span className="text-brand-muted">Current ratio</span>
                <span
                  className={
                    isAtRisk
                      ? 'font-semibold text-yellow-400'
                      : 'font-semibold text-green-400'
                  }
                >
                  {ratio.toFixed(0)}% {isAtRisk ? '⚠' : '✓'} (target {targetRatio.toFixed(0)}%)
                </span>
              </div>
              {liveRatioPct != null && (
                <div className="flex items-center justify-between">
                  <span className="text-brand-muted">Live ratio (oracle-priced)</span>
                  <span
                    className={
                      onChainAtRisk
                        ? 'font-semibold text-red-400'
                        : 'font-semibold text-green-400'
                    }
                  >
                    {liveRatioPct.toFixed(0)}% {onChainAtRisk ? '⚠' : '✓'}
                  </span>
                </div>
              )}
              <div className="flex justify-between">
                <span className="text-brand-muted">{requiredLabel}</span>
                <span className="font-medium">{formatUSDC(requiredAmount)}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-brand-muted">Posted</span>
                <span className="font-medium text-brand-gold">
                  {formatUSDC(collateralDeposit.amount)}
                </span>
              </div>
              <div className="p-3 bg-brand-dark border border-brand-border rounded-xl text-xs text-brand-muted">
                Collateral is locked until the invoice is fully repaid, at which point it will be
                automatically returned to your wallet.
              </div>
              {isOwner && !fullyRepaid && (
                <button
                  onClick={onOpenDeposit}
                  className={
                    onChainAtRisk
                      ? 'w-full px-5 py-3 bg-red-600 text-white font-semibold rounded-xl hover:bg-red-500 transition-colors'
                      : 'w-full px-5 py-3 bg-brand-gold text-brand-dark font-semibold rounded-xl hover:bg-brand-amber transition-colors'
                  }
                >
                  {onChainAtRisk ? 'Top Up Now' : '+ Add Collateral'}
                </button>
              )}
            </div>
          );
        }

        if (!isOwner || metadata.status === 'Paid' || metadata.status === 'Defaulted') {
          return (
            <div className="space-y-3 text-sm">
              <div className="flex justify-between">
                <span className="text-brand-muted">{requiredLabel}</span>
                <span className="font-medium">{formatUSDC(requiredAmount)}</span>
              </div>
              <p className="text-brand-muted">No collateral has been posted.</p>
            </div>
          );
        }

        const isCrossAsset = collateralToken !== USDC_TOKEN_ID;
        const crossAssetPlaceholder = crossAssetPriceLoading
          ? 'Fetching live price...'
          : crossAssetRequiredAmount != null
            ? `Required: ~${fromStroops(crossAssetRequiredAmount).toLocaleString()} ${stablecoinLabel(collateralToken)}`
            : 'Enter amount';

        return (
          <div className="space-y-4">
            <div className="flex justify-between text-sm">
              <span className="text-brand-muted">
                Required (<GlossaryTerm id="collateral-ratio">{pct}%</GlossaryTerm> of invoice)
              </span>
              <span className="font-medium">{formatUSDC(requiredAmount)}</span>
            </div>
            {acceptedTokens.length > 1 && (
              <div>
                <label className="block text-xs text-brand-muted mb-1">Collateral Asset</label>
                <select
                  value={collateralToken}
                  onChange={(e) => {
                    setCollateralToken(e.target.value);
                    setCollateralAmount('');
                  }}
                  disabled={collateralLoading}
                  className="w-full px-4 py-2 bg-brand-dark border border-brand-border rounded-lg text-white focus:border-brand-gold focus:outline-none disabled:opacity-60"
                >
                  {acceptedTokens.map((tok) => (
                    <option key={tok} value={tok}>
                      {stablecoinLabel(tok)}
                    </option>
                  ))}
                </select>
                <p className="text-xs text-brand-muted mt-1">
                  Posting an asset other than the invoice&apos;s funding token is priced via the
                  pool&apos;s oracle — the required amount updates live with its price.
                </p>
              </div>
            )}
            <div>
              <label className="block text-xs text-brand-muted mb-1">
                Collateral Amount ({stablecoinLabel(collateralToken)})
              </label>
              <input
                type="text"
                value={collateralAmount}
                onChange={(e) => setCollateralAmount(e.target.value)}
                placeholder={
                  isCrossAsset
                    ? crossAssetPlaceholder
                    : `Required: ${formatUSDC(requiredAmount)}`
                }
                disabled={collateralLoading}
                className="w-full px-4 py-2 bg-brand-dark border border-brand-border rounded-lg text-white placeholder-brand-muted focus:border-brand-gold focus:outline-none disabled:opacity-60"
              />
            </div>
            <p className="text-xs text-yellow-400">
              Warning: Collateral will be locked until this invoice is fully repaid or resolved.
            </p>
            <button
              onClick={onDepositCollateral}
              disabled={collateralLoading}
              className="w-full px-5 py-3 bg-brand-gold text-brand-dark font-semibold rounded-xl hover:bg-brand-amber transition-colors disabled:opacity-60"
            >
              {collateralLoading ? 'Posting collateral...' : 'Post Collateral'}
            </button>
          </div>
        );
      })()}
    </div>
  );
}
