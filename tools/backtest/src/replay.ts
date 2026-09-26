/**
 * Replays `credit_score` payment/default events from the indexer's Postgres
 * `events` table (see indexer/src/db.ts) into a per-SME score trajectory.
 *
 * Design note (#1041): rather than reimplementing
 * `calculate_score_with_config` (contracts/credit_score/src/lib.rs) in
 * TypeScript — which would drift from the real Rust formula over time — this
 * harness replays the *actual on-chain-computed* `score` that the contract
 * emitted in each `payment`/`default` event. That directly answers the
 * issue's question ("if we'd used this scoring formula historically, would
 * high scores have actually correlated with on-time repayment?") for the
 * formula that was actually deployed. Evaluating a not-yet-deployed
 * candidate formula would require a separate TS reimplementation kept in
 * sync with the Rust — noted as a follow-up, not attempted here to avoid
 * that drift risk.
 */
import { Pool } from 'pg';

export interface PaymentSample {
  ledgerSequence: number;
  ledgerCloseAt: string;
  invoiceId: string;
  /** 'PaidOnTime' | 'PaidLate' | 'Defaulted' */
  status: string;
  /** The contract's stored (pre-attestation-blend) internal score at this point. */
  score: number;
}

export interface SmeTrajectory {
  sme: string;
  samples: PaymentSample[];
  everDefaulted: boolean;
  /** Score from the sample immediately preceding the SME's first default, or the last sample if never defaulted. */
  scoreBeforeOutcome: number | null;
}

function decodeStatus(raw: unknown): string {
  if (typeof raw === 'string') return raw;
  if (Array.isArray(raw) && typeof raw[0] === 'string') return raw[0];
  if (raw && typeof raw === 'object') {
    const keys = Object.keys(raw as object);
    if (keys.length > 0) return keys[0];
  }
  return 'unknown';
}

/**
 * Read every `credit_score` `payment`/`default` event from the indexer DB
 * and group into one trajectory per SME, ordered by ledger sequence.
 */
export async function replay(pool: Pool): Promise<SmeTrajectory[]> {
  const { rows } = await pool.query(
    `SELECT * FROM events
     WHERE contract_type = 'credit_score' AND event_type IN ('payment', 'default')
     ORDER BY ledger_sequence ASC`,
  );

  const bySme = new Map<string, PaymentSample[]>();

  for (const row of rows) {
    let value: any;
    try {
      value = row.value ? (typeof row.value === 'string' ? JSON.parse(row.value) : row.value) : null;
    } catch {
      continue;
    }
    if (!Array.isArray(value) || value.length < 5) continue;

    // credit_score events: `payment` -> (caller, sme, invoice_id, status, score, ...)
    //                      `default` -> (caller, sme, invoice_id, score, ...) (no status field — always Defaulted)
    const sme = String(value[1] ?? '');
    if (!sme) continue;
    const invoiceId = String(value[2] ?? '');

    let status: string;
    let score: number;
    if (row.event_type === 'default') {
      status = 'Defaulted';
      score = Number(value[3] ?? value[value.length - 1]);
    } else {
      status = decodeStatus(value[3]);
      score = Number(value[4] ?? value[value.length - 1]);
    }
    if (!Number.isFinite(score)) continue;

    const list = bySme.get(sme) ?? [];
    list.push({
      ledgerSequence: row.ledger_sequence,
      ledgerCloseAt: row.ledger_close_at,
      invoiceId,
      status,
      score,
    });
    bySme.set(sme, list);
  }

  const trajectories: SmeTrajectory[] = [];
  for (const [sme, samples] of bySme) {
    const firstDefaultIdx = samples.findIndex((s) => s.status === 'Defaulted');
    const everDefaulted = firstDefaultIdx !== -1;
    const scoreBeforeOutcome = everDefaulted
      ? (samples[firstDefaultIdx - 1]?.score ?? samples[firstDefaultIdx].score)
      : (samples[samples.length - 1]?.score ?? null);

    trajectories.push({ sme, samples, everDefaulted, scoreBeforeOutcome });
  }
  return trajectories;
}
