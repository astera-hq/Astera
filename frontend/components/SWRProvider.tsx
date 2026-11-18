'use client';

import { SWRConfig } from 'swr';
import { swrConfig } from '@/lib/swrConfig';

// #800: applies the app-wide retry/backoff policy to every SWR hook.
export default function SWRProvider({ children }: { children: React.ReactNode }) {
  return <SWRConfig value={swrConfig}>{children}</SWRConfig>;
}
