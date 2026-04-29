'use client';

import { useEffect, useMemo, useState } from 'react';
import toast from 'react-hot-toast';
import type { AlertType } from '@/lib/alert-rules';
import {
  DEFAULT_NOTIFICATION_PREFERENCES,
  loadNotificationPreferences,
  NOTIFICATION_EVENTS,
  saveNotificationPreferences,
  isValidEmail,
  isValidWebhookUrl,
  type NotificationEventType,
  type NotificationPreferences,
} from '@/lib/notification-preferences';

function Toggle({
  checked,
  onChange,
  label,
  description,
  rightMeta,
}: {
  checked: boolean;
  onChange: (next: boolean) => void;
  label: string;
  description?: string;
  rightMeta?: React.ReactNode;
}) {
  return (
    <div className="flex items-start justify-between gap-4 py-4">
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <p className="text-sm font-semibold text-white truncate">{label}</p>
          {rightMeta}
        </div>
        {description && <p className="text-xs text-brand-muted mt-1">{description}</p>}
      </div>

      <button
        type="button"
        onClick={() => onChange(!checked)}
        aria-pressed={checked}
        className={`relative inline-flex h-6 w-11 items-center rounded-full border transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-gold ${
          checked ? 'bg-brand-gold/20 border-brand-gold' : 'bg-brand-dark border-brand-border'
        }`}
      >
        <span
          className={`inline-block h-5 w-5 transform rounded-full transition-transform ${
            checked ? 'translate-x-5 bg-brand-gold' : 'translate-x-1 bg-brand-muted'
          }`}
        />
      </button>
    </div>
  );
}

function EventPill({ audience }: { audience: 'SME' | 'Investor' | 'Both' }) {
  const styles =
    audience === 'Both'
      ? 'bg-brand-border text-brand-muted'
      : audience === 'Investor'
        ? 'bg-blue-900/40 text-blue-300 border-blue-800/50'
        : 'bg-emerald-900/40 text-emerald-300 border-emerald-800/50';
  return (
    <span
      className={`shrink-0 text-[10px] border px-2 py-0.5 rounded-full font-semibold ${styles}`}
    >
      {audience}
    </span>
  );
}

export default function NotificationSettingsPage() {
  const [prefs, setPrefs] = useState<NotificationPreferences>(DEFAULT_NOTIFICATION_PREFERENCES);
  const [saving, setSaving] = useState(false);
  const [testingWebhook, setTestingWebhook] = useState(false);

  useEffect(() => {
    setPrefs(loadNotificationPreferences());
  }, []);

  const inAppEvents = useMemo(() => NOTIFICATION_EVENTS, []);

  function setInAppEventEnabled(type: NotificationEventType, enabled: boolean) {
    const next: NotificationPreferences = {
      ...prefs,
      inApp: { ...(prefs.inApp ?? {}), [type satisfies AlertType]: enabled },
    };
    setPrefs(next);
    saveNotificationPreferences(next);
  }

  function toggleArray<T>(arr: T[], value: T, enabled: boolean): T[] {
    if (enabled) return arr.includes(value) ? arr : [...arr, value];
    return arr.filter((v) => v !== value);
  }

  async function saveExternalPreferences() {
    if (prefs.email.enabled) {
      if (!isValidEmail(prefs.email.email)) {
        toast.error('Please enter a valid email address before enabling email notifications.');
        return;
      }
      if (prefs.email.events.length === 0) {
        toast.error('Select at least one event for email notifications (or disable email).');
        return;
      }
    }

    if (prefs.webhook.enabled) {
      if (!isValidWebhookUrl(prefs.webhook.url)) {
        toast.error('Please enter a valid webhook URL (http/https) before enabling webhooks.');
        return;
      }
      if (prefs.webhook.events.length === 0) {
        toast.error('Select at least one event for webhook forwarding (or disable webhooks).');
        return;
      }
    }

    setSaving(true);
    try {
      const res = await fetch('/api/notifications/preferences', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email: prefs.email, webhook: prefs.webhook }),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      toast.success('Notification delivery preferences saved.');
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Failed to save preferences.';
      toast.error(msg);
    } finally {
      setSaving(false);
    }
  }

  async function testWebhook() {
    if (!isValidWebhookUrl(prefs.webhook.url)) {
      toast.error('Enter a valid webhook URL first.');
      return;
    }

    setTestingWebhook(true);
    try {
      const res = await fetch('/api/notifications/webhook/test', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ url: prefs.webhook.url }),
      });
      const body = (await res.json()) as { ok?: boolean; error?: string };
      if (!res.ok || !body.ok) throw new Error(body.error || `HTTP ${res.status}`);
      toast.success('Test webhook sent.');
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Failed to send test webhook.';
      toast.error(msg);
    } finally {
      setTestingWebhook(false);
    }
  }

  function resetToDefaults() {
    setPrefs(DEFAULT_NOTIFICATION_PREFERENCES);
    saveNotificationPreferences(DEFAULT_NOTIFICATION_PREFERENCES);
    toast.success('Reset in-app preferences to defaults.');
  }

  return (
    <div className="max-w-3xl mx-auto px-6 pt-28 pb-16 space-y-8">
      <div className="space-y-2">
        <h1 className="text-3xl font-bold text-white">Notification settings</h1>
        <p className="text-sm text-brand-muted">
          Choose which events you want to see in-app, and optionally subscribe to email or webhook
          delivery.
        </p>
      </div>

      {/* In-app */}
      <section className="p-8 bg-brand-card border border-brand-border rounded-2xl shadow-sm">
        <div className="flex items-start justify-between gap-4 mb-6">
          <div>
            <h2 className="text-lg font-bold text-white">In-app notifications</h2>
            <p className="text-xs text-brand-muted mt-1">
              These preferences are stored locally in this browser.
            </p>
          </div>
          <button
            type="button"
            onClick={resetToDefaults}
            className="text-xs text-brand-muted hover:text-white transition-colors"
          >
            Reset
          </button>
        </div>

        <div className="divide-y divide-brand-border">
          {inAppEvents.map((e) => (
            <Toggle
              key={e.type}
              checked={prefs.inApp?.[e.type] ?? true}
              onChange={(next) => setInAppEventEnabled(e.type, next)}
              label={e.label}
              description={e.description}
              rightMeta={<EventPill audience={e.audience} />}
            />
          ))}
        </div>
      </section>

      {/* Email */}
      <section className="p-8 bg-brand-card border border-brand-border rounded-2xl shadow-sm space-y-6">
        <div>
          <h2 className="text-lg font-bold text-white">Email notifications</h2>
          <p className="text-xs text-brand-muted mt-1">
            This is a UI + mocked backend save for now (delivery service tracked separately).
          </p>
        </div>

        <Toggle
          checked={prefs.email.enabled}
          onChange={(next) => setPrefs((p) => ({ ...p, email: { ...p.email, enabled: next } }))}
          label="Subscribe to email alerts"
          description="Requires a valid email address."
        />

        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div className="md:col-span-2">
            <label className="block text-xs font-semibold text-brand-muted mb-2 uppercase tracking-wider">
              Email address
            </label>
            <input
              type="email"
              value={prefs.email.email}
              onChange={(e) =>
                setPrefs((p) => ({ ...p, email: { ...p.email, email: e.target.value } }))
              }
              className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-3 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold"
              placeholder="you@example.com"
            />
            {prefs.email.enabled &&
              prefs.email.email.trim() &&
              !isValidEmail(prefs.email.email) && (
                <p className="mt-2 text-xs text-red-400">Enter a valid email address.</p>
              )}
          </div>
        </div>

        <div className="border-t border-brand-border pt-6">
          <p className="text-xs font-semibold text-brand-muted mb-3 uppercase tracking-wider">
            Events to send via email
          </p>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            {NOTIFICATION_EVENTS.map((e) => {
              const checked = prefs.email.events.includes(e.type);
              return (
                <label
                  key={`email-${e.type}`}
                  className="flex items-center gap-3 p-3 rounded-xl bg-brand-dark border border-brand-border hover:border-brand-gold/50 transition-colors"
                >
                  <input
                    type="checkbox"
                    checked={checked}
                    onChange={(ev) =>
                      setPrefs((p) => ({
                        ...p,
                        email: {
                          ...p.email,
                          events: toggleArray(p.email.events, e.type, ev.target.checked),
                        },
                      }))
                    }
                    className="accent-brand-gold"
                  />
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="text-sm text-white truncate">{e.label}</span>
                      <EventPill audience={e.audience} />
                    </div>
                    <span className="text-xs text-brand-muted">{e.description}</span>
                  </div>
                </label>
              );
            })}
          </div>
        </div>
      </section>

      {/* Webhook */}
      <section className="p-8 bg-brand-card border border-brand-border rounded-2xl shadow-sm space-y-6">
        <div>
          <h2 className="text-lg font-bold text-white">Webhook forwarding</h2>
          <p className="text-xs text-brand-muted mt-1">
            Forward selected events to a custom endpoint. Use “Test webhook” to send a sample
            payload.
          </p>
        </div>

        <Toggle
          checked={prefs.webhook.enabled}
          onChange={(next) => setPrefs((p) => ({ ...p, webhook: { ...p.webhook, enabled: next } }))}
          label="Enable webhook forwarding"
          description="Requires a valid URL."
        />

        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          <div className="md:col-span-2">
            <label className="block text-xs font-semibold text-brand-muted mb-2 uppercase tracking-wider">
              Webhook URL
            </label>
            <input
              type="url"
              value={prefs.webhook.url}
              onChange={(e) =>
                setPrefs((p) => ({ ...p, webhook: { ...p.webhook, url: e.target.value } }))
              }
              className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-3 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold"
              placeholder="https://example.com/webhook"
            />
            {prefs.webhook.enabled &&
              prefs.webhook.url.trim() &&
              !isValidWebhookUrl(prefs.webhook.url) && (
                <p className="mt-2 text-xs text-red-400">Enter a valid http/https URL.</p>
              )}
          </div>

          <div className="flex items-end">
            <button
              type="button"
              onClick={testWebhook}
              disabled={testingWebhook}
              className="w-full py-3 bg-white text-brand-dark font-bold rounded-xl hover:bg-stone-200 transition-all shadow-lg active:scale-[0.98] disabled:opacity-50"
            >
              {testingWebhook ? 'Testing…' : 'Test webhook'}
            </button>
          </div>
        </div>

        <div className="border-t border-brand-border pt-6">
          <p className="text-xs font-semibold text-brand-muted mb-3 uppercase tracking-wider">
            Events to forward
          </p>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            {NOTIFICATION_EVENTS.map((e) => {
              const checked = prefs.webhook.events.includes(e.type);
              return (
                <label
                  key={`webhook-${e.type}`}
                  className="flex items-center gap-3 p-3 rounded-xl bg-brand-dark border border-brand-border hover:border-brand-gold/50 transition-colors"
                >
                  <input
                    type="checkbox"
                    checked={checked}
                    onChange={(ev) =>
                      setPrefs((p) => ({
                        ...p,
                        webhook: {
                          ...p.webhook,
                          events: toggleArray(p.webhook.events, e.type, ev.target.checked),
                        },
                      }))
                    }
                    className="accent-brand-gold"
                  />
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="text-sm text-white truncate">{e.label}</span>
                      <EventPill audience={e.audience} />
                    </div>
                    <span className="text-xs text-brand-muted">{e.description}</span>
                  </div>
                </label>
              );
            })}
          </div>
        </div>

        <div className="flex items-center justify-end gap-3 pt-2">
          <button
            type="button"
            onClick={saveExternalPreferences}
            disabled={saving}
            className="px-6 py-3 bg-brand-gold text-brand-dark font-bold rounded-xl hover:bg-brand-amber transition-all shadow-lg active:scale-[0.98] disabled:opacity-50"
          >
            {saving ? 'Saving…' : 'Save email & webhook'}
          </button>
        </div>
      </section>
    </div>
  );
}
