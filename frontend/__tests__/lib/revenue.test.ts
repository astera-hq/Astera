import {
  buildMonthlySeries,
  buildWeeklySeries,
  buildFeeVsVolume,
  extractLastWithdrawal,
  countUniqueLenders,
  clearRevenueCache,
  getRevenueCacheTTL,
  fetchRevenueData,
  fetchTreasurySnapshot,
} from '@/lib/revenue';
import {
  getAcceptedTokens,
  getInvoiceCount,
  getMultipleInvoices,
  getPoolConfig,
  getPoolTokenTotals,
  getProtocolRevenue,
  getTokenBalanceOf,
  getTreasuryAddress,
} from '@/lib/contracts';

jest.mock('@/lib/contracts', () => ({
  getAcceptedTokens: jest.fn().mockResolvedValue(['USDC_TOKEN']),
  getInvoiceCount: jest.fn().mockResolvedValue(0),
  getMultipleInvoices: jest.fn().mockResolvedValue([]),
  getPoolConfig: jest.fn().mockResolvedValue({
    invoiceContract: 'TEST',
    admin: 'ADMIN',
    yieldBps: 800,
    factoringFeeBps: 250,
    compoundInterest: false,
  }),
  getPoolTokenTotals: jest.fn().mockResolvedValue({
    totalDeposited: 10000000000n,
    totalDeployed: 5000000000n,
    totalPaidOut: 3000000000n,
    totalFeeRevenue: 100000000n,
  }),
  getProtocolRevenue: jest.fn().mockResolvedValue(25000000n),
  getTokenBalanceOf: jest.fn().mockResolvedValue(50000000n),
  getTreasuryAddress: jest.fn().mockResolvedValue('TREASURY_ADDR'),
}));

jest.mock('@/lib/stellar', () => ({
  rpcGetEvents: jest.fn().mockResolvedValue({ events: [] }),
  rpcGetLatestLedger: jest.fn().mockResolvedValue({ sequence: 100000 }),
  POOL_CONTRACT_ID: 'POOL',
  scValToNative: (v: any) => v,
  stablecoinLabel: jest.fn((id: string) => (id === 'USDC_TOKEN' ? 'USDC' : id)),
}));

const NOW = new Date('2026-07-15T12:00:00Z');

describe('buildMonthlySeries', () => {
  it('returns 12 monthly buckets ending at the current month', () => {
    const series = buildMonthlySeries(1200000000n, 60000000000n, NOW);
    expect(series).toHaveLength(12);
    expect(series[11]).toMatchObject({ month: '2026-07' });
    expect(series[0]).toMatchObject({ month: '2025-08' });
  });

  it('allocates fees summing to the live cumulative total', () => {
    const cumulative = 1200000000n; // $120 in stroops
    const series = buildMonthlySeries(cumulative, 0n, NOW);
    const total = series.reduce((a, p) => a + p.fees, 0);
    expect(total).toBeCloseTo(Number(cumulative) / 10_000_000, 0);
  });

  it('grows over time (newest bucket largest)', () => {
    const series = buildMonthlySeries(1000000000n, 1000000000n, NOW);
    const fees = series.map((p) => p.fees);
    const oldest = fees[0]!;
    const newest = fees[fees.length - 1]!;
    expect(newest).toBeGreaterThan(oldest);
  });

  it('returns zero buckets for a zero total', () => {
    const series = buildMonthlySeries(0n, 0n, NOW);
    expect(series.every((p) => p.fees === 0 && p.fundedVolume === 0)).toBe(true);
  });
});

describe('buildWeeklySeries', () => {
  it('returns 8 weekly buckets with ISO week keys', () => {
    const series = buildWeeklySeries(800000000n, 40000000000n, NOW);
    expect(series).toHaveLength(8);
    for (const point of series) {
      expect(point.week).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    }
  });

  it('sums fees to the cumulative total', () => {
    const series = buildWeeklySeries(800000000n, 0n, NOW);
    const total = series.reduce((a, p) => a + p.fees, 0);
    expect(total).toBeCloseTo(80, 0); // $80
  });
});

describe('buildFeeVsVolume', () => {
  it('projects the current fee rate onto every monthly bucket', () => {
    const monthly = buildMonthlySeries(1000000000n, 50000000000n, NOW);
    const points = buildFeeVsVolume(monthly, 250);
    expect(points).toHaveLength(12);
    for (const p of points) {
      expect(p.feeRatePct).toBe(2.5);
    }
    expect(points[0]!.fundedVolume).toBe(monthly[0]!.fundedVolume);
  });
});

describe('extractLastWithdrawal', () => {
  it('returns null when there are no rev_wdraw events', () => {
    expect(extractLastWithdrawal([])).toBeNull();
    expect(
      extractLastWithdrawal([
        { ledgerCloseAt: '2026-07-01T00:00:00Z', topic: ['pool', 'funded'], value: [] },
      ]),
    ).toBeNull();
  });

  it('picks the most recent rev_wdraw event and decodes fields', () => {
    const events = [
      {
        ledgerCloseAt: '2026-06-01T00:00:00Z',
        topic: ['pool', 'rev_wdraw'],
        value: ['USDC_TOKEN', 100000000n, 'TREASURY'],
      },
      {
        ledgerCloseAt: '2026-07-10T00:00:00Z',
        topic: ['pool', 'rev_wdraw'],
        value: ['USDC_TOKEN', 200000000n, 'TREASURY'],
      },
    ];
    const withdrawal = extractLastWithdrawal(events as any);
    expect(withdrawal).not.toBeNull();
    expect(withdrawal!.amount).toBe(200000000n);
    expect(withdrawal!.token).toBe('USDC_TOKEN');
    expect(withdrawal!.treasury).toBe('TREASURY');
    expect(withdrawal!.label).toBe('USDC');
    expect(withdrawal!.at).toBe(Math.floor(new Date('2026-07-10T00:00:00Z').getTime() / 1000));
  });
});

describe('countUniqueLenders', () => {
  it('counts distinct investors across deposit events', () => {
    const events = [
      {
        ledgerCloseAt: '',
        topic: ['pool', 'deposit'],
        value: ['INV_1', 'USDC_TOKEN', 100n, 100n, 0n],
      },
      {
        ledgerCloseAt: '',
        topic: ['pool', 'deposit'],
        value: ['INV_2', 'USDC_TOKEN', 200n, 200n, 0n],
      },
      {
        ledgerCloseAt: '',
        topic: ['pool', 'deposit'],
        value: ['INV_1', 'USDC_TOKEN', 50n, 50n, 0n],
      },
    ];
    expect(countUniqueLenders(events as any)).toBe(2);
  });

  it('ignores non-deposit events', () => {
    const events = [
      { ledgerCloseAt: '', topic: ['pool', 'funded'], value: [1n, 'SME', 1000n, 'USDC_TOKEN', 0n] },
    ];
    expect(countUniqueLenders(events as any)).toBe(0);
  });
});

describe('clearRevenueCache / getRevenueCacheTTL', () => {
  it('clears the cache without error', () => {
    expect(() => clearRevenueCache()).not.toThrow();
  });

  it('returns 5 minutes in milliseconds', () => {
    expect(getRevenueCacheTTL()).toBe(5 * 60 * 1000);
  });
});

describe('fetchRevenueData', () => {
  beforeEach(() => {
    clearRevenueCache();
    jest.clearAllMocks();
    (getAcceptedTokens as jest.Mock).mockResolvedValue(['USDC_TOKEN']);
    (getPoolConfig as jest.Mock).mockResolvedValue({
      invoiceContract: 'TEST',
      admin: 'ADMIN',
      yieldBps: 800,
      factoringFeeBps: 250,
      compoundInterest: false,
    });
    (getPoolTokenTotals as jest.Mock).mockResolvedValue({
      totalDeposited: 10000000000n,
      totalDeployed: 5000000000n,
      totalPaidOut: 3000000000n,
      totalFeeRevenue: 100000000n,
    });
    (getProtocolRevenue as jest.Mock).mockResolvedValue(25000000n);
    (getTokenBalanceOf as jest.Mock).mockResolvedValue(50000000n);
    (getTreasuryAddress as jest.Mock).mockResolvedValue('TREASURY_ADDR');
  });

  it('returns a complete data structure', async () => {
    const result = await fetchRevenueData();
    expect(result).toHaveProperty('feeRateBps');
    expect(result).toHaveProperty('cumulativeFees');
    expect(result).toHaveProperty('pendingFees');
    expect(result).toHaveProperty('averageFeePerInvoice');
    expect(result).toHaveProperty('perToken');
    expect(result).toHaveProperty('monthly');
    expect(result).toHaveProperty('weekly');
    expect(result).toHaveProperty('feeVsVolume');
    expect(result).toHaveProperty('treasuryAddress');
    expect(result).toHaveProperty('lastWithdrawal');
    expect(result).toHaveProperty('volume');
    expect(result.feeRateBps).toBe(250);
    expect(result.cumulativeFees).toBe(100000000n);
    expect(result.pendingFees).toBe(25000000n);
    expect(result.treasuryAddress).toBe('TREASURY_ADDR');
    expect(result.monthly).toHaveLength(12);
    expect(result.perToken).toHaveLength(1);
    expect(result.perToken[0]).toMatchObject({
      label: 'USDC',
      cumulativeFees: 100000000n,
      pendingFees: 25000000n,
      treasuryBalance: 50000000n,
    });
  });

  it('caches the result within the TTL', async () => {
    const result1 = await fetchRevenueData();
    const result2 = await fetchRevenueData();
    expect(result1).toBe(result2); // Same reference = cached
    expect(getPoolConfig).toHaveBeenCalledTimes(1);
    expect(getPoolTokenTotals).toHaveBeenCalledTimes(1);
  });

  it('refetches after the cache is cleared', async () => {
    const result1 = await fetchRevenueData();
    clearRevenueCache();
    const result2 = await fetchRevenueData();
    expect(result1).not.toBe(result2);
    expect(getPoolConfig).toHaveBeenCalledTimes(2);
  });

  it('handles a missing treasury gracefully', async () => {
    (getTreasuryAddress as jest.Mock).mockResolvedValue(null);
    const result = await fetchRevenueData();
    expect(result.treasuryAddress).toBeNull();
    expect(result.perToken[0]!.treasuryBalance).toBe(0n);
    expect(getTokenBalanceOf).not.toHaveBeenCalled();
  });

  it('aggregates funded volume from invoices', async () => {
    (getInvoiceCount as jest.Mock).mockResolvedValue(1);
    (getMultipleInvoices as jest.Mock).mockResolvedValue([
      {
        id: 1,
        owner: 'BORROWER_1',
        amount: 1000000000n,
        status: 'Funded',
        fundedAt: Math.floor(Date.now() / 1000),
      },
    ]);
    const result = await fetchRevenueData();
    expect(result.volume.totalFunded).toBe(1000000000n);
    expect(result.volume.uniqueBorrowers).toBe(1);
    expect(result.volume.activeToday).toBe(1000000000n);
  });
});

describe('fetchTreasurySnapshot', () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it('returns balances for accepted tokens', async () => {
    const snapshot = await fetchTreasurySnapshot();
    expect(snapshot.treasuryAddress).toBe('TREASURY_ADDR');
    expect(snapshot.balances).toHaveLength(1);
    expect(snapshot.balances[0]).toMatchObject({
      token: 'USDC_TOKEN',
      label: 'USDC',
      balance: 50000000n,
    });
    expect(getTokenBalanceOf).toHaveBeenCalledWith('USDC_TOKEN', 'TREASURY_ADDR');
  });

  it('is not cached (returns fresh data on each call)', async () => {
    await fetchTreasurySnapshot();
    await fetchTreasurySnapshot();
    // Uncached: token-balance reads happen per call.
    expect(getTokenBalanceOf).toHaveBeenCalledTimes(2);
  });
});
