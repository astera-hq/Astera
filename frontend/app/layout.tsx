import type { Metadata, Viewport } from 'next';
import './globals.css';
import Navbar from '@/components/Navbar';
import ThemeProvider from '@/components/ThemeProvider';
import SWRProvider from '@/components/SWRProvider';
import { ClientShell } from '@/components/ClientShell';
import { Toaster } from 'react-hot-toast';
import { assertEnvValid, getAppUrl } from '@/lib/env';

export const viewport: Viewport = {
  width: 'device-width',
  initialScale: 1,
  maximumScale: 5,
};

// #765: base URL used to resolve relative OG/Twitter image paths into the
// absolute URLs social platforms require.
const APP_URL = getAppUrl();

const SITE_TITLE = 'Astera — Real World Assets on Stellar';
const SITE_DESCRIPTION =
  'Tokenize invoices. Fund real businesses. Earn on-chain yield. Built on Stellar Soroban.';

export const metadata: Metadata = {
  metadataBase: new URL(APP_URL),
  title: SITE_TITLE,
  description: SITE_DESCRIPTION,
  openGraph: {
    type: 'website',
    title: SITE_TITLE,
    description: SITE_DESCRIPTION,
    siteName: 'Astera',
    images: [{ url: '/og-image.png', width: 1200, height: 630, alt: SITE_TITLE }],
  },
  twitter: {
    card: 'summary_large_image',
    title: SITE_TITLE,
    description: SITE_DESCRIPTION,
    images: ['/og-image.png'],
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
    <html lang={locale} suppressHydrationWarning>
      <head>
        <script
          dangerouslySetInnerHTML={{
            __html: `(function(){var t=localStorage.getItem('astera-theme');var m=window.matchMedia('(prefers-color-scheme: light)').matches;if(t==='light'||(t===null&&m))document.documentElement.classList.add('light');})()`,
          }}
        />
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
          <SWRProvider>
            <ThemeProvider>
              <ClientShell>
                <Navbar />
                <main id="main-content" role="main">
                  {children}
                </main>
                <Toaster position="top-right" toastOptions={{ duration: 5000 }} />
              </ClientShell>
            </ThemeProvider>
          </SWRProvider>
        </NextIntlClientProvider>
      </body>
    </html>
  );
}
