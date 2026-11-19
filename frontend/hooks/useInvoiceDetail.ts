'use client';

/**
 * Data-loading hook for the invoice detail page.
 *
 * Reads with an SDK equivalent go through the configured `@astera/sdk`
 * clients in `@/lib/sdk`; invoice-specific reads the SDK does not expose stay
 * on `@/lib/contracts`.
 */

import { useCallback, useEffect, useMemo, useState } from 'react';
import type { Dispatch, SetStateAction } from 'react';
import { useParams } from 'next/navigation';
import { useStore } from '@/lib/store';
import { sdk } from '@/lib/sdk';
import {
  getAcceptedTokens,
  getAssetPrice,
  isInvoicePrivate,
  getFullCreditScore,
  getDispute,
  getArbitrationCaseByInvoice,
} from '@/lib/contracts';
import {
  daysUntil,
  formatUSDC,
  rpcGetEvents,
  rpcGetLatestLedger,
  INVOICE_CONTRACT_ID,
  POOL_CONTRACT_ID,
  USDC_TOKEN_ID,
  scValToNative,
  xdr,
} from '@/lib/stellar';
import { projectedInterestStroops } from '@/lib/apy';
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
  WalletState,
} from '@/lib/types';

// The SDK returns the raw contract structs; the frontend's historical type
// aliases predate the SDK and use the same runtime shape. These thin adapters
// keep call sites typed against the frontend types while the source of truth
// for every ABI call is now the SDK client.
const readInvoice = (id: number) =>
  sdk.invoice.getInvoice(id).then((v) => v as unknown as Invoice);
const readMetadata = (id: number) =>
  sdk.invoice.getMetadata(id).then((v) => v as unknown as InvoiceMetadata);
const readPoolConfig = () => sdk.pool.getConfig().then((v) => v as unknown as PoolConfig);
const readFundedInvoice = (id: number) =>
  sdk.pool.getFundedInvoice(id).then((v) => v as unknown as FundedInvoice | null);
const readCollateralConfig = () =>
  sdk.pool.getCollateralConfig().then((v) => v as unknown as CollateralConfig);
const readCollateralDeposit = (id: number) =>
  sdk.pool.getCollateralDeposit(id).then((v) => v as unknown as CollateralDeposit | null);
const readCollateralRiskConfig = () =>
  sdk.auction.getCollateralRiskConfig().then((v) => v as unknown as CollateralRiskConfig);
const readCoFundingRound = (id: number) =>
  sdk.pool.getCoFundingRound(id).then((v) => v as unknown as CoFundingRound | null);
const readArbitrationEvidence = (caseId: number) =>
  sdk.arbitration.getEvidence(caseId).then((v) => v as unknown as EvidenceEntry[]);

export type InvoiceEventKind = 'created' | 'funded' | 'paid' | 'defaulted' | 'repaid';

export interface InvoiceEvent {
  kind: InvoiceEventKind;
  label: string;
  detail: string;
  txHash: string;
  ledger: number;
  timestamp: string;
}

export interface TransactionStep {
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

/** Pure event parser, kept exported so it can be unit-tested in isolation. */
export function parseInvoiceHistory(rawEvents: RawEvent[], invoiceId: number): InvoiceEvent[] {
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

/** Everything the invoice detail page needs to render and derive its UI. */
export interface InvoiceDetailData {
  id: number;
  wallet: WalletState;
  loading: boolean;
  error: string | null;
  invoice: Invoice | null;
  metadata: InvoiceMetadata | null;
  poolConfig: PoolConfig | null;
  fundedInvoice: FundedInvoice | null;
  history: InvoiceEvent[];
  historyLoading: boolean;
  historyError: string | null;
  disputeRecord: DisputeRecord | null;
  arbitrationCase: DisputeCase | null;
  arbitrationEvidence: EvidenceEntry[];
  collateralConfig: CollateralConfig | null;
  collateralDeposit: CollateralDeposit | null;
  liveCollateralRatioBps: number | null;
  collateralRiskConfig: CollateralRiskConfig | null;
  atRiskSince: number | null;
  acceptedTokens: string[];
  collateralToken: string;
  setCollateralToken: Dispatch<SetStateAction<string>>;
  crossAssetRequiredAmount: bigint | null;
  crossAssetPriceLoading: boolean;
  requiredCollateralAmount: bigint | null;
  isPrivate: boolean;
  setIsPrivate: Dispatch<SetStateAction<boolean>>;
  creditScore: FullCreditScore | null;
  coFundingRound: CoFundingRound | null;
  days: number;
  isOwner: boolean;
  isAdmin: boolean;
  canViewInvoice: boolean;
  statusSteps: TransactionStep[];
  projectedInterest: bigint;
  accruedInterest: bigint;
  interestProgress: number;
  remainingDue: bigint;
  fullyRepaid: boolean;
  /** Re-run all reads (used by action handlers after a successful tx). */
  reload: () => Promise<void>;
}

export function useInvoiceDetail(): InvoiceDetailData {
  const { id: rawId } = useParams<{ id: string }>();
  const { wallet } = useStore();
  const id = Number(rawId);

  const [invoice, setInvoice] = useState<Invoice | null>(null);
  const [metadata, setMetadata] = useState<InvoiceMetadata | null>(null);
  const [poolConfig, setPoolConfig] = useState<PoolConfig | null>(null);
  const [fundedInvoice, setFundedInvoice] = useState<FundedInvoice | null>(null);
  const [history, setHistory] = useState<InvoiceEvent[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyError, setHistoryError] = useState<string | null>(null);
  const [disputeRecord, setDisputeRecord] = useState<DisputeRecord | null>(null);
  const [arbitrationCase, setArbitrationCase] = useState<DisputeCase | null>(null);
  const [arbitrationEvidence, setArbitrationEvidence] = useState<EvidenceEntry[]>([]);
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
  const [acceptedTokens, setAcceptedTokens] = useState<string[]>([]);
  const [collateralToken, setCollateralToken] = useState<string>(USDC_TOKEN_ID);
  const [crossAssetRequiredAmount, setCrossAssetRequiredAmount] = useState<bigint | null>(null);
  const [crossAssetPriceLoading, setCrossAssetPriceLoading] = useState(false);
  const [isPrivate, setIsPrivate] = useState(false);
  const [creditScore, setCreditScore] = useState<FullCreditScore | null>(null);
  const [coFundingRound, setCoFundingRound] = useState<CoFundingRound | null>(null);

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
      if (!Number.isFinite(id)) {
        throw new Error('Invalid invoice id.');
      }

      const [inv, meta] = await Promise.all([readInvoice(id), readMetadata(id)]);
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
        readPoolConfig(),
        readFundedInvoice(id),
        readCollateralConfig(),
        readCollateralDeposit(id),
        sdk.auction.getLiveCollateralRatio(id),
        readCollateralRiskConfig(),
        sdk.auction.getAtRiskSince(id),
        getAcceptedTokens(),
        isInvoicePrivate(id),
        getFullCreditScore(inv.owner),
        readCoFundingRound(id),
        getDispute(id),
        getArbitrationCaseByInvoice(id),
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
        readArbitrationEvidence(arbCase.id)
          .then(setArbitrationEvidence)
          .catch(() => setArbitrationEvidence([]));
      } else {
        setArbitrationEvidence([]);
      }

      void loadHistory(id);
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

  return {
    id,
    wallet,
    loading,
    error,
    invoice,
    metadata,
    poolConfig,
    fundedInvoice,
    history,
    historyLoading,
    historyError,
    disputeRecord,
    arbitrationCase,
    arbitrationEvidence,
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
    requiredCollateralAmount,
    isPrivate,
    setIsPrivate,
    creditScore,
    coFundingRound,
    days,
    isOwner,
    isAdmin,
    canViewInvoice,
    statusSteps,
    projectedInterest,
    accruedInterest,
    interestProgress,
    remainingDue,
    fullyRepaid,
    reload: loadInvoice,
  };
}
