/**
 * #867: Horizon event monitor — watches pool/invoice activity for suspicious
 * patterns and calls compliance.request_review on-chain when triggered.
 */

import { Keypair, TransactionBuilder, BASE_FEE, Contract, rpc as StellarRpc, Address, nativeToScVal, scValToNative } from '@stellar/stellar-sdk';
import type { ComplianceConfig, MonitorAlert } from './types';

interface DepositTick {
  at: number;
  amount: bigint;
}

/** #982: called when a tracked subject's clearance has reached its rescreening interval. */
export type RescreenHandler = (address: string) => Promise<void>;

/** Maximum number of alerts retained in memory. */
const MAX_ALERTS = 1_000;

export class Monitor {
  private readonly config: ComplianceConfig;
  private readonly keypair: Keypair;
  private readonly server: StellarRpc.Server;
  private readonly deposits = new Map<string, DepositTick[]>();
  private readonly alerts: MonitorAlert[] = [];
  private readonly trackedSubjects = new Set<string>();
  private cursor = 'now';
  private running = false;
  processedCount = 0;
  private onRescreenDue?: RescreenHandler;

  constructor(config: ComplianceConfig) {
    this.config = config;
    this.keypair = Keypair.fromSecret(config.screenerSecretKey);
    this.server = new StellarRpc.Server(config.rpcUrl, { allowHttp: true });
  }

  /** Register an address whose clearance should be periodically re-checked. */
  trackSubject(address: string): void {
    this.trackedSubjects.add(address);
  }

  /** Install the callback invoked when a tracked subject is due for re-screening. */
  setRescreenHandler(handler: RescreenHandler): void {
    this.onRescreenDue = handler;
  }

  listAlerts(): MonitorAlert[] {
    return [...this.alerts].slice(-100);
  }

  async start(): Promise<void> {
    this.running = true;
    console.log('[monitor] starting Horizon poll loop');
    void this.pollLoop();
    void this.rescreenLoop();
  }

  stop(): void {
    this.running = false;
  }

  private async pollLoop(): Promise<void> {
    while (this.running) {
      try {
        await this.pollOnce();
      } catch (err) {
        console.error('[monitor] poll error:', err);
      }
      await sleep(5000);
    }
  }

  private async pollOnce(): Promise<void> {
    const url = new URL(`${this.config.horizonUrl}/operations`);
    url.searchParams.set('order', 'asc');
    url.searchParams.set('limit', '50');
    url.searchParams.set('cursor', this.cursor);
    url.searchParams.set('include_failed', 'false');

    const res = await fetch(url.toString());
    if (!res.ok) return;
    const body = (await res.json()) as {
      _embedded?: { records?: Array<Record<string, unknown>> };
    };
    const records = body._embedded?.records ?? [];
    for (const rec of records) {
      this.cursor = String(rec.paging_token ?? this.cursor);
      await this.handleRecord(rec);
      this.processedCount += 1;
    }
  }

  /** #982: periodically re-screens tracked subjects once their on-chain clearance expires. */
  private async rescreenLoop(): Promise<void> {
    while (this.running) {
      await sleep(this.config.rescreenCheckIntervalMs);
      try {
        await this.rescreenOnce();
      } catch (err) {
        console.error('[monitor] rescreen check error:', err);
      }
    }
  }

  private async rescreenOnce(): Promise<void> {
    if (this.trackedSubjects.size === 0 || !this.onRescreenDue || !this.config.complianceContractId) return;

    const intervalSecs = Number(await this.readContract('get_rescreening_interval', []));
    const now = Math.floor(Date.now() / 1000);

    for (const address of this.trackedSubjects) {
      try {
        const record = (await this.readContract('get_compliance_record', [
          new Address(address).toScVal(),
        ])) as Record<string, unknown> | null;
        if (!record) continue;

        const expiresAt = Number(record.expires_at ?? 0);
        const screenedAt = Number(record.screened_at ?? 0);
        const dueAt = expiresAt !== 0 ? expiresAt : screenedAt + intervalSecs;
        if (now < dueAt) continue;

        console.log(`[monitor] rescreening ${address} (due at ${dueAt}, now ${now})`);
        await this.onRescreenDue(address);
      } catch (err) {
        console.error(`[monitor] rescreen failed for ${address}:`, err);
      }
    }
  }

  /** Read-only contract call via simulation (no signing/submission needed). */
  private async readContract(method: string, args: ReturnType<typeof nativeToScVal>[]): Promise<unknown> {
    const account = await this.server.getAccount(this.keypair.publicKey());
    const contract = new Contract(this.config.complianceContractId);
    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: this.config.networkPassphrase,
    })
      .addOperation(contract.call(method, ...args))
      .setTimeout(30)
      .build();

    const sim = await this.server.simulateTransaction(tx);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`simulate ${method} failed: ${sim.error}`);
    }
    return scValToNative(sim.result!.retval);
  }

  private async handleRecord(rec: Record<string, unknown>): Promise<void> {
    const type = String(rec.type ?? '');
    if (type !== 'invoke_host_function') return;

    const functionName = String(rec.function ?? '');
    const sourceAccount = String(rec.source_account ?? '');

    // Best-effort heuristic: if the function name contains "withdraw", treat
    // this as a withdrawal event.  Amount is not reliably available from the
    // Horizon operation summary alone (would require full event decoding), so
    // we pass 0 — the rapid-cycle check only needs the *timing* of the
    // withdrawal, not its size.
    if (functionName.toLowerCase().includes('withdraw') && sourceAccount) {
      await this.recordWithdraw(sourceAccount, 0n);
    }
  }

  /** Called by REST or internal hooks when a deposit is observed. */
  async recordDeposit(address: string, amount: bigint): Promise<void> {
    const now = Date.now();
    const window = this.config.structuringWindowMs;
    const ticks = (this.deposits.get(address) ?? []).filter((t) => now - t.at <= window);
    ticks.push({ at: now, amount });
    this.deposits.set(address, ticks);

    const nearThreshold = ticks.filter(
      (t) => t.amount > 0n && t.amount < this.config.structuringThreshold,
    );
    if (nearThreshold.length >= this.config.structuringMaxCount) {
      await this.flag(
        address,
        'structuring',
        `structuring: ${nearThreshold.length} sub-threshold deposits in window`,
      );
      this.deposits.set(address, []);
    }

    // Rapid deposit-then-withdraw heuristic is handled when recordWithdraw is called.
  }

  async recordWithdraw(address: string, _amount: bigint): Promise<void> {
    const ticks = this.deposits.get(address) ?? [];
    const now = Date.now();
    const recent = ticks.filter((t) => now - t.at < 60_000);
    if (recent.length > 0) {
      await this.flag(
        address,
        'rapid_cycle',
        'rapid deposit-then-withdraw within 60s',
      );
    }
  }

  async flag(address: string, pattern: string, reason: string): Promise<void> {
    const alert: MonitorAlert = {
      id: `${Date.now()}-${address.slice(0, 8)}`,
      address,
      reason,
      at: new Date().toISOString(),
      pattern,
    };
    this.alerts.push(alert);
    if (this.alerts.length > MAX_ALERTS) {
      this.alerts.splice(0, this.alerts.length - MAX_ALERTS);
    }
    console.log(`[monitor] alert ${pattern} for ${address}: ${reason}`);

    try {
      await this.requestReviewOnChain(address, reason);
    } catch (err) {
      console.error('[monitor] request_review failed:', err);
    }
  }

  private async requestReviewOnChain(address: string, reason: string): Promise<void> {
    if (!this.config.complianceContractId) return;

    const account = await this.server.getAccount(this.keypair.publicKey());
    const contract = new Contract(this.config.complianceContractId);
    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: this.config.networkPassphrase,
    })
      .addOperation(
        contract.call(
          'request_review',
          new Address(this.keypair.publicKey()).toScVal(),
          new Address(address).toScVal(),
          nativeToScVal(reason.slice(0, 200), { type: 'string' }),
        ),
      )
      .setTimeout(30)
      .build();

    const sim = await this.server.simulateTransaction(tx);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`simulate request_review failed: ${sim.error}`);
    }
    const prepared = StellarRpc.assembleTransaction(tx, sim).build();
    prepared.sign(this.keypair);
    const send = await this.server.sendTransaction(prepared);
    console.log(`[monitor] request_review submitted: ${send.hash}`);
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}
