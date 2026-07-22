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

  // #860: multi-investor co-funding rounds. All co-funding events publish
  // under the same "POOL" topic as every other pool event, so — unlike the
  // oracle registry, which is a separate contract — no dedicated
  // contractType classification is needed; we just filter the pool event
  // stream by event_type and reconstruct round state from it.
  const COFUNDING_EVENT_TYPES = new Set([
    'cf_open',
    'cf_commit',
    'cf_wthdw',
    'cf_cncl',
    'cf_exp',
    'cf_fin',
  ]);

  // #868: credit_score attestation lifecycle events, keyed by attestation id
  // (the first tuple element of each of these three event types).
  const ATTESTATION_LIFECYCLE_EVENT_TYPES = new Set(['att_sub', 'att_disp', 'att_res']);

  app.get('/co-funding/rounds', (_req, res) => {
    try {
      const events = getEvents(db, { contractType: 'pool', limit: 2000, offset: 0 });
      const invoiceIds = new Set<string>();
      for (const evt of events) {
        if (evt.eventType !== 'cf_open') continue;
        const id = extractCoFundingInvoiceId(evt.value);
        if (id !== null) invoiceIds.add(id);
      }
      return res.json({ invoiceIds: Array.from(invoiceIds) });
    } catch (err: any) {
      return res.status(500).json({ error: err.message });
    }
  });

  app.get('/co-funding/rounds/:invoiceId', (req, res) => {
    try {
      const { invoiceId } = req.params;
      if (!invoiceId) {
        return res.status(400).json({ error: 'invoiceId path param is required' });
      }

      const events = getEvents(db, { contractType: 'pool', limit: 2000, offset: 0 });
      const matches = events
        .filter(
          (evt) =>
            COFUNDING_EVENT_TYPES.has(evt.eventType) &&
            extractCoFundingInvoiceId(evt.value) === invoiceId,
        )
        .sort((a, b) => a.ledgerSequence - b.ledgerSequence);

      if (matches.length === 0) {
        return res
          .status(404)
          .json({ error: `No co-funding round found for invoice ${invoiceId}` });
      }

      let status = 'Unknown';
      for (const evt of matches) {
        switch (evt.eventType) {
          case 'cf_open':
            status = 'Open';
            break;
          case 'cf_fin':
            status = 'Filled';
            break;
          case 'cf_exp':
            status = 'Expired';
            break;
          case 'cf_cncl':
            status = 'Cancelled';
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

  // Every co-funding round this investor has ever committed to, derived
  // from cf_commit events (the second tuple element is always the investor
  // address for that event — see contracts/pool/src/lib.rs's cf_commit
  // publish call).
  app.get('/co-funding/investor/:address', (req, res) => {
    try {
      const { address } = req.params;
      if (!address) {
        return res.status(400).json({ error: 'address path param is required' });
      }
      const addressLower = address.toLowerCase();

      const events = getEvents(db, {
        contractType: 'pool',
        eventType: 'cf_commit',
        limit: 2000,
        offset: 0,
      });

      const invoiceIds = new Set<string>();
      for (const evt of events) {
        const value = evt.value;
        const investor = Array.isArray(value) ? value[1] : undefined;
        if (typeof investor === 'string' && investor.toLowerCase() === addressLower) {
          const id = extractCoFundingInvoiceId(value);
          if (id !== null) invoiceIds.add(id);
        }
      }

      return res.json({ address, invoiceIds: Array.from(invoiceIds) });
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

  // #868: reconstruct an SME's attestation history from the credit_score
  // contract's indexed att_sub/att_disp/att_res events. `att_sub` carries
  // (id, sme, attestor, score_contribution); `att_disp`/`att_res` only carry
  // the attestation id, so we first find every id submitted for this SME via
  // att_sub, then replay the matching lifecycle events in ledger order to
  // derive each attestation's current status. Time-based expiry isn't
  // eventful and is therefore not reflected here — callers who need the
  // authoritative status should also check the contract's `get_attestation`.
  app.get('/credit-score/:sme/attestations', (req, res) => {
    try {
      const { sme } = req.params;
      if (!sme) {
        return res.status(400).json({ error: 'sme path param is required' });
      }
      const smeLower = sme.toLowerCase();

      const events = getEvents(db, { contractType: 'credit_score', limit: 5000, offset: 0 });

      const attestationIds = new Set<string>();
      for (const evt of events) {
        if (evt.eventType !== 'att_sub') continue;
        const value = evt.value;
        const smeAddr = Array.isArray(value) ? value[1] : undefined;
        if (typeof smeAddr === 'string' && smeAddr.toLowerCase() === smeLower) {
          attestationIds.add(String(value[0]));
        }
      }

      const lifecycleEvents = events
        .filter((evt) => {
          if (!ATTESTATION_LIFECYCLE_EVENT_TYPES.has(evt.eventType)) return false;
          const id = Array.isArray(evt.value) ? String(evt.value[0]) : null;
          return id !== null && attestationIds.has(id);
        })
        .sort((a, b) => a.ledgerSequence - b.ledgerSequence);

      const attestations = Array.from(attestationIds).map((id) => {
        const idEvents = lifecycleEvents.filter(
          (evt) => Array.isArray(evt.value) && String(evt.value[0]) === id,
        );
        let status = 'Active';
        let attestor: string | null = null;
        let scoreContribution: number | null = null;
        for (const evt of idEvents) {
          const value = evt.value as any[];
          switch (evt.eventType) {
            case 'att_sub':
              attestor = typeof value[2] === 'string' ? value[2] : null;
              scoreContribution = value[3] !== undefined ? Number(value[3]) : null;
              status = 'Active';
              break;
            case 'att_disp':
              status = 'Disputed';
              break;
            case 'att_res':
              status = value[1] ? 'Active' : 'Revoked';
              break;
            default:
              break;
          }
        }
        return {
          id,
          sme,
          attestor,
          scoreContribution,
          status,
          events: idEvents.map((evt) => ({
            eventType: evt.eventType,
            value: evt.value,
            ledgerSequence: evt.ledgerSequence,
            ledgerCloseAt: evt.ledgerCloseAt,
            txHash: evt.txHash,
          })),
        };
      });

      return res.json({ sme, attestations });
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

// #860: every co-funding event's value carries the invoice_id as its first
// tuple element, except `cf_cncl` which publishes the bare invoice_id (not
// wrapped in a tuple) — handle both shapes rather than assuming one.
// Returned as a string since invoice IDs may exceed Number precision once
// serialized through JSON.
function extractCoFundingInvoiceId(value: any): string | null {
  if (value === null || value === undefined) return null;
  const candidate = Array.isArray(value) ? value[0] : value;
  if (candidate === null || candidate === undefined) return null;
  if (typeof candidate === 'object') return null;
  return String(candidate);
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
