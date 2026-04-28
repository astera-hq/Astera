import type { Metadata, Viewport } from 'next';
import './globals.css';
import Navbar from '@/components/Navbar';
import ThemeProvider from '@/components/ThemeProvider';
import { Toaster } from 'react-hot-toast';
import { assertEnvValid } from '@/lib/env';

export const viewport: Viewport = {
  width: 'device-width',
  initialScale: 1,
  maximumScale: 5,
};

export const metadata: Metadata = {
  title: 'Astera — Real World Assets on Stellar',
  description:
    'Tokenize invoices. Fund real businesses. Earn on-chain yield. Built on Stellar Soroban.',
  openGraph: {
    title: 'Astera',
    description: 'Invoice financing for emerging markets, powered by Stellar.',
    siteName: 'Astera',
  },
};

import { NextIntlClientProvider } from 'next-intl';
import { getMessages, getLocale } from 'next-intl/server';

export default async function RootLayout({ children }: { children: React.ReactNode }) {
  // Validate environment variables early
  assertEnvValid();

  const locale = await getLocale();
  const messages = await getMessages();

  return (
    <html lang={locale}>
      <head>
        <link rel="manifest" href="/manifest.json" />
        <meta name="theme-color" content="#ffffff" />
        <link rel="preconnect" href="https://fonts.googleapis.com" />
        <link rel="preconnect" href="https://fonts.gstatic.com" crossOrigin="anonymous" />
        <link
          href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap"
          rel="stylesheet"
        />
      </head>
      <body className="min-h-screen bg-[var(--bg)] text-[var(--text-primary)]">
        <a
          href="#main-content"
          className="sr-only focus:not-sr-only focus:fixed focus:top-4 focus:left-4 focus:z-50 focus:bg-brand-primary focus:text-white focus:p-2 focus:rounded"
        >
          Skip to main content
        </a>
        <NextIntlClientProvider messages={messages}>
          <ThemeProvider>
            <Navbar />
            <main id="main-content" role="main">
              {children}
            </main>
            <Toaster position="top-right" duration={5000} />
          </ThemeProvider>
        </NextIntlClientProvider>
      </body>
    </html>
  );
}
