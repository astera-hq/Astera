# Astera Indexer

Subscribes to Stellar Horizon event streams for Astera contract events,
parses them, and stores them in Postgres for fast querying by the frontend
and other services.

## Running

```bash
export DATABASE_URL=postgres://astera:astera@localhost:5433/astera_indexer
export HORIZON_URL=https://horizon-testnet.stellar.org
export CONTRACT_IDS=<invoice_contract_id>,<pool_contract_id>
npm install
npm run dev
```

On startup the indexer applies any pending Postgres migrations (see
`migrations/`, run via [node-pg-migrate](https://github.com/salsita/node-pg-migrate))
before it starts polling. See `MIGRATION.md` if you're moving from the old
SQLite-backed indexer.

## Backfill

To populate the database from a specific ledger instead of only live-tailing
new events:

```bash
node dist/index.js --backfill 1000000 --to 1050000   # --to is optional; omit to run to chain tip
```

or via env vars (equivalent, useful in `docker-compose.yml`):

```bash
BACKFILL_START_LEDGER=1000000 BACKFILL_END_LEDGER=1050000 node dist/index.js
```

Backfill writes go through the same `INSERT ... ON CONFLICT DO NOTHING` path
as live polling, so a backfill process can safely run **alongside** the
live-tail process — run them as two separate processes/containers, each with
its own Postgres connection pool, and overlapping ranges just dedupe.

## Ledger reorg handling

The indexer tracks a hash chain of recently-indexed ledgers
(`ledger_hashes` table). On each poll, it checks the newest ledger's
`prev_hash` against the hash recorded for the ledger before it. A mismatch
means the chain reorganized: the indexer rolls back everything from that
point onward (`reorg_log` keeps an audit trail of what was rolled back) and
resets its poll cursor so the canonical chain gets re-indexed. See
`src/reorg.ts`.

## Query API

All endpoints are served over HTTP on `API_PORT` (default `3001`).

- `GET /events` — filter by `contract_id`, `contract_type`, `event_type`,
  `actor_address`. Supports offset pagination (`limit`/`offset`, default
  `50`/`0`) and cursor pagination (`after_ledger`/`before_ledger`) for
  scanning forward through new events without re-paging from the start.
- `GET /events/actor/:actorAddress`, `/events/contract/:contractId`,
  `/events/type/:eventType` — convenience filters over the same data.
- Domain-specific read models derived from the event stream: co-funding
  rounds, secondary-market listings/orders, oracle-registry rounds,
  credit-score attestations, pool rate history, compliance screening, and
  tranche APY — see `src/api.ts` for the full list.
- `GET /health` — last-processed and latest-stored ledger.
- `GET /metrics` — Prometheus metrics (events indexed, poll errors, reorgs
  detected, indexing lag in ledgers, backfill progress). See `src/metrics.ts`.

All routes are rate-limited (`RATE_LIMIT_MAX` requests per `RATE_LIMIT_WINDOW_MS`,
default 300/60s), except `/health` and `/metrics`.

## Logging

Structured JSON logs via [pino](https://getpino.io) — set `LOG_LEVEL` to
control verbosity (`info` by default). Pipe through `pino-pretty` locally if
you want human-readable output:

```bash
npm run dev | npx pino-pretty
```
