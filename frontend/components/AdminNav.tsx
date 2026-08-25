'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { useState } from 'react';
import { useTranslations } from 'next-intl';

const adminLinks = [
  {
    href: '/admin/dashboard',
    labelKey: 'dashboard',
    icon: 'M3 13h8V3H3v10zm0 8h8v-6H3v6zm10 0h8V11h-8v10zm0-18v6h8V3h-8z',
  },
  {
    href: '/admin/invoices',
    labelKey: 'invoices',
    icon: 'M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8l-6-6zM6 4h7v5h5v11H6V4z',
  },
  {
    href: '/admin/yield',
    labelKey: 'yield',
    icon: 'M13 7h-2v4H7v2h4v4h2v-4h4v-2h-4V7zm-1-5C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 18c-4.41 0-8-3.59-8-8s3.59-8 8-8 8 3.59 8 8-3.59 8-8 8z',
  },
  {
    href: '/admin/defaults',
    labelKey: 'defaults',
    icon: 'M1 21h22L12 2 1 21zm12-3h-2v-2h2v2zm0-4h-2v-4h2v4z',
  },
  {
    href: '/admin/analytics',
    labelKey: 'analytics',
    icon: 'M3 13h2v8H3v-8zm4-4h2v12H7V9zm4-4h2v16h-2V5zm4 6h2v10h-2V11zm4 3h2v7h-2v-7z',
  },
  {
    href: '/admin/revenue',
    labelKey: 'revenue',
    icon: 'M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1.41 16.09V20h-2.67v-1.93c-1.71-.36-3.16-1.46-3.27-3.4h1.96c.1 1.05.82 1.87 2.65 1.87 1.96 0 2.4-.98 2.4-1.59 0-.83-.44-1.61-2.67-2.14-2.48-.6-4.18-1.62-4.18-3.67 0-1.72 1.39-2.84 3.11-3.21V4h2.67v1.95c1.86.45 2.79 1.86 2.85 3.39H14.3c-.05-1.11-.64-1.87-2.22-1.87-1.5 0-2.4.68-2.4 1.64 0 .84.65 1.39 2.67 1.92 2.56.67 4.18 1.56 4.18 3.66 0 1.86-1.47 2.91-3.12 3.3z',
  },
  {
    href: '/admin/monitoring',
    labelKey: 'monitoring',
    icon: 'M21 3H3C1.89 3 1 3.89 1 5v14c0 1.11.89 2 2 2h18c1.11 0 2-.89 2-2V5c0-1.11-.89-2-2-2zm0 16H3V5h18v14zM11 7h2v6h-2V7zm0 8h2v2h-2v-2z',
  },
  {
    href: '/admin/kyc',
    labelKey: 'kyc',
    icon: 'M12 1L3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4zm0 4l5 2.18V11c0 3.61-2.44 6.99-5 8.13C9.44 17.99 7 14.61 7 11V7.18L12 5z',
  },
  {
    href: '/admin/compliance',
    labelKey: 'compliance',
    icon: 'M12 1L3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4zm-1 6h2v2h-2V7zm0 4h2v6h-2v-6z',
  },
  {
    href: '/admin/exchange-rates',
    labelKey: 'exchangeRates',
    icon: 'M11.8 10.9c-2.27-.59-3-1.2-3-2.15 0-1.09 1.01-1.85 2.7-1.85 1.78 0 2.44.85 2.5 2.1h2.21c-.07-1.72-1.12-3.3-3.21-3.81V3h-3v2.16c-1.94.42-3.5 1.68-3.5 3.61 0 2.31 1.91 3.46 4.7 4.13 2.5.6 3 1.48 3 2.41 0 .69-.49 1.79-2.7 1.79-2.06 0-2.87-.92-2.98-2.1h-2.2c.12 2.19 1.76 3.42 3.68 3.83V21h3v-2.15c1.95-.37 3.5-1.5 3.5-3.55 0-2.84-2.43-3.81-4.7-4.4z',
  },
  {
    href: '/admin/aging',
    labelKey: 'aging',
    icon: 'M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm0 16H5V5h14v14zM7 10h10v2H7v-2zm0 4h6v2H7v-2zm0-8h10v2H7V6z',
  },
  {
    href: '/admin/oracles',
    labelKey: 'oracles',
    icon: 'M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 2c1.1 0 2 .9 2 2s-.9 2-2 2-2-.9-2-2 .9-2 2-2zM6 8c1.1 0 2 .9 2 2s-.9 2-2 2-2-.9-2-2 .9-2 2-2zm12 0c1.1 0 2 .9 2 2s-.9 2-2 2-2-.9-2-2 .9-2 2-2zm-6 4c1.1 0 2 .9 2 2s-.9 2-2 2-2-.9-2-2 .9-2 2-2zm-6 4c1.1 0 2 .9 2 2s-.9 2-2 2-2-.9-2-2 .9-2 2-2zm12 0c1.1 0 2 .9 2 2s-.9 2-2 2-2-.9-2-2 .9-2 2-2z',
  },
  {
    href: '/admin/attestors',
    labelKey: 'attestors',
    icon: 'M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z',
  },
  {
    href: '/admin/roles',
    label: 'Roles & Multisig',
    icon: 'M12 1L3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4zm0 4a3 3 0 110 6 3 3 0 010-6zm0 13.2c-2.5 0-4.71-1.28-6-3.22.03-2 4-3.1 6-3.1s5.97 1.1 6 3.1c-1.29 1.94-3.5 3.22-6 3.22z',
  },
  {
    href: '/admin/governance',
    labelKey: 'governance',
    icon: 'M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-1 17.93c-3.95-.49-7-3.85-7-7.93 0-.62.08-1.21.21-1.79L9 15v1c0 1.1.9 2 2 2v1.93zm6.9-2.54c-.26-.81-1-1.39-1.9-1.39h-1v-3c0-.55-.45-1-1-1H8v-2h2c.55 0 1-.45 1-1V7h2c1.1 0 2-.9 2-2v-.41c2.93 1.19 5 4.06 5 7.41 0 2.08-.8 3.97-2.1 5.39z',
  },
];

export default function AdminNav() {
  const path = usePathname();
  const [open, setOpen] = useState(false);
  const t = useTranslations('Admin.nav');

  return (
    <>
      {/* Hamburger — mobile only (< 768px) */}
      <button
        className="md:hidden fixed top-[72px] left-4 z-50 p-2 rounded-lg bg-brand-card border border-brand-border text-brand-muted hover:text-white"
        onClick={() => setOpen((o) => !o)}
        aria-label={t('toggleMenu')}
      >
        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d={open ? 'M6 18L18 6M6 6l12 12' : 'M4 6h16M4 12h16M4 18h16'}
          />
        </svg>
      </button>

      {/* Mobile backdrop */}
      {open && (
        <div
          className="md:hidden fixed inset-0 z-30 bg-black/50"
          style={{ top: 64 }}
          onClick={() => setOpen(false)}
        />
      )}

      {/*
       * Sidebar:
       *  < 768px  — hidden by default, shown as full-width (w-64) when hamburger is open
       *  768–1024px — always visible, icon-only (w-16)
       *  ≥ 1024px  — always visible, full labels (w-64)
       */}
      <nav
        className={`fixed left-0 top-16 z-40 h-[calc(100vh-64px)] bg-brand-card border-r border-brand-border overflow-y-auto flex-col gap-2
          md:flex md:w-16 md:p-2 md:items-center
          lg:w-64 lg:p-4 lg:items-stretch
          ${open ? 'flex w-64 p-4' : 'hidden'}`}
      >
        <div className="hidden lg:block mb-6 px-4">
          <h2 className="text-xs font-bold uppercase tracking-widest text-brand-muted">
            {t('panel')}
          </h2>
        </div>

        {adminLinks.map((link) => {
          const isActive =
            path === link.href || (link.href === '/admin/dashboard' && path === '/admin');

          return (
            <Link
              key={link.href}
              href={link.href}
              title={t(link.labelKey)}
              onClick={() => setOpen(false)}
              className={`flex items-center gap-3 px-4 py-3 rounded-xl text-sm font-medium transition-all duration-200
                md:justify-center md:px-2
                lg:justify-start lg:px-4
                ${
                  isActive
                    ? 'bg-brand-gold/10 text-brand-gold shadow-inner'
                    : 'text-brand-muted hover:text-white hover:bg-brand-dark/50'
                }`}
            >
              <svg
                className={`w-5 h-5 flex-shrink-0 ${isActive ? 'text-brand-gold' : 'text-brand-muted'}`}
                fill="currentColor"
                viewBox="0 0 24 24"
              >
                <path d={link.icon} />
              </svg>
              <span className="md:hidden lg:block">{t(link.labelKey)}</span>
            </Link>
          );
        })}
      </nav>
    </>
  );
}
