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

jest.mock('@stellar/stellar-sdk', () => ({
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
}));

describe('stellar RPC rate limiting', () => {
  beforeEach(() => {
    jest.resetModules();
    jest.useFakeTimers();
    jest.setSystemTime(new Date('2026-04-29T00:00:00Z'));
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

    blockers[0]!.resolve(0);
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

    blockers[0]!.resolve(0);
    await expect(calls[0]!).rejects.toThrow('read failed');
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
  });

  it('rejects duplicate in-flight submissions and clears the guard when finished', async () => {
    const sendTransaction = jest.fn();
    const getTransaction = jest.fn().mockResolvedValue({ status: 'SUCCESS' });
    const sendBlocker = deferred<{ status: string; hash: string }>();

    sendTransaction.mockReturnValue(sendBlocker.promise);

    // Patch the hoisted mock before importing stellar
    const mockSdk = jest.requireMock('@stellar/stellar-sdk') as {
      rpc: { Server: jest.Mock };
      TransactionBuilder: { fromXDR: jest.Mock };
    };
    mockSdk.rpc.Server = jest.fn(() => ({
      getHealth: jest.fn(),
      sendTransaction,
      getTransaction,
    }));

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

describe('decimal-aware amount formatting (#778)', () => {
  beforeEach(() => {
    jest.resetModules();
  });

  it('formatUSDC and formatAmount(_, 7) agree — 7-decimal behavior is unchanged', async () => {
    const { formatUSDC, formatAmount } = await import('../../lib/stellar');
    expect(formatAmount(1_234_5678n, 7)).toBe(formatUSDC(1_234_5678n));
  });

  it('formatAmount uses the given decimals instead of assuming 7', async () => {
    const { formatAmount } = await import('../../lib/stellar');
    // 1_000_000 raw units at 6 decimals is 1.00, not 0.10 (which dividing by
    // 10^7 would wrongly produce).
    expect(formatAmount(1_000_000n, 6)).toBe('$1.00');
    // 1_000_000_000_000_000_000 raw units at 18 decimals is 1.00.
    expect(formatAmount(1_000_000_000_000_000_000n, 18)).toBe('$1.00');
  });

  it('fromRaw divides by the correct power of ten for the given decimals', async () => {
    const { fromRaw, fromStroops } = await import('../../lib/stellar');
    expect(fromRaw(1_000_000n, 6)).toBe(1);
    expect(fromRaw(50_000_000n, 7)).toBe(fromStroops(50_000_000n));
  });
});
