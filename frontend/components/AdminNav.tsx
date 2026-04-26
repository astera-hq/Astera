'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';

const adminLinks = [
  {
    href: '/admin/dashboard',
    label: 'Dashboard',
    icon: 'M3 13h8V3H3v10zm0 8h8v-6H3v6zm10 0h8V11h-8v10zm0-18v6h8V3h-8z',
  },
  {
    href: '/admin/invoices',
    label: 'Invoices',
    icon: 'M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8l-6-6zM6 4h7v5h5v11H6V4z',
  },
  {
    href: '/admin/yield',
    label: 'Yield Rate',
    icon: 'M13 7h-2v4H7v2h4v4h2v-4h4v-2h-4V7zm-1-5C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 18c-4.41 0-8-3.59-8-8s3.59-8 8-8 8 3.59 8 8-3.59 8-8 8z',
  },
  {
    href: '/admin/defaults',
    label: 'Defaults',
    icon: 'M1 21h22L12 2 1 21zm12-3h-2v-2h2v2zm0-4h-2v-4h2v4z',
  },
  {
    href: '/admin/analytics',
    label: 'Analytics',
    icon: 'M3 13h2v8H3v-8zm4-4h2v12H7V9zm4-4h2v16h-2V5zm4 6h2v10h-2V11zm4 3h2v7h-2v-7z',
  },
  {
    href: '/admin/monitoring',
    label: 'Monitoring',
    icon: 'M21 3H3C1.89 3 1 3.89 1 5v14c0 1.11.89 2 2 2h18c1.11 0 2-.89 2-2V5c0-1.11-.89-2-2-2zm0 16H3V5h18v14zM11 7h2v6h-2V7zm0 8h2v2h-2v-2z',
  },
  {
    href: '/admin/kyc',
    label: 'KYC / Whitelist',
    icon: 'M12 1L3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4zm0 4l5 2.18V11c0 3.61-2.44 6.99-5 8.13C9.44 17.99 7 14.61 7 11V7.18L12 5z',
  },
  {
    href: '/admin/exchange-rates',
    label: 'Exchange Rates',
    icon: 'M11.8 10.9c-2.27-.59-3-1.2-3-2.15 0-1.09 1.01-1.85 2.7-1.85 1.78 0 2.44.85 2.5 2.1h2.21c-.07-1.72-1.12-3.3-3.21-3.81V3h-3v2.16c-1.94.42-3.5 1.68-3.5 3.61 0 2.31 1.91 3.46 4.7 4.13 2.5.6 3 1.48 3 2.41 0 .69-.49 1.79-2.7 1.79-2.06 0-2.87-.92-2.98-2.1h-2.2c.12 2.19 1.76 3.42 3.68 3.83V21h3v-2.15c1.95-.37 3.5-1.5 3.5-3.55 0-2.84-2.43-3.81-4.7-4.4z',
  },
];

export default function AdminNav() {
  const path = usePathname();

  return (
    <nav className="w-64 bg-brand-card border-r border-brand-border h-[calc(100vh-64px)] fixed left-0 top-16 hidden lg:flex flex-col p-4 gap-2">
      <div className="mb-6 px-4">
        <h2 className="text-xs font-bold uppercase tracking-widest text-brand-muted">
          Admin Panel
        </h2>
      </div>

      {adminLinks.map((link) => {
        const isActive =
          path === link.href || (link.href === '/admin/dashboard' && path === '/admin');

        return (
          <Link
            key={link.href}
            href={link.href}
            className={`flex items-center gap-3 px-4 py-3 rounded-xl text-sm font-medium transition-all duration-200 ${
              isActive
                ? 'bg-brand-gold/10 text-brand-gold shadow-inner'
                : 'text-brand-muted hover:text-white hover:bg-brand-dark/50'
            }`}
          >
            <svg
              className={`w-5 h-5 ${isActive ? 'text-brand-gold' : 'text-brand-muted'}`}
              fill="currentColor"
              viewBox="0 0 24 24"
            >
              <path d={link.icon} />
            </svg>
            {link.label}
          </Link>
        );
      })}
    </nav>
  );
}
