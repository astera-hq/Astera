/**
 * SSE Events Polling Service (Issue #157)
 *
 * Implements Option A (polling) for real-time contract event subscription.
 * Polls Stellar Horizon/Soroban RPC event endpoints at configurable intervals,
 * updates the Zustand store, and triggers toast notifications for relevant events.
 *
 * Features:
 * - Configurable polling interval (default: 15 seconds)
 * - Visibility-aware: pauses when page is hidden
 * - Role-based event filters (SME, Investor, Admin)
 * - No memory leaks: cleanup on unmount and page hide
 * - Deduplication of already-processed events
 */

import { rpcGetEvents, rpcGetLatestLedger, INVOICE_CONTRACT_ID, POOL_CONTRACT_ID, scValToNative } from './stellar';
import { notificationService } from './notifications';
import { monitorService, ContractEvent } from './monitoring';
import { useStore } from './store';
import { pushToast } from '../components/Toast';
import type { AlertType, AlertPriority } from './alert-rules';

// ---- Configuration ----

export const DEFAULT_POLL_INTERVAL_MS = 15_000; // 15 seconds (configurable)
export const MIN_POLL_INTERVAL_MS = 5_000; // minimum 5 seconds
export const MAX_EVENTS_HISTORY = 100; // max events to keep in store

// ---- User Role Types ----

export type UserRole = 'SME' | 'Investor' | 'Admin';

// ---- Event Filter Definitions per Role ----

export interface EventFilter {
  /** Event topic types to include */
  eventTypes: string[];
  /** Whether to show toast notifications for these events */
  notify: boolean;
}

export const ROLE_EVENT_FILTERS: Record<UserRole, EventFilter> = {
  SME: {
    eventTypes: ['funded', 'repaid', 'default', 'created'],
    notify: true,
  },
  Investor: {
    eventTypes: ['funded', 'repaid', 'deposit', 'withdraw', 'default'],
    notify: true,
  },
  Admin: {
    eventTypes: ['funded', 'repaid', 'default', 'created', 'deposit', 'withdraw'],
    notify: true,
  },
};

// ---- Toast Notification Mapping ----

export interface ToastConfig {
  title: string;
  description: string;
  kind: 'info' | 'warning' | 'error' | 'success';
}

export const EVENT_TOAST_MAP: Record<string, (value: unknown, txHash?: string) => ToastConfig> = {
  funded: (value: unknown, txHash?: string) => {
    const arr = (value as unknown[]) ?? [];
    const id = arr[0] as number;
    const principal = arr[2] as bigint;
    const amount = Number(principal) / 10_000_000;
    return {
      title: 'Invoice Funded',
      description: `Invoice #${id} has been funded for ${amount.toLocaleString()} USDC${txHash ? ' — view on StellarExpert' : ''}`,
      kind: 'success',
    };
  },
  repaid: (value: unknown) => {
    const arr = (value as unknown[]) ?? [];
    const id = arr[0] as number;
    const principal = arr[1] as bigint;
    const amount = Number(principal) / 10_000_000;
    return {
      title: 'Invoice Repaid',
      description: `Invoice #${id} repaid: ${amount.toLocaleString()} USDC`,
      kind: 'info',
    };
  },
  default: () => ({
    title: 'Invoice Defaulted',
    description: 'An invoice has been marked as defaulted. Review positions immediately.',
    kind: 'error',
  }),
  created: (value: unknown) => {
    const arr = (value as unknown[]) ?? [];
    const id = arr[0] as number;
    const owner = arr[1] as string;
    const amount = arr[2] as bigint;
    const humanAmount = Number(amount) / 10_000_000;
    return {
      title: 'Invoice Created',
      description: `New invoice #${id} created by ${owner?.slice(0, 8)}... for ${humanAmount.toLocaleString()} USDC`,
      kind: 'info',
    };
  },
  deposit: (value: unknown) => {
    const arr = (value as unknown[]) ?? [];
    const investor = arr[0] as string;
    const amount = arr[1] as bigint;
    const humanAmount = Number(amount) / 10_000_000;
    return {
      title: 'Pool Deposit',
      description: `${investor?.slice(0, 8)}... deposited ${humanAmount.toLocaleString()} USDC`,
      kind: 'info',
    };
  },
  withdraw: (value: unknown) => {
    const arr = (value as unknown[]) ?? [];
    const investor = arr[0] as string;
    const amount = arr[1] as bigint;
    const humanAmount = Number(amount) / 10_000_000;
    return {
      title: 'Pool Withdrawal',
      description: `${investor?.slice(0, 8)}... withdrew ${humanAmount.toLocaleString()} USDC`,
      kind: 'warning',
    };
  },
};

// ---- Store Event Type ----

export interface StoreEvent {
  id: string;
  contractId: string;
  topic: string[];
  value: unknown;
  ledger: number;
  ledgerCloseAt: string;
  txHash: string;
  receivedAt: number;
}

// ---- SSE Polling Service Class ----

class SseEventsService {
  private static instance: SseEventsService;

  private intervalId: ReturnType<typeof setInterval> | null = null;
  private pollIntervalMs: number = DEFAULT_POLL_INTERVAL_MS;
  private isRunning: boolean = false;
  private processedEventIds: Set<string> = new Set();
  private currentRole: UserRole = 'Admin';
  private visibilityHandler: (() => void) | null = null;
  private cleanupCallbacks: (() => void)[] = [];

  private constructor() {}

  public static getInstance(): SseEventsService {
    if (!SseEventsService.instance) {
      SseEventsService.instance = new SseEventsService();
    }
    return SseEventsService.instance;
  }

  /**
   * Start the polling loop.
   * @param options - Configuration options
   */
  public start(options?: { intervalMs?: number; role?: UserRole }): void {
    if (this.isRunning) {
      console.warn('[SSE Events] Polling is already running. Call stop() first.');
      return;
    }

    if (typeof window === 'undefined') {
      console.warn('[SSE Events] Cannot start polling on server-side.');
      return;
    }

    if (options?.intervalMs) {
      this.setPollInterval(options.intervalMs);
    }

    if (options?.role) {
      this.currentRole = options.role;
    }

    this.isRunning = true;
    console.log(
      `[SSE Events] Started polling every ${this.pollIntervalMs}ms (role: ${this.currentRole})`,
    );

    // Set up visibility change handler to pause when page is hidden
    this.setupVisibilityHandler();

    // Start polling immediately, then on interval
    this.poll();
    this.intervalId = setInterval(() => {
      this.poll();
    }, this.pollIntervalMs);
  }

  /**
   * Stop the polling loop and clean up all resources.
   */
  public stop(): void {
    if (this.intervalId) {
      clearInterval(this.intervalId);
      this.intervalId = null;
    }

    this.removeVisibilityHandler();

    this.isRunning = false;
    this.processedEventIds.clear();

    // Run cleanup callbacks
    this.cleanupCallbacks.forEach((cb) => cb());
    this.cleanupCallbacks = [];

    console.log('[SSE Events] Polling stopped and cleaned up.');
  }

  /**
   * Configure the polling interval.
   */
  public setPollInterval(intervalMs: number): void {
    const clamped = Math.max(MIN_POLL_INTERVAL_MS, intervalMs);
    this.pollIntervalMs = clamped;

    // Restart interval with new timing if running
    if (this.isRunning && this.intervalId) {
      clearInterval(this.intervalId);
      this.intervalId = setInterval(() => {
        this.poll();
      }, this.pollIntervalMs);
    }
  }

  /**
   * Get the current polling interval.
   */
  public getPollInterval(): number {
    return this.pollIntervalMs;
  }

  /**
   * Check if polling is currently active.
   */
  public isActive(): boolean {
    return this.isRunning;
  }

  /**
   * Set the current user role (affects event filtering).
   */
  public setRole(role: UserRole): void {
    this.currentRole = role;
  }

  /**
   * Register a cleanup callback to run on stop().
   */
  public onCleanup(cb: () => void): void {
    this.cleanupCallbacks.push(cb);
  }

  // ---- Private Methods ----

  /**
   * Execute a single poll cycle.
   */
  private async poll(): Promise<void> {
    if (!this.isRunning) return;

    try {
      const events = await this.fetchNewEvents();

      if (events.length === 0) {
        return;
      }

      console.log(`[SSE Events] Received ${events.length} new event(s).`);

      // Process each event
      for (const event of events) {
        await this.processEvent(event);
      }

      // Update Zustand store with recent events
      this.updateStore(events);

      // Trigger portfolio refresh for investors
      if (this.currentRole === 'Investor') {
        this.refreshInvestorData();
      }
    } catch (error) {
      console.error('[SSE Events] Poll cycle failed:', error);
    }
  }

  /**
   * Fetch new events from the Soroban RPC endpoint.
   */
  private async fetchNewEvents(): Promise<ContractEvent[]> {
    if (!INVOICE_CONTRACT_ID || !POOL_CONTRACT_ID) {
      return [];
    }

    try {
      const latestLedger = await rpcGetLatestLedger();
      const startLedger = Math.max(1, latestLedger.sequence - 50); // Look back 50 ledgers

      const response = await rpcGetEvents({
        startLedger,
        filters: [{ contractIds: [INVOICE_CONTRACT_ID, POOL_CONTRACT_ID] }],
      });

      const events: ContractEvent[] = response.events.map(
        (e: {
          id: string;
          contractId: string;
          topic: unknown[];
          value: unknown;
          ledger: number;
          ledgerCloseAt: string;
          txHash: string;
        }) => ({
          id: e.id,
          contractId: e.contractId,
          topic: e.topic.map((t: unknown) => scValToNative(t as string)),
          value: scValToNative(e.value),
          ledger: e.ledger,
          ledgerCloseAt: e.ledgerCloseAt,
          txHash: e.txHash,
        }),
      );

      // Filter out already-processed events
      const newEvents = events.filter((e) => !this.processedEventIds.has(e.id));

      // Mark as processed
      newEvents.forEach((e) => this.processedEventIds.add(e.id));

      // Apply role-based filters
      return this.filterEventsByRole(newEvents);
    } catch (error) {
      console.error('[SSE Events] Failed to fetch events:', error);
      return [];
    }
  }

  /**
   * Filter events based on the current user role.
   */
  private filterEventsByRole(events: ContractEvent[]): ContractEvent[] {
    const filter = ROLE_EVENT_FILTERS[this.currentRole];
    return events.filter((event) => {
      const [, eventType] = event.topic;
      if (typeof eventType !== 'string') return false;
      return filter.eventTypes.includes(eventType);
    });
  }

  /**
   * Process a single event: show toast, send notification.
   */
  private async processEvent(event: ContractEvent): Promise<void> {
    const [, eventType] = event.topic;
    if (typeof eventType !== 'string') return;

    const filter = ROLE_EVENT_FILTERS[this.currentRole];

    // Show toast notification if configured for this role
    if (filter.notify) {
      const toastBuilder = EVENT_TOAST_MAP[eventType];
      if (toastBuilder) {
        const toast = toastBuilder(event.value, event.txHash);
        pushToast({
          id: `sse-${event.id}`,
          ...toast,
          durationMs: 8000,
        });
      }
    }

    // Also dispatch through the notification service for system-level alerts
    const alertType = this.mapEventTypeToAlertType(eventType);
    if (alertType) {
      await notificationService.send({
        id: `sse-alert-${event.id}`,
        type: alertType,
        priority: this.mapEventTypeToPriority(eventType),
        message: this.buildAlertMessage(eventType, event.value),
        timestamp: Date.now(),
        data: {
          eventType,
          contractType: event.contractId,
          txHash: event.txHash,
          ledger: event.ledger,
          value: event.value,
        },
      });
    }

    // Run through the existing monitor service for alert rules
    await monitorService.pollEvents();
  }

  /**
   * Map event type to AlertType.
   */
  private mapEventTypeToAlertType(eventType: string): AlertType | null {
    const mapping: Record<string, AlertType> = {
      funded: 'INVOICE_FUNDED',
      repaid: 'INVOICE_PAID',
      default: 'INVOICE_DEFAULTED',
    };
    return mapping[eventType] ?? null;
  }

  /**
   * Map event type to AlertPriority.
   */
  private mapEventTypeToPriority(eventType: string): AlertPriority {
    const mapping: Record<string, AlertPriority> = {
      funded: 'MEDIUM',
      repaid: 'MEDIUM',
      default: 'CRITICAL',
      created: 'LOW',
      deposit: 'LOW',
      withdraw: 'MEDIUM',
    };
    return mapping[eventType] ?? 'LOW';
  }

  /**
   * Build a human-readable alert message.
   */
  private buildAlertMessage(eventType: string, value: unknown): string {
    switch (eventType) {
      case 'funded': {
        const arr = (value as unknown[]) ?? [];
        const id = arr[0] as number;
        const principal = arr[2] as bigint;
        const amount = Number(principal) / 10_000_000;
        return `Invoice #${id} funded for ${amount.toLocaleString()} USDC`;
      }
      case 'repaid': {
        const arr = (value as unknown[]) ?? [];
        const id = arr[0] as number;
        const principal = arr[1] as bigint;
        const amount = Number(principal) / 10_000_000;
        return `Invoice #${id} repaid: ${amount.toLocaleString()} USDC`;
      }
      case 'default':
        return 'An invoice has been marked as defaulted.';
      case 'created': {
        const arr = (value as unknown[]) ?? [];
        const id = arr[0] as number;
        return `New invoice #${id} created`;
      }
      case 'deposit': {
        const arr = (value as unknown[]) ?? [];
        const amount = arr[1] as bigint;
        const humanAmount = Number(amount) / 10_000_000;
        return `Pool deposit: ${humanAmount.toLocaleString()} USDC`;
      }
      case 'withdraw': {
        const arr = (value as unknown[]) ?? [];
        const amount = arr[1] as bigint;
        const humanAmount = Number(amount) / 10_000_000;
        return `Pool withdrawal: ${humanAmount.toLocaleString()} USDC`;
      }
      default:
        return `Event: ${eventType}`;
    }
  }

  /**
   * Update the Zustand store with new events.
   */
  private updateStore(events: ContractEvent[]): void {
    const store = useStore.getState();
    const storeEvents: StoreEvent[] = events.map((e) => ({
      id: e.id,
      contractId: e.contractId,
      topic: e.topic.map(String),
      value: e.value,
      ledger: e.ledger,
      ledgerCloseAt: e.ledgerCloseAt,
      txHash: e.txHash,
      receivedAt: Date.now(),
    }));

    // Append new events, keeping max history
    const existing = store.recentEvents ?? [];
    const combined = [...storeEvents, ...existing].slice(0, MAX_EVENTS_HISTORY);
    store.setRecentEvents(combined);

    // Update last poll timestamp
    store.setLastPollTime(Date.now());
  }

  /**
   * Trigger a refresh of investor portfolio data.
   */
  private refreshInvestorData(): void {
    const store = useStore.getState();
    if (store.refreshPosition) {
      store.refreshPosition();
    }
  }

  /**
   * Set up the visibility change handler to pause/resume polling.
   */
  private setupVisibilityHandler(): void {
    this.visibilityHandler = () => {
      if (document.hidden) {
        console.log('[SSE Events] Page hidden — pausing polling.');
        if (this.intervalId) {
          clearInterval(this.intervalId);
          this.intervalId = null;
        }
      } else {
        console.log('[SSE Events] Page visible — resuming polling.');
        if (!this.intervalId && this.isRunning) {
          this.poll(); // Immediate poll on visibility restore
          this.intervalId = setInterval(() => {
            this.poll();
          }, this.pollIntervalMs);
        }
      }
    };

    document.addEventListener('visibilitychange', this.visibilityHandler);
  }

  /**
   * Remove the visibility change handler.
   */
  private removeVisibilityHandler(): void {
    if (this.visibilityHandler) {
      document.removeEventListener('visibilitychange', this.visibilityHandler);
      this.visibilityHandler = null;
    }
  }
}

/** Singleton instance of the SSE Events service */
export const sseEventsService = SseEventsService.getInstance();

/** Export the class for testing purposes */
export { SseEventsService };
