import { AlertPriority, AlertType } from './alert-rules';
import { safeStringify } from './stellar';
import { isInAppEnabled } from './notification-preferences';

/** Notification Record Interface */
export interface NotificationAlert {
  id: string;
  type: AlertType;
  priority: AlertPriority;
  message: string;
  timestamp: number;
  data?: Record<string, unknown>;
}

/**
 * Notification Service
 * Dispatches alerts to dashboard, console, and webhooks.
 */
class NotificationService {
  private static instance: NotificationService;
  private subscribers: ((alert: NotificationAlert) => void)[] = [];

  private constructor() {}

  public static getInstance(): NotificationService {
    if (!NotificationService.instance) {
      NotificationService.instance = new NotificationService();
    }
    return NotificationService.instance;
  }

  /** Send an alert to all dispatchers */
  public async send(alert: NotificationAlert): Promise<void> {
    // 1. Log to Console (Internal Monitoring)
    this.logToConsole(alert);

    // 2. Dispatch to Subscribed UI Components (respect user preferences)
    if (isInAppEnabled(alert.type)) {
      this.subscribers.forEach((sub) => sub(alert));
    }

    // 3. (Mock) Dispatch to Slack/Email if priority is HIGH/CRITICAL
    if (alert.priority === 'HIGH' || alert.priority === 'CRITICAL') {
      await this.dispatchExternal(alert);
    }
  }

  /** Subscribe to new alerts (e.g., from Dashboard) */
  public subscribe(callback: (alert: NotificationAlert) => void): () => void {
    this.subscribers.push(callback);
    return () => {
      this.subscribers = this.subscribers.filter((s) => s !== callback);
    };
  }

  private logToConsole(alert: NotificationAlert): void {
    const icon = alert.priority === 'CRITICAL' ? '🔥' : alert.priority === 'HIGH' ? '🚨' : '⚠️';
    console.log(`[Astera Alert ${icon}] [${alert.priority}] ${alert.type}: ${alert.message}`);
    if (alert.data) {
      console.log('Context Data:', safeStringify(alert.data, 2));
    }
  }

  /** External Webhook Dispatcher */
  private async dispatchExternal(alert: NotificationAlert): Promise<void> {
    try {
      const response = await fetch('/api/notifications/webhook/dispatch', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: safeStringify({
          type: alert.type,
          priority: alert.priority,
          message: alert.message,
          timestamp: alert.timestamp,
          data: alert.data,
        }),
      });

      if (response.ok) {
        console.log(`[Astera] Successfully dispatched ${alert.priority} alert to external webhook.`);
      } else {
        console.warn('[Astera] External webhook dispatch failed:', response.status);
      }
    } catch (error) {
      console.error('[Astera] Failed to dispatch webhook:', error);
    }
  }
}

export const notificationService = NotificationService.getInstance();

// ---- Invoice Status Notification Helpers ----

/**
 * Notify that an invoice has been funded.
 * Call this after a successful `fund_invoice` transaction.
 */
export function notifyInvoiceFunded(invoiceId: number, amount: bigint): void {
  notificationService.send({
    id: `invoice-funded-${invoiceId}-${Date.now()}`,
    type: 'INVOICE_FUNDED',
    priority: 'MEDIUM',
    message: `Invoice #${invoiceId} has been funded for ${Number(amount) / 10_000_000} USDC.`,
    timestamp: Date.now(),
    data: { invoiceId, amount: amount.toString() },
  });
}

/**
 * Notify that an invoice has been repaid.
 * Call this after a successful `repay_invoice` transaction.
 */
export function notifyInvoicePaid(invoiceId: number, amount: bigint): void {
  notificationService.send({
    id: `invoice-paid-${invoiceId}-${Date.now()}`,
    type: 'INVOICE_PAID',
    priority: 'MEDIUM',
    message: `Invoice #${invoiceId} has been fully repaid. Amount: ${Number(amount) / 10_000_000} USDC.`,
    timestamp: Date.now(),
    data: { invoiceId, amount: amount.toString() },
  });
}

/**
 * Notify that an invoice has defaulted.
 * Call this after a `mark_default` transaction is confirmed.
 */
export function notifyInvoiceDefaulted(invoiceId: number): void {
  notificationService.send({
    id: `invoice-defaulted-${invoiceId}-${Date.now()}`,
    type: 'INVOICE_DEFAULTED',
    priority: 'CRITICAL',
    message: `Invoice #${invoiceId} has been marked as DEFAULTED. Investors should review their positions.`,
    timestamp: Date.now(),
    data: { invoiceId },
  });
}
