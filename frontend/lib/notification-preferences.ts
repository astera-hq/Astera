import type { AlertType } from './alert-rules';

const STORAGE_KEY = 'astera.notificationPreferences.v1';

export type NotificationEventType =
  | 'INVOICE_FUNDED'
  | 'PAYMENT_DUE_7_DAYS'
  | 'PAYMENT_DUE_1_DAY'
  | 'PAYMENT_OVERDUE'
  | 'INVOICE_COMPLETED'
  | 'INVOICE_DEFAULTED'
  | 'COLLATERAL_SEIZED';

export interface EmailNotificationPreferences {
  enabled: boolean;
  email: string;
  events: NotificationEventType[];
}

export interface WebhookNotificationPreferences {
  enabled: boolean;
  url: string;
  events: NotificationEventType[];
}

export interface NotificationPreferences {
  inApp: Partial<Record<AlertType, boolean>>;
  email: EmailNotificationPreferences;
  webhook: WebhookNotificationPreferences;
}

export const NOTIFICATION_EVENTS: {
  type: NotificationEventType;
  label: string;
  audience: 'SME' | 'Investor' | 'Both';
  description: string;
}[] = [
  {
    type: 'INVOICE_FUNDED',
    label: 'Invoice funded',
    audience: 'SME',
    description: 'An invoice you submitted has been funded.',
  },
  {
    type: 'PAYMENT_DUE_7_DAYS',
    label: 'Payment due in 7 days',
    audience: 'SME',
    description: 'An invoice payment is due in one week.',
  },
  {
    type: 'PAYMENT_DUE_1_DAY',
    label: 'Payment due tomorrow',
    audience: 'SME',
    description: 'An invoice payment is due in one day.',
  },
  {
    type: 'PAYMENT_OVERDUE',
    label: 'Payment overdue',
    audience: 'Both',
    description: 'An invoice payment is overdue.',
  },
  {
    type: 'INVOICE_COMPLETED',
    label: 'Invoice completed',
    audience: 'Both',
    description: 'An invoice has been fully repaid.',
  },
  {
    type: 'INVOICE_DEFAULTED',
    label: 'Invoice defaulted',
    audience: 'Both',
    description: 'An invoice has been marked as defaulted.',
  },
  {
    type: 'COLLATERAL_SEIZED',
    label: 'Collateral seized',
    audience: 'SME',
    description: 'Collateral was seized after an invoice default.',
  },
];

export const DEFAULT_NOTIFICATION_PREFERENCES: NotificationPreferences = {
  inApp: Object.fromEntries(NOTIFICATION_EVENTS.map((e) => [e.type, true] as const)),
  email: {
    enabled: false,
    email: '',
    events: [],
  },
  webhook: {
    enabled: false,
    url: '',
    events: [],
  },
};

function isBrowser(): boolean {
  return typeof window !== 'undefined' && typeof window.localStorage !== 'undefined';
}

function safeParse(json: string): unknown {
  try {
    return JSON.parse(json);
  } catch {
    return null;
  }
}

export function loadNotificationPreferences(): NotificationPreferences {
  if (!isBrowser()) return DEFAULT_NOTIFICATION_PREFERENCES;

  const raw = window.localStorage.getItem(STORAGE_KEY);
  if (!raw) return DEFAULT_NOTIFICATION_PREFERENCES;

  const parsed = safeParse(raw);
  if (!parsed || typeof parsed !== 'object') return DEFAULT_NOTIFICATION_PREFERENCES;

  const p = parsed as Partial<NotificationPreferences>;

  return {
    inApp:
      (p.inApp && typeof p.inApp === 'object' ? p.inApp : DEFAULT_NOTIFICATION_PREFERENCES.inApp) ??
      {},
    email: {
      enabled: Boolean(p.email?.enabled),
      email: typeof p.email?.email === 'string' ? p.email.email : '',
      events: Array.isArray(p.email?.events) ? (p.email!.events as NotificationEventType[]) : [],
    },
    webhook: {
      enabled: Boolean(p.webhook?.enabled),
      url: typeof p.webhook?.url === 'string' ? p.webhook.url : '',
      events: Array.isArray(p.webhook?.events)
        ? (p.webhook!.events as NotificationEventType[])
        : [],
    },
  };
}

export function saveNotificationPreferences(prefs: NotificationPreferences): void {
  if (!isBrowser()) return;
  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(prefs));
}

export function isInAppEnabled(alertType: AlertType): boolean {
  if (!isBrowser()) return true;
  const prefs = loadNotificationPreferences();
  const value = prefs.inApp?.[alertType];
  return value ?? true;
}

export function isValidEmail(email: string): boolean {
  const trimmed = email.trim();
  if (!trimmed) return false;
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(trimmed);
}

export function isValidWebhookUrl(url: string): boolean {
  try {
    const u = new URL(url);
    return u.protocol === 'https:' || u.protocol === 'http:';
  } catch {
    return false;
  }
}
