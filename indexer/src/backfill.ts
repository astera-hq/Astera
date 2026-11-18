/**
 * Backfill mode: populate the database from a specified ledger range without
 * replaying the entire chain history from genesis.
 *
 * Writes go through storeEvents' INSERT ... ON CONFLICT DO NOTHING, so a
 * backfill run is safe to execute as its own process (its own Pool/DB
 * connection, invoked via `node dist/index.js --backfill <from> [--to <to>]`
 * or the BACKFILL_START_LEDGER/BACKFILL_END_LEDGER env vars) concurrently
 * with the live-tail poller — the two never need to coordinate writes, and a
 * ledger already covered by one is just deduplicated by the other.
 */

import { Pool } from "pg";
import { Horizon } from "stellar-sdk";
import { parseEvents } from "./parser";
import { storeEvents } from "./db";
import { logger } from "./logger";
import { backfillProgressLedger, eventsIndexedTotal } from "./metrics";

export interface BackfillOptions {
  horizonUrl: string;
  contractIds: string[];
  startLedger: number;
  endLedger: number | null;
}

export async function runBackfill(
  pool: Pool,
  options: BackfillOptions,
): Promise<void> {
  const { horizonUrl, contractIds, startLedger, endLedger } = options;

  logger.info(
    { startLedger, endLedger },
    "[backfill] Starting backfill",
  );

  const horizon = new Horizon.Server(horizonUrl);
  let currentLedger = startLedger;
  let totalEvents = 0;
  let retryCount = 0;
  const maxRetries = 8;

  while (true) {
    try {
      const response: any = await horizon
        .effects()
        .cursor(currentLedger.toString())
        .order("asc")
        .limit(100)
        .call();
      retryCount = 0;

      if (!response.records || response.records.length === 0) {
        logger.info(
          { currentLedger, totalEvents },
          "[backfill] Complete — no more records",
        );
        break;
      }

      const events = parseEvents(response.records || []).filter(
        (e) => contractIds.length === 0 || contractIds.includes(e.contractId),
      );

      if (events.length > 0) {
        await storeEvents(pool, events);
        totalEvents += events.length;
        eventsIndexedTotal.inc(events.length);
        const lastEvent = events[events.length - 1];
        currentLedger = lastEvent.ledgerSequence;
        backfillProgressLedger.set(currentLedger);
        logger.info(
          { count: events.length, currentLedger },
          "[backfill] Backfilled events",
        );
      }

      if (endLedger !== null && currentLedger >= endLedger) {
        logger.info(
          { endLedger, totalEvents },
          "[backfill] Reached end ledger",
        );
        break;
      }

      const lastRecord = response.records[response.records.length - 1];
      const nextLedger = lastRecord.ledger_sequence || currentLedger + 1;
      currentLedger = nextLedger === currentLedger ? currentLedger + 1 : nextLedger;

      // Brief delay to avoid rate limiting
      await new Promise((resolve) => setTimeout(resolve, 100));
    } catch (error) {
      retryCount += 1;
      logger.error(
        { err: error, currentLedger, retryCount, maxRetries },
        "[backfill] Error during backfill",
      );
      retryCount += 1;
      if (retryCount > maxRetries) {
        throw new Error(`[backfill] Aborting after ${maxRetries} consecutive failures`);
      }
      const delayMs = Math.min(1_000 * 2 ** (retryCount - 1), 60_000);
      await new Promise((resolve) => setTimeout(resolve, delayMs));
    }
  }
}
