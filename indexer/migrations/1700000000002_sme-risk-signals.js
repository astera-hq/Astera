/**
 * #1041: derived per-SME risk signals (debtor concentration, invoice-size
 * risk) aggregated off-chain from `invoice` `created` events. Recomputed
 * from scratch on each ingest batch that includes an invoice event, same
 * pattern as tranche_apy. This is the read side a risk-signal submitter
 * consumes before calling the credit_score contract's `submit_risk_signal`
 * entrypoint.
 */

exports.up = (pgm) => {
  pgm.sql(`
    CREATE TABLE IF NOT EXISTS sme_risk_signals (
      sme TEXT PRIMARY KEY,
      debtor_concentration_bps INTEGER NOT NULL DEFAULT 0,
      invoice_size_risk_bps INTEGER NOT NULL DEFAULT 0,
      total_volume NUMERIC NOT NULL DEFAULT 0,
      updated_at TIMESTAMPTZ NOT NULL
    );
  `);
};

exports.down = (pgm) => {
  pgm.sql(`DROP TABLE IF EXISTS sme_risk_signals;`);
};
