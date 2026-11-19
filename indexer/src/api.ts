// Express REST API for querying indexed Soroban events.

import express from 'express';
import Database from 'better-sqlite3';
import { register, collectDefaultMetrics, Gauge } from 'prom-client';
import { getEvents, getPoolHistory, getAnalyticsSummary } from './db';

export function startApiServer(db: Database.Database, port: number): void {
  const app = express();
  app.use(express.json());

  app.get('/health', (_req, res) => {
    res.json({ status: 'ok', timestamp: new Date().toISOString() });
  });

  app.get('/events', (req, res) => {
    try {
      const { contract_id, event_type, limit = '50', offset = '0' } = req.query;
      const events = getEvents(db, {
        contractId: contract_id as string | undefined,
        eventType: event_type as string | undefined,
        limit: parseInt(limit as string, 10),
        offset: parseInt(offset as string, 10),
      });
      res.json({ events, count: events.length });
    } catch (err: any) {
      res.status(500).json({ error: err.message });
    }
  });

  app.get('/events/contract/:contractId', (req, res) => {
    try {
      const { contractId } = req.params;
      const { limit = '50', offset = '0' } = req.query;
      const events = getEvents(db, { contractId, limit: parseInt(limit as string, 10), offset: parseInt(offset as string, 10) });
      res.json({ contractId, events, count: events.length });
    } catch (err: any) {
      res.status(500).json({ error: err.message });
    }
  });

  app.get('/events/type/:eventType', (req, res) => {
    try {
      const { eventType } = req.params;
      const { limit = '50', offset = '0' } = req.query;
      const events = getEvents(db, { eventType, limit: parseInt(limit as string, 10), offset: parseInt(offset as string, 10) });
      res.json({ eventType, events, count: events.length });
    } catch (err: any) {
      res.status(500).json({ error: err.message });
    }
  });

  app.get('/ledger/latest', (_req, res) => {
    try {
      const row = (db as any).prepare('SELECT ledger_sequence FROM events ORDER BY ledger_sequence DESC LIMIT 1').get() as
        | { ledger_sequence: number }
        | undefined;
      res.json({ latestLedger: row?.ledger_sequence || null });
    } catch (err: any) {
      res.status(500).json({ error: err.message });
    }
  });

  app.get('/pool/:token/utilization-history', (req, res) => {
    try {
      const limit = parseInt(req.query.limit as string || '90', 10);
      const history = getPoolHistory(db, req.params.token, limit);
      res.json({ token: req.params.token, history, utilizationHistory: history });
    } catch (err: any) {
      res.status(500).json({ error: err.message });
    }
  });

  app.get('/pool/:token/rate-history', (req, res) => {
    try {
      const limit = parseInt(req.query.limit as string || '90', 10);
      const history = getPoolHistory(db, req.params.token, limit).map((row) => ({
        time: row.time,
        defaultRate: row.defaultRate,
      }));
      res.json({ token: req.params.token, history, rateHistory: history });
    } catch (err: any) {
      res.status(500).json({ error: err.message });
    }
  });

  app.get('/analytics/summary', (req, res) => {
    try {
      const limit = parseInt(req.query.limit as string || '90', 10);
      const history = getAnalyticsSummary(db, limit);
      const latest = history[history.length - 1] || {
        time: null,
        tvl: 0,
        volume: 0,
        defaultRate: 0,
        activeInvoices: 0,
      };
      res.json({ latest, history, summary: latest });
    } catch (err: any) {
      res.status(500).json({ error: err.message });
    }
  });

  app.listen(port, () => {
    console.log(`[Astera Indexer API] Server running on port ${port}`);
  });
}

export function startMetricsServer(db: Database.Database, port: number): void {
  collectDefaultMetrics({ register });

  const latestLedger = new Gauge({
    name: 'astera_indexer_latest_ledger',
    help: 'Latest ledger sequence indexed',
    registers: [register],
  });
  const indexedEvents = new Gauge({
    name: 'astera_indexer_events_total',
    help: 'Number of indexed event rows',
    registers: [register],
  });

  const app = express();
  app.get('/metrics', async (_req, res) => {
    try {
      const dbAny = db as any;
      const ledger = dbAny.prepare('SELECT ledger_sequence FROM events ORDER BY ledger_sequence DESC LIMIT 1').get();
      const events = dbAny.prepare('SELECT COUNT(*) AS count FROM events').get();
      latestLedger.set(ledger?.ledger_sequence || 0);
      indexedEvents.set(events?.count || 0);
      res.set('Content-Type', register.contentType);
      res.end(await register.metrics());
    } catch (err: any) {
      res.status(500).json({ error: err.message });
    }
  });

  app.listen(port, () => {
    console.log(`[Astera Indexer Metrics] Internal server running on port ${port}`);
  });
}
