/**
 * #867: Horizon event monitor — watches pool/invoice activity for suspicious
 * patterns and calls compliance.request_review on-chain when triggered.
 */

import * as fs from 'fs';
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
    if (this.trackedSubjects.has(address)) return;
    this.trackedSubjects.add(address);
    this.saveTrackedSubjects();
  }

  /** Addresses currently covered by periodic re-screening. */
  listTrackedSubjects(): string[] {
    return [...this.trackedSubjects];
  }

  /**
   * #1553: rebuilds the re-screening set after a restart from the persisted
   * file plus this screener's on-chain submission history, so coverage for
   * previously screened addresses is not silently dropped.
   */
  async restoreTrackedSubjects(): Promise<void> {
    const fromDisk = this.loadTrackedSubjects();
    for (const address of fromDisk) this.trackedSubjects.add(address);

    let fromChain = 0;
    if (this.config.complianceContractId) {
      try {
        const submissions = (await this.readContract('get_screener_submissions', [
          new Address(this.keypair.publicKey()).toScVal(),
        ])) as Array<Record<string, unknown>> | null;
        for (const entry of submissions ?? []) {
          const address = String(entry.subject_address ?? '');
          if (address && !this.trackedSubjects.has(address)) {
            this.trackedSubjects.add(address);
            fromChain += 1;
          }
        }
      } catch (err) {
        console.error('[monitor] could not reseed tracked subjects from on-chain submissions:', err);
      }
    }

    if (fromChain > 0) this.saveTrackedSubjects();
    console.log(
      `[monitor] tracking ${this.trackedSubjects.size} subject(s) for re-screening ` +
        `(${fromDisk.length} from ${this.config.trackedSubjectsPath}, ${fromChain} reseeded from chain)`,
    );
  }

  private loadTrackedSubjects(): string[] {
    let raw: string;
    try {
      raw = fs.readFileSync(this.config.trackedSubjectsPath, 'utf-8');
    } catch {
      // No file yet (first run, or it was deleted) — start empty.
      return [];
    }
    try {
      const parsed: unknown = JSON.parse(raw);
      if (Array.isArray(parsed)) return parsed.filter((a): a is string => typeof a === 'string' && a !== '');
      console.error(`[monitor] ignoring malformed tracked subjects file ${this.config.trackedSubjectsPath}`);
    } catch (err) {
      console.error(`[monitor] ignoring unreadable tracked subjects file ${this.config.trackedSubjectsPath}:`, err);
    }
    return [];
  }

  private saveTrackedSubjects(): void {
    try {
      fs.writeFileSync(this.config.trackedSubjectsPath, JSON.stringify([...this.trackedSubjects]), 'utf-8');
    } catch (err) {
      console.error('[monitor] failed to persist tracked subjects:', err);
    }
  }

  /** Install the callback invoked when a tracked subject is due for re-screening. */
  setRescreenHandler(handler: RescreenHandler): void {
    this.onRescreenDue = handler;
  }

  listAlerts(): MonitorAlert[] {
    return [...this.alerts].slice(-100);
  }

  async start(): Promise<void> {
    await this.restoreTrackedSubjects();
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

    // #1552: bound the request so a slow or hung Horizon endpoint cannot
    // stall the poll loop indefinitely (mirrors HttpScreenerProvider.screen).
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.config.horizonTimeoutMs);
    let body: { _embedded?: { records?: Array<Record<string, unknown>> } };
    try {
      const res = await fetch(url.toString(), { signal: controller.signal });
      if (!res.ok) return;
      body = (await res.json()) as typeof body;
    } catch (err) {
      if (err instanceof Error && err.name === 'AbortError') {
        throw new Error(`Horizon poll timed out after ${this.config.horizonTimeoutMs}ms`);
      }
      throw err;
    } finally {
      clearTimeout(timer);
    }
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

    const functionName = String(rec.function ?? '').toLowerCase();
    const sourceAccount = String(rec.source_account ?? '');

    // Parse deposit/withdraw from pool contract operations
    // Amount is extracted from operation parameters when available
    if (!sourceAccount) return;

    const contractId = String(rec.contract_id ?? rec.address ?? '');
    if (contractId && this.config.poolContractId && contractId === this.config.poolContractId) {
      // Extract amount from parameters if present
      let amount = 0n;
      if (rec.parameters) {
        const params = Array.isArray(rec.parameters) ? rec.parameters : [rec.parameters];
        if (params.length >= 2) {
          try {
            amount = BigInt(String(params[1] ?? '0'));
          } catch {
            // Invalid bigint; use 0
          }
        }
      }

      if (functionName.includes('deposit') && amount > 0n) {
        await this.recordDeposit(sourceAccount, amount);
      } else if (functionName.includes('withdraw')) {
        // For withdrawals, we record with the amount if available, otherwise 0n
        // The rapid-cycle check only needs timing, not amount
        await this.recordWithdraw(sourceAccount, amount);
      }
    } else if (functionName.includes('withdraw') && !contractId) {
      // Fallback: if no contract ID, use the basic withdrawal heuristic
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
