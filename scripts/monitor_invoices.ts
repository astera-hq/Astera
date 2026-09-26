/**
 * #801: Invoice default keeper monitor.
 *
 * Polls the invoice contract for `Funded` invoices whose grace period has
 * expired and submits `mark_defaulted(id, keeper)` for each, signed by the
 * keeper key configured below. The keeper address must first be whitelisted
 * on-chain via `InvoiceContract.add_keeper(admin, keeper)` — it can only
 * ever call `mark_defaulted`, no other admin function.
 *
 * Designed to be run as a periodic cron job or deployed as a Stellar Turret
 * task; either way it is just "run this script every N minutes".
 *
 * Usage:
 *   INVOICE_CONTRACT_ID=... KEEPER_SECRET_KEY=... npx tsx scripts/monitor_invoices.ts
 *
 * Environment:
 *   RPC_URL              — Soroban RPC endpoint (default: https://soroban-testnet.stellar.org)
 *   NETWORK_PASSPHRASE    — network passphrase (default: Test SDF Network ; September 2015)
 *   INVOICE_CONTRACT_ID   — deployed invoice contract id (required)
 *   KEEPER_SECRET_KEY     — secret key of the whitelisted keeper account (required)
 *   POLL_INTERVAL_MS      — ms between sweeps when run continuously (default: 60000)
 *   RUN_ONCE              — if "true", perform a single sweep and exit (for cron use)
 */

import * as dotenv from 'dotenv';
import {
  Keypair,
  TransactionBuilder,
  BASE_FEE,
  Contract,
  rpc as StellarRpc,
  Address,
  nativeToScVal,
  scValToNative,
} from '@stellar/stellar-sdk';

dotenv.config();

interface Config {
  rpcUrl: string;
  networkPassphrase: string;
  invoiceContractId: string;
  keeperSecretKey: string;
  pollIntervalMs: number;
  runOnce: boolean;
}

function loadConfig(): Config {
  const invoiceContractId = process.env.INVOICE_CONTRACT_ID || '';
  const keeperSecretKey = process.env.KEEPER_SECRET_KEY || '';
  if (!invoiceContractId) throw new Error('INVOICE_CONTRACT_ID is required');
  if (!keeperSecretKey) throw new Error('KEEPER_SECRET_KEY is required');

  return {
    rpcUrl: process.env.RPC_URL || 'https://soroban-testnet.stellar.org',
    networkPassphrase:
      process.env.NETWORK_PASSPHRASE || 'Test SDF Network ; September 2015',
    invoiceContractId,
    keeperSecretKey,
    pollIntervalMs: parseInt(process.env.POLL_INTERVAL_MS || '60000', 10),
    runOnce: process.env.RUN_ONCE === 'true',
  };
}

const INVOICE_STATUS_FUNDED = 'Funded';
const SECS_PER_DAY = 86_400;

class InvoiceMonitor {
  private readonly config: Config;
  private readonly keypair: Keypair;
  private readonly server: StellarRpc.Server;
  private readonly contract: Contract;
  private lastSeenInvoiceCount: number = 0;
  private checkedInvoiceIds: Set<number> = new Set();

  constructor(config: Config) {
    this.config = config;
    this.keypair = Keypair.fromSecret(config.keeperSecretKey);
    this.server = new StellarRpc.Server(config.rpcUrl, { allowHttp: true });
    this.contract = new Contract(config.invoiceContractId);
  }

  async sweep(): Promise<void> {
    const invoiceCount = await this.readInvoiceCount();
    const globalGraceDays = await this.readGraceDays();
    const nowSecs = Math.floor(Date.now() / 1000);

    // Only check invoices beyond what we've already seen, or if count decreased
    const startId = invoiceCount < this.lastSeenInvoiceCount ? 1 : this.lastSeenInvoiceCount + 1;
    this.lastSeenInvoiceCount = invoiceCount;

    for (let id = startId; id <= invoiceCount; id++) {
      // Skip invoices we've already checked unless count was reset
      if (startId > 1 && this.checkedInvoiceIds.has(id)) continue;

      let invoice: any;
      try {
        invoice = await this.readInvoice(id);
      } catch (err) {
        console.error(`[monitor] failed to read invoice ${id}:`, err);
        this.checkedInvoiceIds.add(id);
        continue;
      }

      this.checkedInvoiceIds.add(id);
      if (invoice.status !== INVOICE_STATUS_FUNDED) continue;

      const graceDays: number = invoice.grace_period_override ?? globalGraceDays;
      const defaultAt = Number(invoice.due_date) + graceDays * SECS_PER_DAY;
      if (nowSecs < defaultAt) continue;

      console.log(`[monitor] invoice ${id} past grace period (default_at=${defaultAt}), marking defaulted`);
      try {
        const hash = await this.markDefaulted(id);
        console.log(`[monitor] invoice ${id} mark_defaulted submitted: ${hash}`);
      } catch (err) {
        console.error(`[monitor] mark_defaulted failed for invoice ${id}:`, err);
      }
    }
  }

  async start(): Promise<void> {
    if (this.config.runOnce) {
      await this.sweep();
      return;
    }
    console.log(`[monitor] starting poll loop (every ${this.config.pollIntervalMs}ms)`);
    for (;;) {
      try {
        await this.sweep();
      } catch (err) {
        console.error('[monitor] sweep error:', err);
      }
      await sleep(this.config.pollIntervalMs);
    }
  }

  private async simulateRead(method: string, ...args: ReturnType<typeof nativeToScVal>[]): Promise<any> {
    const account = await this.server.getAccount(this.keypair.publicKey());
    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: this.config.networkPassphrase,
    })
      .addOperation(this.contract.call(method, ...args))
      .setTimeout(30)
      .build();

    const sim = await this.server.simulateTransaction(tx);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`simulate ${method} failed: ${sim.error}`);
    }
    if (!StellarRpc.Api.isSimulationSuccess(sim) || !sim.result) {
      throw new Error(`simulate ${method} returned no result`);
    }
    return scValToNative(sim.result.retval);
  }

  private async readInvoiceCount(): Promise<number> {
    return Number(await this.simulateRead('get_invoice_count'));
  }

  private async readGraceDays(): Promise<number> {
    return Number(await this.simulateRead('get_grace_period'));
  }

  private async readInvoice(id: number): Promise<any> {
    return this.simulateRead('get_invoice', nativeToScVal(id, { type: 'u64' }));
  }

  private async markDefaulted(id: number): Promise<string> {
    const account = await this.server.getAccount(this.keypair.publicKey());
    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: this.config.networkPassphrase,
    })
      .addOperation(
        this.contract.call(
          'mark_defaulted',
          nativeToScVal(id, { type: 'u64' }),
          new Address(this.keypair.publicKey()).toScVal(),
        ),
      )
      .setTimeout(30)
      .build();

    const sim = await this.server.simulateTransaction(tx);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`simulate mark_defaulted failed: ${sim.error}`);
    }
    const prepared = StellarRpc.assembleTransaction(tx, sim).build();
    prepared.sign(this.keypair);
    const send = await this.server.sendTransaction(prepared);
    return send.hash;
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

async function main(): Promise<void> {
  const config = loadConfig();
  const monitor = new InvoiceMonitor(config);
  await monitor.start();
}

if (require.main === module) {
  main().catch((err) => {
    console.error('[monitor] fatal error:', err);
    process.exit(1);
  });
}

export { InvoiceMonitor, loadConfig };
