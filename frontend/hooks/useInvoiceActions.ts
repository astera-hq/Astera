'use client';

/**
 * Action handlers + their local UI state for the invoice detail page.
 *
 * Calls with an SDK equivalent go through the configured `@astera/sdk`
 * clients in `@/lib/sdk`. The remaining invoice-specific mutations (private
 * flag verification dispute, post-default dispute) still use their
 * `@/lib/contracts` builders because the SDK has no equivalent.
 */

import { useCallback, useState } from 'react';
import type { Dispatch, SetStateAction } from 'react';
import toast from 'react-hot-toast';
import { sdk, sdkSigner } from '@/lib/sdk';
import {
  buildDisputeTx,
  buildSetInvoicePrivateTx,
  buildRaiseDisputeTx,
  submitTx,
} from '@/lib/contracts';
import {
  NETWORK,
  nativeToScVal,
  Address,
  toStroops,
  POOL_CONTRACT_ID,
  USDC_TOKEN_ID,
} from '@/lib/stellar';
import { simulateContractCall } from '@/lib/simulateFee';
import { useTransactionSimulation } from '@/hooks/useTransactionSimulation';
import type { SimulationState } from '@/hooks/useTransactionSimulation';
import { downloadInvoicePDF } from '@/components/InvoicePDF';
import { parseStellarAddress } from '@/lib/types';
import type { StellarAddress } from '@/lib/types';
import type { InvoiceDetailData } from './useInvoiceDetail';

export interface InvoiceActions {
  actionLoading: boolean;
  repayAmount: string;
  setRepayAmount: Dispatch<SetStateAction<string>>;
  repaySimulation: SimulationState;
  handleRepay: () => Promise<void>;
  collateralAmount: string;
  setCollateralAmount: Dispatch<SetStateAction<string>>;
  collateralLoading: boolean;
  collateralModalOpen: boolean;
  setCollateralModalOpen: Dispatch<SetStateAction<boolean>>;
  handleDepositCollateral: () => Promise<void>;
  disputeModalOpen: boolean;
  setDisputeModalOpen: Dispatch<SetStateAction<boolean>>;
  disputeReason: string;
  setDisputeReason: Dispatch<SetStateAction<string>>;
  handleDispute: () => Promise<void>;
  defaultDisputeModalOpen: boolean;
  setDefaultDisputeModalOpen: Dispatch<SetStateAction<boolean>>;
  defaultDisputeEvidence: string;
  setDefaultDisputeEvidence: Dispatch<SetStateAction<string>>;
  defaultDisputeRespondent: string;
  setDefaultDisputeRespondent: Dispatch<SetStateAction<string>>;
  handleRaiseDefaultDispute: () => Promise<void>;
  newEvidenceHash: string;
  setNewEvidenceHash: Dispatch<SetStateAction<string>>;
  handleSubmitArbitrationEvidence: () => Promise<void>;
  pdfLoading: boolean;
  exportInvoicePDF: () => Promise<void>;
  handleShare: () => Promise<void>;
  privacyLoading: boolean;
  handleTogglePrivate: () => Promise<void>;
  commitAmount: string;
  setCommitAmount: Dispatch<SetStateAction<string>>;
  commitLoading: boolean;
  handleCommitToInvoice: () => Promise<void>;
}

export function useInvoiceActions(detail: InvoiceDetailData): InvoiceActions {
  const {
    wallet,
    invoice,
    metadata,
    fundedInvoice,
    isOwner,
    remainingDue,
    fullyRepaid,
    collateralConfig,
    collateralDeposit,
    collateralToken,
    crossAssetRequiredAmount,
    requiredCollateralAmount,
    arbitrationCase,
    isPrivate,
    setIsPrivate,
    reload,
  } = detail;

  const [actionLoading, setActionLoading] = useState(false);
  const [repayAmount, setRepayAmount] = useState<string>('');
  const [disputeModalOpen, setDisputeModalOpen] = useState(false);
  const [disputeReason, setDisputeReason] = useState('');
  // #1043: post-default dispute -> arbitration flow. Distinct from the
  // pre-funding "Raise Dispute" button above (which disputes an oracle
  // verification, not a default) — see the invoice contract's two separate
  // dispute mechanisms.
  const [defaultDisputeModalOpen, setDefaultDisputeModalOpen] = useState(false);
  const [defaultDisputeEvidence, setDefaultDisputeEvidence] = useState('');
  const [defaultDisputeRespondent, setDefaultDisputeRespondent] = useState('');
  const [newEvidenceHash, setNewEvidenceHash] = useState('');
  const [collateralAmount, setCollateralAmount] = useState<string>('');
  const [collateralLoading, setCollateralLoading] = useState(false);
  const [collateralModalOpen, setCollateralModalOpen] = useState(false);
  const [pdfLoading, setPdfLoading] = useState(false);
  const [privacyLoading, setPrivacyLoading] = useState(false);
  const [commitAmount, setCommitAmount] = useState<string>('');
  const [commitLoading, setCommitLoading] = useState(false);

  const simulateRepay = useCallback(() => {
    if (!wallet.address || !invoice) return null;
    const amount = repayAmount ? BigInt(repayAmount) : remainingDue;
    if (amount <= 0n) return null;
    return simulateContractCall(
      POOL_CONTRACT_ID,
      'repay_invoice',
      [
        nativeToScVal(invoice.id, { type: 'u64' }),
        new Address(wallet.address).toScVal(),
        nativeToScVal(amount, { type: 'i128' }),
      ],
      wallet.address,
    );
  }, [wallet.address, invoice, repayAmount, remainingDue]);

  const repaySimulation = useTransactionSimulation(
    simulateRepay,
    isOwner &&
      metadata?.status === 'Funded' &&
      !!fundedInvoice &&
      !fullyRepaid &&
      !!wallet.address &&
      !!invoice &&
      (!!repayAmount || remainingDue > 0n),
  );

  async function handleRepay() {
    if (!wallet.address || !invoice || !fundedInvoice) return;

    const amount = repayAmount ? BigInt(repayAmount) : remainingDue;
    if (amount <= 0n) {
      toast.error('Please enter a valid repayment amount.');
      return;
    }
    if (amount > remainingDue) {
      toast.error('Payment exceeds remaining amount due.');
      return;
    }

    setActionLoading(true);

    try {
      await sdk.pool.repay({
        signer: await sdkSigner(wallet.address),
        payer: wallet.address,
        invoiceId: invoice.id,
        amount,
      });
      const msg =
        amount === remainingDue
          ? 'Invoice repaid successfully!'
          : 'Partial payment recorded successfully!';
      toast.success(msg);
      setRepayAmount('');
      await reload();
    } catch (e) {
      const msg = e instanceof Error ? e.message : 'Failed to repay invoice.';
      toast.error(msg);
      console.error(e);
    } finally {
      setActionLoading(false);
    }
  }

  async function handleDepositCollateral() {
    if (!wallet.address || !invoice || !collateralConfig) return;

    // #1036: a top-up must match the asset already posted (the contract
    // enforces this); only a fresh deposit lets the SME pick which accepted
    // asset to post via the selector below.
    const token = collateralDeposit ? collateralDeposit.token : collateralToken;
    if (!token) {
      toast.error('No accepted token configured.');
      return;
    }

    const defaultAmount =
      !collateralDeposit && token !== USDC_TOKEN_ID && crossAssetRequiredAmount != null
        ? crossAssetRequiredAmount
        : (requiredCollateralAmount ?? 0n);
    const amount = collateralAmount ? BigInt(collateralAmount) : defaultAmount;
    if (amount <= 0n) {
      toast.error('Please enter a valid collateral amount.');
      return;
    }

    setCollateralLoading(true);
    try {
      const signer = await sdkSigner(wallet.address);
      if (collateralDeposit) {
        await sdk.pool.topUpCollateral({
          signer,
          invoiceId: invoice.id,
          depositor: wallet.address,
          token,
          amount,
        });
      } else {
        await sdk.pool.depositCollateral({
          signer,
          invoiceId: invoice.id,
          depositor: wallet.address,
          token,
          amount,
        });
      }
      toast.success(
        collateralDeposit
          ? 'Collateral topped up successfully!'
          : 'Collateral posted successfully!',
      );
      setCollateralAmount('');
      setCollateralModalOpen(false);
      await reload();
    } catch (e) {
      const msg = e instanceof Error ? e.message : 'Failed to post collateral.';
      toast.error(msg);
      console.error(e);
    } finally {
      setCollateralLoading(false);
    }
  }

  async function handleDispute() {
    if (!wallet.address || !invoice || !disputeReason.trim()) return;

    setActionLoading(true);

    try {
      const xdr = await buildDisputeTx({
        disputer: wallet.address,
        invoiceId: invoice.id,
        reason: disputeReason,
      });
      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
        networkPassphrase: NETWORK,
        address: wallet.address,
      });

      if (signError) throw new Error(signError.message || 'Signing rejected.');

      await submitTx(signedTxXdr);
      toast.success('Dispute raised successfully. Your invoice is now under review.');
      setDisputeModalOpen(false);
      setDisputeReason('');
      await reload();
    } catch (e) {
      const msg = e instanceof Error ? e.message : 'Failed to raise dispute.';
      toast.error(msg);
      console.error(e);
    } finally {
      setActionLoading(false);
    }
  }

  // #1043: post-default dispute — routes to arbitration when the invoice
  // clears the configured value threshold. `respondent` stands in for the
  // debtor's side of the case (invoice.debtor has no wallet today; see the
  // arbitration contract's docs for why this is a deliberate limitation).
  async function handleRaiseDefaultDispute() {
    if (!wallet.address || !invoice || !defaultDisputeEvidence.trim()) return;
    let respondent: StellarAddress;
    try {
      respondent = parseStellarAddress(defaultDisputeRespondent.trim());
    } catch {
      toast.error('Enter a valid respondent Stellar address.');
      return;
    }

    setActionLoading(true);
    try {
      const xdr = await buildRaiseDisputeTx({
        borrower: wallet.address as StellarAddress,
        invoiceId: invoice.id,
        evidenceHash: defaultDisputeEvidence,
        respondent,
      });
      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
        networkPassphrase: NETWORK,
        address: wallet.address,
      });
      if (signError) throw new Error(signError.message || 'Signing rejected.');

      await submitTx(signedTxXdr);
      toast.success('Dispute raised. Above-threshold disputes are routed to arbitration.');
      setDefaultDisputeModalOpen(false);
      setDefaultDisputeEvidence('');
      setDefaultDisputeRespondent('');
      await reload();
    } catch (e) {
      const msg = e instanceof Error ? e.message : 'Failed to raise dispute.';
      toast.error(msg);
      console.error(e);
    } finally {
      setActionLoading(false);
    }
  }

  async function handleSubmitArbitrationEvidence() {
    if (!wallet.address || !arbitrationCase || !newEvidenceHash.trim()) return;
    setActionLoading(true);
    try {
      await sdk.arbitration.submitEvidence({
        signer: await sdkSigner(wallet.address),
        submitter: wallet.address,
        caseId: arbitrationCase.id,
        evidenceHash: newEvidenceHash,
      });
      toast.success('Evidence submitted.');
      setNewEvidenceHash('');
      await reload();
    } catch (e) {
      const msg = e instanceof Error ? e.message : 'Failed to submit evidence.';
      toast.error(msg);
      console.error(e);
    } finally {
      setActionLoading(false);
    }
  }

  async function exportInvoicePDF() {
    if (!invoice || !metadata) return;

    setPdfLoading(true);
    try {
      await downloadInvoicePDF(invoice, metadata);
    } catch (e) {
      toast.error('Failed to generate PDF.');
      console.error(e);
    } finally {
      setPdfLoading(false);
    }
  }

  async function handleShare() {
    if (!invoice) return;
    const url = `${window.location.origin}/invoice/${invoice.id}`;
    try {
      await navigator.clipboard.writeText(url);
      toast.success('Invoice link copied to clipboard!');
    } catch (e) {
      toast.error('Failed to copy link.');
      console.error(e);
    }
  }

  async function handleTogglePrivate() {
    if (!wallet.address || !invoice) return;

    const next = !isPrivate;
    setPrivacyLoading(true);
    try {
      const xdr = await buildSetInvoicePrivateTx({
        owner: wallet.address,
        invoiceId: invoice.id,
        private: next,
      });
      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
        networkPassphrase: NETWORK,
        address: wallet.address,
      });

      if (signError) throw new Error(signError.message || 'Signing rejected.');

      await submitTx(signedTxXdr);
      setIsPrivate(next);
      toast.success(next ? 'Invoice is now private.' : 'Invoice is now public.');
    } catch (e) {
      const msg = e instanceof Error ? e.message : 'Failed to update sharing setting.';
      toast.error(msg);
      console.error(e);
    } finally {
      setPrivacyLoading(false);
    }
  }

  async function handleCommitToInvoice() {
    if (!wallet.address || !invoice) return;

    const amountNum = Number(commitAmount);
    if (!commitAmount || !Number.isFinite(amountNum) || amountNum <= 0) {
      toast.error('Enter a valid amount to fund.');
      return;
    }

    setCommitLoading(true);
    try {
      const investor = parseStellarAddress(wallet.address);
      await sdk.pool.commitToInvoice({
        signer: await sdkSigner(wallet.address),
        investor,
        invoiceId: invoice.id,
        amount: toStroops(amountNum),
      });
      toast.success(`Committed to invoice #${invoice.id}.`);
      setCommitAmount('');
      await reload();
    } catch (e) {
      const msg = e instanceof Error ? e.message : 'Failed to fund invoice.';
      toast.error(msg);
      console.error(e);
    } finally {
      setCommitLoading(false);
    }
  }

  return {
    actionLoading,
    repayAmount,
    setRepayAmount,
    repaySimulation,
    handleRepay,
    collateralAmount,
    setCollateralAmount,
    collateralLoading,
    collateralModalOpen,
    setCollateralModalOpen,
    handleDepositCollateral,
    disputeModalOpen,
    setDisputeModalOpen,
    disputeReason,
    setDisputeReason,
    handleDispute,
    defaultDisputeModalOpen,
    setDefaultDisputeModalOpen,
    defaultDisputeEvidence,
    setDefaultDisputeEvidence,
    defaultDisputeRespondent,
    setDefaultDisputeRespondent,
    handleRaiseDefaultDispute,
    newEvidenceHash,
    setNewEvidenceHash,
    handleSubmitArbitrationEvidence,
    pdfLoading,
    exportInvoicePDF,
    handleShare,
    privacyLoading,
    handleTogglePrivate,
    commitAmount,
    setCommitAmount,
    commitLoading,
    handleCommitToInvoice,
  };
}
