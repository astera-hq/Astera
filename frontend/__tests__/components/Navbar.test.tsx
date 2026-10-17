import React from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import '@testing-library/jest-dom';
import { setStoredLocale } from '@/lib/i18n';

// Mock usePathname and useRouter from next/navigation
const mockPathname = jest.fn();
const mockRefresh = jest.fn();
jest.mock('next/navigation', () => ({
  usePathname: () => mockPathname(),
  useRouter: () => ({ refresh: mockRefresh }),
}));

jest.mock('@/lib/i18n', () => ({
  setStoredLocale: jest.fn(),
}));

let currentLocale = 'en';
jest.mock('next-intl', () => ({
  useLocale: () => currentLocale,
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
jest.mock('@/components/NetworkIndicator', () => ({
  __esModule: true,
  default: () => <div data-testid="stub-network-indicator" />,
}));
jest.mock('@/components/NetworkMismatchBanner', () => ({
  __esModule: true,
  default: () => <div data-testid="stub-network-mismatch-banner" />,
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

  describe('LanguageSelector (#1529)', () => {
    it('renders EN, FR, and ES language selector buttons', () => {
      mockPathname.mockReturnValue('/');
      render(<Navbar />);
      const enButtons = screen.getAllByRole('button', { name: 'English' });
      const frButtons = screen.getAllByRole('button', { name: 'Français' });
      const esButtons = screen.getAllByRole('button', { name: 'Español' });
      expect(enButtons.length).toBeGreaterThan(0);
      expect(frButtons.length).toBeGreaterThan(0);
      expect(esButtons.length).toBeGreaterThan(0);
      expect(esButtons[0]).toHaveAttribute('aria-pressed', 'false');
    });

    it('sets stored locale to es and refreshes router on Spanish button click', () => {
      mockPathname.mockReturnValue('/');
      render(<Navbar />);
      const esButtons = screen.getAllByRole('button', { name: 'Español' });
      fireEvent.click(esButtons[0]);
      expect(setStoredLocale).toHaveBeenCalledWith('es');
      expect(mockRefresh).toHaveBeenCalled();
    });

    it('applies active styling to the ES button when current locale is es', () => {
      currentLocale = 'es';
      mockPathname.mockReturnValue('/');
      render(<Navbar />);
      const esButtons = screen.getAllByRole('button', { name: 'Español' });
      expect(esButtons[0]).toHaveAttribute('aria-pressed', 'true');
      expect(esButtons[0].className).toContain('bg-brand-gold text-brand-dark');
      currentLocale = 'en';
    });
  });
});
