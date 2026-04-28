import type { Metadata, Viewport } from 'next';
import './globals.css';
import Navbar from '@/components/Navbar';
import ThemeProvider from '@/components/ThemeProvider';
import { Toaster } from 'react-hot-toast';

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
  const locale = await getLocale();
  const messages = await getMessages();

  return (
    <html lang={locale}>
      <head>
        <link rel="preconnect" href="https://fonts.googleapis.com" />
        <link rel="preconnect" href="https://fonts.gstatic.com" crossOrigin="anonymous" />
        <link
          href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap"
          rel="stylesheet"
        />
      </head>
      <body className="min-h-screen bg-[var(--bg)] text-[var(--text-primary)]">
        <NextIntlClientProvider messages={messages}>
          <ThemeProvider>
            <Navbar />
            <main>{children}</main>
            <Toaster position="top-right" duration={5000} />
          </ThemeProvider>
        </NextIntlClientProvider>
      </body>
    </html>
  );
}
