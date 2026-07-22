/**
 * Express REST API for querying indexed Soroban events.
 */

import express from 'express';
import Database from 'better-sqlite3';
import { getEvents } from './db';

export function startApiServer(db: Database.Database, port: number): void {
  const app = express();
  app.use(express.json());

  // Health check
  app.get('/health', (_req, res) => {
    res.json({ status: 'ok', timestamp: new Date().toISOString() });
  });

  // Get events with optional filters
  app.get('/events', (req, res) => {
    try {
      const {
        contract_id,
        contract_type,
        event_type,
        limit = '50',
        offset = '0',
      } = req.query;

      const events = getEvents(db, {
        contractId: contract_id as string | undefined,
        contractType: contract_type as string | undefined,
        eventType: event_type as string | undefined,
        limit: parseInt(limit as string, 10),
        offset: parseInt(offset as string, 10),
      });

      res.json({ events, count: events.length });
    } catch (err: any) {
      res.status(500).json({ error: err.message });
    }
  });

  // Get events by contract
  app.get('/events/contract/:contractId', (req, res) => {
    try {
      const { contractId } = req.params;
      const { limit = '50', offset = '0' } = req.query;

      const events = getEvents(db, {
        contractId,
        limit: parseInt(limit as string, 10),
        offset: parseInt(offset as string, 10),
      });

      res.json({ contractId, events, count: events.length });
    } catch (err: any) {
      res.status(500).json({ error: err.message });
    }
  });

  // Get events by type
  app.get('/events/type/:eventType', (req, res) => {
    try {
      const { eventType } = req.params;
      const { limit = '50', offset = '0' } = req.query;

      const events = getEvents(db, {
        eventType,
        limit: parseInt(limit as string, 10),
        offset: parseInt(offset as string, 10),
      });

      res.json({ eventType, events, count: events.length });
    } catch (err: any) {
      res.status(500).json({ error: err.message });
    }
  });

  // #702: Get all invoice events for a specific SME owner address.
  // Supports an optional ?status= filter (e.g. Funded) which matches against
  // either the indexed event_type (lowercased) or a status field embedded in
  // the event value payload.
  app.get('/api/invoices/by-owner/:address', (req, res) => {
    try {
      const { address } = req.params;
      if (!address || typeof address !== 'string') {
        return res.status(400).json({ error: 'address path param is required' });
      }
      const statusFilter = typeof req.query.status === 'string' ? req.query.status : undefined;

      // Fetch invoice-contract events ordered newest-first; cap to a generous
      // limit so the by-owner scan covers typical SME histories.
      const events = getEvents(db, {
        contractType: 'invoice',
        limit: 1000,
        offset: 0,
      });

      const ownerLower = address.toLowerCase();
      const invoices = events
        .filter((evt) => {
          const value = evt.value;
          if (!value) return false;
          // Match the owner address across common shapes the indexer may store:
          // raw string, { owner }, { sme }, or topic-embedded address strings.
          const candidates: any[] = [
            value.owner,
            value.sme,
            value.address,
            value.from,
            value,
          ];
          const matchesOwner = candidates.some((c) => {
            if (!c) return false;
            if (typeof c === 'string') return c.toLowerCase() === ownerLower;
            if (typeof c === 'object') {
              return JSON.stringify(c).toLowerCase().includes(ownerLower);
            }
            return false;
          });
          if (!matchesOwner) return false;
          if (!statusFilter) return true;
          const wanted = statusFilter.toLowerCase();
          if (evt.eventType?.toLowerCase() === wanted) return true;
          if (typeof value.status === 'string' && value.status.toLowerCase() === wanted) {
            return true;
          }
          return false;
        })
        .map((evt) => ({
          invoiceId:
            (evt.value && (evt.value.invoice_id ?? evt.value.invoiceId ?? evt.value.id)) ??
            null,
          status: (evt.value && evt.value.status) || evt.eventType,
          amount: (evt.value && (evt.value.amount ?? evt.value.value)) ?? null,
          createdAt: evt.ledgerCloseAt,
          txHash: evt.txHash,
        }));

      return res.json(invoices);
    } catch (err: any) {
      return res.status(500).json({ error: err.message });
    }
  });

  // #861: reconstruct a VerificationRound's status/history off-chain from the
  // oracle_registry contract's indexed events, rather than requiring a
  // simulated read against the live contract for every query. `rnd_open`
  // opens/reopens a round, `voted` records individual votes, and
  // `consensus`/`rnd_exp`/`fallback` are the terminal-or-expiry transitions —
  // the latest one of those (by ledger sequence) is the round's current
  // status.
  app.get('/oracle-registry/rounds/:invoiceId', (req, res) => {
    try {
      const { invoiceId } = req.params;
      if (!invoiceId) {
        return res.status(400).json({ error: 'invoiceId path param is required' });
      }

      const events = getEvents(db, {
        contractType: 'oracle_registry',
        limit: 1000,
        offset: 0,
      });

      const matches = events
        .filter((evt) => extractInvoiceId(evt.value) === invoiceId)
        .sort((a, b) => a.ledgerSequence - b.ledgerSequence);

      if (matches.length === 0) {
        return res.status(404).json({ error: `No round found for invoice ${invoiceId}` });
      }

      let status: string = 'Unknown';
      for (const evt of matches) {
        switch (evt.eventType) {
          case 'rnd_open':
            status = 'Open';
            break;
          case 'consensus':
          case 'fallback':
            status = roundApproved(evt.value) ? 'ConsensusApproved' : 'ConsensusRejected';
            break;
          case 'rnd_exp':
            status = 'Expired';
            break;
          default:
            break;
        }
      }

      return res.json({
        invoiceId,
        status,
        events: matches.map((evt) => ({
          eventType: evt.eventType,
          value: evt.value,
          ledgerSequence: evt.ledgerSequence,
          ledgerCloseAt: evt.ledgerCloseAt,
          txHash: evt.txHash,
        })),
      });
    } catch (err: any) {
      return res.status(500).json({ error: err.message });
    }
  });

  // #865: reconstruct current withdrawal-queue depth for `token` off-chain from the
  // pool contract's indexed wd_queue/wd_cncl/wd_part/wd_full events, rather than
  // requiring a live contract simulation for every query (same approach as the
  // oracle-registry rounds endpoint above). Each event's value is a positional tuple
  // matching the Rust event payload: wd_queue = (investor, token, shares, request_id);
  // wd_cncl = (investor, token, request_id); wd_part = (investor, token, amount,
  // shares_to_burn, request_id); wd_full = (investor, token, payout, shares,
  // request_id) — request_id is 0 on wd_full when the withdrawal settled immediately
  // rather than draining a queued request, and is ignored for reconstruction here.
  //
  // This endpoint intentionally only reports queue *depth and membership* — an
  // authoritative wait-time estimate depends on live pool state (trailing inflow
  // rate, current liquidity, invoice due dates) that isn't reconstructable from the
  // event log alone. Callers wanting `estimated_wait_secs` should simulate the pool
  // contract's `estimate_withdrawal_wait` directly (as the frontend already does).
  app.get('/pool/:token/withdrawal-queue', (req, res) => {
    try {
      const { token } = req.params;
      if (!token) {
        return res.status(400).json({ error: 'token path param is required' });
      }

      const events = getEvents(db, {
        contractType: 'pool',
        limit: 5000,
        offset: 0,
      })
        .filter((evt) =>
          ['wd_queue', 'wd_cncl', 'wd_part', 'wd_full'].includes(evt.eventType),
        )
        .sort((a, b) => a.ledgerSequence - b.ledgerSequence);

      const pending = new Map<
        string,
        { requestId: string; investor: string; shares: string; queuedAt: string; ledgerSequence: number }
      >();

      for (const evt of events) {
        const value = evt.value;
        if (!Array.isArray(value) || value.length < 2) continue;
        if (String(value[1]) !== token) continue;

        switch (evt.eventType) {
          case 'wd_queue': {
            const [investor, , shares, requestId] = value;
            pending.set(String(requestId), {
              requestId: String(requestId),
              investor: String(investor),
              shares: String(shares),
              queuedAt: evt.ledgerCloseAt,
              ledgerSequence: evt.ledgerSequence,
            });
            break;
          }
          case 'wd_cncl': {
            const requestId = String(value[2]);
            pending.delete(requestId);
            break;
          }
          case 'wd_part': {
            const requestId = String(value[4]);
            const sharesBurned = BigInt(String(value[3] ?? 0));
            const entry = pending.get(requestId);
            if (entry) {
              const remaining = BigInt(entry.shares) - sharesBurned;
              entry.shares = remaining.toString();
            }
            break;
          }
          case 'wd_full': {
            const requestId = String(value[4]);
            if (requestId !== '0') {
              pending.delete(requestId);
            }
            break;
          }
          default:
            break;
        }
      }

      const requests = Array.from(pending.values()).sort(
        (a, b) => a.ledgerSequence - b.ledgerSequence,
      );

      return res.json({
        token,
        depth: requests.length,
        requests: requests.map((r, i) => ({
          queuePosition: i + 1,
          requestId: r.requestId,
          investor: r.investor,
          shares: r.shares,
          queuedAt: r.queuedAt,
        })),
      });
    } catch (err: any) {
      return res.status(500).json({ error: err.message });
    }
  });

  // Get latest ledger
  app.get('/ledger/latest', (_req, res) => {
    try {
      const db_any = db as any;
      const row = db_any
        .prepare('SELECT ledger_sequence FROM events ORDER BY ledger_sequence DESC LIMIT 1')
        .get() as { ledger_sequence: number } | undefined;

      res.json({ latestLedger: row?.ledger_sequence || null });
    } catch (err: any) {
      res.status(500).json({ error: err.message });
    }
  });

  app.listen(port, () => {
    console.log(`[Astera Indexer API] Server running on port ${port}`);
  });
}

// #861: every oracle_registry event's value carries the invoice_id as its
// first tuple element, except `rnd_exp` which publishes the bare invoice_id
// (not wrapped in a tuple) — handle both shapes rather than assuming one.
function extractInvoiceId(value: any): string | null {
  if (value === null || value === undefined) return null;
  const candidate = Array.isArray(value) ? value[0] : value;
  if (candidate === null || candidate === undefined) return null;
  if (typeof candidate === 'object') return null;
  return String(candidate);
}

// `consensus` publishes (invoice_id, approved) and `fallback` publishes
// (invoice_id, approved, admin, reason) — both carry `approved` at index 1.
function roundApproved(value: any): boolean {
  if (!Array.isArray(value) || value.length < 2) return false;
  return Boolean(value[1]);
}
