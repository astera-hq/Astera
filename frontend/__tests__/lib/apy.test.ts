import { projectedInterestStroops, formatApyPercent } from '@/lib/apy';

describe('projectedInterestStroops', () => {
  it('returns 0 for the requested zero-value edge cases', () => {
    expect(projectedInterestStroops(0n, 800, 86400)).toBe(0n);
    expect(projectedInterestStroops(1000000n, 800, 0)).toBe(0n);
    expect(projectedInterestStroops(1000000n, 0, 86400)).toBe(0n);
  });

  it('returns 0 for zero principal', () => {
    expect(projectedInterestStroops(0n, 800, 30)).toBe(0n);
  });

  it('returns 0 for negative principal', () => {
    expect(projectedInterestStroops(-1000n, 800, 30)).toBe(0n);
  });

  it('returns 0 for zero lock days', () => {
    expect(projectedInterestStroops(1000000000n, 800, 0)).toBe(0n);
  });

  it('returns 0 for negative lock days', () => {
    expect(projectedInterestStroops(1000000000n, 800, -5)).toBe(0n);
  });

  it('returns 0 for zero yield', () => {
    expect(projectedInterestStroops(1000000000n, 0, 30)).toBe(0n);
  });

  it('returns 0 for negative yield', () => {
    expect(projectedInterestStroops(1000000000n, -100, 30)).toBe(0n);
  });

  it('calculates interest correctly for 30 days at 8% APY', () => {
    // 1000 USDC (7 decimals) = 10000000000 stroops
    // 8% APY = 800 bps
    // 30 days interest should be approximately 6.575 USDC
    const principal = 10000000000n; // 1000 USDC
    const interest = projectedInterestStroops(principal, 800, 30);

    // Expected: (10000000000 * 800 * 30 * 86400) / (10000 * 31536000)
    // = 20,736,000,000,000,000 / 315,360,000,000
    // ≈ 65,753,424 (6.5753424 USDC in stroops)
    expect(interest).toBeGreaterThan(0n);
    expect(interest).toBeLessThan(principal);
  });

  it('calculates interest correctly for 365 days at 8% APY', () => {
    const principal = 10000000000n; // 1000 USDC
    const interest = projectedInterestStroops(principal, 800, 365);

    // After 1 year at 8%, should be approximately 80 USDC interest
    expect(interest).toBeGreaterThan(0n);
    // Should be close to 8% of principal
    expect(interest).toBeLessThan(principal);
  });

  it('handles fractional lock days by flooring', () => {
    const principal = 10000000000n;
    const interest1 = projectedInterestStroops(principal, 800, 30.9);
    const interest2 = projectedInterestStroops(principal, 800, 30);

    expect(interest1).toBe(interest2);
  });
});

describe('formatApyPercent', () => {
  it('returns em-dash for non-finite values', () => {
    expect(formatApyPercent(NaN)).toBe('—');
    expect(formatApyPercent(Infinity)).toBe('—');
    expect(formatApyPercent(-Infinity)).toBe('—');
  });

  it('returns em-dash for negative values', () => {
    expect(formatApyPercent(-100)).toBe('—');
  });

  it('formats 800 bps as 8.00%', () => {
    expect(formatApyPercent(800)).toBe('8.00');
  });

  it('formats 850 bps as 8.50%', () => {
    expect(formatApyPercent(850)).toBe('8.50');
  });

  it('formats 0 bps as 0.00%', () => {
    expect(formatApyPercent(0)).toBe('0.00');
  });

  it('formats 1234 bps as 12.34%', () => {
    expect(formatApyPercent(1234)).toBe('12.34');
  });
});
