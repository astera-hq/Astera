import { createHmac, randomUUID } from 'crypto';
import { isSafeWebhookUrl } from '@/lib/server/webhook-url';

export type DeliveryEvent =
  | 'INVOICE_FUNDED'
  | 'PAYMENT_DUE_7_DAYS'
  | 'PAYMENT_DUE_1_DAY'
  | 'PAYMENT_OVERDUE'
  | 'INVOICE_COMPLETED'
  | 'INVOICE_DEFAULTED'
  | 'COLLATERAL_SEIZED';

export type DeliveryPreferences = {
  email?: { enabled: boolean; email: string; events: DeliveryEvent[] };
  webhook?: { enabled: boolean; url: string; events: DeliveryEvent[] };
};

export type NotificationPayload = {
  event: DeliveryEvent;
  recipient: string;
  invoiceId?: number;
  occurredAt?: string;
  data?: Record<string, unknown>;
};

const MAX_ATTEMPTS = 3;

function webhookSecret(): string {
  const secret = process.env.NOTIFICATION_WEBHOOK_SECRET;
  if (!secret) throw new Error('NOTIFICATION_WEBHOOK_SECRET is not configured');
  return secret;
}

function html(payload: NotificationPayload, unsubscribeUrl: string): string {
  return `<h1>Astera notification</h1><p>${payload.event.replaceAll('_', ' ')}</p><p>Invoice: ${payload.invoiceId ?? 'n/a'}</p><p><a href="${unsubscribeUrl}">Unsubscribe from these notifications</a></p>`;
}

async function retry(operation: () => Promise<void>): Promise<void> {
  let lastError: unknown;
  for (let attempt = 0; attempt < MAX_ATTEMPTS; attempt += 1) {
    try {
      await operation();
      return;
    } catch (error) {
      lastError = error;
      if (attempt < MAX_ATTEMPTS - 1)
        await new Promise((resolve) => setTimeout(resolve, 250 * 2 ** attempt));
    }
  }
  throw lastError;
}

async function sendEmail(payload: NotificationPayload): Promise<void> {
  const apiKey = process.env.RESEND_API_KEY;
  if (!apiKey) throw new Error('RESEND_API_KEY is not configured');
  const appUrl = process.env.NEXT_PUBLIC_APP_URL ?? 'http://localhost:3000';
  const unsubscribeUrl = `${appUrl}/settings/notifications?unsubscribe=${encodeURIComponent(payload.recipient)}`;
  const response = await fetch('https://api.resend.com/emails', {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${apiKey}`,
      'Content-Type': 'application/json',
      'Idempotency-Key': `invoice-event-${payload.event}-${payload.invoiceId ?? 'none'}-${payload.recipient}`,
    },
    body: JSON.stringify({
      from: process.env.NOTIFICATION_FROM_EMAIL ?? 'Astera <onboarding@resend.dev>',
      to: [payload.recipient],
      subject: `Astera: ${payload.event.replaceAll('_', ' ')}`,
      html: html(payload, unsubscribeUrl),
    }),
  });
  if (!response.ok) throw new Error(`Email provider returned HTTP ${response.status}`);
}

async function sendWebhook(url: string, payload: NotificationPayload): Promise<void> {
  if (!(await isSafeWebhookUrl(url))) throw new Error('Invalid webhook URL');

  const body = JSON.stringify({
    id: randomUUID(),
    ...payload,
    occurredAt: payload.occurredAt ?? new Date().toISOString(),
  });
  const signature = createHmac('sha256', webhookSecret()).update(body).digest('hex');
  const response = await fetch(url, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Astera-Signature': `sha256=${signature}`,
      'X-Astera-Event': payload.event,
    },
    body,
    redirect: 'error',
  });
  if (!response.ok) throw new Error(`Webhook returned HTTP ${response.status}`);
}

/** Called by the event indexer after it maps a contract event to recipients. */
export async function deliverNotification(
  payload: NotificationPayload,
  preferences: DeliveryPreferences,
): Promise<{ email: boolean; webhook: boolean }> {
  const email = Boolean(
    preferences.email?.enabled && preferences.email.events.includes(payload.event),
  );
  const webhook = Boolean(
    preferences.webhook?.enabled && preferences.webhook.events.includes(payload.event),
  );
  if (email) await retry(() => sendEmail(payload));
  if (webhook && preferences.webhook?.url)
    await retry(() => sendWebhook(preferences.webhook.url, payload));
  return { email, webhook };
}
