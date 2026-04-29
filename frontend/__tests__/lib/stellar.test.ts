type Deferred<T> = {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason?: unknown) => void;
};

function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });

  return { promise, resolve, reject };
}

async function flushMicrotasks() {
  for (let i = 0; i < 8; i++) {
    await Promise.resolve();
  }
}

function mockStellarSdk(overrides: Record<string, unknown> = {}) {
  jest.doMock(
    '@stellar/stellar-sdk',
    () => ({
      Networks: { TESTNET: 'testnet-passphrase' },
      BASE_FEE: '100',
      Contract: jest.fn(),
      rpc: {
        Server: jest.fn(() => ({
          getHealth: jest.fn(),
        })),
      },
      scValToNative: jest.fn((value: unknown) => value),
      nativeToScVal: jest.fn((value: unknown) => value),
      Address: jest.fn(),
      xdr: {},
      TransactionBuilder: {
        fromXDR: jest.fn(() => ({
          hash: () => new Uint8Array([1, 2, 3]),
        })),
      },
      ...overrides,
    }),
    { virtual: true },
  );
}

describe('stellar RPC rate limiting', () => {
  beforeEach(() => {
    jest.resetModules();
    jest.useFakeTimers();
    jest.setSystemTime(new Date('2026-04-29T00:00:00Z'));
    mockStellarSdk();
  });

  afterEach(() => {
    jest.useRealTimers();
  });

  it('queues simultaneous reads beyond the concurrency limit without dropping them', async () => {
    const { readContract } = await import('../../lib/stellar');
    const blockers = Array.from({ length: 7 }, () => deferred<number>());
    let active = 0;
    let maxActive = 0;
    let started = 0;

    const calls = blockers.map((blocker) =>
      readContract(async () => {
        started++;
        active++;
        maxActive = Math.max(maxActive, active);
        try {
          return await blocker.promise;
        } finally {
          active--;
        }
      }),
    );

    await flushMicrotasks();

    expect(started).toBe(5);
    expect(maxActive).toBe(5);

    blockers[0].resolve(0);
    await flushMicrotasks();

    expect(started).toBe(6);

    blockers.slice(1).forEach((blocker, index) => blocker.resolve(index + 1));
    await expect(Promise.all(calls)).resolves.toEqual([0, 1, 2, 3, 4, 5, 6]);
  });

  it('limits burst reads to ten starts per second', async () => {
    const { readContract } = await import('../../lib/stellar');
    let started = 0;

    const calls = Array.from({ length: 12 }, (_, index) =>
      readContract(async () => {
        started++;
        return index;
      }),
    );

    await flushMicrotasks();

    expect(started).toBe(10);

    jest.advanceTimersByTime(999);
    await flushMicrotasks();
    expect(started).toBe(10);

    jest.advanceTimersByTime(1);
    await flushMicrotasks();

    expect(started).toBe(12);
    await expect(Promise.all(calls)).resolves.toEqual(
      Array.from({ length: 12 }, (_, index) => index),
    );
  });

  it('frees read capacity after a rejected call', async () => {
    const { readContract } = await import('../../lib/stellar');
    const blockers = Array.from({ length: 6 }, () => deferred<number>());
    let started = 0;

    const calls = blockers.map((blocker, index) =>
      readContract(async () => {
        started++;
        if (index === 0) {
          await blocker.promise;
          throw new Error('read failed');
        }
        return blocker.promise;
      }),
    );

    await flushMicrotasks();
    expect(started).toBe(5);

    blockers[0].resolve(0);
    await expect(calls[0]).rejects.toThrow('read failed');
    await flushMicrotasks();

    expect(started).toBe(6);

    blockers.slice(1).forEach((blocker, index) => blocker.resolve(index + 1));
    await expect(Promise.all(calls.slice(1))).resolves.toEqual([1, 2, 3, 4, 5]);
  });
});

describe('stellar transaction submission guard', () => {
  beforeEach(() => {
    jest.resetModules();
    jest.useFakeTimers();
    jest.setSystemTime(new Date('2026-04-29T00:00:00Z'));
  });

  afterEach(() => {
    jest.useRealTimers();
    jest.dontMock('@stellar/stellar-sdk');
  });

  it('rejects duplicate in-flight submissions and clears the guard when finished', async () => {
    const sendTransaction = jest.fn();
    const getTransaction = jest.fn().mockResolvedValue({ status: 'SUCCESS' });
    const sendBlocker = deferred<{ status: string; hash: string }>();

    sendTransaction.mockReturnValue(sendBlocker.promise);

    mockStellarSdk({
      rpc: {
        Server: jest.fn(() => ({
          getHealth: jest.fn(),
          sendTransaction,
          getTransaction,
        })),
      },
      TransactionBuilder: {
        fromXDR: jest.fn(() => ({
          hash: () => new Uint8Array([1, 2, 3]),
        })),
      },
    });

    const { submitTx } = await import('../../lib/stellar');

    const first = submitTx('signed-xdr');
    await expect(submitTx('signed-xdr')).rejects.toThrow('Transaction already in progress');

    await flushMicrotasks();
    expect(sendTransaction).toHaveBeenCalledTimes(1);

    sendBlocker.resolve({ status: 'PENDING', hash: 'tx-hash' });
    await expect(first).resolves.toEqual({ status: 'SUCCESS' });

    sendTransaction.mockResolvedValueOnce({ status: 'PENDING', hash: 'tx-hash-2' });
    await expect(submitTx('signed-xdr')).resolves.toEqual({ status: 'SUCCESS' });
    expect(sendTransaction).toHaveBeenCalledTimes(2);
  });
});
