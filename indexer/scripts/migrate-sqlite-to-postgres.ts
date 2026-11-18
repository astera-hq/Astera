#!/usr/bin/env ts-node
/**
 * One-time migration of an existing SQLite indexer database into Postgres.
 *
 * Usage:
 *   DATABASE_URL=postgres://... npx ts-node scripts/migrate-sqlite-to-postgres.ts --sqlite ./indexer.db
 *
 * Run `npm run migrate:up` against DATABASE_URL first so the target schema
 * exists. This script only copies rows — it does not touch the SQLite file.
 */

import Database from "better-sqlite3";
import { Pool } from "pg";
import { runMigrations, storeEvents } from "../src/db";
import { IndexedEvent } from "../src/parser";

const BATCH_SIZE = 1000;

function parseArgs(argv: string[]): { sqlitePath: string } {
  const idx = argv.indexOf("--sqlite");
  const sqlitePath = idx !== -1 ? argv[idx + 1] : "./indexer.db";
  if (!sqlitePath) {
    throw new Error("--sqlite <path> is required");
  }
  return { sqlitePath };
}

function rowToIndexedEvent(row: any): IndexedEvent {
  return {
    id: row.id,
    contractId: row.contract_id,
    contractType: (row.contract_type || "unknown") as IndexedEvent["contractType"],
    eventType: row.event_type,
    topic: JSON.parse(row.topic),
    value: row.value ? JSON.parse(row.value) : null,
    actor: row.actor_address ?? null,
    ledgerSequence: row.ledger_sequence,
    ledgerCloseAt: row.ledger_close_at,
    txHash: row.tx_hash,
    createdAt: row.created_at,
  };
}

async function main() {
  const { sqlitePath } = parseArgs(process.argv.slice(2));
  const databaseUrl = process.env.DATABASE_URL;
  if (!databaseUrl) {
    throw new Error("DATABASE_URL is required (target Postgres connection string)");
  }

  console.log(`[migrate] Applying Postgres migrations against ${databaseUrl}`);
  await runMigrations(databaseUrl);

  const sqlite = new Database(sqlitePath, { readonly: true });
  const pool = new Pool({ connectionString: databaseUrl });

  try {
    const total = (
      sqlite.prepare("SELECT COUNT(*) as c FROM events").get() as { c: number }
    ).c;
    console.log(`[migrate] Copying ${total} events from ${sqlitePath}`);

    let copied = 0;
    let lastId = "";
    // Deterministic keyset pagination over the SQLite source (id is a
    // primary key derived from Horizon's paging_token, which sorts
    // chronologically) — avoids OFFSET, which gets slower as it grows.
    const stmt = sqlite.prepare(
      "SELECT * FROM events WHERE id > ? ORDER BY id ASC LIMIT ?",
    );

    while (true) {
      const rows = stmt.all(lastId, BATCH_SIZE) as any[];
      if (rows.length === 0) break;

      const events = rows.map(rowToIndexedEvent);
      const { inserted } = await storeEvents(pool, events);
      copied += rows.length;
      lastId = rows[rows.length - 1].id;

      console.log(
        `[migrate] Copied ${copied}/${total} events (${inserted} newly inserted this batch)`,
      );
    }

    const trancheRows = sqlite.prepare("SELECT * FROM tranche_apy").all() as any[];
    if (trancheRows.length > 0) {
      console.log(`[migrate] Copying ${trancheRows.length} tranche_apy rows`);
      for (const row of trancheRows) {
        await pool.query(
          `INSERT INTO tranche_apy
             (token, tranche_class, realized_principal, realized_return, closed_positions, realized_apy_bps, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           ON CONFLICT (token, tranche_class) DO UPDATE SET
             realized_principal = $3,
             realized_return = $4,
             closed_positions = $5,
             realized_apy_bps = $6,
             updated_at = $7`,
          [
            row.token,
            row.tranche_class,
            row.realized_principal,
            row.realized_return,
            row.closed_positions,
            row.realized_apy_bps,
            row.updated_at,
          ],
        );
      }
    }

    const { rows: countRows } = await pool.query("SELECT COUNT(*) as c FROM events");
    console.log(
      `[migrate] Done. SQLite had ${total} events; Postgres now has ${countRows[0].c}.`,
    );
  } finally {
    sqlite.close();
    await pool.end();
  }
}

main().catch((err) => {
  console.error("[migrate] Fatal error:", err);
  process.exit(1);
});
