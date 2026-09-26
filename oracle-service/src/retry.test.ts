import { describe, it, beforeEach, afterEach, mock } from 'node:test';
import assert from 'node:assert/strict';
import { isRetriableError, retryWithBackoff } from './retry.ts';

describe('isRetriableError', () => {
  it('treats unknown errors as retriable', () => {
    assert.equal(isRetriableError(new Error('network timeout')), true);
    assert.equal(isRetriableError('temporary blip'), true);
  });

  it('rejects permanent failure patterns', () => {
    assert.equal(isRetriableError(new Error('Simulation failed: foo')), false);
    assert.equal(isRetriableError(new Error('Transaction failed on-chain')), false);
    assert.equal(isRetriableError(new Error('Transaction failed: insufficient')), false);
    assert.equal(isRetriableError(new Error('Document verification failed: hash')), false);
  });
});

describe('retryWithBackoff', () => {
  let randomStub: ReturnType<typeof mock.method>;
  let warnStub: ReturnType<typeof mock.method>;

  beforeEach(() => {
    // Deterministic jitter: always pick delay 0 so tests are fast.
    randomStub = mock.method(Math, 'random', () => 0);
    warnStub = mock.method(console, 'warn', () => {});
  });

  afterEach(() => {
    randomStub.mock.restore();
    warnStub.mock.restore();
  });

  it('returns on first success', async () => {
    let calls = 0;
    const result = await retryWithBackoff(async () => {
      calls += 1;
      return 'ok';
    }, 'success-path', { maxAttempts: 3, baseDelayMs: 1, maxDelayMs: 1 });
    assert.equal(result, 'ok');
    assert.equal(calls, 1);
  });

  it('retries retriable errors then succeeds', async () => {
    let calls = 0;
    const result = await retryWithBackoff(
      async () => {
        calls += 1;
        if (calls < 3) throw new Error('rpc unavailable');
        return 42;
      },
      'flaky',
      { maxAttempts: 3, baseDelayMs: 1, maxDelayMs: 1 },
    );
    assert.equal(result, 42);
    assert.equal(calls, 3);
    assert.ok(warnStub.mock.callCount() >= 2);
  });

  it('does not retry non-retriable errors', async () => {
    let calls = 0;
    await assert.rejects(
      () =>
        retryWithBackoff(
          async () => {
            calls += 1;
            throw new Error('Simulation failed');
          },
          'permanent',
          { maxAttempts: 5, baseDelayMs: 1, maxDelayMs: 1 },
        ),
      /Simulation failed/,
    );
    assert.equal(calls, 1);
  });

  it('throws last error after exhausting attempts', async () => {
    let calls = 0;
    await assert.rejects(
      () =>
        retryWithBackoff(
          async () => {
            calls += 1;
            throw new Error(`boom-${calls}`);
          },
          'exhausted',
          { maxAttempts: 3, baseDelayMs: 1, maxDelayMs: 1 },
        ),
      /boom-3/,
    );
    assert.equal(calls, 3);
  });
});
