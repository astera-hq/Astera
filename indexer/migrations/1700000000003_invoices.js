exports.up = (pgm) => {
  pgm.sql(`
    CREATE EXTENSION IF NOT EXISTS pg_trgm;

    CREATE TABLE IF NOT EXISTS invoices (
      id TEXT PRIMARY KEY,
      owner TEXT NOT NULL DEFAULT '',
      debtor TEXT NOT NULL DEFAULT '',
      amount NUMERIC NOT NULL DEFAULT 0,
      status TEXT NOT NULL DEFAULT 'Pending',
      due_date BIGINT,
      updated_ledger BIGINT NOT NULL DEFAULT 0
    );

    CREATE INDEX IF NOT EXISTS idx_invoices_owner ON invoices(owner);
    CREATE INDEX IF NOT EXISTS idx_invoices_debtors ON invoices(debtor);
    CREATE INDEX IF NOT EXISTS idx_invoices_status ON invoices(status);
    CREATE INDEX IF NOT EXISTS idx_invoices_due_date ON invoices(due_date);
    CREATE INDEX IF NOT EXISTS idx_invoices_updated_ledger ON invoices(updated_ledger);
    CREATE INDEX IF NOT EXISTS idx_invoices_owner_trgm ON invoices USING gin (owner gin_trgm_ops);
    CREATE INDEX IF NOT EXISTS idx_invoices_debtor_trgm ON invoices USING gin (debtor gin_trgm_ops);
    CREATE INDEX IF NOT EXISTS idx_invoices_id_trgm ON invoices USING gin (id gin_trgm_ops);
  `);
};

exports.down = (pgm) => {
  pgm.sql(`DROP TABLE IF EXISTS invoices;`);
};
