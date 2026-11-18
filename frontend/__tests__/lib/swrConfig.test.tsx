import React from 'react';
import { renderHook, waitFor } from '@testing-library/react';
import useSWR, { SWRConfig } from 'swr';
import { swrConfig, MAX_RETRY_COUNT, RETRY_BASE_DELAY_MS } from '../../lib/swrConfig';

function wrapper({ children }: { children: React.ReactNode }) {
  return <SWRConfig value={swrConfig}>{children}</SWRConfig>;
}

// onErrorRetry doesn't read its `config` argument — a loosely-typed stand-in
// avoids fabricating every required (but unused) PublicConfiguration field.
const unusedConfig = swrConfig as Parameters<NonNullable<typeof swrConfig.onErrorRetry>>[2];

describe('swrConfig.onErrorRetry (#800)', () => {
  beforeEach(() => {
    jest.useFakeTimers();
  });

  afterEach(() => {
    jest.useRealTimers();
  });

  it('schedules a retry with exponential backoff for a transient error', () => {
    const revalidate = jest.fn();

    swrConfig.onErrorRetry!(
      new Error('network error, please retry'),
      'some-key',
      unusedConfig,
      revalidate,
      { retryCount: 0, dedupe: false },
    );
    expect(revalidate).not.toHaveBeenCalled();

    jest.advanceTimersByTime(RETRY_BASE_DELAY_MS - 1);
    expect(revalidate).not.toHaveBeenCalled();

    jest.advanceTimersByTime(1);
    expect(revalidate).toHaveBeenCalledWith({ retryCount: 0 });
  });

  it('doubles the delay on each successive retry', () => {
    const revalidate = jest.fn();

    swrConfig.onErrorRetry!(new Error('timeout'), 'key', unusedConfig, revalidate, {
      retryCount: 2,
      dedupe: false,
    });

    jest.advanceTimersByTime(RETRY_BASE_DELAY_MS * 2 ** 2 - 1);
    expect(revalidate).not.toHaveBeenCalled();

    jest.advanceTimersByTime(1);
    expect(revalidate).toHaveBeenCalledWith({ retryCount: 2 });
  });

  it('stops retrying once errorRetryCount is reached', () => {
    const revalidate = jest.fn();

    swrConfig.onErrorRetry!(new Error('timeout'), 'key', unusedConfig, revalidate, {
      retryCount: MAX_RETRY_COUNT,
      dedupe: false,
    });

    jest.advanceTimersByTime(60_000);
    expect(revalidate).not.toHaveBeenCalled();
  });

  it('does not retry a permanent "not found" error', () => {
    const revalidate = jest.fn();

    swrConfig.onErrorRetry!(new Error('invoice not found'), 'key', unusedConfig, revalidate, {
      retryCount: 0,
      dedupe: false,
    });

    jest.advanceTimersByTime(60_000);
    expect(revalidate).not.toHaveBeenCalled();
  });
});

describe('SWR + swrConfig integration (#800)', () => {
  // Real timers here — exercising the actual RETRY_BASE_DELAY_MS backoff
  // through SWR's own retry engine end-to-end (not just our callback logic).
  it('mock RPC fails twice then succeeds — data loads correctly', async () => {
    let attempts = 0;
    const fetcher = jest.fn(async () => {
      attempts++;
      if (attempts < 3) {
        throw new Error('network error, please retry');
      }
      return 'ok';
    });

    const { result } = renderHook(() => useSWR('real-timer-retry-key', fetcher), { wrapper });

    await waitFor(() => expect(result.current.data).toBe('ok'), { timeout: 10_000 });
    expect(fetcher).toHaveBeenCalledTimes(3);
    expect(result.current.error).toBeUndefined();
  }, 15_000);
});
