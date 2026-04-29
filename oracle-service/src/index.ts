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

  if (!config.invoiceContractId) {
    console.error('Error: INVOICE_CONTRACT_ID is not set.');
    process.exit(1);
  }

  const verifier = new Verifier(config);
  const listener = new Listener(config, verifier);

  await listener.start();

  console.log('Oracle Service is running and listening for events...');

  // Keep alive
  process.on('SIGINT', () => {
    console.log('Shutting down...');
    process.exit(0);
  });
}

main().catch(err => {
  console.error('Fatal error:', err);
  process.exit(1);
});
