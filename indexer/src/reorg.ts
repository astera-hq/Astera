/**
 * Ledger-reorg detection.
 *
 * Stellar reorgs are rare but not impossible; without this, an event indexed
 * from an orphaned ledger would sit in the database indefinitely. We track a
 * hash chain of recently-indexed ledgers (ledger_hashes) and, for each new
 * ledger the poll loop observes, check whether its prev_hash still matches
 * the hash we recorded for the previous ledger. A mismatch means the chain
 * reorganized underneath us: everything from the previous ledger onward is
 * rolled back and the poll cursor is rewound so it gets re-indexed off the
 * canonical chain.
 */

import { Pool } from "pg";
import { Horizon } from "stellar-sdk";
import { logger } from "./logger";

export interface LedgerMeta {
  sequence: number;
  hash: string;
  prevHash: string;
}

export async function fetchLedgerMeta(
  horizon: Horizon.Server,
  sequence: number,
): Promise<LedgerMeta | null> {
  try {
    const ledger: any = await horizon.ledgers().ledger(sequence).call();
    return {
      sequence,
      hash: ledger.hash,
      prevHash: ledger.prev_hash,
    };
  } catch (err) {
    logger.warn({ err, sequence }, "[reorg] Failed to fetch ledger metadata");
    return null;
  }
}

/**
 * True if the ledger immediately before `meta.sequence` that we previously
 * recorded has a different hash than `meta.prevHash` — i.e. the chain we
 * indexed is no longer the parent of the current tip.
 */
export async function detectReorg(
  pool: Pool,
  meta: LedgerMeta,
): Promise<boolean> {
  const { rows } = await pool.query(
    "SELECT ledger_hash FROM ledger_hashes WHERE ledger_sequence = $1",
    [meta.sequence - 1],
  );
  if (rows.length === 0) return false;
  return rows[0].ledger_hash !== meta.prevHash;
}

export async function recordLedgerHash(
  pool: Pool,
  meta: LedgerMeta,
): Promise<void> {
  await pool.query(
    `INSERT INTO ledger_hashes (ledger_sequence, ledger_hash, prev_hash)
     VALUES ($1, $2, $3)
     ON CONFLICT (ledger_sequence) DO UPDATE SET
       ledger_hash = $2, prev_hash = $3, indexed_at = now()`,
    [meta.sequence, meta.hash, meta.prevHash],
  );
}

/**
 * Deletes indexed events and hash-chain records from `fromLedger` onward so
 * the poll loop can re-fetch and re-index the canonical chain starting
 * there. Returns the number of events removed.
 */
export async function rollbackFrom(
  pool: Pool,
  fromLedger: number,
): Promise<number> {
  const client = await pool.connect();
  try {
    await client.query("BEGIN");
    const eventsResult = await client.query(
      "DELETE FROM events WHERE ledger_sequence >= $1",
      [fromLedger],
    );
    await client.query("DELETE FROM ledger_hashes WHERE ledger_sequence >= $1", [
      fromLedger,
    ]);
    const eventsRemoved = eventsResult.rowCount ?? 0;
    await client.query(
      `INSERT INTO reorg_log (reorg_ledger, rolled_back_from, events_removed, detail)
       VALUES ($1, $1, $2, $3)`,
      [
        fromLedger,
        eventsRemoved,
        `Rolled back ${eventsRemoved} event(s) from ledger ${fromLedger} onward`,
      ],
    );
    await client.query("COMMIT");
    logger.warn(
      { fromLedger, eventsRemoved },
      "[reorg] Rolled back and will re-index from this ledger",
    );
    return eventsRemoved;
  } catch (err) {
    await client.query("ROLLBACK");
    throw err;
  } finally {
    client.release();
  }
}
