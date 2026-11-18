import type { Metadata } from 'next';

// #783: /analytics is a client component (page.tsx), which can't itself
// export `metadata` — a sibling server-component layout is the standard
// Next.js App Router way to attach a canonical tag to a client page.
export const metadata: Metadata = {
  title: 'Analytics — Astera',
  description: 'Live pool utilization, yield performance, and invoice funnel analytics.',
  alternates: { canonical: '/analytics' },
};

export default function AnalyticsLayout({ children }: { children: React.ReactNode }) {
  return children;
}
