/**
 * SQLite database for storing indexed Soroban events.
 */

import Database from 'better-sqlite3';
import { IndexedEvent } from './parser';

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
  `);

  return db;
}

export function storeEvents(db: Database.Database, events: IndexedEvent[]): void {
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

  return rows.map((row) => ({
    id: row.id,
    contractId: row.contract_id,
    eventType: row.event_type,
    topic: JSON.parse(row.topic),
    value: row.value ? JSON.parse(row.value) : null,
    ledgerSequence: row.ledger_sequence,
    ledgerCloseAt: row.ledger_close_at,
    txHash: row.tx_hash,
    createdAt: row.created_at,
  }));
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
