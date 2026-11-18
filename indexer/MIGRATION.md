# Migrating the indexer from SQLite to Postgres

The indexer used to store events in a local SQLite file (`better-sqlite3`).
It now runs against Postgres, with schema migrations managed by
[node-pg-migrate](https://github.com/salsita/node-pg-migrate) instead of the
old hand-rolled `MIGRATIONS` array. This guide walks through moving an
existing SQLite deployment over.

## 1. Stand up Postgres

Any Postgres 13+ instance works. For local development, `docker-compose.yml`
now includes an `indexer-postgres` service:

```bash
docker compose up -d indexer-postgres
```

Set `DATABASE_URL` to point at it, e.g.:

```bash
export DATABASE_URL=postgres://astera:astera@localhost:5433/astera_indexer
```

## 2. Apply the schema

```bash
cd indexer
npm install
npm run migrate:up
```

This creates the `events`, `tranche_apy`, `ledger_hashes`, and `reorg_log`
tables (see `migrations/`). It's safe to re-run — node-pg-migrate tracks
applied migrations in a `pgmigrations` table and skips ones already run.

## 3. Copy existing data

If you have an existing SQLite database (default path `./indexer.db`), copy
its rows into Postgres with the bundled script:

```bash
DATABASE_URL=$DATABASE_URL npm run migrate:sqlite-to-postgres -- --sqlite ./indexer.db
```

The script paginates through the SQLite `events` table by primary key
(avoiding a slow `OFFSET` scan on large databases), inserts through the same
`storeEvents` path the live indexer uses (so dedup is consistent), and
copies `tranche_apy` afterward. It only reads from SQLite — nothing is
deleted or modified there, so it's safe to re-run if interrupted.

At the end it prints the row counts from both databases so you can verify
they match before decommissioning the SQLite file.

## 4. Switch the indexer over

Set `DATABASE_URL` in the indexer's environment (or `docker-compose.yml`)
and remove the old `DB_PATH` variable — it's no longer read. On startup the
indexer runs pending migrations automatically before it starts polling, so
step 2 above is optional in practice, but running it explicitly first lets
you catch schema errors before the indexer is live.

## 5. Decommission SQLite

Once you've confirmed the row counts match and the indexer is serving
traffic from Postgres (`GET /health` and `GET /events` against the new
instance), the old `indexer.db` file (and the `./indexer/data` volume in
`docker-compose.yml`) can be deleted.

## Backfilling a fresh instance instead

If you'd rather stand up a new indexer from scratch than migrate an old
database, use backfill mode instead of this script — see the "Backfill" and
"Ledger reorg handling" sections in `README.md`.
