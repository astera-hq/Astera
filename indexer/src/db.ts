/**
 * SQLite database for storing indexed Soroban events.
 */

import Database from 'better-sqlite3';
import { IndexedEvent } from './parser';
import { Pool } from 'pg';

export function initDb(dbPath: string): Database.Database {
  const db = new Database(dbPath);

  db.pragma('journal_mode = WAL');
  db.pragma('synchronous = NORMAL');

  db.exec(`
    CREATE TABLE IF NOT EXISTS events (
      id TEXT PRIMARY KEY,
      contract_id TEXT NOT NULL,
      event_type TEXT NOT NULL,
      topic TEXT NOT NULL,
      value TEXT,
      ledger_sequence INTEGER NOT NULL,
      ledger_close_at TEXT NOT NULL,
      tx_hash TEXT NOT NULL,
      created_at TEXT NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_events_contract
      ON events(contract_id);
    CREATE INDEX IF NOT EXISTS idx_events_type
      ON events(event_type);
    CREATE INDEX IF NOT EXISTS idx_events_ledger
      ON events(ledger_sequence);

    CREATE TABLE IF NOT EXISTS pool_history (
      token TEXT NOT NULL,
      snapshot_date TEXT NOT NULL,
      deposited REAL NOT NULL DEFAULT 0,
      deployed REAL NOT NULL DEFAULT 0,
      utilization REAL NOT NULL DEFAULT 0,
      default_rate REAL NOT NULL DEFAULT 0,
      PRIMARY KEY (token, snapshot_date)
    );

    CREATE TABLE IF NOT EXISTS analytics_summary (
      snapshot_date TEXT PRIMARY KEY,
      tvl REAL NOT NULL DEFAULT 0,
      volume REAL NOT NULL DEFAULT 0,
      default_rate REAL NOT NULL DEFAULT 0,
      active_invoices INTEGER NOT NULL DEFAULT 0
    );

    CREATE TABLE IF NOT EXISTS invoices (
      id TEXT PRIMARY KEY,
      owner TEXT NOT NULL DEFAULT '',
      debtor TEXT NOT NULL DEFAULT '',
      amount TEXT NOT NULL DEFAULT '0',
      status TEXT NOT NULL DEFAULT 'Pending',
      due_date INTEGER,
      updated_ledger INTEGER NOT NULL DEFAULT 0
    );

    CREATE INDEX IF NOT EXISTS idx_invoices_owner ON invoices(owner);
    CREATE INDEX IF NOT EXISTS idx_invoices_debtors ON invoices(debtor);
    CREATE INDEX IF NOT EXISTS idx_invoices_status ON invoices(status);
    CREATE INDEX IF NOT EXISTS idx_invoices_due_date ON invoices(due_date);
    CREATE INDEX IF NOT EXISTS idx_invoices_updated_ledger ON invoices(updated_ledger);
  `);

  return db;
}

export async function storeEvents(db: Database.Database | Pool, events: IndexedEvent[]): Promise<void> {
  if ('query' in db) {
    for (const event of events) {
      await db.query(`
        INSERT INTO events
          (id, contract_id, contract_type, event_type, topic, value, actor_address, ledger_sequence, ledger_close_at, tx_hash, created_at)
        VALUES ($1,$2,$3,$4,$5::jsonb,$6::jsonb,$7,$8,$9,$10,$11)
        ON CONFLICT (id) DO NOTHING
      `, [event.id, event.contractId, event.contractType, event.eventType, JSON.stringify(event.topic), event.value === null ? null : JSON.stringify(event.value), event.actor, event.ledgerSequence, event.ledgerCloseAt, event.txHash, event.createdAt]);
    }
    return;
  }

  if (events.length === 0) return;

  const stmt = db.prepare(`
    INSERT OR IGNORE INTO events
      (id, contract_id, event_type, topic, value, ledger_sequence, ledger_close_at, tx_hash, created_at)
    VALUES
      (@id, @contractId, @eventType, @topic, @value, @ledgerSequence, @ledgerCloseAt, @txHash, @createdAt)
  `);

  const insertMany = db.transaction((events: IndexedEvent[]) => {
    for (const event of events) {
      stmt.run({
        id: event.id,
        contractId: event.contractId,
        eventType: event.eventType,
        topic: JSON.stringify(event.topic),
        value: JSON.stringify(event.value),
        ledgerSequence: event.ledgerSequence,
        ledgerCloseAt: event.ledgerCloseAt,
        txHash: event.txHash,
        createdAt: event.createdAt,
      });
      updateInvoiceFromEvent(db, event);
    }
  });

  insertMany(events);
}

export function getEvents(
  db: Database.Database,
  options: {
    contractId?: string;
    eventType?: string;
    limit?: number;
    offset?: number;
  } = {}
): IndexedEvent[] {
  const { contractId, eventType, limit = 50, offset = 0 } = options;

  let query = 'SELECT * FROM events WHERE 1=1';
  const params: any[] = [];

  if (contractId) {
    query += ' AND contract_id = ?';
    params.push(contractId);
  }

  if (eventType) {
    query += ' AND event_type = ?';
    params.push(eventType);
  }

  query += ' ORDER BY ledger_sequence DESC LIMIT ? OFFSET ?';
  params.push(limit, offset);

  const rows = db.prepare(query).all(...params) as any[];

  return rows.map((row) => {
    const value = row.value ? JSON.parse(row.value) : null;
    const topic = JSON.parse(row.topic) as string[];
    const eventType = row.event_type as string;
    const contractType = topic?.[0]?.toLowerCase() === 'invoice'
      ? 'invoice'
      : (topic?.[0]?.toLowerCase() as any) || 'unknown';
    return {
      id: row.id,
      contractId: row.contract_id,
      contractType,
      eventType,
      topic,
      value,
      actor: null,
      ledgerSequence: row.ledger_sequence,
      ledgerCloseAt: row.ledger_close_at,
      txHash: row.tx_hash,
      createdAt: row.created_at,
    };
  });
}

export function getLatestLedger(db: Database.Database): string | null {
  const row = db.prepare('SELECT ledger_sequence FROM events ORDER BY ledger_sequence DESC LIMIT 1').get() as
    | { ledger_sequence: number }
    | undefined;

  return row ? row.ledger_sequence.toString() : null;
}

export function upsertPoolHistory(
  db: Database.Database,
  token: string,
  snapshot: { deposited: number; deployed: number; utilization: number; defaultRate: number },
): void {
  const date = new Date().toISOString().slice(0, 10);
  db.prepare(`
    INSERT INTO pool_history (token, snapshot_date, deposited, deployed, utilization, default_rate)
    VALUES (?, ?, ?, ?, ?, ?)
    ON CONFLICT(token, snapshot_date) DO UPDATE SET
      deposited = excluded.deposited,
      deployed = excluded.deployed,
      utilization = excluded.utilization,
      default_rate = excluded.default_rate
  `).run(token, date, snapshot.deposited, snapshot.deployed, snapshot.utilization, snapshot.defaultRate);
}

export function getPoolHistory(db: Database.Database, token: string, limit = 90): any[] {
  const safeLimit = Math.max(1, Math.min(limit, 365));
  return db.prepare(`
    SELECT snapshot_date AS time, deposited, deployed, utilization, default_rate AS defaultRate
    FROM pool_history
    WHERE token = ?
    ORDER BY snapshot_date DESC
    LIMIT ?
  `).all(token, safeLimit).reverse();
}

export function upsertAnalyticsSummary(
  db: Database.Database,
  snapshot: { tvl: number; volume: number; defaultRate: number; activeInvoices: number },
): void {
  const date = new Date().toISOString().slice(0, 10);
  db.prepare(`
    INSERT INTO analytics_summary (snapshot_date, tvl, volume, default_rate, active_invoices)
    VALUES (?, ?, ?, ?, ?)
    ON CONFLICT(snapshot_date) DO UPDATE SET
      tvl = excluded.tvl,
      volume = excluded.volume,
      default_rate = excluded.default_rate,
      active_invoices = excluded.active_invoices
  `).run(date, snapshot.tvl, snapshot.volume, snapshot.defaultRate, snapshot.activeInvoices);
}

export function getAnalyticsSummary(db: Database.Database, limit = 90): any[] {
  const safeLimit = Math.max(1, Math.min(limit, 365));
  return db.prepare(`
    SELECT snapshot_date AS time, tvl, volume, default_rate AS defaultRate,
           active_invoices AS activeInvoices
    FROM analytics_summary
    ORDER BY snapshot_date DESC
    LIMIT ?
  `).all(safeLimit).reverse();
}

export interface IndexedInvoice {
  id: string;
  owner: string;
  debtor: string;
  amount: string;
  status: string;
  dueDate: number | null;
  updatedLedger: number;
}

function valueAt(value: any, arrayIndex: number, objectKeys: string[]): any {
  if (Array.isArray(value)) return value[arrayIndex];
  if (value && typeof value === 'object') {
    for (const key of objectKeys) {
      if (value[key] !== undefined && value[key] !== null) return value[key];
    }
  }
  return undefined;
}

function invoiceStatus(eventType: string): string | null {
  switch (eventType.toLowerCase()) {
    case 'created': return 'Pending';
    case 'awaitingverification': return 'AwaitingVerification';
    case 'verified': return 'Verified';
    case 'disputed': return 'Disputed';
    case 'funded': return 'Funded';
    case 'paid': return 'Paid';
    case 'default':
    case 'defaulted': return 'Defaulted';
    case 'cancelled':
    case 'canceled': return 'Cancelled';
    case 'expired': return 'Expired';
    default: return null;
  }
}

function scalarToString(value: any): string {
  if (value === undefined || value === null) return '';
  if (typeof value === 'string') return value;
  if (typeof value === 'number') return Number.isSafeInteger(value) ? String(value) : String(value);
  if (typeof value === 'bigint') return String(value);
  if (typeof value === 'object' && value !== null) return String(value);
  return '';
}

export function updateInvoiceFromEvent(db: Database.Database, event: IndexedEvent): void {
  if (event.contractType !== 'invoice') return;

  const idValue = valueAt(event.value, 0, ['id', 'invoiceId', 'invoice_id']);
  if (idValue === undefined) return;
  const id = scalarToString(idValue);

  const owner = scalarToString(valueAt(event.value, 1, ['owner', 'sme', 'ownerAddress']));
  const debtor = scalarToString(valueAt(event.value, 5, ['debtor', 'debtorName', 'debtor_id']));
  const amount = scalarToString(valueAt(event.value, 2, ['amount', 'invoiceAmount']));
  const dueDateValue = valueAt(event.value, 6, ['dueDate', 'due_date']);
  const dueDate = dueDateValue === undefined ? null : Number(dueDateValue);
  const status = invoiceStatus(event.eventType);

  if (event.eventType.toLowerCase() === 'created') {
    db.prepare(`
      INSERT INTO invoices (id, owner, debtor, amount, status, due_date, updated_ledger)
      VALUES (?, ?, ?, ?, ?, ?, ?)
      ON CONFLICT(id) DO UPDATE SET
        owner = CASE WHEN excluded.owner != '' THEN excluded.owner ELSE invoices.owner END,
        debtor = CASE WHEN excluded.debtor != '' THEN excluded.debtor ELSE invoices.debtor END,
        amount = CASE WHEN excluded.amount != '' THEN excluded.amount ELSE invoices.amount END,
        status = excluded.status,
        due_date = CASE WHEN excluded.due_date IS NOT NULL THEN excluded.due_date ELSE invoices.due_date END,
        updated_ledger = excluded.updated_ledger
    `).run(id, owner, debtor, amount || '0', status || 'Pending', dueDate, event.ledgerSequence);
  } else if (status) {
    db.prepare(`
      INSERT INTO invoices (id, owner, debtor, amount, status, due_date, updated_ledger)
      VALUES (?, ?, ?, '0', ?, ?, ?)
      ON CONFLICT(id) DO UPDATE SET
        status = excluded.status,
        due_date = CASE WHEN excluded.due_date IS NOT NULL THEN excluded.due_date ELSE invoices.due_date END,
        updated_ledger = excluded.updated_ledger
    `).run(id, owner, debtor, status, dueDate, event.ledgerSequence);
  }
}

export function searchInvoices(
  db: Database.Database,
  query: string,
  options: { status?: string; limit?: number; offset?: number } = {},
): IndexedInvoice[] {
  const limit = Math.max(1, Math.min(options.limit ?? 50, 100));
  const offset = Math.max(0, options.offset ?? 0);
  const term = query.trim();

  let sql = 'SELECT id, owner, debtor, amount, status, due_date, updated_ledger FROM invoices WHERE 1 = 1';
  const params: any[] = [];

  if (term) {
    sql += ' AND (LOWER(owner) LIKE ? OR LOWER(debtor) LIKE ? OR LOWER(id) LIKE ?)';
    const pattern = `%${term.toLowerCase().replace(/[\\%_]/g, (char) => `\\${char}`)}%`;
    params.push(pattern, pattern, pattern);
  }
  if (options.status) {
    sql += ' AND LOWER(status) = LOWER(?)';
    params.push(options.status);
  }

  sql += ' ORDER BY updated_ledger DESC, id ASC LIMIT ? OFFSET ?';
  params.push(limit, offset);

  const rows = db.prepare(sql).all(...params) as any[];
  return rows.map((row) => ({
    id: row.id,
    owner: row.owner,
    debtor: row.debtor,
    amount: row.amount,
    status: row.status,
    dueDate: row.due_date,
    updatedLedger: row.updated_ledger,
  }));
}

export function createPool(): Pool {
  return new Pool({ connectionString: process.env.DATABASE_URL });
}

export async function recomputeSmeRiskSignals(pool: Pool, updatedAt: string): Promise<void> {
  await pool.query(`
    WITH created AS (
      SELECT
        COALESCE(actor_address, value->>1, '') AS sme,
        COALESCE(value->>5, '') AS debtor,
        COALESCE((value->>2)::numeric, 0) AS amount
      FROM events
      WHERE contract_type = 'invoice' AND event_type = 'created'
    ),
    totals AS (
      SELECT sme, SUM(amount) AS total_volume FROM created WHERE sme <> '' GROUP BY sme
    ),
    concentrations AS (
      SELECT debtor_totals.sme,
             ROUND(MAX(debtor_totals.debtor_volume) * 10000.0 / NULLIF(totals.total_volume, 0))::integer AS concentration,
             totals.total_volume
      FROM (
        SELECT sme, debtor, SUM(amount) AS debtor_volume
        FROM created WHERE sme <> '' GROUP BY sme, debtor
      ) debtor_totals
      JOIN totals ON totals.sme = debtor_totals.sme
      GROUP BY debtor_totals.sme, totals.total_volume
    )
    INSERT INTO sme_risk_signals (sme, debtor_concentration_bps, invoice_size_risk_bps, total_volume, updated_at)
    SELECT sme, concentration, 0, total_volume, $1::timestamptz
    FROM concentrations
    ON CONFLICT (sme) DO UPDATE SET
      debtor_concentration_bps = excluded.debtor_concentration_bps,
      invoice_size_risk_bps = excluded.invoice_size_risk_bps,
      total_volume = excluded.total_volume,
      updated_at = excluded.updated_at
  `, [updatedAt]);
}

export async function getSmeRiskSignals(pool: Pool, sme: string): Promise<Array<{
  sme: string;
  debtorConcentrationBps: number;
  invoiceSizeRiskBps: number;
  totalVolume: bigint;
}>> {
  const result = await pool.query(`
    SELECT sme, debtor_concentration_bps, invoice_size_risk_bps, total_volume
    FROM sme_risk_signals WHERE sme = $1
  `, [sme]);
  return result.rows.map((row) => ({
    sme: row.sme,
    debtorConcentrationBps: Number(row.debtor_concentration_bps),
    invoiceSizeRiskBps: Number(row.invoice_size_risk_bps),
    totalVolume: BigInt(row.total_volume),
  }));
}
