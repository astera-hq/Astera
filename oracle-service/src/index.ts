import * as http from 'http';
import * as dotenv from 'dotenv';
import { Keypair } from '@stellar/stellar-sdk';
import { AsteraClient } from 'astera-sdk';
import { Listener } from './listener';
import { Verifier } from './verifier';
import { ConsensusTracker } from './consensus';
import { checkStakeOnStartup, registerIfRequested, getStakingMetrics, StakingMetricsTracker } from './staking';
import { OracleConfig } from './types';

dotenv.config();

const config: OracleConfig = {
  rpcUrl: process.env.RPC_URL || 'https://soroban-testnet.stellar.org',
  horizonUrl: process.env.HORIZON_URL || 'https://horizon-testnet.stellar.org',
  networkPassphrase: process.env.NETWORK_PASSPHRASE || 'Test SDF Network ; September 2015',
  oracleSecretKey: process.env.ORACLE_SECRET_KEY || '',
  invoiceContractId: process.env.INVOICE_CONTRACT_ID || '',
  autoVerifyDelayMs: parseInt(process.env.AUTO_VERIFY_DELAY_MS || '30000', 10),
  // #861: N-of-M staked oracle consensus network — optional. When unset, this
  // node runs the pre-#861 single-oracle `verify_invoice` flow unmodified.
  oracleRegistryContractId: process.env.ORACLE_REGISTRY_CONTRACT_ID || undefined,
  stakeTokenId: process.env.STAKE_TOKEN_ID || undefined,
  registerStakeAmount: process.env.REGISTER_STAKE_AMOUNT
    ? BigInt(process.env.REGISTER_STAKE_AMOUNT)
    : undefined,
  listenerCursorPath: process.env.LISTENER_CURSOR_PATH || '.oracle-listener-cursor',
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

  const oracleKeypair = Keypair.fromSecret(config.oracleSecretKey);

  // #861: `npm start -- --register` performs the one-time stake registration
  // and exits, rather than running the listener.
  if (process.argv.includes('--register')) {
    const registryClient = new AsteraClient({
      rpcUrl: config.rpcUrl,
      network: config.networkPassphrase,
      invoiceContractId: config.invoiceContractId,
      poolContractId: '',
      oracleRegistryContractId: config.oracleRegistryContractId,
    });
    await registerIfRequested(config, registryClient, oracleKeypair);
    process.exit(0);
  }

  let registryClient: AsteraClient | undefined;
  if (config.oracleRegistryContractId) {
    console.log(`Running as a consensus-network oracle node (registry: ${config.oracleRegistryContractId})`);
    registryClient = new AsteraClient({
      rpcUrl: config.rpcUrl,
      network: config.networkPassphrase,
      invoiceContractId: config.invoiceContractId,
      poolContractId: '',
      oracleRegistryContractId: config.oracleRegistryContractId,
    });
    await checkStakeOnStartup(config, registryClient, oracleKeypair.publicKey());
  } else {
    console.log('Running in legacy single-oracle mode (ORACLE_REGISTRY_CONTRACT_ID not set)');
  }

  const consensusTracker = config.oracleRegistryContractId
    ? new ConsensusTracker(oracleKeypair.publicKey())
    : undefined;
  // #979: tracks this node's own slash events so `/metrics` can report them
  // without querying the chain directly.
  const stakingTracker = config.oracleRegistryContractId
    ? new StakingMetricsTracker(oracleKeypair.publicKey())
    : undefined;
  const verifier = new Verifier(config, consensusTracker);
  const listener = new Listener(config, verifier, consensusTracker, stakingTracker);
  const healthPort = parseInt(process.env.HEALTH_PORT || '8080', 10);
  const server = http.createServer((req, res) => {
    if (req.url === '/health') {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(
        JSON.stringify({
          status: 'ok',
          processed: listener.processedCount,
          mode: config.oracleRegistryContractId ? 'consensus' : 'legacy',
          rounds: consensusTracker?.list() ?? undefined,
        }),
      );
      return;
    }

    // #979: on-demand stake/cooldown/slash status for this operator, so they
    // don't have to query the chain directly to check on their own node.
    if (req.url === '/metrics') {
      if (!registryClient || !stakingTracker) {
        res.writeHead(404, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ status: 'not_found', reason: 'ORACLE_REGISTRY_CONTRACT_ID not set' }));
        return;
      }
      getStakingMetrics(registryClient, oracleKeypair.publicKey(), stakingTracker)
        .then((metrics) => {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify(metrics));
        })
        .catch((error) => {
          res.writeHead(502, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ status: 'error', error: String(error) }));
        });
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
