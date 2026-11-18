import assert from 'node:assert';
import { Pool } from 'pg';
import { replay } from '../src/replay';
import { computePredictiveQuality } from '../src/metrics';

const SME_GOOD = 'GBRPYHIL2CI3FNQ4BXLFMNDLFJUNPU2HY3ZMFXYCZTM6WPIXY6OROLET';
const SME_BAD = 'GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37';

async function makeFixtureDb(): Promise<Pool> {
  const pool = new Pool({ connectionString: process.env.DATABASE_URL });
  await pool.query(`
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
  `);
  await pool.query('DELETE FROM events');

  let seq = 1;
  const insertRow = async (
    sme: string,
    eventType: 'payment' | 'default',
    invoiceId: number,
    status: string | null,
    score: number,
  ) => {
    const value =
      eventType === 'default'
        ? [sme, sme, invoiceId, score, 1_700_000_000]
        : [sme, sme, invoiceId, [status], score, 1_700_000_000];
    await pool.query(
      `INSERT INTO events (id, contract_id, contract_type, event_type, topic, value, ledger_sequence, ledger_close_at, tx_hash, created_at)
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)`,
      [
        `evt-${seq}`,
        'CCREDIT',
        'credit_score',
        eventType,
        JSON.stringify(['CREDIT', eventType]),
        JSON.stringify(value),
        seq++,
        '2026-08-21T00:00:00Z',
        `tx-${seq}`,
        '2026-08-21T00:00:00Z',
      ],
    );
  };

  // SME_GOOD: steady on-time payments, rising score, never defaults.
  await insertRow(SME_GOOD, 'payment', 1, 'PaidOnTime', 550);
  await insertRow(SME_GOOD, 'payment', 2, 'PaidOnTime', 580);
  await insertRow(SME_GOOD, 'payment', 3, 'PaidOnTime', 610);

  // SME_BAD: late payments, falling score, ends in default.
  await insertRow(SME_BAD, 'payment', 4, 'PaidLate', 480);
  await insertRow(SME_BAD, 'payment', 5, 'PaidLate', 450);
  await insertRow(SME_BAD, 'default', 6, null, 400);

  return pool;
}

async function runTests() {
  console.log('[backtest test] Running tests...');

  const pool = await makeFixtureDb();
  const trajectories = await replay(pool);

  assert.strictEqual(trajectories.length, 2);
  const good = trajectories.find((t) => t.sme === SME_GOOD)!;
  const bad = trajectories.find((t) => t.sme === SME_BAD)!;

  assert.ok(good, 'expected a trajectory for SME_GOOD');
  assert.strictEqual(good.everDefaulted, false);
  assert.strictEqual(good.scoreBeforeOutcome, 610);

  assert.ok(bad, 'expected a trajectory for SME_BAD');
  assert.strictEqual(bad.everDefaulted, true);
  assert.strictEqual(bad.scoreBeforeOutcome, 450, 'score from the sample before the default event');

  const quality = computePredictiveQuality(trajectories);
  assert.strictEqual(quality.cohortSize.defaulted, 1);
  assert.strictEqual(quality.cohortSize.nonDefaulted, 1);
  assert.strictEqual(
    quality.separationAuc,
    1.0,
    'the good cohort score (610) is strictly higher than the bad cohort score (450)',
  );

  await pool.end();
  console.log('[backtest test] All tests passed!');
}

runTests();
