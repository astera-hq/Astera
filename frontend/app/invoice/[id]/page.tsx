'use client';

import { useEffect, useState, useCallback } from 'react';
import { useParams } from 'next/navigation';
import Link from 'next/link';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { Skeleton } from '@/components/Skeleton';
import ConfirmActionModal from '@/components/ConfirmActionModal';
import {
  getInvoice,
  getInvoiceMetadata,
  getPoolConfig,
  getFundedInvoice,
  buildRepayTx,
  buildDisputeTx,
  getCollateralConfig,
  getCollateralDeposit,
  buildDepositCollateralTx,
  submitTx,
} from '@/lib/contracts';
import {
  formatUSDC,
  formatDate,
  daysUntil,
  truncateAddress,
  rpcGetEvents,
  rpcGetLatestLedger,
  INVOICE_CONTRACT_ID,
  POOL_CONTRACT_ID,
  USDC_TOKEN_ID,
  scValToNative,
  xdr,
} from '@/lib/stellar';
import { projectedInterestStroops, formatApyPercent } from '@/lib/apy';
import type {
  FundedInvoice,
  Invoice,
  InvoiceMetadata,
  PoolConfig,
  CollateralConfig,
  CollateralDeposit,
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
    const namespace = scValToNative(topics[0]) as string;
    const action = scValToNative(topics[1]) as string;
    const value = event.value ? scValToNative(event.value) : null;

    if (contract === INVOICE_CONTRACT_ID && namespace === 'INVOICE') {
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

    if (contract === POOL_CONTRACT_ID && namespace === 'POOL') {
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
  const [collateralConfig, setCollateralConfig] = useState<CollateralConfig | null>(null);
  const [collateralDeposit, setCollateralDeposit] = useState<CollateralDeposit | null>(null);
  const [collateralAmount, setCollateralAmount] = useState<string>('');
  const [collateralLoading, setCollateralLoading] = useState(false);

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

      const [poolResult, fundedResult, collateralConfigResult, collateralDepositResult] =
        await Promise.allSettled([
          getPoolConfig(),
          getFundedInvoice(numId),
          getCollateralConfig(),
          getCollateralDeposit(numId),
        ]);

      setPoolConfig(poolResult.status === 'fulfilled' ? poolResult.value : null);
      setFundedInvoice(fundedResult.status === 'fulfilled' ? fundedResult.value : null);
      setCollateralConfig(
        collateralConfigResult.status === 'fulfilled' ? collateralConfigResult.value : null,
      );
      const deposit =
        collateralDepositResult.status === 'fulfilled' ? collateralDepositResult.value : null;
      setCollateralDeposit(deposit);

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
  const isOwner = invoice ? wallet.address === invoice.owner : false;
  const isAdmin = poolConfig ? wallet.address === poolConfig.admin : false;
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

    const requiredAmount = (metadata!.amount * BigInt(collateralConfig.collateralBps)) / 10_000n;
    const amount = collateralAmount ? BigInt(collateralAmount) : requiredAmount;
    if (amount <= 0n) {
      toast.error('Please enter a valid collateral amount.');
      return;
    }

    const token = USDC_TOKEN_ID;
    if (!token) {
      toast.error('No accepted token configured. Check NEXT_PUBLIC_USDC_TOKEN_ID.');
      return;
    }

    setCollateralLoading(true);
    try {
      const txXdr = await buildDepositCollateralTx({
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
      toast.success('Collateral posted successfully!');
      setCollateralAmount('');
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

  function exportInvoicePDF() {
    window.print();
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
          onClick={exportInvoicePDF}
          className="print:hidden text-sm text-brand-muted hover:text-white ml-4"
        >
          Export PDF
        </button>

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

          <div className="text-4xl font-bold gradient-text mb-6">{formatUSDC(metadata.amount)}</div>

          <div className="grid grid-cols-2 gap-4 text-sm">
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
              <p className="font-mono text-xs text-white break-all">{invoice.owner}</p>
            </div>
            {metadata.description && (
              <div className="col-span-2">
                <p className="text-brand-muted mb-1">Description</p>
                <p className="text-sm">{metadata.description}</p>
              </div>
            )}
          </div>
        </div>

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

                if (collateralDeposit && collateralDeposit.settled) {
                  if (metadata.status === 'Defaulted') {
                    return (
                      <div className="p-4 bg-red-900/20 border border-red-800/50 rounded-xl text-sm">
                        <p className="font-semibold text-red-400 mb-1">Collateral Seized</p>
                        <p className="text-brand-muted">
                          Your collateral of {formatUSDC(collateralDeposit.amount)} was seized
                          because this invoice was not repaid. The funds were redistributed to pool
                          investors to offset the default loss.
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
                  return (
                    <div className="space-y-3 text-sm">
                      <div className="flex justify-between">
                        <span className="text-brand-muted">Required ({pct}%)</span>
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
                    </div>
                  );
                }

                if (!isOwner || metadata.status === 'Paid' || metadata.status === 'Defaulted') {
                  return (
                    <div className="space-y-3 text-sm">
                      <div className="flex justify-between">
                        <span className="text-brand-muted">Required ({pct}%)</span>
                        <span className="font-medium">{formatUSDC(requiredAmount)}</span>
                      </div>
                      <p className="text-brand-muted">No collateral has been posted.</p>
                    </div>
                  );
                }

                return (
                  <div className="space-y-4">
                    <div className="flex justify-between text-sm">
                      <span className="text-brand-muted">Required ({pct}% of invoice)</span>
                      <span className="font-medium">{formatUSDC(requiredAmount)}</span>
                    </div>
                    <div>
                      <label className="block text-xs text-brand-muted mb-1">
                        Collateral Amount (USDC)
                      </label>
                      <input
                        type="text"
                        value={collateralAmount}
                        onChange={(e) => setCollateralAmount(e.target.value)}
                        placeholder={`Required: ${formatUSDC(requiredAmount)}`}
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
              <button
                onClick={() => void handleRepay()}
                disabled={actionLoading || !repayAmount}
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
            </div>
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
