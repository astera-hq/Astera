/**
 * Integration test for the sme_risk_signals derived table (#1041). Requires
 * a real Postgres instance with migrations applied:
 *
 *   docker compose up -d indexer-postgres
 *   cd indexer && npm run migrate:up
 *   DATABASE_URL=postgres://astera:astera@localhost:5433/astera_indexer \
 *     ts-node src/db.test.ts
 *
 * Not run in CI (there's no indexer job) — this mirrors how the rest of the
 * indexer's Postgres-backed code is exercised (manually, against a real
 * instance), same as recomputeTrancheApy before it.
 */
import assert from 'node:assert';
import { createPool, storeEvents, recomputeSmeRiskSignals, getSmeRiskSignals } from './db';
import { IndexedEvent } from './parser';

const SME_A = 'GBRPYHIL2CI3FNQ4BXLFMNDLFJUNPU2HY3ZMFXYCZTM6WPIXY6OROLET';
const SME_B = 'GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37';

function createdEvent(
  id: number,
  owner: string,
  amount: number,
  debtor: string,
  ledgerSequence: number,
): IndexedEvent {
  return {
    id: `db-test-evt-${id}`,
    contractId: 'CINVOICETEST',
    contractType: 'invoice',
    eventType: 'created',
    topic: ['INVOICE', 'created'],
    // #1041: (id, owner, amount, metadata_uri, timestamp, debtor)
    value: [id, owner, String(amount), 'ipfs://meta', 1_700_000_000, debtor],
    actor: owner,
    ledgerSequence,
    ledgerCloseAt: '2026-08-21T00:00:00Z',
    txHash: `db-test-tx-${id}`,
    createdAt: '2026-08-21T00:00:00Z',
  };
}

async function main() {
  console.log('[indexer db test] Running sme_risk_signals tests...');

  const pool = createPool();
  try {
    // SME_A: 8000 to one debtor, 2000 to another — concentrated.
    // SME_B: 5000/5000 split across two debtors — diversified.
    await storeEvents(pool, [
      createdEvent(900001, SME_A, 8000, 'db-test-debtor-1', 900001),
      createdEvent(900002, SME_A, 2000, 'db-test-debtor-2', 900002),
      createdEvent(900003, SME_B, 5000, 'db-test-debtor-3', 900003),
      createdEvent(900004, SME_B, 5000, 'db-test-debtor-4', 900004),
    ]);

    await recomputeSmeRiskSignals(pool, '2026-08-21T00:00:00Z');

    const rowA = (await getSmeRiskSignals(pool, SME_A))[0];
    assert.ok(rowA, 'expected a risk-signal row for SME_A');
    assert.strictEqual(rowA.debtorConcentrationBps, 8000, 'debtor-1 is 80% of SME_A volume');
    assert.strictEqual(rowA.totalVolume, 10000n);

    const rowB = (await getSmeRiskSignals(pool, SME_B))[0];
    assert.ok(rowB, 'expected a risk-signal row for SME_B');
    assert.strictEqual(rowB.debtorConcentrationBps, 5000, 'no single debtor exceeds 50% for SME_B');

    // Idempotent: recomputing again from the same event stream must not change results.
    await recomputeSmeRiskSignals(pool, '2026-08-21T00:05:00Z');
    const rowAAgain = (await getSmeRiskSignals(pool, SME_A))[0];
    assert.strictEqual(rowAAgain.debtorConcentrationBps, rowA.debtorConcentrationBps);

    console.log('[indexer db test] All sme_risk_signals tests passed!');
  } finally {
    await pool.query("DELETE FROM sme_risk_signals WHERE sme IN ($1, $2)", [SME_A, SME_B]);
    await pool.query("DELETE FROM events WHERE id LIKE 'db-test-evt-%'");
    await pool.end();
  }
}

main().catch((err) => {
  console.error('[indexer db test] FAILED:', err);
  process.exit(1);
});
