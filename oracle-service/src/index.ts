import * as http from 'http';
import * as dotenv from 'dotenv';
import { Listener } from './listener';
import { Verifier } from './verifier';
import { OracleConfig } from './types';

dotenv.config();

const config: OracleConfig = {
  rpcUrl: process.env.RPC_URL || 'https://soroban-testnet.stellar.org',
  horizonUrl: process.env.HORIZON_URL || 'https://horizon-testnet.stellar.org',
  networkPassphrase: process.env.NETWORK_PASSPHRASE || 'Test SDF Network ; September 2015',
  oracleSecretKey: process.env.ORACLE_SECRET_KEY || '',
  invoiceContractId: process.env.INVOICE_CONTRACT_ID || '',
  autoVerifyDelayMs: parseInt(process.env.AUTO_VERIFY_DELAY_MS || '30000', 10),
};

async function main() {
  console.log('=== Astera Oracle Integration Service ===');

  if (!config.oracleSecretKey) {
    console.error('Error: ORACLE_SECRET_KEY is not set.');
    process.exit(1);
  }

  // Validate the key format up front so a malformed key fails fast at startup
  // instead of producing a cryptic error at the first signing attempt.
  if (!/^S[A-Z2-7]{55}$/.test(config.oracleSecretKey)) {
    console.error(
      'Error: ORACLE_SECRET_KEY is not a valid Stellar secret key (must start with "S" and be 56 characters).'
    );
    process.exit(1);
  }

  if (!config.invoiceContractId) {
    console.error('Error: INVOICE_CONTRACT_ID is not set.');
    process.exit(1);
  }

  const verifier = new Verifier(config);
  const listener = new Listener(config, verifier);
  const healthPort = parseInt(process.env.HEALTH_PORT || '8080', 10);
  const server = http.createServer((req, res) => {
    if (req.url === '/health') {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ status: 'ok', processed: listener.processedCount }));
      return;
    }

    res.writeHead(404, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ status: 'not_found' }));
  });

  await listener.start();
  server.listen(healthPort, () => {
    console.log(`Health server listening on port ${healthPort}`);
  });

  console.log('Oracle Service is running and listening for events...');

  // Keep alive
  process.on('SIGINT', () => {
    console.log('Shutting down...');
    server.close(() => process.exit(0));
  });

  process.on('SIGTERM', () => {
    console.log('Shutting down...');
    server.close(() => process.exit(0));
  });
}

main().catch(err => {
  console.error('Fatal error:', err);
  process.exit(1);
});
