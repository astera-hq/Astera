/**
 * #867: Compliance / sanctions screening service.
 *
 * Wires MockScreener + Monitor, exposes /health and admin REST
 * (POST /screen, GET /flags) for the frontend compliance dashboard.
 */

import * as http from 'http';
import * as crypto from 'crypto';
import * as dotenv from 'dotenv';
import { Keypair, TransactionBuilder, BASE_FEE, Contract, rpc as StellarRpc, Address, nativeToScVal, xdr } from '@stellar/stellar-sdk';
import { MockScreener, HttpScreenerProvider, ScreenerProvider } from './screener';
import { Monitor } from './monitor';
import type { ComplianceConfig, RiskTier, ScreenDecision } from './types';

dotenv.config();

const config: ComplianceConfig = {
  rpcUrl: process.env.RPC_URL || 'https://soroban-testnet.stellar.org',
  horizonUrl: process.env.HORIZON_URL || 'https://horizon-testnet.stellar.org',
  networkPassphrase: process.env.NETWORK_PASSPHRASE || 'Test SDF Network ; September 2015',
  screenerSecretKey: process.env.COMPLIANCE_SCREENER_SECRET_KEY || process.env.ORACLE_SECRET_KEY || '',
  complianceContractId: process.env.COMPLIANCE_CONTRACT_ID || '',
  poolContractId: process.env.POOL_CONTRACT_ID || '',
  invoiceContractId: process.env.INVOICE_CONTRACT_ID || '',
  healthPort: parseInt(process.env.HEALTH_PORT || '8081', 10),
  adminToken: process.env.COMPLIANCE_ADMIN_TOKEN || '',
  structuringThreshold: BigInt(process.env.STRUCTURING_THRESHOLD || '10000000000'),
  structuringWindowMs: parseInt(process.env.STRUCTURING_WINDOW_MS || '3600000', 10),
  structuringMaxCount: parseInt(process.env.STRUCTURING_MAX_COUNT || '5', 10),
  screeningProviderUrl: process.env.SCREENING_PROVIDER_URL || '',
  screeningProviderApiKey: process.env.SCREENING_PROVIDER_API_KEY || '',
  screeningProviderTimeoutMs: parseInt(process.env.SCREENING_PROVIDER_TIMEOUT_MS || '5000', 10),
  rescreenCheckIntervalMs: parseInt(process.env.RESCREEN_CHECK_INTERVAL_MS || String(60 * 60 * 1000), 10),
};

const screener: ScreenerProvider = config.screeningProviderUrl
  ? new HttpScreenerProvider(
      config.screeningProviderUrl,
      config.screeningProviderTimeoutMs,
      config.screeningProviderApiKey || undefined,
    )
  : new MockScreener();
let monitor: Monitor | null = null;

function requireAdmin(req: http.IncomingMessage): boolean {
  if (!config.adminToken) return true; // open in local/dev when unset
  const header = req.headers['x-admin-token'] || req.headers['authorization'];
  if (!header) return false;
  const value = Array.isArray(header) ? header[0] : header;

  const tokenBuffer = Buffer.from(config.adminToken);
  const valueBuffer = Buffer.from(value);

  if (valueBuffer.length === tokenBuffer.length) {
    try {
      if (crypto.timingSafeEqual(valueBuffer, tokenBuffer)) return true;
    } catch {
      // Buffer length mismatch or other error
    }
  }

  if (value.startsWith('Bearer ')) {
    const bearerToken = value.slice(7);
    const bearerBuffer = Buffer.from(bearerToken);
    const tokenBuffer2 = Buffer.from(config.adminToken);
    if (bearerBuffer.length === tokenBuffer2.length) {
      try {
        if (crypto.timingSafeEqual(bearerBuffer, tokenBuffer2)) return true;
      } catch {
        // Buffer length mismatch or other error
      }
    }
  }

  return false;
}

async function submitOnChain(
  address: string,
  status: ScreenDecision,
  reasonCode: number,
  riskTier: RiskTier,
  notesHash: string,
): Promise<string> {
  if (!config.screenerSecretKey || !config.complianceContractId) {
    throw new Error('COMPLIANCE_SCREENER_SECRET_KEY and COMPLIANCE_CONTRACT_ID required');
  }
  const keypair = Keypair.fromSecret(config.screenerSecretKey);
  const server = new StellarRpc.Server(config.rpcUrl, { allowHttp: true });
  const account = await server.getAccount(keypair.publicKey());
  const contract = new Contract(config.complianceContractId);

  const statusScVal = xdr.ScVal.scvVec([nativeToScVal(status, { type: 'symbol' })]);
  const riskScVal = xdr.ScVal.scvVec([nativeToScVal(riskTier, { type: 'symbol' })]);
  const expiresAt = Math.floor(Date.now() / 1000) + 180 * 24 * 60 * 60;

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: config.networkPassphrase,
  })
    .addOperation(
      contract.call(
        'submit_screening_result',
        new Address(keypair.publicKey()).toScVal(),
        new Address(address).toScVal(),
        statusScVal,
        nativeToScVal(reasonCode, { type: 'u32' }),
        riskScVal,
        nativeToScVal(expiresAt, { type: 'u64' }),
        nativeToScVal(notesHash.slice(0, 64), { type: 'string' }),
      ),
    )
    .setTimeout(30)
    .build();

  const sim = await server.simulateTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed: ${sim.error}`);
  }
  const prepared = StellarRpc.assembleTransaction(tx, sim).build();
  prepared.sign(keypair);
  const send = await server.sendTransaction(prepared);
  return send.hash;
}

/** #982: re-runs the screener for a subject due for periodic re-screening and submits on-chain. */
async function runRescreen(address: string): Promise<void> {
  const result = await screener.screen(address);
  const hash = await submitOnChain(address, result.status, result.reasonCode, result.riskTier, result.notes);
  console.log(`[rescreen] ${address} -> ${result.status} (tx ${hash})`);
}

function readBody(req: http.IncomingMessage): Promise<string> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    req.on('data', (c) => chunks.push(Buffer.from(c)));
    req.on('end', () => resolve(Buffer.concat(chunks).toString('utf8')));
    req.on('error', reject);
  });
}

async function main() {
  console.log('=== Astera Compliance Service (#867) ===');

  if (config.screenerSecretKey && !/^S[A-Z2-7]{55}$/.test(config.screenerSecretKey)) {
    console.error('Error: COMPLIANCE_SCREENER_SECRET_KEY is not a valid Stellar secret key');
    process.exit(1);
  }

  if (config.screenerSecretKey && config.complianceContractId) {
    monitor = new Monitor(config);
    monitor.setRescreenHandler(runRescreen);
    await monitor.start();
  } else {
    console.warn(
      'COMPLIANCE_SCREENER_SECRET_KEY or COMPLIANCE_CONTRACT_ID unset — monitor disabled; REST screening still available offline',
    );
  }

  const server = http.createServer(async (req, res) => {
    const url = new URL(req.url || '/', `http://127.0.0.1`);
    const path = url.pathname;

    const json = (code: number, body: unknown) => {
      res.writeHead(code, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify(body));
    };

    try {
      if (req.method === 'GET' && path === '/health') {
        return json(200, {
          status: 'ok',
          processed: monitor?.processedCount ?? 0,
          complianceContractId: config.complianceContractId || null,
          monitor: Boolean(monitor),
        });
      }

      if (req.method === 'GET' && path === '/flags') {
        if (!requireAdmin(req)) return json(401, { error: 'unauthorized' });
        return json(200, { alerts: monitor?.listAlerts() ?? [] });
      }

      if (req.method === 'POST' && path === '/screen') {
        if (!requireAdmin(req)) return json(401, { error: 'unauthorized' });
        const raw = await readBody(req);
        const body = JSON.parse(raw || '{}') as {
          address?: string;
          name?: string;
          jurisdiction?: string;
          submitOnChain?: boolean;
          status?: ScreenDecision;
          reasonCode?: number;
          riskTier?: RiskTier;
          notesHash?: string;
        };
        if (!body.address) return json(400, { error: 'address required' });

        // If operator supplies an explicit decision, use it; else run mock screener.
        let result = await screener.screen(body.address, {
          name: body.name,
          jurisdiction: body.jurisdiction,
        });
        if (body.status) {
          result = {
            ...result,
            status: body.status,
            reasonCode: body.reasonCode ?? result.reasonCode,
            riskTier: body.riskTier ?? result.riskTier,
            notes: body.notesHash ?? result.notes,
          };
        }

        let txHash: string | undefined;
        if (body.submitOnChain) {
          txHash = await submitOnChain(
            body.address,
            result.status,
            result.reasonCode,
            result.riskTier,
            body.notesHash || result.notes,
          );
          monitor?.trackSubject(body.address);
        }

        return json(200, { result, txHash });
      }

      if (req.method === 'POST' && path === '/monitor/deposit') {
        if (!requireAdmin(req)) return json(401, { error: 'unauthorized' });
        const raw = await readBody(req);
        const body = JSON.parse(raw || '{}') as { address?: string; amount?: string };
        if (!body.address || body.amount === undefined) {
          return json(400, { error: 'address and amount required' });
        }
        await monitor?.recordDeposit(body.address, BigInt(body.amount));
        return json(200, { ok: true });
      }

      return json(404, { status: 'not_found' });
    } catch (err) {
      console.error('[api] error:', err);
      return json(500, { error: err instanceof Error ? err.message : 'internal error' });
    }
  });

  server.listen(config.healthPort, () => {
    console.log(`Compliance service listening on port ${config.healthPort}`);
  });

  const shutdown = () => {
    console.log('Shutting down...');
    monitor?.stop();
    server.close(() => process.exit(0));
  };
  process.on('SIGINT', shutdown);
  process.on('SIGTERM', shutdown);
}

main().catch((err) => {
  console.error('Fatal error:', err);
  process.exit(1);
});
