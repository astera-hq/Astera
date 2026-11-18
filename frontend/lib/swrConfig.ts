import type { SWRConfiguration } from 'swr';
import { CACHE_CONFIG } from './cache';
import { isNotFoundError } from './errorHandling';

// #800: Soroban RPC / Horizon occasionally return transient errors (rate
// limiting, momentary unavailability, network blips). Without retry logic a
// single transient failure put the whole page into a permanent error state
// requiring a manual reload. This config makes SWR retry transient failures
// with exponential backoff, while permanent ("not found") failures fail fast.
export const MAX_RETRY_COUNT = 3;
export const RETRY_BASE_DELAY_MS = 1000;

export const swrConfig: SWRConfiguration = {
  provider: () => new Map(),
  ...CACHE_CONFIG.invoice,
  errorRetryCount: MAX_RETRY_COUNT,
  errorRetryInterval: RETRY_BASE_DELAY_MS,
  onErrorRetry: (error, _key, _config, revalidate, { retryCount }) => {
    if (isNotFoundError(error)) return;
    if (retryCount >= MAX_RETRY_COUNT) return;
    setTimeout(() => revalidate({ retryCount }), RETRY_BASE_DELAY_MS * 2 ** retryCount);
  },
};
