#!/usr/bin/env node
/**
 * Astera Soroban Event Indexer
 *
 * Subscribes to Stellar Horizon event streams for Astera contract events,
 * parses them, and stores them in a SQLite database for fast querying.
 */

import { Horizon } from '@stellar/stellar-sdk';
import { parseEvents } from './parser';
import { initDb, storeEvents, getLatestLedger, upsertPoolHistory, upsertAnalyticsSummary } from './db';
import { startApiServer, startMetricsServer } from './api';

const HORIZON_URL = process.env.HORIZON_URL || 'https://horizon-testnet.stellar.org';
const CONTRACT_IDS = (process.env.CONTRACT_IDS || '').split(',').filter(Boolean);
const POLLING_INTERVAL_MS = parseInt(process.env.POLLING_INTERVAL_MS || '5000', 10);
const API_PORT = parseInt(process.env.API_PORT || '3001', 10);
const DB_PATH = process.env.DB_PATH || './indexer.db';
const METRICS_PORT = parseInt(process.env.METRICS_PORT || '9091', 10);

async function main() {
  console.log('[Astera Indexer] Starting...');
  console.log(`[Astera Indexer] Horizon: ${HORIZON_URL}`);
  console.log(`[Astera Indexer] Contracts: ${CONTRACT_IDS.join(', ') || '(none)'}`);
  console.log(`[Astera Indexer] DB: ${DB_PATH}`);

  // Initialize database
  const db = initDb(DB_PATH);

  // Start API server
  startApiServer(db, API_PORT);
  // Start Metrics server
  startMetricsServer(db, METRICS_PORT);

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

        const numericValue = (event: any, keys: string[]): number => {
          if (!event?.value || typeof event.value !== 'object') return 0;
          for (const key of keys) {
            if (typeof event.value[key] === 'number') return event.value[key];
            if (typeof event.value[key] === 'string') {
              const parsed = Number(event.value[key]);
              if (Number.isFinite(parsed)) return parsed;
            }
          }
          return 0;
        };

        const deposited = events.reduce((sum, event) => sum + numericValue(event, ['deposited', 'totalDeposited', 'tvl']), 0);
        const deployed = events.reduce((sum, event) => sum + numericValue(event, ['deployed', 'totalDeployed']), 0);
        const utilization = deposited > 0 ? Math.min(100, (deployed / deposited) * 100) : 0;
        const defaultCount = events.filter((event) => event.eventType.toLowerCase().includes('default')).length;
        const defaultRate = events.length > 0 ? (defaultCount / events.length) * 100 : 0;
        const activeInvoices = events.filter((event) => /invoice|fund|verify|create/i.test(event.eventType)).length;
        const volume = events.reduce((sum, event) => sum + numericValue(event, ['amount', 'volume', 'value']), 0);

        if (CONTRACT_IDS.length > 0) {
          upsertPoolHistory(db, CONTRACT_IDS[0], { deposited, deployed, utilization, defaultRate });
        }
        upsertAnalyticsSummary(db, { tvl: deposited, volume, defaultRate, activeInvoices });
      }

      // Check if there are more pages
      if (response.records && response.records.length > 0) {
        const lastRecord = response.records[response.records.length - 1];
        cursor = lastRecord.paging_token || cursor;
      }

      await new Promise((resolve) => setTimeout(resolve, POLLING_INTERVAL_MS));
    } catch (error) {
      console.error('[Astera Indexer] Error polling:', error);
      await new Promise((resolve) => setTimeout(resolve, POLLING_INTERVAL_MS * 2));
    }
  }
}

main().catch((err) => {
  console.error('[Astera Indexer] Fatal error:', err);
  process.exit(1);
});
