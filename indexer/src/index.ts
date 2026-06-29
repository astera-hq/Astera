#!/usr/bin/env node
/**
 * Astera Soroban Event Indexer
 *
 * Subscribes to Stellar Horizon event streams for Astera contract events,
 * parses them, and stores them in a SQLite database for fast querying.
 */

import { Horizon } from 'stellar-sdk';
import { parseEvents } from './parser';
import { initDb, storeEvents, getEvents, getLatestLedger } from './db';
import { startApiServer } from './api';

const HORIZON_URL = process.env.HORIZON_URL || 'https://horizon-testnet.stellar.org';
// #700: also watch the credit_score contract so SME payment history is
// queryable off-chain. Accept it either inside CONTRACT_IDS (legacy) or as a
// dedicated CREDIT_SCORE_CONTRACT_ID env var. Dedupe to keep Horizon happy.
const INVOICE_POOL_CONTRACT_IDS = (process.env.CONTRACT_IDS || '')
  .split(',')
  .filter(Boolean);
const CREDIT_SCORE_CONTRACT_ID = (process.env.CREDIT_SCORE_CONTRACT_ID || '').trim();
const CONTRACT_IDS = Array.from(
  new Set(
    [...INVOICE_POOL_CONTRACT_IDS, CREDIT_SCORE_CONTRACT_ID].filter(Boolean),
  ),
);
const POLLING_INTERVAL_MS = parseInt(process.env.POLLING_INTERVAL_MS || '5000', 10);
const API_PORT = parseInt(process.env.API_PORT || '3001', 10);
const DB_PATH = process.env.DB_PATH || './indexer.db';

async function main() {
  console.log('[Astera Indexer] Starting...');
  console.log(`[Astera Indexer] Horizon: ${HORIZON_URL}`);
  console.log(`[Astera Indexer] Contracts: ${CONTRACT_IDS.join(', ') || '(none)'}`);
  if (CREDIT_SCORE_CONTRACT_ID) {
    console.log(`[Astera Indexer] Credit-score contract: ${CREDIT_SCORE_CONTRACT_ID}`);
  }
  console.log(`[Astera Indexer] DB: ${DB_PATH}`);

  // Initialize database
  const db = initDb(DB_PATH);

  // Start API server
  startApiServer(db, API_PORT);

  // Start polling
  await pollLoop(db);

  process.on('SIGINT', () => {
    console.log('\n[Astera Indexer] Shutting down...');
    process.exit(0);
  });
}

async function pollLoop(db: any) {
  let cursor = getLatestLedger(db);
  console.log(`[Astera Indexer] Starting from ledger: ${cursor || 'latest'}`);

  const BASE_DELAY_MS = 1000;
  const MAX_DELAY_MS = 60000;
  const BACKOFF_MULTIPLIER = 2;
  const ALERT_THRESHOLD = 10;

  let consecutiveFailures = 0;

  while (true) {
    try {
      const horizon = new Horizon.Server(HORIZON_URL);
      const params: any = {
        join: 'transactions',
        limit: 100,
      };

      if (cursor) {
        params.cursor = cursor;
      }

      if (CONTRACT_IDS.length > 0) {
        params.contractIds = CONTRACT_IDS;
      }

      const response: any = await horizon
        .effects()
        .cursor(cursor || '')
        .order('asc')
        .call();

      const events = parseEvents(response.records || []);

      if (events.length > 0) {
        storeEvents(db, events);
        console.log(`[Astera Indexer] Stored ${events.length} events`);
        const lastEvent = events[events.length - 1];
        cursor = lastEvent.ledgerSequence?.toString() || cursor;
      }

      // Check if there are more pages
      if (response.records && response.records.length > 0) {
        const lastRecord = response.records[response.records.length - 1];
        cursor = lastRecord.paging_token || cursor;
      }

      consecutiveFailures = 0;

      await new Promise((resolve) => setTimeout(resolve, POLLING_INTERVAL_MS));
    } catch (error) {
      consecutiveFailures++;

      const delay = Math.min(
        BASE_DELAY_MS * Math.pow(BACKOFF_MULTIPLIER, consecutiveFailures - 1),
        MAX_DELAY_MS,
      );

      console.error(
        `[Astera Indexer] Error polling Horizon (attempt ${consecutiveFailures}):`,
        error,
      );
      console.log(`[Astera Indexer] Retrying in ${delay}ms...`);

      if (consecutiveFailures >= ALERT_THRESHOLD) {
        console.error(
          `[Astera Indexer] ALERT: ${consecutiveFailures} consecutive polling failures. Continuing to retry...`,
        );
      }

      await new Promise((resolve) => setTimeout(resolve, delay));
    }
  }
}

main().catch((err) => {
  console.error('[Astera Indexer] Fatal error:', err);
  process.exit(1);
});
