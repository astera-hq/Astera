import type { AlertType } from './alert-rules';

const STORAGE_KEY = 'astera.notificationPreferences.v1';

export type NotificationEventType =
  | 'INVOICE_FUNDED'
  | 'INVOICE_PAID'
  | 'INVOICE_DEFAULTED'
  | 'DISPUTE_RAISED'
  | 'DISPUTE_RESOLVED'
  | 'YIELD_RATE_CHANGED'
  | 'COLLATERAL_REQUIRED';

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
    type: 'INVOICE_PAID',
    label: 'Invoice repaid',
    audience: 'Investor',
    description: 'A repayment has been received for an invoice you funded.',
  },
  {
    type: 'INVOICE_DEFAULTED',
    label: 'Invoice defaulted',
    audience: 'Both',
    description: 'An invoice has been marked as defaulted.',
  },
  {
    type: 'DISPUTE_RAISED',
    label: 'Dispute raised',
    audience: 'Both',
    description: 'A dispute was raised on an invoice.',
  },
  {
    type: 'DISPUTE_RESOLVED',
    label: 'Dispute resolved',
    audience: 'Both',
    description: 'A dispute was resolved on an invoice.',
  },
  {
    type: 'YIELD_RATE_CHANGED',
    label: 'Yield rate changed',
    audience: 'Investor',
    description: 'The pool yield rate was updated.',
  },
  {
    type: 'COLLATERAL_REQUIRED',
    label: 'Collateral required',
    audience: 'SME',
    description: 'Additional collateral is required for an invoice or position.',
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

export function updateInAppPreference(
  alertType: AlertType,
  enabled: boolean,
): NotificationPreferences {
  const prefs = loadNotificationPreferences();
  const next: NotificationPreferences = {
    ...prefs,
    inApp: { ...(prefs.inApp ?? {}), [alertType]: enabled },
  };
  saveNotificationPreferences(next);
  return next;
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
