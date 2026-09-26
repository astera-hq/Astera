#!/usr/bin/env node
/**
 * Astera Soroban Event Indexer
 *
 * Subscribes to Stellar Horizon event streams for Astera contract events,
 * parses them, and stores them in Postgres for fast querying.
 */

import { Pool } from "pg";
import { Horizon } from "stellar-sdk";
import { parseEvents } from "./parser";
import {
  createPool,
  runMigrations,
  storeEvents,
  getLatestLedger,
  recomputeTrancheApy,
  recomputeSmeRiskSignals,
} from "./db";
import { startApiServer } from "./api";
import { runBackfill } from "./backfill";
import { detectReorg, fetchLedgerMeta, recordLedgerHash, rollbackFrom } from "./reorg";
import { logger } from "./logger";
import {
  eventsIndexedTotal,
  indexerLagLedgers,
  pollErrorsTotal,
  reorgsDetectedTotal,
} from "./metrics";

const HORIZON_URL =
  process.env.HORIZON_URL || "https://horizon-testnet.stellar.org";
// #700: also watch the credit_score contract so SME payment history is
// queryable off-chain. Accept it either inside CONTRACT_IDS (legacy) or as a
// dedicated CREDIT_SCORE_CONTRACT_ID env var. Dedupe to keep Horizon happy.
const INVOICE_POOL_CONTRACT_IDS = (process.env.CONTRACT_IDS || "")
  .split(",")
  .filter(Boolean);
const CREDIT_SCORE_CONTRACT_ID = (
  process.env.CREDIT_SCORE_CONTRACT_ID || ""
).trim();
// #861: also watch the oracle_registry contract, if deployed, so
// VerificationRound / vote / slash events are queryable off-chain.
const ORACLE_REGISTRY_CONTRACT_ID = (
  process.env.ORACLE_REGISTRY_CONTRACT_ID || ""
).trim();
// #1044: also watch the secondary_market contract, if deployed, so
// lst_open/lst_cncl/lst_buy listing events are queryable off-chain.
const SECONDARY_MARKET_CONTRACT_ID = (
  process.env.SECONDARY_MARKET_CONTRACT_ID || ""
).trim();
// #1055: also watch the insurance contract, if deployed, so
// covered/claimed/reserve events are queryable off-chain.
const INSURANCE_CONTRACT_ID = (process.env.INSURANCE_CONTRACT_ID || "").trim();
const CONTRACT_IDS = Array.from(
  new Set(
    [
      ...INVOICE_POOL_CONTRACT_IDS,
      CREDIT_SCORE_CONTRACT_ID,
      ORACLE_REGISTRY_CONTRACT_ID,
      SECONDARY_MARKET_CONTRACT_ID,
      INSURANCE_CONTRACT_ID,
    ].filter(Boolean),
  ),
);
const POLLING_INTERVAL_MS = parseInt(
  process.env.POLLING_INTERVAL_MS || "5000",
  10,
);
const API_PORT = parseInt(process.env.API_PORT || "3001", 10);
const DATABASE_URL = process.env.DATABASE_URL || "";
// #1039: --backfill <from> [--to <to>] CLI flags, in addition to the
// original BACKFILL_START_LEDGER/BACKFILL_END_LEDGER env vars, so a backfill
// run can be kicked off as a one-line command alongside the live-tail
// process (see backfill.ts).
const cliBackfill = parseBackfillArgs(process.argv.slice(2));
const BACKFILL_START_LEDGER =
  cliBackfill?.startLedger ??
  (process.env.BACKFILL_START_LEDGER
    ? parseInt(process.env.BACKFILL_START_LEDGER, 10)
    : null);
const BACKFILL_END_LEDGER =
  cliBackfill?.endLedger ??
  (process.env.BACKFILL_END_LEDGER
    ? parseInt(process.env.BACKFILL_END_LEDGER, 10)
    : null);
// #976: lookback window on restart to catch events missed during downtime
const LOOKBACK_LEDGERS = parseInt(process.env.LOOKBACK_LEDGERS || "100", 10);

function parseBackfillArgs(
  args: string[],
): { startLedger: number; endLedger: number | null } | null {
  const backfillIdx = args.indexOf("--backfill");
  if (backfillIdx === -1) return null;
  const startLedger = parseInt(args[backfillIdx + 1], 10);
  if (Number.isNaN(startLedger)) {
    throw new Error("--backfill requires a numeric starting ledger sequence");
  }
  const toIdx = args.indexOf("--to");
  const endLedger =
    toIdx !== -1 && args[toIdx + 1] ? parseInt(args[toIdx + 1], 10) : null;
  return { startLedger, endLedger: Number.isNaN(endLedger as number) ? null : endLedger };
}

async function main() {
  logger.info("[Astera Indexer] Starting...");
  logger.info(`[Astera Indexer] Horizon: ${HORIZON_URL}`);
  logger.info(
    `[Astera Indexer] Contracts: ${CONTRACT_IDS.join(", ") || "(none)"}`,
  );
  if (!DATABASE_URL) {
    throw new Error("DATABASE_URL is required (Postgres connection string)");
  }

  const pool = createPool();
  await runMigrations(DATABASE_URL);

  // #975: Check if backfill mode
  if (BACKFILL_START_LEDGER !== null) {
    logger.info(
      { from: BACKFILL_START_LEDGER, to: BACKFILL_END_LEDGER },
      "[Astera Indexer] Backfill mode",
    );
    await runBackfill(pool, {
      horizonUrl: HORIZON_URL,
      contractIds: CONTRACT_IDS,
      startLedger: BACKFILL_START_LEDGER,
      endLedger: BACKFILL_END_LEDGER,
    });
    logger.info("[Astera Indexer] Backfill complete. Exiting.");
    await pool.end();
    process.exit(0);
  }

  // #973: Store indexer state for health endpoint
  const state = { lastProcessedLedger: (await getLatestLedger(pool)) || "0" };

  // Start API server with state reference
  startApiServer(pool, API_PORT, state);

  // Start polling
  await pollLoop(pool, state);

  process.on("SIGINT", async () => {
    logger.info("[Astera Indexer] Shutting down...");
    await pool.end();
    process.exit(0);
  });
}

async function pollLoop(pool: Pool, state: { lastProcessedLedger: string }) {
  let cursor = await getLatestLedger(pool);

  // #976: Apply lookback window on startup to catch any events missed during downtime
  if (cursor) {
    const lookbackLedger = Math.max(0, parseInt(cursor, 10) - LOOKBACK_LEDGERS);
    logger.info(
      { lookbackLedger, cursor, LOOKBACK_LEDGERS },
      "[Astera Indexer] Applying lookback",
    );
    cursor = lookbackLedger.toString();
  }

  logger.info(`[Astera Indexer] Starting from ledger: ${cursor || "latest"}`);

  const horizon = new Horizon.Server(HORIZON_URL);

  const BASE_DELAY_MS = 1000;
  const MAX_DELAY_MS = 60000;
  const BACKOFF_MULTIPLIER = 2;
  const ALERT_THRESHOLD = 10;

  let consecutiveFailures = 0;

  while (true) {
    try {
      const response: any = await horizon
        .effects()
        .cursor(cursor || "")
        .order("asc")
        .call();

      const events = parseEvents(response.records || []);
      let reorgDetected = false;

      if (events.length > 0) {
        await storeEvents(pool, events);
        eventsIndexedTotal.inc(events.length);
        logger.info(`[Astera Indexer] Stored ${events.length} events`);
        const lastEvent = events[events.length - 1];
        cursor = lastEvent.ledgerSequence?.toString() || cursor;
        state.lastProcessedLedger = cursor || state.lastProcessedLedger;

        // #1039: reorg detection — check the highest ledger in this batch
        // against the hash chain we've recorded so far.
        const ledgersInBatch = Array.from(
          new Set(events.map((e) => e.ledgerSequence)),
        ).sort((a, b) => a - b);
        const tipLedger = ledgersInBatch[ledgersInBatch.length - 1];
        const meta = await fetchLedgerMeta(horizon, tipLedger);
        if (meta) {
          reorgDetected = await detectReorg(pool, meta);
          if (reorgDetected) {
            reorgsDetectedTotal.inc();
            const rollbackPoint = tipLedger - 1;
            logger.error(
              { tipLedger, rollbackPoint },
              "[Astera Indexer] ALERT: ledger reorg detected — rolling back and re-indexing",
            );
            await rollbackFrom(pool, rollbackPoint);
            cursor = rollbackPoint.toString();
            state.lastProcessedLedger = cursor;
          } else {
            await recordLedgerHash(pool, meta);
          }
        }

        // #862: refresh the derived per-tranche realized-APY table whenever a
        // tranche event lands in the batch, so the /tranches/apy endpoint stays
        // current for the frontend invest page.
        if (events.some((e) => e.contractType === "tranche")) {
          try {
            await recomputeTrancheApy(pool, new Date().toISOString());
          } catch (apyErr) {
            logger.error(
              { err: apyErr },
              "[Astera Indexer] Failed to recompute tranche APY",
            );
          }
        }

        // #1041: refresh derived per-SME risk signals whenever an invoice
        // event lands, so a risk-signal submitter reading
        // /credit-score/:sme/risk-signals sees current debtor-concentration/
        // invoice-size data.
        if (events.some((e) => e.contractType === "invoice")) {
          try {
            await recomputeSmeRiskSignals(pool, new Date().toISOString());
          } catch (riskErr) {
            logger.error(
              { err: riskErr },
              "[Astera Indexer] Failed to recompute SME risk signals",
            );
          }
        }
      }

      // Check if there are more pages. Skipped when a reorg was just
      // detected: `cursor` has already been rewound to the rollback point
      // above, and this batch's paging_token belongs to the discarded,
      // now-orphaned chain — advancing past it would undo the rewind.
      if (!reorgDetected && response.records && response.records.length > 0) {
        const lastRecord = response.records[response.records.length - 1];
        cursor = lastRecord.paging_token || cursor;
      }

      // #1039: lag metric — how far behind the chain tip we are.
      try {
        const tip: any = await horizon
          .ledgers()
          .order("desc")
          .limit(1)
          .call();
        const tipSequence = tip.records?.[0]?.sequence;
        if (tipSequence && cursor) {
          indexerLagLedgers.set(Math.max(0, tipSequence - parseInt(cursor, 10)));
        }
      } catch {
        // best-effort metric; ignore failures
      }

      consecutiveFailures = 0;

      await new Promise((resolve) => setTimeout(resolve, POLLING_INTERVAL_MS));
    } catch (error) {
      consecutiveFailures++;
      pollErrorsTotal.inc();

      const delay = Math.min(
        BASE_DELAY_MS * Math.pow(BACKOFF_MULTIPLIER, consecutiveFailures - 1),
        MAX_DELAY_MS,
      );

      logger.error(
        { err: error, consecutiveFailures },
        "[Astera Indexer] Error polling Horizon",
      );
      logger.info(`[Astera Indexer] Retrying in ${delay}ms...`);

      if (consecutiveFailures >= ALERT_THRESHOLD) {
        logger.error(
          { consecutiveFailures },
          "[Astera Indexer] ALERT: consecutive polling failures. Continuing to retry...",
        );
      }

      await new Promise((resolve) => setTimeout(resolve, delay));
    }
  }
}

main().catch((err) => {
  logger.error({ err }, "[Astera Indexer] Fatal error");
  process.exit(1);
});
