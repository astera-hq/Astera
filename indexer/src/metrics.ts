/**
 * Prometheus metrics for the indexer, exposed via GET /metrics (see api.ts).
 */

import client from "prom-client";

export const register = new client.Registry();
client.collectDefaultMetrics({ register });

export const eventsIndexedTotal = new client.Counter({
  name: "astera_indexer_events_indexed_total",
  help: "Total number of events successfully indexed",
  registers: [register],
});

export const pollErrorsTotal = new client.Counter({
  name: "astera_indexer_poll_errors_total",
  help: "Total number of Horizon polling failures encountered",
  registers: [register],
});

export const reorgsDetectedTotal = new client.Counter({
  name: "astera_indexer_reorgs_detected_total",
  help: "Total number of ledger reorgs detected and rolled back",
  registers: [register],
});

export const indexerLagLedgers = new client.Gauge({
  name: "astera_indexer_lag_ledgers",
  help: "Number of ledgers between the chain tip and the last indexed ledger",
  registers: [register],
});

export const backfillProgressLedger = new client.Gauge({
  name: "astera_indexer_backfill_progress_ledger",
  help: "Ledger sequence most recently processed by an in-progress backfill run",
  registers: [register],
});
