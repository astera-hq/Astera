#!/usr/bin/env node
import { Pool } from 'pg';
import { replay } from './replay';
import { computePredictiveQuality } from './metrics';
import { renderReport } from './report';

function parseArg(name: string): string | undefined {
  const idx = process.argv.indexOf(`--${name}`);
  return idx !== -1 ? process.argv[idx + 1] : undefined;
}

async function main() {
  const connectionString = parseArg('database-url') || process.env.DATABASE_URL;
  if (!connectionString) {
    console.error(
      'Usage: npm run backtest -- --database-url <postgres-connection-string>\n' +
        '(or set DATABASE_URL). The connection string points at the indexer\'s Postgres database.',
    );
    process.exit(1);
  }

  const pool = new Pool({ connectionString });
  try {
    const trajectories = await replay(pool);
    const quality = computePredictiveQuality(trajectories);
    console.log(renderReport(trajectories, quality));
  } finally {
    await pool.end();
  }
}

main();
