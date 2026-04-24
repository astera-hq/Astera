'use client';

import { useEffect, useState, useCallback } from 'react';
import { useParams } from 'next/navigation';
import Link from 'next/link';
import {
  getInvoice,
  getInvoiceMetadata,
  getPoolConfig,
  getFundedInvoice,
  buildRepayTx,
  submitTx,
} from '@/lib/contracts';
import {
  formatUSDC,
  formatDate,
  daysUntil,
  truncateAddress,
  rpc,
  INVOICE_CONTRACT_ID,
  POOL_CONTRACT_ID,
  scValToNative,
  xdr,
} from '@/lib/stellar';
import { projectedInterestStroops, formatApyPercent } from '@/lib/apy';
import type { FundedInvoice, Invoice, InvoiceMetadata, PoolConfig } from '@/lib/types';
import { useStore } from '@/lib/store';

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
  const [actionError, setActionError] = useState<string | null>(null);

  const loadHistory = useCallback(async (invoiceId: number) => {
    if (!INVOICE_CONTRACT_ID || !POOL_CONTRACT_ID) {
      setHistory([]);
      setHistoryError('Transaction history requires configured contract IDs.');
      return;
    }

    setHistoryLoading(true);
    setHistoryError(null);

    try {
      const latest = await rpc.getLatestLedger();
      const startLedger = Math.max(1, latest.sequence - 50_000);
      const response = await rpc.getEvents({
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

      const [poolResult, fundedResult] = await Promise.allSettled([
        getPoolConfig(),
        getFundedInvoice(numId),
      ]);

      setPoolConfig(poolResult.status === 'fulfilled' ? poolResult.value : null);
      setFundedInvoice(fundedResult.status === 'fulfilled' ? fundedResult.value : null);

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

  async function handleRepay() {
    if (!wallet.address || !invoice) return;

    setActionLoading(true);
    setActionError(null);

    try {
      const xdr = await buildRepayTx({ payer: wallet.address, invoiceId: invoice.id });
      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
        networkPassphrase: 'Test SDF Network ; September 2015',
        address: wallet.address,
      });

      if (signError) throw new Error(signError.message || 'Signing rejected.');

      await submitTx(signedTxXdr);
      await loadInvoice();
    } catch (e) {
      const msg = e instanceof Error ? e.message : 'Failed to repay invoice.';
      setActionError(msg);
      console.error(e);
    } finally {
      setActionLoading(false);
    }
  }

  if (loading) {
    return (
      <div className="min-h-screen pt-24 px-6">
        <div className="max-w-2xl mx-auto space-y-4">
          {[1, 2, 3].map((n) => (
            <div key={n} className="h-24 bg-brand-card rounded-2xl animate-pulse" />
          ))}
        </div>
      </div>
    );
  }

  if (error || !invoice || !metadata) {
    return (
      <div className="min-h-screen pt-24 px-6 flex flex-col items-center justify-center text-center">
        <p className="text-red-400 mb-4">{error ?? 'Invoice not found.'}</p>
        <Link href="/dashboard" className="text-brand-gold hover:underline text-sm">
          Back to Dashboard
        </Link>
      </div>
    );
  }

  return (
    <div className="min-h-screen pt-24 pb-16 px-6">
      <div className="max-w-2xl mx-auto">
        <Link
          href="/dashboard"
          className="text-brand-muted hover:text-white text-sm mb-6 inline-flex items-center gap-2 transition-colors"
        >
          ← Back to Dashboard
        </Link>

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
                  {formatUSDC(fundedInvoice.principal + projectedInterest)}
                </span>
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
          {actionError && (
            <div className="p-4 bg-red-900/20 border border-red-800/50 rounded-xl text-sm text-red-300">
              {actionError}
            </div>
          )}

          {isOwner && metadata.status === 'Funded' && fundedInvoice && (
            <button
              onClick={() => void handleRepay()}
              disabled={actionLoading}
              className="w-full px-5 py-3 bg-brand-gold text-brand-dark font-semibold rounded-xl hover:bg-brand-amber transition-colors disabled:opacity-60"
            >
              {actionLoading ? 'Processing repayment...' : 'Repay invoice'}
            </button>
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
        </div>
      </div>
    </div>
  );
}
