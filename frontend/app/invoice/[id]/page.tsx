'use client';

import { useEffect, useState, useCallback, useMemo } from 'react';
import { useParams } from 'next/navigation';
import Link from 'next/link';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { Skeleton } from '@/components/Skeleton';
import ConfirmActionModal from '@/components/ConfirmActionModal';
import WalletConnect from '@/components/WalletConnect';
import { downloadInvoicePDF } from '@/components/InvoicePDF';
// Pre-existing build breakage found while working this branch: this page
// uses <GlossaryTerm> below but never imported it, so `tsc`/the Next.js
// build failed at HEAD. Not part of any of the four assigned issues.
import GlossaryTerm from '@/components/GlossaryTerm';
import {
  getInvoice,
  getInvoiceMetadata,
  getPoolConfig,
  getFundedInvoice,
  buildRepayTx,
  buildDisputeTx,
  getCollateralConfig,
  getCollateralDeposit,
  getLiveCollateralRatio,
  getCollateralRiskConfig,
  getAtRiskSince,
  getAcceptedTokens,
  getAssetPrice,
  buildDepositCollateralTx,
  buildTopUpCollateralTx,
  isInvoicePrivate,
  buildSetInvoicePrivateTx,
  getFullCreditScore,
  getCoFundingRound,
  buildCommitToInvoiceTx,
  submitTx,
  buildRaiseDisputeTx,
  getDispute,
  getArbitrationCaseByInvoice,
  getArbitrationEvidence,
  buildSubmitEvidenceTx,
} from '@/lib/contracts';
import {
  formatAmount,
  formatUSDC,
  formatDate,
  daysUntil,
  truncateAddress,
  rpcGetEvents,
  rpcGetLatestLedger,
  toStroops,
  INVOICE_CONTRACT_ID,
  POOL_CONTRACT_ID,
  USDC_TOKEN_ID,
  nativeToScVal,
  Address,
  scValToNative,
  xdr,
  stablecoinLabel,
  fromStroops,
} from '@/lib/stellar';
import { simulateContractCall } from '@/lib/simulateFee';
import { useTransactionSimulation } from '@/hooks/useTransactionSimulation';
import EstimatedFee from '@/components/EstimatedFee';
import BorrowerCreditBadge from '@/components/BorrowerCreditBadge';
import InsuranceCoverageBadge from '@/components/InsuranceCoverageBadge';
import { projectedInterestStroops, formatApyPercent } from '@/lib/apy';
import { parseStellarAddress } from '@/lib/types';
import type {
  FundedInvoice,
  Invoice,
  InvoiceMetadata,
  PoolConfig,
  CollateralConfig,
  CollateralDeposit,
  CollateralRiskConfig,
  CoFundingRound,
  FullCreditScore,
  DisputeRecord,
  DisputeCase,
  EvidenceEntry,
} from '@/lib/types';

type InvoiceEventKind = 'created' | 'funded' | 'paid' | 'defaulted' | 'repaid';

interface InvoiceEvent {
  kind: InvoiceEventKind;
  label: string;
  detail: string;
  txHash: string;
  ledger: number;
  timestamp: string;
}

interface TransactionStep {
  label: string;
  done: boolean;
  ts: number;
}

interface RawEvent {
  contractId?: string;
  topic?: xdr.ScVal[];
  value?: xdr.ScVal;
  pagingToken?: string;
  ledgerClosedAt?: string;
  ledger?: number;
  txHash?: string;
}

function parseInvoiceHistory(rawEvents: RawEvent[], invoiceId: number): InvoiceEvent[] {
  const events: InvoiceEvent[] = [];

  for (const event of rawEvents) {
    const topics = event.topic ?? [];
    if (topics.length < 2) continue;

    const contract = event.contractId ?? '';
    const t0 = topics[0];
    const t1 = topics[1];
    if (!t0 || !t1) continue;
    const namespace = scValToNative(t0) as string;
    const action = scValToNative(t1) as string;
    const value = event.value ? scValToNative(event.value) : null;

    if (contract === INVOICE_CONTRACT_ID && namespace === 'invoice') {
      if (action === 'created') {
        const [id, owner, amount] = Array.isArray(value) ? value : [value];
        if (Number(id) !== invoiceId) continue;
        events.push({
          kind: 'created',
          label: 'Invoice created',
          detail: `${owner ? `${String(owner)} created the invoice` : 'Invoice created'}${amount ? ` for ${formatUSDC(BigInt(String(amount)))}` : ''}`,
          txHash: event.txHash ?? '',
          ledger: Number(event.ledger ?? 0),
          timestamp: event.ledgerClosedAt ?? '',
        });
      } else if (action === 'funded' && Number(value) === invoiceId) {
        events.push({
          kind: 'funded',
          label: 'Invoice funded',
          detail: 'Pool funded this invoice.',
          txHash: event.txHash ?? '',
          ledger: Number(event.ledger ?? 0),
          timestamp: event.ledgerClosedAt ?? '',
        });
      } else if (action === 'paid' && Number(value) === invoiceId) {
        events.push({
          kind: 'paid',
          label: 'Invoice repaid',
          detail: 'SME repaid the invoice.',
          txHash: event.txHash ?? '',
          ledger: Number(event.ledger ?? 0),
          timestamp: event.ledgerClosedAt ?? '',
        });
      } else if (action === 'default' && Number(value) === invoiceId) {
        events.push({
          kind: 'defaulted',
          label: 'Invoice defaulted',
          detail: 'Grace period expired before repayment.',
          txHash: event.txHash ?? '',
          ledger: Number(event.ledger ?? 0),
          timestamp: event.ledgerClosedAt ?? '',
        });
      }
    }

    if (contract === POOL_CONTRACT_ID && namespace === 'pool') {
      if (action === 'funded') {
        const [id] = Array.isArray(value) ? value : [value];
        if (Number(id) !== invoiceId) continue;
        events.push({
          kind: 'funded',
          label: 'Pool funded invoice',
          detail: 'Funding moved from the pool to the SME.',
          txHash: event.txHash ?? '',
          ledger: Number(event.ledger ?? 0),
          timestamp: event.ledgerClosedAt ?? '',
        });
      } else if (action === 'repaid') {
        const [id] = Array.isArray(value) ? value : [value];
        if (Number(id) !== invoiceId) continue;
        events.push({
          kind: 'repaid',
          label: 'Pool received repayment',
          detail: 'Repayment was recorded by the pool contract.',
          txHash: event.txHash ?? '',
          ledger: Number(event.ledger ?? 0),
          timestamp: event.ledgerClosedAt ?? '',
        });
      }
    }
  }

  return events.sort((a, b) => b.ledger - a.ledger);
}

export default function InvoiceDetailPage() {
  const { id } = useParams<{ id: string }>();
  const { wallet } = useStore();
  const [invoice, setInvoice] = useState<Invoice | null>(null);
  const [metadata, setMetadata] = useState<InvoiceMetadata | null>(null);
  const [poolConfig, setPoolConfig] = useState<PoolConfig | null>(null);
  const [fundedInvoice, setFundedInvoice] = useState<FundedInvoice | null>(null);
  const [history, setHistory] = useState<InvoiceEvent[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyError, setHistoryError] = useState<string | null>(null);
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
  const [disputeRecord, setDisputeRecord] = useState<DisputeRecord | null>(null);
  const [arbitrationCase, setArbitrationCase] = useState<DisputeCase | null>(null);
  const [arbitrationEvidence, setArbitrationEvidence] = useState<EvidenceEntry[]>([]);
  const [newEvidenceHash, setNewEvidenceHash] = useState('');
  const [collateralConfig, setCollateralConfig] = useState<CollateralConfig | null>(null);
  const [collateralDeposit, setCollateralDeposit] = useState<CollateralDeposit | null>(null);
  // #1036: live, oracle-priced ratio (bps) and the risk config it's judged
  // against — null when the invoice isn't funded yet or no oracle is configured.
  const [liveCollateralRatioBps, setLiveCollateralRatioBps] = useState<number | null>(null);
  const [collateralRiskConfig, setCollateralRiskConfig] = useState<CollateralRiskConfig | null>(
    null,
  );
  // #1036: ledger timestamp the position was first flagged at-risk — tracked
  // entirely in the auction satellite's own storage, not on pool's
  // CollateralDeposit (it's monitoring state, not fund-movement state).
  const [atRiskSince, setAtRiskSince] = useState<number | null>(null);
  const [collateralAmount, setCollateralAmount] = useState<string>('');
  const [collateralLoading, setCollateralLoading] = useState(false);
  const [collateralModalOpen, setCollateralModalOpen] = useState(false);
  // #1036: which accepted token the SME is posting as collateral — may differ
  // from the invoice's funding token (assumed USDC throughout this page, same
  // as every other amount already rendered via formatUSDC).
  const [acceptedTokens, setAcceptedTokens] = useState<string[]>([]);
  const [collateralToken, setCollateralToken] = useState<string>(USDC_TOKEN_ID);
  const [crossAssetRequiredAmount, setCrossAssetRequiredAmount] = useState<bigint | null>(null);
  const [crossAssetPriceLoading, setCrossAssetPriceLoading] = useState(false);
  const [pdfLoading, setPdfLoading] = useState(false);
  const [isPrivate, setIsPrivate] = useState(false);
  const [privacyLoading, setPrivacyLoading] = useState(false);
  const [creditScore, setCreditScore] = useState<FullCreditScore | null>(null);
  const [coFundingRound, setCoFundingRound] = useState<CoFundingRound | null>(null);
  const [commitAmount, setCommitAmount] = useState<string>('');
  const [commitLoading, setCommitLoading] = useState(false);

  // #1036: required collateral in the invoice's funding token (assumed USDC —
  // see the comment on `collateralToken` above), independent of which asset
  // is actually being posted.
  const requiredCollateralAmount = useMemo(() => {
    if (!collateralConfig || !metadata) return null;
    return (metadata.amount * BigInt(collateralConfig.collateralBps)) / 10_000n;
  }, [collateralConfig, metadata]);

  // #1036: when the SME picks a collateral asset other than the funding
  // token, convert the funding-token-denominated requirement into that
  // asset's native amount via live oracle prices, so the form can show/submit
  // a sensible default instead of forcing manual price math.
  useEffect(() => {
    if (!requiredCollateralAmount || requiredCollateralAmount <= 0n) {
      setCrossAssetRequiredAmount(null);
      return;
    }
    if (!collateralToken || !USDC_TOKEN_ID || collateralToken === USDC_TOKEN_ID) {
      setCrossAssetRequiredAmount(null);
      return;
    }
    let cancelled = false;
    setCrossAssetPriceLoading(true);
    (async () => {
      try {
        const [fundingPrice, assetPrice] = await Promise.all([
          getAssetPrice(USDC_TOKEN_ID),
          getAssetPrice(collateralToken),
        ]);
        if (cancelled) return;
        setCrossAssetRequiredAmount(
          assetPrice > 0n ? (requiredCollateralAmount * fundingPrice) / assetPrice : null,
        );
      } catch (e) {
        console.error(e);
        if (!cancelled) setCrossAssetRequiredAmount(null);
      } finally {
        if (!cancelled) setCrossAssetPriceLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [collateralToken, requiredCollateralAmount]);

  const loadHistory = useCallback(async (invoiceId: number) => {
    if (!INVOICE_CONTRACT_ID || !POOL_CONTRACT_ID) {
      setHistory([]);
      setHistoryError('Transaction history requires configured contract IDs.');
      return;
    }

    setHistoryLoading(true);
    setHistoryError(null);

    try {
      const latest = await rpcGetLatestLedger();
      const startLedger = Math.max(1, latest.sequence - 50_000);
      const response = await rpcGetEvents({
        startLedger,
        limit: 200,
        filters: [
          {
            type: 'contract',
            contractIds: [INVOICE_CONTRACT_ID, POOL_CONTRACT_ID],
          },
        ],
      });

      const raw = (response.events ?? []) as RawEvent[];
      setHistory(parseInvoiceHistory(raw, invoiceId));
    } catch (e) {
      setHistory([]);
      setHistoryError('Unable to load transaction history.');
      console.error(e);
    } finally {
      setHistoryLoading(false);
    }
  }, []);

  const loadInvoice = useCallback(async () => {
    setLoading(true);
    setError(null);

    try {
      const numId = Number(id);
      if (!Number.isFinite(numId)) {
        throw new Error('Invalid invoice id.');
      }

      const [inv, meta] = await Promise.all([getInvoice(numId), getInvoiceMetadata(numId)]);
      setInvoice(inv);
      setMetadata(meta);

      const [
        poolResult,
        fundedResult,
        collateralConfigResult,
        collateralDepositResult,
        liveRatioResult,
        collateralRiskConfigResult,
        atRiskSinceResult,
        acceptedTokensResult,
        privateResult,
        creditScoreResult,
        coFundingResult,
        disputeResult,
        arbitrationCaseResult,
      ] = await Promise.allSettled([
        getPoolConfig(),
        getFundedInvoice(numId),
        getCollateralConfig(),
        getCollateralDeposit(numId),
        getLiveCollateralRatio(numId),
        getCollateralRiskConfig(),
        getAtRiskSince(numId),
        getAcceptedTokens(),
        isInvoicePrivate(numId),
        getFullCreditScore(inv.owner),
        getCoFundingRound(numId),
        getDispute(numId),
        getArbitrationCaseByInvoice(numId),
      ]);

      setPoolConfig(poolResult.status === 'fulfilled' ? poolResult.value : null);
      setFundedInvoice(fundedResult.status === 'fulfilled' ? fundedResult.value : null);
      setCollateralConfig(
        collateralConfigResult.status === 'fulfilled' ? collateralConfigResult.value : null,
      );
      const deposit =
        collateralDepositResult.status === 'fulfilled' ? collateralDepositResult.value : null;
      setCollateralDeposit(deposit);
      // #1036: getLiveCollateralRatio reverts (e.g. no oracle configured, or the
      // invoice isn't funded yet) far more often than it succeeds today — that's
      // expected, not an error, so it's swallowed the same way the other
      // best-effort reads above are.
      setLiveCollateralRatioBps(
        liveRatioResult.status === 'fulfilled' ? liveRatioResult.value : null,
      );
      setCollateralRiskConfig(
        collateralRiskConfigResult.status === 'fulfilled' ? collateralRiskConfigResult.value : null,
      );
      setAtRiskSince(atRiskSinceResult.status === 'fulfilled' ? atRiskSinceResult.value : null);
      const tokens =
        acceptedTokensResult.status === 'fulfilled' && Array.isArray(acceptedTokensResult.value)
          ? acceptedTokensResult.value
          : [];
      setAcceptedTokens(tokens);
      setCollateralToken((prev) => {
        if (prev && tokens.includes(prev)) return prev;
        return USDC_TOKEN_ID && tokens.includes(USDC_TOKEN_ID) ? USDC_TOKEN_ID : (tokens[0] ?? '');
      });
      setIsPrivate(privateResult.status === 'fulfilled' ? privateResult.value : false);
      setCreditScore(creditScoreResult.status === 'fulfilled' ? creditScoreResult.value : null);
      setCoFundingRound(coFundingResult.status === 'fulfilled' ? coFundingResult.value : null);
      setDisputeRecord(disputeResult.status === 'fulfilled' ? disputeResult.value : null);
      const arbCase =
        arbitrationCaseResult.status === 'fulfilled' ? arbitrationCaseResult.value : null;
      setArbitrationCase(arbCase);
      if (arbCase) {
        getArbitrationEvidence(arbCase.id)
          .then(setArbitrationEvidence)
          .catch(() => setArbitrationEvidence([]));
      } else {
        setArbitrationEvidence([]);
      }

      void loadHistory(numId);
    } catch (e) {
      setError('Invoice not found or contracts are not deployed.');
      console.error(e);
    } finally {
      setLoading(false);
    }
  }, [id, loadHistory]);

  useEffect(() => {
    void loadInvoice();
  }, [loadInvoice]);

  const days = metadata ? daysUntil(metadata.dueDate) : 0;
  const isOwner = Boolean(invoice && wallet.connected && wallet.address === invoice.owner);
  const isAdmin = Boolean(poolConfig && wallet.connected && wallet.address === poolConfig.admin);
  const canViewInvoice = !isPrivate || isOwner;
  const statusSteps: TransactionStep[] = invoice
    ? [
        { label: 'Created', done: true, ts: invoice.createdAt },
        {
          label: 'Funded',
          done: invoice.fundedAt > 0,
          ts: invoice.fundedAt,
        },
        {
          label: invoice.status === 'Defaulted' ? 'Defaulted' : 'Paid',
          done: invoice.status === 'Paid' || invoice.status === 'Defaulted',
          ts: invoice.paidAt,
        },
      ]
    : [];

  const projectedInterest =
    fundedInvoice && poolConfig
      ? projectedInterestStroops(
          fundedInvoice.principal,
          poolConfig.yieldBps,
          Math.max(0, Math.ceil((fundedInvoice.dueDate - fundedInvoice.fundedAt) / 86_400)),
        )
      : 0n;
  const accruedInterest =
    fundedInvoice && poolConfig
      ? projectedInterestStroops(
          fundedInvoice.principal,
          poolConfig.yieldBps,
          Math.max(0, Math.floor((Date.now() / 1000 - fundedInvoice.fundedAt) / 86_400)),
        )
      : 0n;
  const interestProgress =
    fundedInvoice && metadata && fundedInvoice.dueDate > fundedInvoice.fundedAt
      ? Math.min(
          100,
          Math.max(
            0,
            ((Date.now() / 1000 - fundedInvoice.fundedAt) /
              (fundedInvoice.dueDate - fundedInvoice.fundedAt)) *
              100,
          ),
        )
      : 0;

  // Calculate remaining amount due for partial repayments
  const remainingDue =
    fundedInvoice && poolConfig
      ? fundedInvoice.principal +
        projectedInterest +
        fundedInvoice.factoringFee -
        fundedInvoice.repaidAmount
      : 0n;
  const fullyRepaid = remainingDue <= 0n;

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
      const xdr = await buildRepayTx({ payer: wallet.address, invoiceId: invoice.id, amount });
      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
        networkPassphrase: 'Test SDF Network ; September 2015',
        address: wallet.address,
      });

      if (signError) throw new Error(signError.message || 'Signing rejected.');

      await submitTx(signedTxXdr);
      const msg =
        amount === remainingDue
          ? 'Invoice repaid successfully!'
          : 'Partial payment recorded successfully!';
      toast.success(msg);
      setRepayAmount('');
      await loadInvoice();
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
      const buildCollateralTx = collateralDeposit
        ? buildTopUpCollateralTx
        : buildDepositCollateralTx;
      const txXdr = await buildCollateralTx({
        invoiceId: invoice.id,
        depositor: wallet.address,
        token,
        amount,
      });
      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error: signError } = await freighter.signTransaction(txXdr, {
        networkPassphrase: 'Test SDF Network ; September 2015',
        address: wallet.address,
      });

      if (signError) throw new Error(signError.message || 'Signing rejected.');

      await submitTx(signedTxXdr);
      toast.success(
        collateralDeposit
          ? 'Collateral topped up successfully!'
          : 'Collateral posted successfully!',
      );
      setCollateralAmount('');
      setCollateralModalOpen(false);
      await loadInvoice();
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
        networkPassphrase: 'Test SDF Network ; September 2015',
        address: wallet.address,
      });

      if (signError) throw new Error(signError.message || 'Signing rejected.');

      await submitTx(signedTxXdr);
      toast.success('Dispute raised successfully. Your invoice is now under review.');
      setDisputeModalOpen(false);
      setDisputeReason('');
      await loadInvoice();
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
    let respondent: string;
    try {
      respondent = parseStellarAddress(defaultDisputeRespondent.trim());
    } catch {
      toast.error('Enter a valid respondent Stellar address.');
      return;
    }

    setActionLoading(true);
    try {
      const xdr = await buildRaiseDisputeTx({
        borrower: wallet.address,
        invoiceId: invoice.id,
        evidenceHash: defaultDisputeEvidence,
        respondent,
      });
      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
        networkPassphrase: 'Test SDF Network ; September 2015',
        address: wallet.address,
      });
      if (signError) throw new Error(signError.message || 'Signing rejected.');

      await submitTx(signedTxXdr);
      toast.success('Dispute raised. Above-threshold disputes are routed to arbitration.');
      setDefaultDisputeModalOpen(false);
      setDefaultDisputeEvidence('');
      setDefaultDisputeRespondent('');
      await loadInvoice();
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
      const xdr = await buildSubmitEvidenceTx({
        submitter: wallet.address,
        caseId: arbitrationCase.id,
        evidenceHash: newEvidenceHash,
      });
      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
        networkPassphrase: 'Test SDF Network ; September 2015',
        address: wallet.address,
      });
      if (signError) throw new Error(signError.message || 'Signing rejected.');

      await submitTx(signedTxXdr);
      toast.success('Evidence submitted.');
      setNewEvidenceHash('');
      await loadInvoice();
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
        networkPassphrase: 'Test SDF Network ; September 2015',
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
      const xdr = await buildCommitToInvoiceTx({
        investor,
        invoiceId: invoice.id,
        amount: toStroops(amountNum),
      });
      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
        networkPassphrase: 'Test SDF Network ; September 2015',
        address: wallet.address,
      });

      if (signError) throw new Error(signError.message || 'Signing rejected.');

      await submitTx(signedTxXdr);
      toast.success(`Committed to invoice #${invoice.id}.`);
      setCommitAmount('');
      await loadInvoice();
    } catch (e) {
      const msg = e instanceof Error ? e.message : 'Failed to fund invoice.';
      toast.error(msg);
      console.error(e);
    } finally {
      setCommitLoading(false);
    }
  }

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
        <button
          onClick={() => void exportInvoicePDF()}
          disabled={pdfLoading}
          className="print:hidden text-sm text-brand-muted hover:text-white ml-4 disabled:opacity-60"
        >
          {pdfLoading ? 'Generating PDF...' : 'Export PDF'}
        </button>
        <button
          onClick={() => void handleShare()}
          className="print:hidden text-sm text-brand-muted hover:text-white ml-4"
        >
          Share
        </button>
        {isOwner && (
          <button
            onClick={() => void handleTogglePrivate()}
            disabled={privacyLoading}
            className="print:hidden text-sm text-brand-muted hover:text-white ml-4 disabled:opacity-60"
          >
            {privacyLoading ? 'Updating...' : isPrivate ? 'Make Public' : 'Make Private'}
          </button>
        )}

        <div className="p-6 bg-brand-card border border-brand-border rounded-2xl mb-6">
          {metadata.image ? (
            <div className="mb-6 rounded-xl overflow-hidden border border-brand-border bg-brand-dark">
              <img src={metadata.image} alt="" className="w-full h-40 object-cover" />
            </div>
          ) : null}
          <div className="flex items-start justify-between mb-6 gap-4">
            <div className="min-w-0">
              <p className="text-xs text-brand-muted mb-1">
                {metadata.symbol} · Invoice #{invoice.id}
              </p>
              <h1 className="text-2xl font-bold">{metadata.name}</h1>
              <p className="text-brand-muted mt-1">{metadata.debtor}</p>
            </div>
            <span
              className={`text-sm font-medium px-3 py-1.5 rounded-full flex-shrink-0 badge-${metadata.status.toLowerCase()}`}
            >
              {metadata.status}
            </span>
          </div>

          <div className="text-4xl font-bold gradient-text mb-6">
            {formatAmount(metadata.amount, metadata.decimals)}
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 text-sm">
            <div>
              <p className="text-brand-muted mb-1">Due Date</p>
              <p className="font-medium">{formatDate(metadata.dueDate)}</p>
            </div>
            <div>
              <p className="text-brand-muted mb-1">Time Remaining</p>
              <p
                className={`font-medium ${
                  days < 0 ? 'text-red-400' : days <= 7 ? 'text-yellow-400' : 'text-white'
                }`}
              >
                {days < 0 ? `${Math.abs(days)} days overdue` : `${days} days`}
              </p>
            </div>
            <div className="col-span-2">
              <p className="text-brand-muted mb-1">Owner</p>
              {/* #775: only the owner sees their own full address; every other
                  viewer (the public sharing case) only sees it abbreviated. */}
              <p className="font-mono text-xs text-white break-all">
                {isOwner ? invoice.owner : truncateAddress(invoice.owner)}
              </p>
            </div>
            <div className="col-span-2">
              <BorrowerCreditBadge borrower={invoice.owner} />
            </div>
            {metadata.description && (
              <div className="col-span-2">
                <p className="text-brand-muted mb-1">Description</p>
                <p className="text-sm">{metadata.description}</p>
              </div>
            )}
          </div>
        </div>

        {creditScore && (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl mb-6">
            <div className="flex items-center justify-between gap-4">
              <div>
                <h2 className="text-lg font-semibold mb-1">Borrower Credit Score</h2>
                <p className="text-xs text-brand-muted">
                  {creditScore.totalInvoices > 0
                    ? `${creditScore.paidOnTime}/${creditScore.totalInvoices} invoices paid on time`
                    : 'No repayment history yet'}
                </p>
              </div>
              <div className="text-right">
                <div className="text-3xl font-bold gradient-text">{creditScore.blendedScore}</div>
                <span className="text-xs text-brand-muted">
                  {creditScore.totalInvoices === 0
                    ? 'No history'
                    : creditScore.blendedScore >= 750
                      ? 'Excellent'
                      : creditScore.blendedScore >= 650
                        ? 'Good'
                        : creditScore.blendedScore >= 550
                          ? 'Fair'
                          : 'Building'}
                </span>
              </div>
            </div>
          </div>
        )}

        {fundedInvoice && (
          <div className="mb-6">
            <InsuranceCoverageBadge fundedInvoice={fundedInvoice} />
          </div>
        )}

        <div className="p-6 bg-brand-card border border-brand-border rounded-2xl mb-6">
          <div className="flex items-center justify-between gap-4 mb-6">
            <h2 className="text-lg font-semibold">Timeline</h2>
            <span
              className={`text-xs px-2.5 py-1 rounded-full badge-${metadata.status.toLowerCase()}`}
            >
              {metadata.status}
            </span>
          </div>
          <div className="space-y-4">
            {statusSteps.map((step, i) => (
              <div key={step.label} className="flex items-center gap-4">
                <div
                  className={`w-8 h-8 rounded-full flex items-center justify-center flex-shrink-0 text-xs font-bold ${
                    step.done ? 'bg-brand-gold text-brand-dark' : 'bg-brand-border text-brand-muted'
                  }`}
                >
                  {step.done ? '✓' : i + 1}
                </div>
                <div className="flex-1 flex justify-between">
                  <span className={step.done ? 'text-white font-medium' : 'text-brand-muted'}>
                    {step.label}
                  </span>
                  {step.done && step.ts > 0 && (
                    <span className="text-brand-muted text-sm">{formatDate(step.ts)}</span>
                  )}
                </div>
              </div>
            ))}
          </div>
        </div>

        {poolConfig && (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl mb-6">
            <h2 className="text-lg font-semibold mb-4">Pool Details</h2>
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 text-sm">
              <div>
                <p className="text-brand-muted mb-1">Pool Contract</p>
                <p className="font-mono text-xs break-all">{invoice.poolContract || '—'}</p>
              </div>
              <div>
                <p className="text-brand-muted mb-1">Pool Admin</p>
                <p className="font-mono text-xs break-all">{truncateAddress(poolConfig.admin)}</p>
              </div>
              <div>
                <p className="text-brand-muted mb-1">APY</p>
                <p>{formatApyPercent(poolConfig.yieldBps)}%</p>
              </div>
              <div>
                <p className="text-brand-muted mb-1">Factoring Fee</p>
                <p>{(poolConfig.factoringFeeBps / 100).toFixed(2)}%</p>
              </div>
            </div>
            {fundedInvoice && (
              <div className="mt-4 border-t border-brand-border pt-4 text-sm grid grid-cols-1 sm:grid-cols-2 gap-4">
                <div>
                  <p className="text-brand-muted mb-1">Funding Token</p>
                  <p className="font-mono text-xs break-all">
                    {truncateAddress(fundedInvoice.token)}
                  </p>
                </div>
                <div>
                  <p className="text-brand-muted mb-1">Principal</p>
                  <p>{formatUSDC(fundedInvoice.principal)}</p>
                </div>
              </div>
            )}
          </div>
        )}

        {fundedInvoice && poolConfig && (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl mb-6">
            <h2 className="text-lg font-semibold mb-4">Interest Accrual</h2>
            <div className="space-y-3 text-sm">
              <div className="flex items-center justify-between">
                <span className="text-brand-muted">Accrued interest</span>
                <span className="font-medium">{formatUSDC(accruedInterest)}</span>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-brand-muted">Projected interest to due date</span>
                <span className="font-medium">{formatUSDC(projectedInterest)}</span>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-brand-muted">Estimated total due</span>
                <span className="font-semibold">
                  {formatUSDC(
                    fundedInvoice.principal + projectedInterest + fundedInvoice.factoringFee,
                  )}
                </span>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-brand-muted">Already repaid</span>
                <span className="font-medium text-green-400">
                  {formatUSDC(fundedInvoice.repaidAmount)}
                </span>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-brand-muted font-semibold">Remaining due</span>
                <span className="font-semibold text-brand-gold">{formatUSDC(remainingDue)}</span>
              </div>
              <div className="h-2 bg-brand-border rounded-full overflow-hidden">
                <div
                  className="h-full bg-brand-gold rounded-full transition-all"
                  style={{ width: `${interestProgress}%` }}
                />
              </div>
              <p className="text-xs text-brand-muted">
                Estimated against {poolConfig.yieldBps / 100}% APY over the remaining term.
              </p>
            </div>
          </div>
        )}

        {collateralConfig &&
          metadata &&
          (metadata.amount >= collateralConfig.threshold || collateralDeposit) && (
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
                              Your collateral of {formatUSDC(collateralDeposit.amount)} was
                              liquidated on {formatDate(collateralDeposit.seizedAt)} after its
                              oracle-priced value fell below the danger threshold and wasn&apos;t
                              topped up in time.
                            </>
                          ) : (
                            <>
                              Your collateral of {formatUSDC(collateralDeposit.amount)} was seized
                              because this invoice was not repaid. The funds were redistributed to
                              pool investors to offset the default loss.
                            </>
                          )}
                        </p>
                      </div>
                    );
                  }
                  return (
                    <div className="p-4 bg-green-900/20 border border-green-800/50 rounded-xl text-sm text-green-400">
                      Collateral of {formatUSDC(collateralDeposit.amount)} was returned to your
                      wallet after full repayment. ✓
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
                                <span className="text-white">{formatDate(topUpDeadline)}</span> or
                                it may be liquidated by any keeper.
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
                          {ratio.toFixed(0)}% {isAtRisk ? '⚠' : '✓'} (target{' '}
                          {targetRatio.toFixed(0)}%)
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
                        Collateral is locked until the invoice is fully repaid, at which point it
                        will be automatically returned to your wallet.
                      </div>
                      {isOwner && !fullyRepaid && (
                        <button
                          onClick={() => setCollateralModalOpen(true)}
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
                        Required (<GlossaryTerm id="collateral-ratio">{pct}%</GlossaryTerm> of
                        invoice)
                      </span>
                      <span className="font-medium">{formatUSDC(requiredAmount)}</span>
                    </div>
                    {acceptedTokens.length > 1 && (
                      <div>
                        <label className="block text-xs text-brand-muted mb-1">
                          Collateral Asset
                        </label>
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
                          Posting an asset other than the invoice&apos;s funding token is priced via
                          the pool&apos;s oracle — the required amount updates live with its price.
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
                      Warning: Collateral will be locked until this invoice is fully repaid or
                      resolved.
                    </p>
                    <button
                      onClick={() => void handleDepositCollateral()}
                      disabled={collateralLoading}
                      className="w-full px-5 py-3 bg-brand-gold text-brand-dark font-semibold rounded-xl hover:bg-brand-amber transition-colors disabled:opacity-60"
                    >
                      {collateralLoading ? 'Posting collateral...' : 'Post Collateral'}
                    </button>
                  </div>
                );
              })()}
            </div>
          )}

        {coFundingRound && coFundingRound.status === 'Open' && (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl mb-6 space-y-4">
            <div>
              <h2 className="text-lg font-semibold mb-1">Fund This Invoice</h2>
              <p className="text-xs text-brand-muted">
                Join other lenders co-funding this invoice. Committed capital earns a proportional
                share of this invoice&apos;s principal and interest.
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

            {!wallet.connected ? (
              <div className="space-y-2">
                <p className="text-sm text-brand-muted">
                  Connect your wallet to fund this invoice.
                </p>
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
                  onClick={() => void handleCommitToInvoice()}
                  disabled={commitLoading || !commitAmount}
                  className="px-5 py-2.5 bg-brand-gold text-brand-dark rounded-xl text-sm font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50"
                >
                  {commitLoading ? 'Funding...' : 'Fund This Invoice'}
                </button>
              </div>
            )}
          </div>
        )}

        {historyLoading ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl mb-6">
            <div className="h-5 bg-brand-border rounded w-40 mb-4 animate-pulse" />
            <div className="space-y-3">
              {[1, 2, 3].map((n) => (
                <div key={n} className="h-14 bg-brand-dark rounded-xl animate-pulse" />
              ))}
            </div>
          </div>
        ) : (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl mb-6">
            <div className="flex items-center justify-between gap-4 mb-4">
              <h2 className="text-lg font-semibold">Transaction History</h2>
              {historyError && <span className="text-xs text-brand-muted">{historyError}</span>}
            </div>
            {history.length === 0 ? (
              <p className="text-sm text-brand-muted">No related transactions found.</p>
            ) : (
              <div className="space-y-3">
                {history.map((event) => (
                  <div
                    key={`${event.kind}-${event.ledger}-${event.txHash}`}
                    className="p-4 rounded-xl border border-brand-border bg-brand-dark/60"
                  >
                    <div className="flex items-start justify-between gap-4">
                      <div>
                        <p className="font-medium text-white">{event.label}</p>
                        <p className="text-sm text-brand-muted mt-1">{event.detail}</p>
                      </div>
                      {event.txHash && (
                        <a
                          href={`https://stellar.expert/explorer/testnet/tx/${event.txHash}`}
                          target="_blank"
                          rel="noreferrer"
                          className="text-xs text-brand-gold hover:underline break-all"
                        >
                          {truncateAddress(event.txHash)}
                        </a>
                      )}
                    </div>
                    {event.timestamp && (
                      <p className="text-xs text-brand-muted mt-2">
                        {new Date(event.timestamp).toLocaleString('en-US', {
                          year: 'numeric',
                          month: 'short',
                          day: 'numeric',
                          hour: '2-digit',
                          minute: '2-digit',
                        })}
                      </p>
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        <div className="space-y-3">
          {isOwner && metadata.status === 'Funded' && fundedInvoice && !fullyRepaid && (
            <div className="p-4 bg-brand-card border border-brand-border rounded-2xl space-y-3">
              <label className="block text-sm text-brand-muted mb-1">Repayment Amount (USDC)</label>
              <input
                type="text"
                value={repayAmount}
                onChange={(e) => setRepayAmount(e.target.value)}
                placeholder={`Max: ${formatUSDC(remainingDue)}`}
                disabled={actionLoading}
                className="w-full px-4 py-2 bg-brand-dark border border-brand-border rounded-lg text-white placeholder-brand-muted focus:border-brand-gold focus:outline-none disabled:opacity-60"
              />
              <EstimatedFee simulation={repaySimulation} />

              <button
                onClick={() => void handleRepay()}
                disabled={actionLoading || !repayAmount || repaySimulation.status === 'loading'}
                className="w-full px-5 py-3 bg-brand-gold text-brand-dark font-semibold rounded-xl hover:bg-brand-amber transition-colors disabled:opacity-60"
              >
                {actionLoading
                  ? 'Processing payment...'
                  : repayAmount
                    ? `Pay ${formatUSDC(BigInt(repayAmount))}`
                    : 'Pay full amount'}
              </button>
            </div>
          )}

          {isOwner && metadata.status === 'Funded' && fundedInvoice && fullyRepaid && (
            <div className="p-4 bg-green-900/20 border border-green-800/50 rounded-xl text-sm text-green-400 text-center">
              Invoice fully repaid ✓
            </div>
          )}

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

          {metadata.status === 'Disputed' && (
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
                Our team will review your dispute within 3-5 business days. You will be notified
                once the issue is resolved.
              </p>

              {arbitrationCase && (
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
                    wallet.address &&
                    (wallet.address === arbitrationCase.claimant ||
                      wallet.address === arbitrationCase.respondent) && (
                      <div className="flex gap-2">
                        <input
                          type="text"
                          value={newEvidenceHash}
                          onChange={(e) => setNewEvidenceHash(e.target.value)}
                          placeholder="Evidence hash / URI"
                          className="flex-1 bg-brand-dark border border-brand-border rounded-lg px-3 py-2 text-xs text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold"
                        />
                        <button
                          onClick={() => void handleSubmitArbitrationEvidence()}
                          disabled={actionLoading || !newEvidenceHash.trim()}
                          className="px-3 py-2 bg-brand-gold text-brand-dark rounded-lg text-xs font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50"
                        >
                          Submit
                        </button>
                      </div>
                    )}
                </div>
              )}
              {!arbitrationCase && disputeRecord && disputeRecord.outcome === 'Pending' && (
                <p className="mt-2 text-xs text-brand-muted">
                  This dispute is below the arbitration value threshold and stays on the standard
                  admin review path.
                </p>
              )}
            </div>
          )}

          {isOwner && metadata.status === 'Defaulted' && (
            <>
              <button
                onClick={() => setDefaultDisputeModalOpen(true)}
                className="w-full px-5 py-3 border border-red-700/50 text-red-400 font-semibold rounded-xl hover:bg-red-900/20 transition-colors"
              >
                Dispute Default
              </button>
              {defaultDisputeModalOpen && (
                <ConfirmActionModal
                  title={`Dispute Default for Invoice #${invoice?.id}`}
                  description="Disputing a default flag files a claim (e.g. non-delivery, wrongful default). Invoices above the configured value threshold are routed to staked-juror arbitration instead of a unilateral admin decision."
                  confirmLabel="Raise Dispute"
                  onConfirm={() => void handleRaiseDefaultDispute()}
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
            (metadata.status === 'Verified' ||
              metadata.status === 'Funded' ||
              metadata.status === 'AwaitingVerification') && (
              <button
                onClick={() => setDisputeModalOpen(true)}
                className="w-full px-5 py-3 border border-red-700/50 text-red-400 font-semibold rounded-xl hover:bg-red-900/20 transition-colors"
              >
                Raise Dispute
              </button>
            )}

          {disputeModalOpen && (
            <ConfirmActionModal
              title={`Raise Dispute for Invoice #${invoice?.id}`}
              description="Disputing an invoice will flag it for manual review. This action cannot be undone. Please provide a clear reason for the dispute."
              confirmLabel="Confirm Dispute"
              onConfirm={() => void handleDispute()}
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
        </div>
      </div>
    </div>
  );
}
