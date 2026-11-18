/**
 * #863: the client-side curve port must produce exactly the same values as
 * the on-chain `compute_current_rate` — these vectors mirror the Rust unit
 * tests in contracts/pool/tests/rate_model_tests.rs.
 */

import { computeCurrentRateBps, sampleRateCurve, validateRateModelConfig } from '@/lib/rate-model';
import type { RateModelConfig } from '@/lib/types';

const standardModel: RateModelConfig = {
  baseRateBps: 200,
  optimalUtilizationBps: 8_000,
  slope1Bps: 600,
  slope2Bps: 2_400,
  maxRateBps: 5_000,
};

describe('computeCurrentRateBps', () => {
  it('returns the base rate at 0% utilization', () => {
    expect(computeCurrentRateBps(0, standardModel)).toBe(200);
  });

  it('prorates slope1 just below the kink', () => {
    expect(computeCurrentRateBps(7_999, standardModel)).toBe(799);
  });

  it('returns base + slope1 exactly at the kink', () => {
    expect(computeCurrentRateBps(8_000, standardModel)).toBe(800);
  });

  it('adds slope2 pro-rata just above the kink', () => {
    expect(computeCurrentRateBps(8_001, standardModel)).toBe(801);
  });

  it('returns base + slope1 + slope2 at 100% utilization', () => {
    expect(computeCurrentRateBps(10_000, standardModel)).toBe(3_200);
  });

  it('clamps to the configured max rate', () => {
    const model = { ...standardModel, maxRateBps: 1_000 };
    expect(computeCurrentRateBps(10_000, model)).toBe(1_000);
    expect(computeCurrentRateBps(8_000, model)).toBe(800);
  });

  it('clamps utilization above 100%', () => {
    expect(computeCurrentRateBps(12_345, standardModel)).toBe(
      computeCurrentRateBps(10_000, standardModel),
    );
  });

  it('has no steep region when the kink is at 100%', () => {
    const model = { ...standardModel, optimalUtilizationBps: 10_000 };
    expect(computeCurrentRateBps(10_000, model)).toBe(800);
    expect(computeCurrentRateBps(5_000, model)).toBe(500);
  });

  it('is flat when both slopes are zero', () => {
    const model = { ...standardModel, slope1Bps: 0, slope2Bps: 0 };
    expect(computeCurrentRateBps(0, model)).toBe(200);
    expect(computeCurrentRateBps(10_000, model)).toBe(200);
  });

  it('is monotonically non-decreasing across the full range', () => {
    let prev = -1;
    for (let util = 0; util <= 10_000; util++) {
      const rate = computeCurrentRateBps(util, standardModel);
      expect(rate).toBeGreaterThanOrEqual(prev);
      prev = rate;
    }
  });
});

describe('sampleRateCurve', () => {
  it('returns endpoints matching the curve', () => {
    const points = sampleRateCurve(standardModel);
    expect(points[0]).toEqual({ utilizationBps: 0, rateBps: 200 });
    expect(points[points.length - 1]).toEqual({ utilizationBps: 10_000, rateBps: 3_200 });
  });
});

describe('validateRateModelConfig', () => {
  it('accepts the standard model', () => {
    expect(validateRateModelConfig(standardModel)).toBeNull();
  });

  it.each([
    [{ optimalUtilizationBps: 0 }, 'kink at 0'],
    [{ optimalUtilizationBps: 10_001 }, 'kink above 100%'],
    [{ maxRateBps: 0 }, 'zero ceiling'],
    [{ maxRateBps: 5_001 }, 'ceiling above protocol cap'],
    [{ baseRateBps: 4_000, maxRateBps: 3_000 }, 'base above ceiling'],
    [{ slope1Bps: 5_001 }, 'slope1 above cap'],
    [{ slope2Bps: 5_001 }, 'slope2 above cap'],
  ])('rejects %s', (patch, _label) => {
    expect(validateRateModelConfig({ ...standardModel, ...patch })).not.toBeNull();
  });
});
