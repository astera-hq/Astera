/**
 * Initial Postgres schema for the indexer, mirroring the previous SQLite
 * `events`/`tranche_apy` tables (see the removed `MIGRATIONS` array in the
 * pre-Postgres src/db.ts) with Postgres-native types: JSONB for topic/value
 * instead of stringified JSON, BIGINT for ledger sequences, TIMESTAMPTZ for
 * timestamps, and NUMERIC for the i128-precision tranche APY fields.
 */

exports.up = (pgm) => {
  pgm.sql(`
    CREATE TABLE IF NOT EXISTS events (
      id TEXT PRIMARY KEY,
      contract_id TEXT NOT NULL,
      contract_type TEXT NOT NULL DEFAULT 'unknown',
      event_type TEXT NOT NULL,
      topic JSONB NOT NULL,
      value JSONB,
      actor_address TEXT,
      ledger_sequence BIGINT NOT NULL,
      ledger_close_at TIMESTAMPTZ NOT NULL,
      tx_hash TEXT NOT NULL,
      created_at TIMESTAMPTZ NOT NULL DEFAULT now()
    );

    CREATE INDEX IF NOT EXISTS idx_events_contract ON events(contract_id);
    CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
    CREATE INDEX IF NOT EXISTS idx_events_ledger ON events(ledger_sequence);
    CREATE INDEX IF NOT EXISTS idx_events_contract_type ON events(contract_type);
    CREATE INDEX IF NOT EXISTS idx_events_actor ON events(actor_address);

    CREATE TABLE IF NOT EXISTS tranche_apy (
      token TEXT NOT NULL,
      tranche_class TEXT NOT NULL,
      realized_principal NUMERIC NOT NULL DEFAULT 0,
      realized_return NUMERIC NOT NULL DEFAULT 0,
      closed_positions INTEGER NOT NULL DEFAULT 0,
      realized_apy_bps INTEGER NOT NULL DEFAULT 0,
      updated_at TIMESTAMPTZ NOT NULL,
      PRIMARY KEY (token, tranche_class)
    );
  `);
};

exports.down = (pgm) => {
  pgm.sql(`
    DROP TABLE IF EXISTS tranche_apy;
    DROP TABLE IF EXISTS events;
  `);
};
