'use client';

import { useCallback, useEffect, useState } from 'react';
import Link from 'next/link';

interface Props {
  walletConnected: boolean;
  invoiceCount: number;
  onDismiss: () => void;
}

type StepKey = 'connect-wallet' | 'first-invoice';

interface Step {
  key: StepKey;
  label: string;
  href: string;
}

const STEPS: Step[] = [
  { key: 'connect-wallet', label: 'Connect wallet', href: '#' },
  { key: 'first-invoice', label: 'Create your first invoice', href: '/invoice/new' },
];

const DISMISSED_KEY = 'astera_onboarding_checklist_dismissed';

function isDismissed(): boolean {
  if (typeof window === 'undefined') return true;
  return localStorage.getItem(DISMISSED_KEY) === 'true';
}

export default function SMEOnboardingChecklist({
  walletConnected,
  invoiceCount,
  onDismiss,
}: Props) {
  const [dismissed, setDismissed] = useState(true);
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    setDismissed(isDismissed());
    setMounted(true);
  }, []);

  const handleDismiss = useCallback(() => {
    if (typeof window !== 'undefined') {
      localStorage.setItem(DISMISSED_KEY, 'true');
    }
    setDismissed(true);
    onDismiss();
  }, [onDismiss]);

  if (!mounted || invoiceCount >= 1) return null;
  if (dismissed) return null;

  const completed: Record<StepKey, boolean> = {
    'connect-wallet': walletConnected,
    'first-invoice': invoiceCount >= 1,
  };

  const allDone = STEPS.every((step) => completed[step.key]);
  if (allDone) return null;

  return (
    <div className="rounded-2xl border border-brand-border bg-brand-card p-5">
      <div className="flex items-center justify-between mb-4">
        <h3 className="font-semibold text-sm">Get started on Astera</h3>
        <button
          onClick={handleDismiss}
          className="text-brand-muted hover:text-white transition-colors text-xs"
          aria-label="Dismiss checklist"
        >
          Dismiss
        </button>
      </div>
      <ul className="space-y-2">
        {STEPS.map((step) => {
          const done = completed[step.key];
          return (
            <li key={step.key}>
              {step.key === 'connect-wallet' && !walletConnected ? (
                <span className="flex items-center gap-2 text-sm text-brand-muted">
                  <span className="w-5 h-5 rounded-full border border-brand-border flex items-center justify-center text-xs">
                    {''}
                  </span>
                  {step.label}
                </span>
              ) : done ? (
                <span className="flex items-center gap-2 text-sm text-green-400">
                  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth={2}
                      d="M5 13l4 4L19 7"
                    />
                  </svg>
                  {step.label}
                </span>
              ) : (
                <Link
                  href={step.href}
                  className="flex items-center gap-2 text-sm text-brand-gold hover:underline"
                >
                  <span className="w-5 h-5 rounded-full border border-brand-gold flex items-center justify-center text-xs text-brand-gold">
                    {''}
                  </span>
                  {step.label}
                </Link>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}
