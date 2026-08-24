/**
 * Postgres database layer for storing indexed Soroban events.
 *
 * Replaces the previous SQLite/better-sqlite3 layer (a single-writer file
 * database with a hand-rolled migration array) so the indexer can run
 * against a real Postgres instance with concurrent readers/writers, proper
 * migrations (see migrations/, run via node-pg-migrate), and a backfill
 * process that can run alongside the live-tail poller without contending
 * for writes (both write through INSERT ... ON CONFLICT DO NOTHING, so
 * concurrent inserts of the same event just dedupe).
 */

import path from "path";
import { Pool, PoolConfig } from "pg";
import { IndexedEvent } from "./parser";
import { logger } from "./logger";

export function createPool(config?: PoolConfig): Pool {
  const pool = new Pool({
    connectionString: process.env.DATABASE_URL,
    max: parseInt(process.env.PG_POOL_MAX || "10", 10),
    ...config,
  });

  pool.on("error", (err) => {
    logger.error({ err }, "[db] Unexpected error on idle Postgres client");
  });

  return pool;
}

/**
 * Applies all pending migrations in migrations/ using node-pg-migrate's
 * programmatic runner. Safe to call on every startup — it's a no-op once the
 * schema is current.
 */
export async function runMigrations(connectionString: string): Promise<void> {
  const runner = require("node-pg-migrate").default;

  const MAX_RETRIES = 5;
  const RETRY_DELAY_MS = 1000;
  let lastError: unknown;

  for (let attempt = 1; attempt <= MAX_RETRIES; attempt++) {
    try {
      await runner({
        databaseUrl: connectionString,
        dir: path.join(__dirname, "..", "migrations"),
        direction: "up",
        migrationsTable: "pgmigrations",
        log: (msg: string) => logger.info(msg),
      });
      logger.info("[db] Migrations applied successfully");
      return;
    } catch (error) {
      lastError = error;
      logger.error(
        { err: error, attempt, maxRetries: MAX_RETRIES },
        "[db] Failed to run migrations",
      );
      if (attempt < MAX_RETRIES) {
        await new Promise((resolve) =>
          setTimeout(resolve, RETRY_DELAY_MS * attempt),
        );
      }
    }
  }

  throw new Error(
    `[db] Failed to run migrations after ${MAX_RETRIES} attempts: ${lastError}`,
  );
}

export async function storeEvents(
  pool: Pool,
  events: IndexedEvent[],
): Promise<{ inserted: number; skipped: number }> {
  if (events.length === 0) return { inserted: 0, skipped: 0 };

  const ids = events.map((e) => e.id);
  const contractIds = events.map((e) => e.contractId);
  const contractTypes = events.map((e) => e.contractType || "unknown");
  const eventTypes = events.map((e) => e.eventType);
  const topics = events.map((e) => JSON.stringify(e.topic));
  const values = events.map((e) =>
    e.value === undefined ? null : JSON.stringify(e.value),
  );
  const actors = events.map((e) => e.actor);
  const ledgerSequences = events.map((e) => e.ledgerSequence);
  const ledgerCloseAts = events.map((e) => e.ledgerCloseAt);
  const txHashes = events.map((e) => e.txHash);
  const createdAts = events.map((e) => e.createdAt);

  const result = await pool.query(
    `INSERT INTO events
       (id, contract_id, contract_type, event_type, topic, value, actor_address, ledger_sequence, ledger_close_at, tx_hash, created_at)
     SELECT * FROM UNNEST(
       $1::text[], $2::text[], $3::text[], $4::text[], $5::jsonb[], $6::jsonb[],
       $7::text[], $8::bigint[], $9::timestamptz[], $10::text[], $11::timestamptz[]
     )
     ON CONFLICT (id) DO NOTHING`,
    [
      ids,
      contractIds,
      contractTypes,
      eventTypes,
      topics,
      values,
      actors,
      ledgerSequences,
      ledgerCloseAts,
      txHashes,
      createdAts,
    ],
  );

  const inserted = result.rowCount ?? 0;
  const skipped = events.length - inserted;
  if (skipped > 0) {
    logger.info({ inserted, skipped }, "[db] Deduplicated events");
  }

  return { inserted, skipped };
}

export interface GetEventsOptions {
  contractId?: string;
  contractType?: string;
  eventType?: string;
  actorAddress?: string;
  limit?: number;
  offset?: number;
  /** Cursor pagination: only events after this ledger, ordered ascending. */
  afterLedger?: number;
  beforeLedger?: number;
}

export async function getEvents(
  pool: Pool,
  options: GetEventsOptions = {},
): Promise<IndexedEvent[]> {
  const {
    contractId,
    contractType,
    eventType,
    actorAddress,
    limit = 50,
    offset = 0,
    afterLedger,
    beforeLedger,
  } = options;

  const clauses: string[] = [];
  const params: any[] = [];

  const addClause = (column: string, value: any) => {
    params.push(value);
    clauses.push(`${column} = $${params.length}`);
  };

  if (contractId) addClause("contract_id", contractId);
  if (contractType) addClause("contract_type", contractType);
  if (eventType) addClause("event_type", eventType);
  if (actorAddress) addClause("actor_address", actorAddress);
  if (afterLedger !== undefined) {
    params.push(afterLedger);
    clauses.push(`ledger_sequence > $${params.length}`);
  }
  if (beforeLedger !== undefined) {
    params.push(beforeLedger);
    clauses.push(`ledger_sequence < $${params.length}`);
  }

  const where = clauses.length ? `WHERE ${clauses.join(" AND ")}` : "";
  // Cursor pagination (afterLedger) reads oldest-to-newest so callers can
  // page forward through new events; everything else keeps the original
  // newest-first ordering.
  const order =
    afterLedger !== undefined
      ? "ORDER BY ledger_sequence ASC"
      : "ORDER BY ledger_sequence DESC";

  params.push(limit);
  const limitIdx = params.length;
  params.push(offset);
  const offsetIdx = params.length;

  const { rows } = await pool.query(
    `SELECT * FROM events ${where} ${order} LIMIT $${limitIdx} OFFSET $${offsetIdx}`,
    params,
  );

  return rows.map(rowToEvent);
}

function rowToEvent(row: any): IndexedEvent {
  return {
    id: row.id,
    contractId: row.contract_id,
    contractType: (row.contract_type || "unknown") as IndexedEvent["contractType"],
    eventType: row.event_type,
    topic: row.topic,
    value: row.value,
    actor: row.actor_address,
    ledgerSequence: Number(row.ledger_sequence),
    ledgerCloseAt:
      row.ledger_close_at instanceof Date
        ? row.ledger_close_at.toISOString()
        : row.ledger_close_at,
    txHash: row.tx_hash,
    createdAt:
      row.created_at instanceof Date
        ? row.created_at.toISOString()
        : row.created_at,
  };
}

export async function getLatestLedger(pool: Pool): Promise<string | null> {
  const { rows } = await pool.query(
    "SELECT ledger_sequence FROM events ORDER BY ledger_sequence DESC LIMIT 1",
  );
  return rows[0] ? String(rows[0].ledger_sequence) : null;
}

// #1173: persisted resume checkpoint for backfill runs (see
// migrations/1700000000003_backfill-checkpoints.js). Previously backfill
// progress lived only in-memory and in a Prometheus gauge, so a crashed or
// restarted process always restarted from the original `--backfill <from>`
// argument instead of where it left off.
export interface BackfillCheckpoint {
  jobKey: string;
  startLedger: number;
  endLedger: number | null;
  currentLedger: number;
  status: "running" | "completed";
  updatedAt: string;
}

export async function getBackfillCheckpoint(
  pool: Pool,
  jobKey: string,
): Promise<BackfillCheckpoint | null> {
  const { rows } = await pool.query(
    `SELECT job_key, start_ledger, end_ledger, current_ledger, status, updated_at
       FROM backfill_checkpoints WHERE job_key = $1`,
    [jobKey],
  );
  if (!rows[0]) return null;
  const row = rows[0];
  return {
    jobKey: row.job_key,
    startLedger: Number(row.start_ledger),
    endLedger: row.end_ledger === null ? null : Number(row.end_ledger),
    currentLedger: Number(row.current_ledger),
    status: row.status,
    updatedAt:
      row.updated_at instanceof Date
        ? row.updated_at.toISOString()
        : row.updated_at,
  };
}

/**
 * Upserts the checkpoint row for `jobKey`. Called after every successfully
 * processed batch (so a crash loses at most one in-flight page) and once
 * more with `status: "completed"` when the backfill finishes, so a later
 * re-run of the same command doesn't needlessly re-walk a finished range.
 */
export async function saveBackfillCheckpoint(
  pool: Pool,
  jobKey: string,
  fields: {
    startLedger: number;
    endLedger: number | null;
    currentLedger: number;
    status: "running" | "completed";
  },
): Promise<void> {
  await pool.query(
    `INSERT INTO backfill_checkpoints
       (job_key, start_ledger, end_ledger, current_ledger, status, updated_at)
     VALUES ($1, $2, $3, $4, $5, now())
     ON CONFLICT (job_key) DO UPDATE SET
       end_ledger = $3,
       current_ledger = $4,
       status = $5,
       updated_at = now()`,
    [jobKey, fields.startLedger, fields.endLedger, fields.currentLedger, fields.status],
  );
}

// #862: historical realized APY per tranche per token.
//
// Each closed invoice contributes to the trailing realized yield of both
// tranches. From the indexed tranche events we reconstruct, per invoice:
//   fund   -> (invoice_id, token, senior_deployed, junior_deployed)
//   repay  -> (invoice_id, token, senior_payout,   junior_payout)
//   default-> (invoice_id, token, junior_loss,     senior_loss)
// Realized return for a tranche on an invoice is payout - deployed (a repaid
// invoice), or -loss (a defaulted invoice). Aggregated across every closed
// invoice for a token, realized_return / realized_principal is the trailing
// realized yield; we annualize crudely to basis points assuming the sampled
// invoices represent roughly one turnover. Callers wanting a time-weighted
// figure can derive it from the raw fields, which are exposed as strings to
// preserve i128 precision.
export interface TrancheApyRow {
  token: string;
  trancheClass: "Senior" | "Junior";
  realizedPrincipal: bigint;
  realizedReturn: bigint;
  closedPositions: number;
  realizedApyBps: number;
  updatedAt: string;
}

function toBigInt(v: unknown): bigint {
  try {
    return BigInt(String(v ?? 0));
  } catch {
    return 0n;
  }
}

/**
 * Recompute the tranche_apy table from scratch off the indexed tranche event
 * stream. Idempotent: safe to call after every ingest batch.
 * #1171: paginate through all events, not just the first 100k.
 */
export async function recomputeTrancheApy(
  pool: Pool,
  updatedAt: string,
): Promise<void> {
  // token -> tranche -> accumulator
  type Acc = { principal: bigint; ret: bigint; closed: number };
  const agg = new Map<string, { Senior: Acc; Junior: Acc }>();
  const bucket = (token: string) => {
    let b = agg.get(token);
    if (!b) {
      b = {
        Senior: { principal: 0n, ret: 0n, closed: 0 },
        Junior: { principal: 0n, ret: 0n, closed: 0 },
      };
      agg.set(token, b);
    }
    return b;
  };

  // Deployed principal per invoice, from `fund` events, so repay/default can
  // net against it.
  const deployed = new Map<
    string,
    { token: string; senior: bigint; junior: bigint }
  >();

  // Paginate through all tranche events in chronological order
  const pageSize = 10000;
  let offset = 0;
  let hasMoreEvents = true;

  while (hasMoreEvents) {
    const { rows } = await pool.query(
      `SELECT * FROM events WHERE contract_type = 'tranche' ORDER BY ledger_sequence ASC LIMIT $1 OFFSET $2`,
      [pageSize, offset],
    );
    hasMoreEvents = rows.length === pageSize;
    offset += pageSize;

    const ordered = rows.map(rowToEvent);

    for (const evt of ordered) {
      const v = evt.value;
      if (!Array.isArray(v) || v.length < 4) continue;
      const invoiceId = String(v[0]);
      const token = typeof v[1] === "string" ? v[1] : String(v[1]);

      switch (evt.eventType) {
        case "fund": {
          deployed.set(invoiceId, {
            token,
            senior: toBigInt(v[2]),
            junior: toBigInt(v[3]),
          });
          break;
        }
        case "repay": {
          const dep = deployed.get(invoiceId);
          if (!dep) break;
          const b = bucket(token);
          b.Senior.principal += dep.senior;
          b.Senior.ret += toBigInt(v[2]) - dep.senior;
          b.Senior.closed += 1;
          b.Junior.principal += dep.junior;
          b.Junior.ret += toBigInt(v[3]) - dep.junior;
          b.Junior.closed += 1;
          deployed.delete(invoiceId);
          break;
        }
        case "default": {
          const dep = deployed.get(invoiceId);
          const b = bucket(token);
          // default value tuple is (invoice_id, token, junior_loss, senior_loss)
          const juniorLoss = toBigInt(v[2]);
          const seniorLoss = toBigInt(v[3]);
          b.Junior.principal += dep ? dep.junior : juniorLoss;
          b.Junior.ret += -juniorLoss;
          b.Junior.closed += 1;
          b.Senior.principal += dep ? dep.senior : seniorLoss;
          b.Senior.ret += -seniorLoss;
          b.Senior.closed += 1;
          deployed.delete(invoiceId);
          break;
        }
        default:
          break;
      }
    }
  }

  const client = await pool.connect();
  try {
    await client.query("BEGIN");
    for (const [token, b] of agg) {
      for (const cls of ["Senior", "Junior"] as const) {
        const acc = b[cls];
        const apyBps =
          acc.principal > 0n ? Number((acc.ret * 10_000n) / acc.principal) : 0;
        await client.query(
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
            token,
            cls,
            acc.principal.toString(),
            acc.ret.toString(),
            acc.closed,
            apyBps,
            updatedAt,
          ],
        );
      }
    }
    await client.query("COMMIT");
  } catch (err) {
    await client.query("ROLLBACK");
    throw err;
  } finally {
    client.release();
  }
}

export async function getTrancheApy(
  pool: Pool,
  token?: string,
): Promise<TrancheApyRow[]> {
  const params: any[] = [];
  let query = "SELECT * FROM tranche_apy";
  if (token) {
    query += " WHERE token = $1";
    params.push(token);
  }
  query += " ORDER BY token, tranche_class";

  const { rows } = await pool.query(query, params);
  return rows.map((r) => ({
    token: r.token,
    trancheClass: r.tranche_class,
    realizedPrincipal: toBigInt(r.realized_principal),
    realizedReturn: toBigInt(r.realized_return),
    closedPositions: r.closed_positions,
    realizedApyBps: r.realized_apy_bps,
    updatedAt:
      r.updated_at instanceof Date ? r.updated_at.toISOString() : r.updated_at,
  }));
}

// #1041: per-SME derived risk signals (see migrations/1700000000002_sme-risk-signals.js).
export interface SmeRiskSignalRow {
  sme: string;
  debtorConcentrationBps: number;
  invoiceSizeRiskBps: number;
  totalVolume: bigint;
  updatedAt: string;
}

/**
 * Recompute `sme_risk_signals` from scratch off the indexed `invoice`
 * `created` event stream. Idempotent: safe to call after every ingest batch.
 *
 * `created` events carry `(id, owner, amount, metadata_uri, timestamp, debtor)`
 * — debtor concentration and invoice-size risk are both derivable purely
 * from invoice-creation volume, with no need to correlate funding/
 * settlement events. `debtor_concentration_bps` is the largest single
 * debtor's share of an SME's total created volume; `invoice_size_risk_bps`
 * is the share of volume sitting in invoices at least 3x the SME's own mean
 * invoice size.
 * #1171: paginate through all events, not just the first 100k.
 */
export async function recomputeSmeRiskSignals(
  pool: Pool,
  updatedAt: string,
): Promise<void> {
  type Acc = { total: bigint; amounts: bigint[]; byDebtor: Map<string, bigint> };
  const bySme = new Map<string, Acc>();

  // Paginate through all invoice created events in chronological order
  const pageSize = 10000;
  let offset = 0;
  let hasMoreEvents = true;

  while (hasMoreEvents) {
    const { rows } = await pool.query(
      `SELECT * FROM events WHERE contract_type = 'invoice' AND event_type = 'created' ORDER BY ledger_sequence ASC LIMIT $1 OFFSET $2`,
      [pageSize, offset],
    );
    hasMoreEvents = rows.length === pageSize;
    offset += pageSize;

    const ordered = rows.map(rowToEvent);

    for (const evt of ordered) {
      const v = evt.value;
      if (!Array.isArray(v) || v.length < 2) continue;
      const owner = String(v[1] ?? "");
      if (!owner) continue;
      const amount = toBigInt(v[2]);
      const debtor = v.length > 5 ? String(v[5] ?? "unknown") : "unknown";

      let acc = bySme.get(owner);
      if (!acc) {
        acc = { total: 0n, amounts: [], byDebtor: new Map() };
        bySme.set(owner, acc);
      }
      acc.total += amount;
      acc.amounts.push(amount);
      acc.byDebtor.set(debtor, (acc.byDebtor.get(debtor) ?? 0n) + amount);
    }
  }

  const client = await pool.connect();
  try {
    await client.query("BEGIN");
    for (const [sme, acc] of bySme) {
      if (acc.total === 0n) continue;

      let maxDebtorVolume = 0n;
      for (const vol of acc.byDebtor.values()) {
        if (vol > maxDebtorVolume) maxDebtorVolume = vol;
      }
      const debtorBps = Number((maxDebtorVolume * 10_000n) / acc.total);

      const meanAmount = acc.total / BigInt(acc.amounts.length);
      const largeInvoiceThreshold = meanAmount * 3n;
      let largeVolume = 0n;
      for (const amount of acc.amounts) {
        if (amount >= largeInvoiceThreshold) largeVolume += amount;
      }
      const sizeBps = Number((largeVolume * 10_000n) / acc.total);

      await client.query(
        `INSERT INTO sme_risk_signals
           (sme, debtor_concentration_bps, invoice_size_risk_bps, total_volume, updated_at)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (sme) DO UPDATE SET
           debtor_concentration_bps = $2,
           invoice_size_risk_bps = $3,
           total_volume = $4,
           updated_at = $5`,
        [sme, debtorBps, sizeBps, acc.total.toString(), updatedAt],
      );
    }
    await client.query("COMMIT");
  } catch (err) {
    await client.query("ROLLBACK");
    throw err;
  } finally {
    client.release();
  }
}

export async function getSmeRiskSignals(
  pool: Pool,
  sme?: string,
): Promise<SmeRiskSignalRow[]> {
  const params: any[] = [];
  let query = "SELECT * FROM sme_risk_signals";
  if (sme) {
    query += " WHERE sme = $1";
    params.push(sme);
  }
  query += " ORDER BY sme";

  const { rows } = await pool.query(query, params);
  return rows.map((r) => ({
    sme: r.sme,
    debtorConcentrationBps: r.debtor_concentration_bps,
    invoiceSizeRiskBps: r.invoice_size_risk_bps,
    totalVolume: toBigInt(r.total_volume),
    updatedAt:
      r.updated_at instanceof Date ? r.updated_at.toISOString() : r.updated_at,
  }));
}
