import { isRetriableError, retryWithBackoff, RetryOptions } from './retry';

// #1181: no test coverage previously existed for retry/backoff classification
// (isRetriableError) or the retry loop itself (retryWithBackoff). These
// tests exercise both: which errors are treated as transient vs. permanent,
// and the loop's attempt-count / non-retriable short-circuit behavior.

describe('isRetriableError', () => {
  it('treats an unrecognized error message as retriable (transient) by default', () => {
    expect(isRetriableError(new Error('ECONNRESET: socket hang up'))).toBe(true);
    expect(isRetriableError(new Error('timeout of 5000ms exceeded'))).toBe(true);
    expect(isRetriableError(new Error('network error'))).toBe(true);
  });

  it('treats a plain non-Error thrown value as retriable when it does not match a pattern', () => {
    expect(isRetriableError('some string error')).toBe(true);
    expect(isRetriableError({ code: 'ETIMEDOUT' })).toBe(true);
  });

  it('classifies "Simulation failed" as non-retriable (permanent contract error)', () => {
    expect(isRetriableError(new Error('Simulation failed: contract invocation reverted'))).toBe(
      false,
    );
  });

  it('classifies "Transaction failed on-chain" as non-retriable', () => {
    expect(isRetriableError(new Error('Transaction failed on-chain: tx_failed'))).toBe(false);
  });

  it('classifies "Transaction failed:" as non-retriable', () => {
    expect(isRetriableError(new Error('Transaction failed: op_underfunded'))).toBe(false);
  });

  it('classifies "Document verification failed" as non-retriable', () => {
    expect(
      isRetriableError(new Error('Document verification failed: hash mismatch')),
    ).toBe(false);
  });

  it('matches non-retriable patterns as a substring, not just an exact message', () => {
    expect(
      isRetriableError(
        new Error('Invoice 42: Simulation failed: contract invocation reverted at frame 3'),
      ),
    ).toBe(false);
  });

  it('is case-sensitive when matching non-retriable patterns', () => {
    // A differently-cased message should NOT match one of the known
    // permanent-failure patterns, and therefore falls back to retriable.
    expect(isRetriableError(new Error('simulation failed'))).toBe(true);
  });

  it('handles a non-Error thrown value by stringifying it for pattern matching', () => {
    expect(isRetriableError('Simulation failed: bad input')).toBe(false);
  });
});

describe('retryWithBackoff', () => {
  const fastOptions: Partial<RetryOptions> = {
    maxAttempts: 3,
    baseDelayMs: 1,
    maxDelayMs: 2,
  };

  beforeEach(() => {
    jest.spyOn(console, 'warn').mockImplementation(() => {});
  });

  afterEach(() => {
    jest.restoreAllMocks();
  });

  it('returns the result on the first successful attempt without retrying', async () => {
    const fn = jest.fn().mockResolvedValue('ok');
    const result = await retryWithBackoff(fn, 'test-op', fastOptions);
    expect(result).toBe('ok');
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('retries a retriable error up to maxAttempts and eventually succeeds', async () => {
    const fn = jest
      .fn()
      .mockRejectedValueOnce(new Error('network error'))
      .mockRejectedValueOnce(new Error('network error'))
      .mockResolvedValueOnce('recovered');

    const result = await retryWithBackoff(fn, 'test-op', fastOptions);
    expect(result).toBe('recovered');
    expect(fn).toHaveBeenCalledTimes(3);
  });

  it('throws the last error once maxAttempts is exhausted on a retriable failure', async () => {
    const fn = jest.fn().mockRejectedValue(new Error('network error'));

    await expect(retryWithBackoff(fn, 'test-op', fastOptions)).rejects.toThrow('network error');
    expect(fn).toHaveBeenCalledTimes(fastOptions.maxAttempts as number);
  });

  it('does not retry a non-retriable error — fails immediately on the first attempt', async () => {
    const fn = jest.fn().mockRejectedValue(new Error('Simulation failed: bad args'));

    await expect(retryWithBackoff(fn, 'test-op', fastOptions)).rejects.toThrow(
      'Simulation failed',
    );
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('wraps a non-Error rejection in an Error before classifying and rethrowing', async () => {
    const fn = jest.fn().mockRejectedValue('Document verification failed: mismatch');

    await expect(retryWithBackoff(fn, 'test-op', fastOptions)).rejects.toThrow(
      'Document verification failed',
    );
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('respects a custom maxAttempts of 1 (no retries at all)', async () => {
    const fn = jest.fn().mockRejectedValue(new Error('network error'));

    await expect(
      retryWithBackoff(fn, 'test-op', { ...fastOptions, maxAttempts: 1 }),
    ).rejects.toThrow('network error');
    expect(fn).toHaveBeenCalledTimes(1);
  });
});
