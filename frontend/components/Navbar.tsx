'use client';

import { useState, useEffect } from 'react';
import Link from 'next/link';
import { useState, useEffect, useRef } from 'react';
import { usePathname } from 'next/navigation';
import WalletConnect from './WalletConnect';

const links = [
  { href: '/dashboard', label: 'Dashboard' },
  { href: '/invest', label: 'Invest' },
  { href: '/invoice/new', label: 'New Invoice' },
];

export default function Navbar() {
  const path = usePathname();
  const [drawerOpen, setDrawerOpen] = useState(false);

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

  return (
    <>
      <header className="fixed top-0 left-0 right-0 z-50 border-b border-brand-border bg-brand-dark/80 backdrop-blur-md">
        <div className="max-w-7xl mx-auto px-6 h-16 flex items-center justify-between gap-8">
          <Link href="/" className="font-bold text-xl tracking-tight">
            <span className="gradient-text">Astera</span>
          </Link>

          {/* Desktop nav — unchanged */}
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

          {/* Desktop wallet button */}
          <div className="hidden md:block">
            <WalletConnect />
          </div>

          {/* Mobile hamburger */}
          <button
            onClick={() => setDrawerOpen(true)}
            aria-label="Open menu"
            aria-expanded={drawerOpen}
            className="md:hidden p-2 rounded-lg text-brand-muted hover:text-white hover:bg-brand-card transition-colors"
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
            className="p-2 rounded-lg text-brand-muted hover:text-white hover:bg-brand-card transition-colors"
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
              className={`px-4 py-3 rounded-xl text-sm font-medium transition-colors ${
                path === l.href
                  ? 'bg-brand-gold/10 text-brand-gold'
                  : 'text-brand-muted hover:text-white hover:bg-brand-card'
              }`}
            >
              {l.label}
            </Link>
          ))}
        </nav>

        {/* Wallet connect */}
        <div className="p-6 border-t border-brand-border shrink-0">
          <WalletConnect />
        </div>
      </aside>
    </>
  );
}
