import React from 'react';
import { render, screen, act } from '@testing-library/react';
import '@testing-library/jest-dom';
import type { NotificationAlert } from '@/lib/notifications';

// Capture the subscriber callback so tests can push alerts at will.
let subscriber: ((alert: NotificationAlert) => void) | null = null;
const unsubscribe = jest.fn();

jest.mock('@/lib/notifications', () => ({
  notificationService: {
    subscribe: (cb: (alert: NotificationAlert) => void) => {
      subscriber = cb;
      return unsubscribe;
    },
  },
}));

// All alert types are in-app enabled for these tests.
jest.mock('@/lib/notification-preferences', () => ({
  isInAppEnabled: () => true,
}));

jest.mock('next-intl', () => ({
  useTranslations: () => (key: string, vars?: Record<string, unknown>) =>
    vars ? `${key}:${JSON.stringify(vars)}` : key,
}));

import NotificationBell from '@/components/NotificationBell';

function makeAlert(i: number): NotificationAlert {
  return {
    id: `alert-${i}-${Math.random()}`,
    type: 'INVOICE_DUE' as NotificationAlert['type'],
    priority: 'MEDIUM' as NotificationAlert['priority'],
    message: `Message ${i}`,
    timestamp: Date.now(),
  };
}

function pushAlerts(count: number) {
  act(() => {
    for (let i = 0; i < count; i++) {
      subscriber?.(makeAlert(i));
    }
  });
}

// The unread badge is the small red circle rendered inside the bell button.
function getBadge(container: HTMLElement): HTMLElement | null {
  return container.querySelector('span.bg-red-500');
}

describe('NotificationBell unread badge (#693)', () => {
  beforeEach(() => {
    subscriber = null;
    unsubscribe.mockClear();
  });

  it('hides the badge when the unread count is 0', () => {
    const { container } = render(<NotificationBell />);
    expect(getBadge(container)).toBeNull();
  });

  it('shows the exact count for values between 1 and 9', () => {
    const { container } = render(<NotificationBell />);
    pushAlerts(1);
    expect(getBadge(container)).toHaveTextContent('1');

    pushAlerts(2); // total 3
    expect(getBadge(container)).toHaveTextContent('3');

    pushAlerts(6); // total 9
    expect(getBadge(container)).toHaveTextContent('9');
  });

  it('shows "9+" when the count exceeds 9', () => {
    const { container } = render(<NotificationBell />);
    pushAlerts(10);
    expect(getBadge(container)).toHaveTextContent('9+');
  });
});
