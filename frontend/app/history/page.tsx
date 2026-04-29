'use client';

import { useEffect, useState, useCallback } from 'react';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { HistoryEventSkeleton } from '@/components/Skeleton';
import {
  rpcGetEvents,
  rpcGetLatestLedger,
  INVOICE_CONTRACT_ID,
  POOL_CONTRACT_ID,
  scValToNative,
  formatUSDC,
  fromStroops,
  truncateAddress,
} from '@/lib/stellar';

const EXPLORER_BASE = 'https://stellar.expert/explorer/testnet';
const PAGE_SIZE = 20;
const EVENT_LIMIT = 200;
const INDEXER_URL = process.env.NEXT_PUBLIC_INDEXER_URL || 'http://localhost:3001';

type EventKind =
  | 'invoice_created'
  | 'invoice_funded'
  | 'invoice_paid'
  | 'invoice_defaulted'
  | 'pool_deposit'
  | 'pool_funded'
  | 'pool_repaid'
  | 'pool_withdraw';

interface HistoryEvent {
  id: string;
  kind: EventKind;
  invoiceId?: bigint;
  amount?: bigint;
  interest?: bigint;
  address?: string;
  timestamp: string;
  ledger: number;
  txHash: string;
}

const KIND_LABELS: Record<EventKind, string> = {
  invoice_created: 'Invoice Created',
  invoice_funded: 'Invoice Funded',
  invoice_paid: 'Invoice Repaid',
  invoice_defaulted: 'Invoice Defaulted',
  pool_deposit: 'Pool Deposit',
  pool_funded: 'Pool Funded Invoice',
  pool_repaid: 'Repayment Received',
  pool_withdraw: 'Pool Withdrawal',
};

const KIND_COLORS: Record<EventKind, string> = {
  invoice_created: 'text-blue-400 bg-blue-400/10 border-blue-400/20',
  invoice_funded: 'text-brand-gold bg-brand-gold/10 border-brand-gold/20',
  invoice_paid: 'text-green-400 bg-green-400/10 border-green-400/20',
  invoice_defaulted: 'text-red-400 bg-red-400/10 border-red-400/20',
  pool_deposit: 'text-purple-400 bg-purple-400/10 border-purple-400/20',
  pool_funded: 'text-brand-gold bg-brand-gold/10 border-brand-gold/20',
  pool_repaid: 'text-green-400 bg-green-400/10 border-green-400/20',
  pool_withdraw: 'text-orange-400 bg-orange-400/10 border-orange-400/20',
};

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function parseEvents(rawEvents: any[], walletAddress: string): HistoryEvent[] {
  // First pass: find all invoice IDs this wallet owns (from "created" events)
  const ownedInvoiceIds = new Set<bigint>();
  for (const e of rawEvents) {
    try {
      const contract = e.contractId ?? '';
      if (contract !== INVOICE_CONTRACT_ID) continue;
      const topics = e.topic ?? [];
      if (topics.length < 2) continue;
      const action = scValToNative(topics[1]) as string;
      if (action !== 'created') continue;
      const val = scValToNative(e.value);
      const [id, owner] = Array.isArray(val) ? val : [val, undefined];
      if (String(owner) === walletAddress) {
        ownedInvoiceIds.add(BigInt(id as bigint));
      }
    } catch {
      // skip
    }
  }

  const events: HistoryEvent[] = [];

  for (const e of rawEvents) {
    try {
      const contract = e.contractId ?? '';
      const topics = e.topic ?? [];
      if (topics.length < 2) continue;

      const ns = scValToNative(topics[0]) as string;
      const action = scValToNative(topics[1]) as string;
      const val = scValToNative(e.value);

      let kind: EventKind | null = null;
      let invoiceId: bigint | undefined;
      let amount: bigint | undefined;
      let interest: bigint | undefined;
      let address: string | undefined;
      let relevant = false;

      if (ns === 'INVOICE' && contract === INVOICE_CONTRACT_ID) {
        if (action === 'created') {
          const [id, owner, amt] = Array.isArray(val) ? val : [val, undefined, undefined];
          invoiceId = BigInt(id as bigint);
          amount = amt !== undefined ? BigInt(amt as bigint) : undefined;
          address = String(owner);
          kind = 'invoice_created';
          relevant = String(owner) === walletAddress;
        } else if (action === 'funded') {
          invoiceId = BigInt(val as bigint);
          kind = 'invoice_funded';
          relevant = ownedInvoiceIds.has(invoiceId);
        } else if (action === 'paid') {
          invoiceId = BigInt(val as bigint);
          kind = 'invoice_paid';
          relevant = ownedInvoiceIds.has(invoiceId);
        } else if (action === 'default') {
          invoiceId = BigInt(val as bigint);
          kind = 'invoice_defaulted';
          relevant = ownedInvoiceIds.has(invoiceId);
        }
      } else if (ns === 'POOL' && contract === POOL_CONTRACT_ID) {
        if (action === 'deposit') {
          const [inv, amt] = Array.isArray(val) ? val : [val, undefined];
          address = String(inv);
          amount = amt !== undefined ? BigInt(amt as bigint) : undefined;
          kind = 'pool_deposit';
          relevant = String(inv) === walletAddress;
        } else if (action === 'funded') {
          const [id, sme, principal] = Array.isArray(val) ? val : [val, undefined, undefined];
          invoiceId = BigInt(id as bigint);
          amount = principal !== undefined ? BigInt(principal as bigint) : undefined;
          address = String(sme);
          kind = 'pool_funded';
          relevant = ownedInvoiceIds.has(invoiceId) || String(sme) === walletAddress;
        } else if (action === 'repaid') {
          const [id, principal, int_] = Array.isArray(val) ? val : [val, undefined, undefined];
          invoiceId = BigInt(id as bigint);
          amount = principal !== undefined ? BigInt(principal as bigint) : undefined;
          interest = int_ !== undefined ? BigInt(int_ as bigint) : undefined;
          kind = 'pool_repaid';
          relevant = ownedInvoiceIds.has(invoiceId);
        } else if (action === 'withdraw') {
          const [inv, amt] = Array.isArray(val) ? val : [val, undefined];
          address = String(inv);
          amount = amt !== undefined ? BigInt(amt as bigint) : undefined;
          kind = 'pool_withdraw';
          relevant = String(inv) === walletAddress;
        }
      }

      if (!kind || !relevant) continue;

      events.push({
        id: e.pagingToken ?? `${e.ledger}-${events.length}`,
        kind,
        invoiceId,
        amount,
        interest,
        address,
        timestamp: e.ledgerClosedAt ?? '',
        ledger: Number(e.ledger ?? 0),
        txHash: e.txHash ?? '',
      });
    } catch {
      // skip malformed events
    }
  }

  return events.sort((a, b) => b.ledger - a.ledger);
}

function formatTs(ts: string): string {
  if (!ts) return '—';
  return new Date(ts).toLocaleString('en-US', {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function exportCSV(events: HistoryEvent[]) {
  const header = [
    'Date',
    'Type',
    'Invoice ID',
    'Amount (USD)',
    'Interest (USD)',
    'Address',
    'Tx Hash',
  ];
  const rows = events.map((e) => [
    e.timestamp ? new Date(e.timestamp).toISOString() : '',
    KIND_LABELS[e.kind],
    e.invoiceId !== undefined ? e.invoiceId.toString() : '',
    e.amount !== undefined ? fromStroops(e.amount).toFixed(7) : '',
    e.interest !== undefined ? fromStroops(e.interest).toFixed(7) : '',
    e.address ?? '',
    e.txHash,
  ]);

  const csv = [header, ...rows]
    .map((r) => r.map((cell) => `"${String(cell).replace(/"/g, '""')}"`).join(','))
    .join('\n');

  const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `astera-history-${new Date().toISOString().slice(0, 10)}.csv`;
  a.click();
  URL.revokeObjectURL(url);
}

function exportPDF() {
  window.print();
}

export default function HistoryPage() {
  const { wallet } = useStore();
  const [events, setEvents] = useState<HistoryEvent[]>([]);
  const [loading, setLoading] = useState(true);
  const [visible, setVisible] = useState(PAGE_SIZE);
  const [hasMore, setHasMore] = useState(false);
  const [cursor, setCursor] = useState<string | undefined>(undefined);
  const [loadingMore, setLoadingMore] = useState(false);

  const fetchEvents = useCallback(
    async (append = false, nextCursor?: string) => {
      if (!wallet.connected || !wallet.address) return;

      if (!INVOICE_CONTRACT_ID || !POOL_CONTRACT_ID) {
        toast.error(
          'Contract IDs not configured. Set NEXT_PUBLIC_INVOICE_CONTRACT_ID and NEXT_PUBLIC_POOL_CONTRACT_ID.',
        );
        setLoading(false);
        return;
      }

      if (!append) setLoading(true);
      else setLoadingMore(true);

      try {
        let parsed: HistoryEvent[] = [];
        let eventCount = 0;
        let responseCursor: string | undefined;

        // Try indexer API first (Option A optimization from #240)
        if (INDEXER_URL) {
          try {
            const params = new URLSearchParams({
              limit: EVENT_LIMIT.toString(),
              ...(nextCursor ? { cursor: nextCursor } : {}),
            });
            if (INVOICE_CONTRACT_ID) params.append('contract_id', INVOICE_CONTRACT_ID);
            if (POOL_CONTRACT_ID) params.append('contract_id', POOL_CONTRACT_ID);

            const res = await fetch(`${INDEXER_URL}/events?${params.toString()}`);
            if (res.ok) {
              const data = await res.json();
              const raw = (data.events || []).map((e: any) => ({
                contractId: e.contractId,
                topic: e.topic || [],
                value: e.value,
                ledger: e.ledgerSequence,
                ledgerCloseAt: e.ledgerCloseAt,
                txHash: e.txHash,
              }));
              eventCount = raw.length;
              parsed = parseEvents(raw, wallet.address);
            }
          } catch (indexerErr) {
            console.warn('[History] Indexer unavailable, falling back to Horizon:', indexerErr);
          }
        }

        // Fallback to Horizon RPC if indexer didn't work
        if (parsed.length === 0) {
          const latest = await rpcGetLatestLedger();
          const startLedger = Math.max(1, latest.sequence - 500_000);

          const response = await rpcGetEvents({
            ...(nextCursor ? { cursor: nextCursor } : { startLedger }),
            filters: [
              {
                type: 'contract',
                contractIds: [INVOICE_CONTRACT_ID, POOL_CONTRACT_ID].filter(Boolean) as string[],
              },
            ],
            limit: EVENT_LIMIT,
          });

          const raw = response.events ?? [];
          eventCount = raw.length;
          responseCursor =
            typeof response === 'object' && response !== null && 'cursor' in response
              ? (response as { cursor?: string }).cursor
              : undefined;
          parsed = parseEvents(raw, wallet.address);
        }

        if (append) {
          setEvents((prev) => [...prev, ...parsed]);
        } else {
          setEvents(parsed);
          setVisible(PAGE_SIZE);
        }

        setHasMore(eventCount === EVENT_LIMIT && Boolean(responseCursor));
        setCursor(responseCursor);
      } catch (e) {
        toast.error('Failed to load transaction history. Make sure contracts are deployed.');
        console.error(e);
      } finally {
        setLoading(false);
        setLoadingMore(false);
      }
    },
    [wallet.connected, wallet.address],
  );

  useEffect(() => {
    if (!wallet.connected) {
      setLoading(false);
      return;
    }
    fetchEvents();
  }, [wallet.connected, wallet.address, fetchEvents]);

  const visibleEvents = events.slice(0, visible);
  const canRevealMore = visible < events.length;

  function handleLoadMore() {
    if (canRevealMore) {
      setVisible((v) => v + PAGE_SIZE);
    } else if (hasMore && cursor) {
      fetchEvents(true, cursor);
    }
  }

  return (
    <div className="min-h-screen pt-24 pb-16 px-4 sm:px-6">
      <div className="max-w-3xl mx-auto">
        <div className="mb-8 flex flex-wrap items-start justify-between gap-4">
          <div>
            <h1 className="text-3xl font-bold mb-1">Transaction History</h1>
            <p className="text-brand-muted">On-chain activity for your wallet</p>
          </div>
          {events.length > 0 && (
            <div className="flex gap-2 print:hidden">
              <button
                onClick={() => exportCSV(events)}
                className="px-4 py-2 bg-brand-card border border-brand-border rounded-xl text-sm font-medium hover:border-brand-gold/50 hover:text-brand-gold transition-colors"
              >
                Export CSV
              </button>
              <button
                onClick={exportPDF}
                className="px-4 py-2 bg-brand-card border border-brand-border rounded-xl text-sm font-medium hover:border-brand-gold/50 hover:text-brand-gold transition-colors"
              >
                Export PDF
              </button>
            </div>
          )}
        </div>

        {!wallet.connected ? (
          <div className="flex flex-col items-center justify-center py-32 text-center">
            <div className="text-4xl mb-4">◈</div>
            <h2 className="text-xl font-semibold mb-2">Connect your wallet</h2>
            <p className="text-brand-muted">Connect Freighter to view your transaction history.</p>
          </div>
        ) : loading ? (
          <div className="space-y-3">
            {Array.from({ length: 6 }).map((_, i) => (
              <HistoryEventSkeleton key={i} />
            ))}
          </div>
        ) : events.length === 0 ? (
          <div className="p-12 bg-brand-card border border-brand-border rounded-2xl text-center">
            <p className="text-brand-muted">No on-chain activity found for this wallet.</p>
          </div>
        ) : (
          <>
            <div className="space-y-3">
              {visibleEvents.map((evt) => (
                <div
                  key={evt.id}
                  className="p-4 bg-brand-card border border-brand-border rounded-2xl flex items-start gap-4"
                >
                  <span
                    className={`mt-0.5 shrink-0 text-xs font-semibold px-2.5 py-1 rounded-lg border ${KIND_COLORS[evt.kind]}`}
                  >
                    {KIND_LABELS[evt.kind]}
                  </span>

                  <div className="flex-1 min-w-0">
                    <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                      {evt.invoiceId !== undefined && (
                        <span className="text-sm font-medium">
                          Invoice #{evt.invoiceId.toString()}
                        </span>
                      )}
                      {evt.amount !== undefined && (
                        <span className="text-sm text-brand-gold font-semibold">
                          {formatUSDC(evt.amount)}
                        </span>
                      )}
                      {evt.interest !== undefined && evt.interest > 0n && (
                        <span className="text-xs text-brand-muted">
                          +{formatUSDC(evt.interest)} interest
                        </span>
                      )}
                      {evt.address && evt.kind !== 'invoice_created' && (
                        <span className="text-xs text-brand-muted font-mono">
                          {truncateAddress(evt.address)}
                        </span>
                      )}
                    </div>
                    <div className="flex items-center gap-3 mt-1.5">
                      <span className="text-xs text-brand-muted">{formatTs(evt.timestamp)}</span>
                      {evt.txHash && (
                        <a
                          href={`${EXPLORER_BASE}/tx/${evt.txHash}`}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="text-xs text-brand-gold hover:underline"
                        >
                          View tx ↗
                        </a>
                      )}
                    </div>
                  </div>
                </div>
              ))}
            </div>

            {(canRevealMore || (hasMore && cursor)) && (
              <div className="mt-6 flex justify-center">
                <button
                  onClick={handleLoadMore}
                  disabled={loadingMore}
                  className="px-6 py-2.5 bg-brand-card border border-brand-border rounded-xl text-sm font-medium hover:border-brand-gold/50 hover:text-brand-gold transition-colors disabled:opacity-50"
                >
                  {loadingMore ? 'Loading…' : 'Load more'}
                </button>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
