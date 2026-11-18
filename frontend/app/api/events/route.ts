import { type NextRequest } from 'next/server';
import { INVOICE_CONTRACT_ID, POOL_CONTRACT_ID } from '../../../lib/stellar';
import { consumeTicket } from '@/lib/sse/ticketStore';

export const dynamic = 'force-dynamic';

const POLL_INTERVAL_MS = Number(process.env.NEXT_PUBLIC_SSE_POLL_INTERVAL_MS ?? 10_000);
const MIN_POLL_INTERVAL_MS = 3_000;
const HEARTBEAT_INTERVAL_MS = 30_000;
const INITIAL_LOOKBACK_LEDGERS = 10;
// #1039: source live events from the indexer's Postgres-backed query API
// instead of polling Soroban RPC directly — the indexer already ingests
// every INVOICE/POOL event, so this endpoint just needs to tail it.
const INDEXER_URL = process.env.NEXT_PUBLIC_INDEXER_URL || 'http://localhost:3001';

interface IndexerEventRow {
  id: string;
  contractId: string;
  topic: unknown[];
  value: unknown;
  ledgerSequence: number;
  ledgerCloseAt: string;
  txHash: string;
}

async function fetchIndexerLatestLedger(): Promise<number> {
  const res = await fetch(`${INDEXER_URL}/ledger/latest`, { cache: 'no-store' });
  if (!res.ok) throw new Error(`indexer /ledger/latest returned ${res.status}`);
  const data = (await res.json()) as { latestLedger: number | null };
  return data.latestLedger ?? 0;
}

async function fetchIndexerEvents(contractId: string, afterLedger: number) {
  const url = new URL(`${INDEXER_URL}/events`);
  url.searchParams.set('contract_id', contractId);
  url.searchParams.set('after_ledger', String(afterLedger));
  url.searchParams.set('limit', '200');
  const res = await fetch(url.toString(), { cache: 'no-store' });
  if (!res.ok) throw new Error(`indexer /events returned ${res.status}`);
  const data = (await res.json()) as { events: IndexerEventRow[] };
  return data.events.map((e) => ({
    id: e.id,
    contractId: e.contractId,
    topic: e.topic,
    value: e.value,
    ledger: e.ledgerSequence,
    txHash: e.txHash,
    ledgerCloseAt: e.ledgerCloseAt,
  }));
}

type SseClient = {
  controller: ReadableStreamDefaultController;
  encoder: TextEncoder;
  invoiceId: string | null;
};

let sharedPollerTimer: ReturnType<typeof setInterval> | null = null;
const sharedClients: Set<SseClient> = new Set();
let sharedLastSeenLedger = 0;

async function sharedPoll() {
  if (sharedClients.size === 0) return;

  const contractIds = [INVOICE_CONTRACT_ID, POOL_CONTRACT_ID].filter(Boolean);
  if (contractIds.length === 0) return;

  try {
    if (sharedLastSeenLedger === 0) {
      const latest = await fetchIndexerLatestLedger();
      sharedLastSeenLedger = Math.max(0, latest - INITIAL_LOOKBACK_LEDGERS);
    }

    const batches = await Promise.all(
      contractIds.map((contractId) => fetchIndexerEvents(contractId, sharedLastSeenLedger)),
    );
    const events = batches.flat().sort((a, b) => a.ledger - b.ledger);
    if (events.length === 0) return;

    sharedLastSeenLedger = events.reduce((max, e) => Math.max(max, e.ledger), sharedLastSeenLedger);

    for (const e of events) {
      const eventInvoiceId = (e.value as unknown[] | null)?.[0];
      const payload = JSON.stringify(e);

      for (const client of sharedClients) {
        if (client.invoiceId !== null && String(eventInvoiceId) !== client.invoiceId) continue;
        try {
          client.controller.enqueue(client.encoder.encode(`data: ${payload}\n\n`));
        } catch {
          client.controller.error(new Error('SSE client disconnected'));
        }
      }
    }
  } catch (err) {
    console.error('[SSE /api/events] shared poll error:', err);
    for (const client of sharedClients) {
      try {
        client.controller.enqueue(
          client.encoder.encode(`event: error\ndata: ${JSON.stringify({ type: 'error' })}\n\n`),
        );
      } catch {
        client.controller.error(new Error('SSE client disconnected'));
      }
    }
  }
}

function startSharedPoller(): void {
  if (sharedPollerTimer) return;
  const interval = Math.max(MIN_POLL_INTERVAL_MS, POLL_INTERVAL_MS);
  if (sharedClients.size > 0) {
    void sharedPoll();
  }
  sharedPollerTimer = setInterval(sharedPoll, interval);
}

function stopSharedPollerIfEmpty(): void {
  if (sharedClients.size === 0 && sharedPollerTimer) {
    clearInterval(sharedPollerTimer);
    sharedPollerTimer = null;
    sharedLastSeenLedger = 0;
  }
}

function registerClient(client: SseClient): void {
  sharedClients.add(client);
  startSharedPoller();
}

function unregisterClient(client: SseClient): void {
  sharedClients.delete(client);
  stopSharedPollerIfEmpty();
}

export async function GET(request: NextRequest) {
  const { searchParams } = request.nextUrl;
  const ticket = searchParams.get('ticket');
  const invoiceIdParam = searchParams.get('invoiceId');

  if (!ticket) {
    return new Response('Unauthorized', { status: 401 });
  }

  const account = consumeTicket(ticket);
  if (!account) {
    return new Response('Unauthorized', { status: 401 });
  }

  const contractIds = [INVOICE_CONTRACT_ID, POOL_CONTRACT_ID].filter(Boolean);
  if (contractIds.length === 0) {
    return new Response('Contract IDs not configured', { status: 503 });
  }

  const encoder = new TextEncoder();

  const stream = new ReadableStream({
    start(controller) {
      const client: SseClient = {
        controller,
        encoder,
        invoiceId: invoiceIdParam,
      };

      controller.enqueue(
        encoder.encode(`event: connected\ndata: ${JSON.stringify({ type: 'connected' })}\n\n`),
      );

      registerClient(client);

      const heartbeatTimer = setInterval(() => {
        try {
          controller.enqueue(encoder.encode('event: ping\ndata: \n\n'));
        } catch {
          controller.error(new Error('SSE client disconnected'));
        }
      }, HEARTBEAT_INTERVAL_MS);

      request.signal.addEventListener('abort', () => {
        clearInterval(heartbeatTimer);
        unregisterClient(client);
        try {
          controller.close();
        } catch {
          // already closed
        }
      });
    },
  });

  return new Response(stream, {
    headers: {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache, no-transform',
      Connection: 'keep-alive',
      'X-Accel-Buffering': 'no',
    },
  });
}
