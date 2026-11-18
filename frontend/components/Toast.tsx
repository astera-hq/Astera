'use client';

import { useEffect, useState } from 'react';
import { useTranslations } from 'next-intl';

export type ToastKind = 'info' | 'warning' | 'error' | 'success';

export interface ToastMessage {
  id: string;
  kind: ToastKind;
  title: string;
  description?: string;
  /** Auto-dismiss after this many ms. 0 or undefined = persist until dismissed. */
  durationMs?: number;
}

type Listener = (toast: ToastMessage) => void;
const listeners = new Set<Listener>();

/**
 * Imperatively push a toast from any module (e.g. from a polling side-effect
 * that runs outside the React tree). `<ToastHost />` renders and dismisses
 * these messages.
 */
export function pushToast(toast: Omit<ToastMessage, 'id'> & { id?: string }): string {
  const id = toast.id ?? `toast-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  const message: ToastMessage = { durationMs: 6000, ...toast, id };
  listeners.forEach((l) => l(message));
  return id;
}

const KIND_STYLES: Record<ToastKind, string> = {
  info: 'bg-brand-card border-brand-border text-white',
  warning: 'bg-yellow-900/40 border-yellow-700/60 text-yellow-100',
  error: 'bg-red-900/40 border-red-700/60 text-red-100',
  success: 'bg-green-900/40 border-green-700/60 text-green-100',
};

export default function ToastHost() {
  const tr = useTranslations('Notifications.toast');
  const [toasts, setToasts] = useState<ToastMessage[]>([]);

  useEffect(() => {
    const listener: Listener = (toast) => {
      setToasts((prev) => {
        // Dedupe by id so the same toast doesn't stack.
        if (prev.some((t) => t.id === toast.id)) return prev;
        return [...prev, toast];
      });
      if (toast.durationMs && toast.durationMs > 0) {
        setTimeout(() => {
          setToasts((prev) => prev.filter((t) => t.id !== toast.id));
        }, toast.durationMs);
      }
    };
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }, []);

  function dismiss(id: string) {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }

  if (toasts.length === 0) return null;

  return (
    <div
      aria-live="polite"
      aria-atomic="false"
      className="fixed bottom-4 right-4 z-[300] flex flex-col gap-2 max-w-sm"
    >
      {toasts.map((t) => (
        <div
          key={t.id}
          role="status"
          className={`px-4 py-3 rounded-xl border shadow-lg backdrop-blur-sm ${KIND_STYLES[t.kind]}`}
        >
          <div className="flex items-start gap-3">
            <div className="flex-1 min-w-0">
              <p className="font-semibold text-sm">{t.title}</p>
              {t.description && (
                <p className="text-xs mt-0.5 opacity-90 leading-snug">{t.description}</p>
              )}
            </div>
            <button
              onClick={() => dismiss(t.id)}
              aria-label={tr('dismiss')}
              className="shrink-0 opacity-70 hover:opacity-100 transition-opacity"
            >
              <svg
                width="14"
                height="14"
                viewBox="0 0 16 16"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
              >
                <line x1="2" y1="2" x2="14" y2="14" />
                <line x1="14" y1="2" x2="2" y2="14" />
              </svg>
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}
