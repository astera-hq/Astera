/**
 * Ledger-hash chain used to detect Horizon-reported reorgs (see src/reorg.ts):
 * every ledger we index records its own hash and its parent's hash, so the
 * next ledger's prev_hash can be checked against what we stored for its
 * predecessor. reorg_log keeps an audit trail of detected reorgs and the
 * rollback each one triggered.
 */

exports.up = (pgm) => {
  pgm.sql(`
    CREATE TABLE IF NOT EXISTS ledger_hashes (
      ledger_sequence BIGINT PRIMARY KEY,
      ledger_hash TEXT NOT NULL,
      prev_hash TEXT NOT NULL,
      indexed_at TIMESTAMPTZ NOT NULL DEFAULT now()
    );

    CREATE TABLE IF NOT EXISTS reorg_log (
      id SERIAL PRIMARY KEY,
      detected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      reorg_ledger BIGINT NOT NULL,
      rolled_back_from BIGINT NOT NULL,
      events_removed INTEGER NOT NULL,
      detail TEXT
    );
  `);
};

exports.down = (pgm) => {
  pgm.sql(`
    DROP TABLE IF EXISTS reorg_log;
    DROP TABLE IF EXISTS ledger_hashes;
  `);
};
