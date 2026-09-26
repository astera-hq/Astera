import { HttpScreenerProvider, MockScreener } from './screener';

describe('MockScreener', () => {
  it('flags a sanctioned address as blocked', async () => {
    const screener = new MockScreener();

    const result = await screener.screen('SANCTIONED_ENTITY_ALPHA');

    expect(result.status).toBe('Blocked');
    expect(result.reasonCode).toBe(9001);
    expect(result.riskTier).toBe('High');
    expect(result.matchedList).toBe('OFAC-SDN-fixture');
  });

  it('returns a clear result for a clean address without triggering review logic', async () => {
    const screener = new MockScreener();

    const result = await screener.screen('GB7V5QKQY5K4N4ZE6M7U4FZ4ZQJYQ8Z4S7J3W2A');

    expect(result.status).toBe('Cleared');
    expect(result.reasonCode).toBe(0);
    expect(result.riskTier).toBe('Low');
    expect(result.notes).toBe('No sanctions hit');
  });

  it('throws a typed validation error for an empty address', async () => {
    const screener = new MockScreener();

    await expect(screener.screen('   ')).rejects.toThrow('address is required');
  });

  it('surfaces upstream screening failures instead of silently returning clean', async () => {
    const originalFetch = global.fetch;
    const provider = new HttpScreenerProvider('https://example.invalid');
    global.fetch = jest.fn().mockRejectedValue(new Error('upstream unavailable')) as typeof fetch;

    try {
      await expect(provider.screen('SANCTIONED_ENTITY_ALPHA')).rejects.toThrow('upstream unavailable');
    } finally {
      global.fetch = originalFetch;
    }
  });
});
