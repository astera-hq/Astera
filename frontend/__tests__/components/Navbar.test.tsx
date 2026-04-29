import React from 'react';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';

// Mock usePathname from next/navigation to control the active link.
const mockPathname = jest.fn();
jest.mock('next/navigation', () => ({
  usePathname: () => mockPathname(),
  useRouter: () => ({ refresh: jest.fn() }),
}));

jest.mock('next-intl', () => ({
  useLocale: () => 'en',
  useTranslations: () => (key: string) => {
    const map: Record<string, string> = {
      dashboard: 'Dashboard',
      invest: 'Invest',
      analytics: 'Analytics',
      portfolio: 'Portfolio',
      newInvoice: 'New Invoice',
      toggleTheme: 'Toggle Theme',
    };
    return map[key] ?? key;
  },
}));

// next/link → plain <a>.
jest.mock('next/link', () => {
  const Link = ({
    href,
    children,
    className,
  }: {
    href: string;
    children: React.ReactNode;
    className?: string;
  }) => (
    <a href={href} className={className}>
      {children}
    </a>
  );
  Link.displayName = 'Link';
  return { __esModule: true, default: Link };
});

// The navbar renders WalletConnect, NotificationBell, ThemeToggle — stub them
// so the test stays focused on navigation behavior.
jest.mock('@/components/WalletConnect', () => ({
  __esModule: true,
  default: () => <div data-testid="stub-wallet-connect" />,
}));
jest.mock('@/components/NotificationBell', () => ({
  __esModule: true,
  default: () => <div data-testid="stub-notification-bell" />,
}));
jest.mock('@/components/ThemeToggle', () => ({
  __esModule: true,
  default: () => <div data-testid="stub-theme-toggle" />,
}));

// Don't pull in Freighter/contracts via the store consumers above.
jest.mock('@/lib/contracts', () => ({}));

import Navbar from '@/components/Navbar';

const ACTIVE_CLASS = 'bg-brand-gold/10';

describe('Navbar', () => {
  it('highlights the Dashboard link when on /dashboard', () => {
    mockPathname.mockReturnValue('/dashboard');
    render(<Navbar />);
    // There are desktop and mobile renders of each link; assert at least one
    // active link is pointed at /dashboard.
    const dashboardLinks = screen.getAllByRole('link', { name: 'Dashboard' });
    expect(dashboardLinks.some((el) => el.className.includes(ACTIVE_CLASS))).toBe(true);
  });

  it('highlights the Invest link when on /invest', () => {
    mockPathname.mockReturnValue('/invest');
    render(<Navbar />);
    const investLinks = screen.getAllByRole('link', { name: 'Invest' });
    expect(investLinks.some((el) => el.className.includes(ACTIVE_CLASS))).toBe(true);
    // And the Dashboard link is NOT active.
    const dashboardLinks = screen.getAllByRole('link', { name: 'Dashboard' });
    expect(dashboardLinks.every((el) => !el.className.includes(ACTIVE_CLASS))).toBe(true);
  });

  it('renders all primary nav entries', () => {
    mockPathname.mockReturnValue('/');
    render(<Navbar />);
    // Each link appears in both desktop and mobile nav.
    expect(screen.getAllByRole('link', { name: 'Dashboard' }).length).toBeGreaterThan(0);
    expect(screen.getAllByRole('link', { name: 'Invest' }).length).toBeGreaterThan(0);
    expect(screen.getAllByRole('link', { name: 'Portfolio' }).length).toBeGreaterThan(0);
    expect(screen.getAllByRole('link', { name: 'New Invoice' }).length).toBeGreaterThan(0);
  });
});
