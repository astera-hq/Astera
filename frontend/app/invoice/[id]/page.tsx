'use client';

/**
 * Invoice detail page — thin composition/orchestration layer.
 *
 * All data loading lives in `useInvoiceDetail` and all transaction handlers in
 * `useInvoiceActions`; the visual concerns are split into feature components
 * under `components/invoice/`. The private-access guard intentionally stays
 * here so no invoice content (including child components) renders before the
 * ownership check completes.
 */

import Link from 'next/link';
import { Skeleton } from '@/components/Skeleton';

import { useInvoiceDetail } from '@/hooks/useInvoiceDetail';
import { useInvoiceActions } from '@/hooks/useInvoiceActions';

import InvoiceHeader from '@/components/invoice/InvoiceHeader';
import InvoicePdfExport from '@/components/invoice/InvoicePdfExport';
import InvoicePrivacyControl from '@/components/invoice/InvoicePrivacyControl';
import CreditScorePanel from '@/components/invoice/CreditScorePanel';
import Timeline from '@/components/invoice/Timeline';
import PoolDetails from '@/components/invoice/PoolDetails';
import InterestAccrual from '@/components/invoice/InterestAccrual';
import CollateralPanel from '@/components/invoice/CollateralPanel';
import CoFundingPanel from '@/components/invoice/CoFundingPanel';
import InvoiceHistory from '@/components/invoice/InvoiceHistory';
import RepaymentPanel from '@/components/invoice/RepaymentPanel';
import DisputePanel from '@/components/invoice/DisputePanel';

export default function InvoiceDetailPage() {
  const detail = useInvoiceDetail();
  const actions = useInvoiceActions(detail);
  const {
    loading,
    error,
    invoice,
    metadata,
    poolConfig,
    fundedInvoice,
    isOwner,
    isAdmin,
    canViewInvoice,
    statusSteps,
    projectedInterest,
    accruedInterest,
    interestProgress,
    remainingDue,
    fullyRepaid,
    days,
    isPrivate,
    creditScore,
    collateralConfig,
    collateralDeposit,
    liveCollateralRatioBps,
    collateralRiskConfig,
    atRiskSince,
    acceptedTokens,
    collateralToken,
    setCollateralToken,
    crossAssetRequiredAmount,
    crossAssetPriceLoading,
    disputeRecord,
    arbitrationCase,
    arbitrationEvidence,
    coFundingRound,
    history,
    historyLoading,
    historyError,
    wallet,
  } = detail;

  if (loading) {
    return (
      <div className="min-h-screen pt-24 px-4 sm:px-6">
        <div className="max-w-2xl mx-auto space-y-4">
          <Skeleton className="h-10 w-48 rounded-lg" />
          <Skeleton className="h-24 rounded-2xl" />
          <Skeleton className="h-24 rounded-2xl" />
          <Skeleton className="h-24 rounded-2xl" />
        </div>
      </div>
    );
  }

  if (error || !invoice || !metadata) {
    return (
      <div className="min-h-screen pt-24 px-4 sm:px-6 flex flex-col items-center justify-center text-center">
        <p className="text-red-400 mb-4">{error ?? 'Invoice not found.'}</p>
        <Link href="/dashboard" className="text-brand-gold hover:underline text-sm">
          Back to Dashboard
        </Link>
      </div>
    );
  }

  // #775: the borrower opted this invoice out of the public sharing link —
  // hide it from everyone except the connected owner, same as a genuinely missing invoice.
  if (!canViewInvoice) {
    return (
      <div className="min-h-screen pt-24 px-4 sm:px-6 flex flex-col items-center justify-center text-center">
        <p className="text-red-400 mb-4">Invoice not found.</p>
        <Link href="/dashboard" className="text-brand-gold hover:underline text-sm">
          Back to Dashboard
        </Link>
      </div>
    );
  }

  return (
    <div className="min-h-screen pt-24 pb-16 px-4 sm:px-6">
      <div className="max-w-2xl mx-auto">
        <Link
          href="/dashboard"
          className="text-brand-muted hover:text-white text-sm mb-6 inline-flex items-center gap-2 transition-colors"
        >
          ← Back to Dashboard
        </Link>
        <InvoicePdfExport
          pdfLoading={actions.pdfLoading}
          onExport={() => void actions.exportInvoicePDF()}
          onShare={() => void actions.handleShare()}
        />
        {isOwner && (
          <InvoicePrivacyControl
            isPrivate={isPrivate}
            privacyLoading={actions.privacyLoading}
            onToggle={() => void actions.handleTogglePrivate()}
          />
        )}

        <InvoiceHeader invoice={invoice} metadata={metadata} days={days} isOwner={isOwner} />

        <CreditScorePanel creditScore={creditScore} fundedInvoice={fundedInvoice} />

        <Timeline statusSteps={statusSteps} status={metadata.status} />

        <PoolDetails invoice={invoice} poolConfig={poolConfig} fundedInvoice={fundedInvoice} />

        <InterestAccrual
          fundedInvoice={fundedInvoice}
          poolConfig={poolConfig}
          projectedInterest={projectedInterest}
          accruedInterest={accruedInterest}
          interestProgress={interestProgress}
          remainingDue={remainingDue}
        />

        <CollateralPanel
          collateralConfig={collateralConfig}
          metadata={metadata}
          collateralDeposit={collateralDeposit}
          fundedInvoice={fundedInvoice}
          liveCollateralRatioBps={liveCollateralRatioBps}
          collateralRiskConfig={collateralRiskConfig}
          atRiskSince={atRiskSince}
          acceptedTokens={acceptedTokens}
          collateralToken={collateralToken}
          setCollateralToken={setCollateralToken}
          crossAssetRequiredAmount={crossAssetRequiredAmount}
          crossAssetPriceLoading={crossAssetPriceLoading}
          collateralAmount={actions.collateralAmount}
          setCollateralAmount={actions.setCollateralAmount}
          collateralLoading={actions.collateralLoading}
          isOwner={isOwner}
          fullyRepaid={fullyRepaid}
          remainingDue={remainingDue}
          onOpenDeposit={() => actions.setCollateralModalOpen(true)}
          onDepositCollateral={() => void actions.handleDepositCollateral()}
        />

        <CoFundingPanel
          coFundingRound={coFundingRound}
          walletConnected={wallet.connected}
          commitAmount={actions.commitAmount}
          setCommitAmount={actions.setCommitAmount}
          commitLoading={actions.commitLoading}
          onCommit={() => void actions.handleCommitToInvoice()}
        />

        <InvoiceHistory
          history={history}
          historyLoading={historyLoading}
          historyError={historyError}
        />

        <div className="space-y-3">
          <RepaymentPanel
            isOwner={isOwner}
            status={metadata.status}
            fundedInvoice={fundedInvoice}
            remainingDue={remainingDue}
            fullyRepaid={fullyRepaid}
            actionLoading={actions.actionLoading}
            repayAmount={actions.repayAmount}
            setRepayAmount={actions.setRepayAmount}
            repaySimulation={actions.repaySimulation}
            onRepay={() => void actions.handleRepay()}
          />

          {isAdmin && (metadata.status === 'Pending' || metadata.status === 'Verified') && (
            <Link
              href="/admin/invoices"
              className="block w-full px-5 py-3 border border-brand-border text-white font-semibold rounded-xl hover:border-brand-gold/50 transition-colors text-center"
            >
              Open funding queue
            </Link>
          )}

          {isOwner && metadata.status === 'Pending' && (
            <div className="p-4 bg-brand-gold/10 border border-brand-gold/20 rounded-xl text-sm text-brand-muted">
              Your invoice is pending review. Once approved, the pool will fund it and USDC will be
              sent to your wallet.
            </div>
          )}

          <DisputePanel
            invoiceId={invoice.id}
            status={metadata.status}
            isOwner={isOwner}
            walletAddress={wallet.address}
            disputeRecord={disputeRecord}
            arbitrationCase={arbitrationCase}
            arbitrationEvidence={arbitrationEvidence}
            actionLoading={actions.actionLoading}
            defaultDisputeModalOpen={actions.defaultDisputeModalOpen}
            setDefaultDisputeModalOpen={actions.setDefaultDisputeModalOpen}
            defaultDisputeEvidence={actions.defaultDisputeEvidence}
            setDefaultDisputeEvidence={actions.setDefaultDisputeEvidence}
            defaultDisputeRespondent={actions.defaultDisputeRespondent}
            setDefaultDisputeRespondent={actions.setDefaultDisputeRespondent}
            onRaiseDefaultDispute={() => void actions.handleRaiseDefaultDispute()}
            disputeModalOpen={actions.disputeModalOpen}
            setDisputeModalOpen={actions.setDisputeModalOpen}
            disputeReason={actions.disputeReason}
            setDisputeReason={actions.setDisputeReason}
            onDispute={() => void actions.handleDispute()}
            newEvidenceHash={actions.newEvidenceHash}
            setNewEvidenceHash={actions.setNewEvidenceHash}
            onSubmitEvidence={() => void actions.handleSubmitArbitrationEvidence()}
          />
        </div>
      </div>
    </div>
  );
}
