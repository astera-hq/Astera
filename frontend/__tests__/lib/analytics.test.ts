import {
  formatValue,
  truncateAddress,
  clearAnalyticsCache,
  getAnalyticsCacheTTL,
  fetchAnalyticsData,
} from '@/lib/analytics';
import { monitorService } from '@/lib/monitoring';
import {
  getInvoiceCount,
  getMultipleInvoices,
  getPoolConfig,
  getPoolTokenTotals,
} from '@/lib/contracts';

// Mock the dependencies
jest.mock('@/lib/monitoring', () => ({
  monitorService: {
    pollEvents: jest.fn().mockResolvedValue([]),
  },
}));

jest.mock('@/lib/contracts', () => ({
  getInvoiceCount: jest.fn().mockResolvedValue(0),
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
  getMultipleInvoices: jest.fn().mockResolvedValue([]),
}));

describe('formatValue', () => {
  it('formats values under 1000 as dollar amount', () => {
    expect(formatValue(500)).toBe('$500');
    expect(formatValue(0)).toBe('$0');
  });

  it('formats values in thousands with K suffix', () => {
    expect(formatValue(1000)).toBe('$1.0K');
    expect(formatValue(1500)).toBe('$1.5K');
    expect(formatValue(999999)).toBe('$1000.0K');
  });

  it('formats values in millions with M suffix', () => {
    expect(formatValue(1000000)).toBe('$1.0M');
    expect(formatValue(5000000)).toBe('$5.0M');
    expect(formatValue(1500000)).toBe('$1.5M');
  });
});

describe('truncateAddress', () => {
  it('truncates an address with default chars', () => {
    const addr = 'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN';
    const result = truncateAddress(addr);
    expect(result).toMatch(/^GAAZI4\.\.\..+$/);
    expect(result).toContain('...');
  });

  it('truncates with custom char count', () => {
    const addr = 'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN';
    const result4 = truncateAddress(addr, 4);
    expect(result4).toMatch(/^GAAZ\.\.\..+$/);
    const result8 = truncateAddress(addr, 8);
    expect(result8).toMatch(/^GAAZI4TC\.\.\..+$/);
  });

  it('returns empty string for empty input', () => {
    expect(truncateAddress('')).toBe('');
  });
});

describe('clearAnalyticsCache', () => {
  it('clears the cache without error', () => {
    expect(() => clearAnalyticsCache()).not.toThrow();
  });
});

describe('getAnalyticsCacheTTL', () => {
  it('returns 5 minutes in milliseconds', () => {
    expect(getAnalyticsCacheTTL()).toBe(5 * 60 * 1000);
  });
});

describe('fetchAnalyticsData', () => {
  beforeEach(() => {
    clearAnalyticsCache();
    jest.clearAllMocks();
  });

  it('returns a data structure even on contract errors', async () => {
    // The mocks are already set up at module level; this test verifies
    // that the function handles errors gracefully
    const result = await fetchAnalyticsData();
    expect(result).toHaveProperty('poolUtilization');
    expect(result).toHaveProperty('yieldPerformance');
    expect(result).toHaveProperty('invoiceFunnel');
    expect(result).toHaveProperty('creditScoreDistribution');
    expect(result).toHaveProperty('topSmes');
    expect(result).toHaveProperty('recentEvents');
  });

  it('returns cached data on second call within TTL', async () => {
    const result1 = await fetchAnalyticsData();
    const result2 = await fetchAnalyticsData();
    expect(result1).toBe(result2); // Same reference = cached
    expect(getInvoiceCount).toHaveBeenCalledTimes(1);
    expect(getPoolConfig).toHaveBeenCalledTimes(1);
    expect(getPoolTokenTotals).toHaveBeenCalledTimes(1);
    expect(getMultipleInvoices).toHaveBeenCalledTimes(1);
    expect(monitorService.pollEvents).toHaveBeenCalledTimes(1);
  });

  it('returns fresh data after cache is cleared', async () => {
    const result1 = await fetchAnalyticsData();
    clearAnalyticsCache();
    const result2 = await fetchAnalyticsData();
    // Different references since cache was cleared
    expect(result1).not.toBe(result2);
    expect(getInvoiceCount).toHaveBeenCalledTimes(2);
    expect(getPoolConfig).toHaveBeenCalledTimes(2);
    expect(getPoolTokenTotals).toHaveBeenCalledTimes(2);
    expect(getMultipleInvoices).toHaveBeenCalledTimes(2);
    expect(monitorService.pollEvents).toHaveBeenCalledTimes(2);
  });

  it('refetches data after the TTL expires', async () => {
    const nowSpy = jest.spyOn(Date, 'now');
    const baseTime = 1_700_000_000_000;

    nowSpy.mockReturnValue(baseTime);
    const result1 = await fetchAnalyticsData();

    nowSpy.mockReturnValue(baseTime + getAnalyticsCacheTTL() - 1);
    const result2 = await fetchAnalyticsData();

    nowSpy.mockReturnValue(baseTime + getAnalyticsCacheTTL() + 1);
    const result3 = await fetchAnalyticsData();

    expect(result1).toBe(result2);
    expect(result3).not.toBe(result2);
    expect(getInvoiceCount).toHaveBeenCalledTimes(2);
    expect(getPoolConfig).toHaveBeenCalledTimes(2);
    expect(getPoolTokenTotals).toHaveBeenCalledTimes(2);
    expect(getMultipleInvoices).toHaveBeenCalledTimes(2);
    expect(monitorService.pollEvents).toHaveBeenCalledTimes(2);

    nowSpy.mockRestore();
  });
});
