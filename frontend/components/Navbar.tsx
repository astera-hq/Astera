'use client';

import { useState, useEffect, useRef } from 'react';
import { useTranslations, useLocale } from 'next-intl';
import { useRouter, usePathname } from 'next/navigation';
import Link from 'next/link';
import { setStoredLocale } from '@/lib/i18n';
import NotificationBell from '@/components/NotificationBell';
import ThemeToggle from '@/components/ThemeToggle';
import WalletConnect from '@/components/WalletConnect';
import NetworkIndicator from '@/components/NetworkIndicator';
import NetworkMismatchBanner from '@/components/NetworkMismatchBanner';

function LanguageSelector() {
  const locale = useLocale();
  const router = useRouter();

  const handleLanguageChange = (newLocale: string) => {
    setStoredLocale(newLocale);
    router.refresh();
  };

  return (
    <div
      className="flex items-center gap-1 bg-brand-card border border-brand-border rounded-lg p-1"
      role="group"
      aria-label="Language selector"
    >
      <button
        onClick={() => handleLanguageChange('en')}
        aria-pressed={locale === 'en'}
        aria-label="English"
        className={`px-2 py-1 text-[10px] font-bold rounded transition-colors ${
          locale === 'en' ? 'bg-brand-gold text-brand-dark' : 'text-brand-muted hover:text-white'
        }`}
      >
        EN
      </button>
      <button
        onClick={() => handleLanguageChange('fr')}
        aria-pressed={locale === 'fr'}
        aria-label="Français"
        className={`px-2 py-1 text-[10px] font-bold rounded transition-colors ${
          locale === 'fr' ? 'bg-brand-gold text-brand-dark' : 'text-brand-muted hover:text-white'
        }`}
      >
        FR
      </button>
    </div>
  );
}

export default function Navbar() {
  const t = useTranslations('Navbar');
  const path = usePathname();
  const [drawerOpen, setDrawerOpen] = useState(false);

  const links = [
    { href: '/dashboard', label: t('dashboard') },
    { href: '/invest', label: t('invest') },
    { href: '/analytics', label: t('analytics') },
    { href: '/governance', label: 'Governance' },
    { href: '/portfolio', label: t('portfolio') },
    { href: '/invoice/new', label: t('newInvoice') },
  ];

  // Close drawer whenever the route changes
  const prevPath = useRef(path);
  useEffect(() => {
    if (prevPath.current !== path && drawerOpen) {
      setTimeout(() => setDrawerOpen(false), 0);
    }
    prevPath.current = path;
  }, [path, drawerOpen]);

  // Prevent background scroll while the drawer is open
  useEffect(() => {
    document.body.style.overflow = drawerOpen ? 'hidden' : '';
    return () => {
      document.body.style.overflow = '';
    };
  }, [drawerOpen]);

  // Close drawer on Escape key
  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape' && drawerOpen) setDrawerOpen(false);
    }
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [drawerOpen]);

  return (
    <>
      <NetworkMismatchBanner />
      <header className="fixed top-0 left-0 right-0 z-50 border-b border-brand-border bg-brand-dark/80 backdrop-blur-md">
        <div className="max-w-7xl mx-auto px-6 h-16 flex items-center justify-between gap-8">
          <Link href="/" className="font-bold text-xl tracking-tight">
            <span className="gradient-text">Astera</span>
          </Link>

          {/* Desktop nav */}
          <nav className="hidden md:flex items-center gap-1">
            {links.map((l) => (
              <Link
                key={l.href}
                href={l.href}
                className={`px-4 py-2 rounded-lg text-sm font-medium transition-colors ${
                  path === l.href
                    ? 'bg-brand-gold/10 text-brand-gold'
                    : 'text-brand-muted hover:text-white hover:bg-brand-card'
                }`}
              >
                {l.label}
              </Link>
            ))}
          </nav>

          {/* Desktop theme toggle + notification bell + wallet button */}
          <div className="hidden md:flex items-center gap-2">
            <LanguageSelector />
            <NetworkIndicator />
            <NotificationBell />
            <ThemeToggle />
            <WalletConnect />
          </div>

          {/* Mobile: theme toggle + hamburger */}
          <div className="md:hidden flex items-center gap-1">
            <LanguageSelector />
            <ThemeToggle />
            <button
              onClick={() => setDrawerOpen(true)}
              aria-label="Open menu"
              aria-expanded={drawerOpen}
              aria-controls="mobile-nav"
              className="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-brand-muted hover:text-white hover:bg-brand-card transition-colors"
            >
              <svg
                width="20"
                height="20"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                aria-hidden="true"
              >
                <line x1="2" y1="5" x2="18" y2="5" />
                <line x1="2" y1="10" x2="18" y2="10" />
                <line x1="2" y1="15" x2="18" y2="15" />
              </svg>
            </button>
          </div>
        </div>
      </header>

      {/*
        Backdrop — sibling of <header> so it escapes the header's stacking
        context. z-[60] sits above the header (z-50) and below the drawer.
      */}
      <div
        onClick={() => setDrawerOpen(false)}
        aria-hidden="true"
        className={`fixed inset-0 z-[60] bg-black/60 transition-opacity duration-300 md:hidden ${
          drawerOpen ? 'opacity-100 pointer-events-auto' : 'opacity-0 pointer-events-none'
        }`}
      />

      {/*
        Drawer — slides in from the right. z-[70] keeps it above the backdrop.
        Both the backdrop and drawer are hidden on md+ so desktop is unaffected.
      */}
      <aside
        id="mobile-nav"
        role="dialog"
        aria-label="Navigation menu"
        aria-modal="true"
        className={`fixed top-0 right-0 z-[70] h-full w-72 bg-brand-navy border-l border-brand-border flex flex-col transition-transform duration-300 ease-in-out md:hidden ${
          drawerOpen ? 'translate-x-0' : 'translate-x-full'
        }`}
      >
        {/* Drawer header */}
        <div className="flex items-center justify-between px-6 h-16 border-b border-brand-border shrink-0">
          <span className="font-bold text-xl gradient-text">Astera</span>
          <button
            onClick={() => setDrawerOpen(false)}
            aria-label="Close menu"
            className="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-brand-muted hover:text-white hover:bg-brand-card transition-colors"
          >
            <svg
              width="18"
              height="18"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              aria-hidden="true"
            >
              <line x1="2" y1="2" x2="16" y2="16" />
              <line x1="16" y1="2" x2="2" y2="16" />
            </svg>
          </button>
        </div>

        {/* Nav links */}
        <nav className="flex flex-col gap-1 p-4 flex-1">
          {links.map((l) => (
            <Link
              key={l.href}
              href={l.href}
              className={`px-4 py-3 rounded-xl text-sm font-medium transition-colors min-h-[44px] flex items-center ${
                path === l.href
                  ? 'bg-brand-gold/10 text-brand-gold'
                  : 'text-brand-muted hover:text-white hover:bg-brand-card'
              }`}
            >
              {l.label}
            </Link>
          ))}
        </nav>

        {/* Wallet connect + theme toggle */}
        <div className="p-6 border-t border-brand-border shrink-0 flex flex-col gap-3">
          <WalletConnect />
          <div className="flex items-center gap-2 text-sm text-brand-muted">
            <ThemeToggle />
            <span>{t('toggleTheme')}</span>
          </div>
        </div>
      </aside>
    </>
  );
}
