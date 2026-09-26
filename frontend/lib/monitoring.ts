import * as fs from 'fs';
import * as path from 'path';
import {
  rpcGetEvents,
  rpcGetLatestLedger,
  INVOICE_CONTRACT_ID,
  POOL_CONTRACT_ID,
  scValToNative,
} from './stellar';
import { getInvoiceCount, getMultipleInvoices } from './contracts';
import { notificationService } from './notifications';
import {
  LARGE_TX_THRESHOLD,
  ACTIVITY_THRESHOLD_COUNT,
  ACTIVITY_WINDOW_SECONDS,
} from './alert-rules';
import type { Invoice, InvoiceStatus, InvoiceTtlWarning } from './types';

/** Contract Event Interface */
export interface ContractEvent {
  id: string;
  contractId: string;
  topic: string[];
  value: any;
  ledger: number;
  ledgerCloseAt: string;
  txHash: string;
}

/** Tracking state for unusual activity */
interface ActivityTracker {
  [address: string]: {
    [type: string]: number[]; // timestamps
  };
}

/** Persisted checkpoint state */
interface CheckpointState {
  lastLedger: number;
  timestamp: number;
}

class ContractMonitor {
  private static instance: ContractMonitor;
  private lastLedger: number = 0;
  private activityHistory: ActivityTracker = {};
  private checkpointPath: string;
  private lastPruneTime: number = Date.now();
  private static readonly LEDGER_SECONDS = 5;
  private static readonly WARNING_WINDOW_DAYS = 30;
  private static readonly ACTIVE_TTL_DAYS = 365;
  private static readonly TERMINAL_TTL_DAYS = 30;
  private static readonly MAX_ACTIVITY_ENTRIES_PER_TYPE = 100;
  private static readonly ACTIVITY_HISTORY_PRUNE_INTERVAL_MS = 60_000;

  private constructor() {
    this.checkpointPath = path.join(process.cwd(), '.monitor-checkpoint.json');
    this.loadCheckpoint();
  }

  /** Load persisted checkpoint from disk */
  private loadCheckpoint(): void {
    try {
      if (fs.existsSync(this.checkpointPath)) {
        const data = fs.readFileSync(this.checkpointPath, 'utf-8');
        const checkpoint: CheckpointState = JSON.parse(data);
        this.lastLedger = checkpoint.lastLedger || 0;
        console.log(`[Astera Monitor] Loaded checkpoint: lastLedger=${this.lastLedger}`);
      }
    } catch (error) {
      console.warn('[Astera Monitor] Failed to load checkpoint:', error);
      this.lastLedger = 0;
    }
  }

  /** Persist checkpoint to disk */
  private saveCheckpoint(): void {
    try {
      const checkpoint: CheckpointState = {
        lastLedger: this.lastLedger,
        timestamp: Date.now(),
      };
      fs.writeFileSync(this.checkpointPath, JSON.stringify(checkpoint, null, 2), 'utf-8');
    } catch (error) {
      console.error('[Astera Monitor] Failed to save checkpoint:', error);
    }
  }

  /** Prune activity history to prevent unbounded growth */
  private pruneActivityHistory(): void {
    const now = Math.floor(Date.now() / 1000);
    for (const address in this.activityHistory) {
      for (const type in this.activityHistory[address]) {
        // Filter out old entries and cap the array
        this.activityHistory[address][type] = this.activityHistory[address][type]
          .filter((ts) => now - ts < ACTIVITY_WINDOW_SECONDS)
          .slice(-ContractMonitor.MAX_ACTIVITY_ENTRIES_PER_TYPE);

        // Clean up empty entries
        if (this.activityHistory[address][type].length === 0) {
          delete this.activityHistory[address][type];
        }
      }
      if (Object.keys(this.activityHistory[address]).length === 0) {
        delete this.activityHistory[address];
      }
    }
  }

  public static getInstance(): ContractMonitor {
    if (!ContractMonitor.instance) {
      ContractMonitor.instance = new ContractMonitor();
    }
    return ContractMonitor.instance;
  }

  /** Fetch and process events for Invoice and Pool contracts */
  public async pollEvents(): Promise<ContractEvent[]> {
    if (!INVOICE_CONTRACT_ID || !POOL_CONTRACT_ID) {
      console.warn('[Astera Monitor] Contract IDs not configured. Skipping poll.');
      return [];
    }

    try {
      // Periodically prune activity history to prevent unbounded growth
      if (Date.now() - this.lastPruneTime > ContractMonitor.ACTIVITY_HISTORY_PRUNE_INTERVAL_MS) {
        this.pruneActivityHistory();
        this.lastPruneTime = Date.now();
      }

      // 1. Fetch current latest ledger
      const latestLedger = await rpcGetLatestLedger();
      const startLedger = this.lastLedger || latestLedger.sequence - 100; // Look back 100 ledgers if no checkpoint
      const endLedger = latestLedger.sequence;

      console.log(`[Astera Monitor] Polling ledgers ${startLedger} to ${endLedger}`);

      // 2. Query Events for both contracts
      const response = await rpcGetEvents({
        startLedger,
        filters: [{ contractIds: [INVOICE_CONTRACT_ID, POOL_CONTRACT_ID] }],
      });

      const events: ContractEvent[] = response.events.map((e: any) => ({
        id: e.id,
        contractId: e.contractId,
        topic: e.topic.map((t: any) => scValToNative(t)),
        value: scValToNative(e.value),
        ledger: e.ledger,
        ledgerCloseAt: e.ledgerCloseAt,
        txHash: e.txHash ?? '',
      }));

      // 3. Process each event for alerts
      for (const event of events) {
        await this.processEvent(event);
      }

      // 4. Update and persist ledger checkpoint
      this.lastLedger = endLedger + 1;
      this.saveCheckpoint();

      return events;
    } catch (error) {
      console.error('[Astera Monitor] Failed to poll events:', error);
      return [];
    }
  }

  public async getInvoiceTtlWarnings(): Promise<InvoiceTtlWarning[]> {
    if (!INVOICE_CONTRACT_ID) return [];

    try {
      const latestLedger = await rpcGetLatestLedger();
      const count = await getInvoiceCount();
      if (count <= 0) return [];

      const ids = Array.from({ length: count }, (_, index) => index + 1);
      const invoices = await getMultipleInvoices(ids);
      const warnings = invoices
        .map((invoice) => this.buildTtlWarning(invoice, latestLedger.sequence))
        .filter((warning): warning is InvoiceTtlWarning => warning !== null)
        .sort((a, b) => a.expiryLedger - b.expiryLedger);

      return warnings;
    } catch (error) {
      console.error('[Astera Monitor] Failed to load TTL warnings:', error);
      return [];
    }
  }

  private async processEvent(event: ContractEvent): Promise<void> {
    const contractType = event.topic[0] ?? '';
    const eventType = event.topic[1] ?? '';
    const value = event.value;

    // A. Check for Large Transactions
    // value could be [id, owner, amount] for 'created'
    // or [investor, amount] for 'deposit'/'withdraw'
    // or [id, principal, interest] for 'repaid'
    let amount: bigint = 0n;
    let sourceAddress: string = '';

    if (contractType === 'invoice') {
      if (eventType === 'created') {
        // schema: (id, owner, amount, metadata_uri, timestamp)
        const [id, owner, amt] = value;
        amount = BigInt(amt);
        sourceAddress = owner;
      } else if (eventType === 'default') {
        // schema: (id, timestamp)
        const [id] = Array.isArray(value) ? value : [value];
        await notificationService.send({
          id: `alert-default-${event.id}`,
          type: 'CONTRACT_DEFAULT',
          priority: 'CRITICAL',
          message: `Invoice #${id} has been marked as DEFAULTED.`,
          timestamp: Date.now(),
          data: { invoiceId: id, txHash: event.txHash },
        });
      }
    } else if (contractType === 'pool') {
      if (eventType === 'deposit' || eventType === 'withdraw') {
        // schema: (investor, amount, shares, timestamp)
        const [investor, amt] = value;
        amount = BigInt(amt);
        sourceAddress = investor;
      } else if (eventType === 'funded') {
        // schema: (invoice_id, sme, principal, token, timestamp)
        const [id, sme, principal] = value;
        amount = BigInt(principal);
        sourceAddress = sme;
      } else if (eventType === 'repaid') {
        // schema: (invoice_id, principal, interest, timestamp)
        const [id, principal] = value;
        amount = BigInt(principal);
      } else if (eventType === 'util_warn') {
        // schema: (token, utilization_bps)
        const [token, utilizationBps] = value;
        const pct = Math.round(Number(utilizationBps) / 100);
        await notificationService.send({
          id: `alert-util-${event.id}`,
          type: 'UNUSUAL_ACTIVITY',
          priority: pct >= 90 ? 'HIGH' : 'MEDIUM',
          message: `Pool utilization alert: ${pct}% for token ${String(token).slice(0, 8)}…`,
          timestamp: Date.now(),
          data: { token, utilizationBps, txHash: event.txHash ?? '' },
        });
      }
    }

    // Evaluate Large Transaction Rule
    // Threshold is human unit (e.g. 5000 USDC), on-chain is 7 decimals
    const humanAmount = Number(amount) / 10_000_000;
    if (humanAmount >= LARGE_TX_THRESHOLD) {
      await notificationService.send({
        id: `alert-large-${event.id}`,
        type: 'LARGE_TRANSACTION',
        priority: 'HIGH',
        message: `Large ${eventType} detected: ${humanAmount.toLocaleString()} units.`,
        timestamp: Date.now(),
        data: {
          amount: humanAmount,
          type: eventType,
          source: sourceAddress,
          txHash: event.txHash ?? '',
        },
      });
    }

    // B. Check for Unusual Activity Patterns (Spamming)
    if (sourceAddress) {
      await this.checkUnusualActivity(sourceAddress, eventType, event.txHash ?? '');
    }
  }

  private async checkUnusualActivity(address: string, type: string, txHash: string): Promise<void> {
    const now = Math.floor(Date.now() / 1000);

    if (!this.activityHistory[address]) this.activityHistory[address] = {};
    if (!this.activityHistory[address][type]) this.activityHistory[address][type] = [];

    // Filter for events within the window and cap the array to prevent unbounded growth
    this.activityHistory[address][type] = this.activityHistory[address][type]
      .filter((ts) => now - ts < ACTIVITY_WINDOW_SECONDS)
      .slice(-ContractMonitor.MAX_ACTIVITY_ENTRIES_PER_TYPE);

    this.activityHistory[address][type].push(now);

    if (this.activityHistory[address][type].length >= ACTIVITY_THRESHOLD_COUNT) {
      await notificationService.send({
        id: `alert-unusual-${address}-${type}-${now}`,
        type: 'UNUSUAL_ACTIVITY',
        priority: 'MEDIUM',
        message: `High frequency of '${type}' events from address ${address.slice(0, 6)}...${address.slice(-4)}.`,
        timestamp: Date.now(),
        data: {
          address,
          eventType: type,
          count: this.activityHistory[address][type].length,
          txHash,
        },
      });

      // Reset tracker for this address/type to avoid duplicate alerts for the same burst
      this.activityHistory[address][type] = [];
    }
  }

  private buildTtlWarning(invoice: Invoice, currentLedger: number): InvoiceTtlWarning | null {
    if (!this.shouldTrackInvoice(invoice.status)) {
      return null;
    }

    const baseTimestamp =
      invoice.status === 'Paid' || invoice.status === 'Defaulted' || invoice.status === 'Cancelled'
        ? invoice.paidAt || invoice.fundedAt || invoice.createdAt
        : invoice.status === 'Disputed'
          ? invoice.disputedAt || invoice.fundedAt || invoice.createdAt
          : invoice.fundedAt || invoice.createdAt;

    const ttlDays = this.isTerminalStatus(invoice.status)
      ? ContractMonitor.TERMINAL_TTL_DAYS
      : ContractMonitor.ACTIVE_TTL_DAYS;
    const ttlLedgers = ttlDays * 17_280;
    const elapsedLedgers = Math.max(
      0,
      Math.floor((Date.now() - baseTimestamp * 1000) / (ContractMonitor.LEDGER_SECONDS * 1000)),
    );
    const expiryLedger = elapsedLedgers + ttlLedgers;
    const remainingLedgers = expiryLedger - currentLedger;
    const remainingDays = Math.ceil(remainingLedgers / 17_280);

    if (remainingDays > ContractMonitor.WARNING_WINDOW_DAYS) {
      return null;
    }

    return {
      id: invoice.id,
      status: invoice.status,
      expiryLedger,
      remainingDays: Math.max(0, remainingDays),
      severity: remainingDays <= 7 ? 'high' : remainingDays <= 14 ? 'medium' : 'low',
    };
  }

  private shouldTrackInvoice(status: InvoiceStatus): boolean {
    return status !== 'Expired';
  }

  private isTerminalStatus(status: InvoiceStatus): boolean {
    return (
      status === 'Paid' || status === 'Defaulted' || status === 'Cancelled' || status === 'Expired'
    );
  }
}

export const monitorService = ContractMonitor.getInstance();
