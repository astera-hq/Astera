/**
 * #863: client-side port of the pool contract's kinked interest-rate curve
 * (`compute_current_rate` in contracts/pool/src/lib.rs).
 *
 * Kept deliberately pure and dependency-free so chart components and the
 * admin curve editor can render/preview the curve without one RPC round-trip
 * per point. The math must stay in lockstep with the on-chain version:
 *
 *   util <= optimal: rate = base + util * slope1 / optimal
 *   util >  optimal: rate = base + slope1 + (util - optimal) * slope2 / (10_000 - optimal)
 *
 * clamped to `maxRateBps`. Integer truncation mirrors the Rust integer math.
 */

import type { RateModelConfig } from './types';

export const BPS_DENOM = 10_000;

export function computeCurrentRateBps(utilizationBps: number, config: RateModelConfig): number {
  const util = Math.min(Math.max(0, Math.floor(utilizationBps)), BPS_DENOM);
  const optimal = config.optimalUtilizationBps;
  if (optimal <= 0 || optimal > BPS_DENOM) return config.maxRateBps;

  const below = Math.min(util, optimal);
  let rate = config.baseRateBps + Math.floor((below * config.slope1Bps) / optimal);

  if (util > optimal) {
    const span = BPS_DENOM - optimal;
    rate += Math.floor(((util - optimal) * config.slope2Bps) / span);
  }

  return Math.min(rate, config.maxRateBps);
}

/** Sample the curve at evenly-spaced utilization points for charting. */
export function sampleRateCurve(
  config: RateModelConfig,
  points = 101,
): { utilizationBps: number; rateBps: number }[] {
  const samples: { utilizationBps: number; rateBps: number }[] = [];
  const step = BPS_DENOM / (points - 1);
  for (let i = 0; i < points; i++) {
    const utilizationBps = Math.round(i * step);
    samples.push({ utilizationBps, rateBps: computeCurrentRateBps(utilizationBps, config) });
  }
  return samples;
}

/** Client-side mirror of the contract's `validate_rate_model_config`. */
export function validateRateModelConfig(
  config: RateModelConfig,
  t?: (key: string, values?: Record<string, unknown>) => string,
): string | null {
  const MAX_RATE_BPS_CAP = 5_000;
  if (
    !Number.isInteger(config.optimalUtilizationBps) ||
    config.optimalUtilizationBps <= 0 ||
    config.optimalUtilizationBps > BPS_DENOM
  ) {
    return t
      ? t('optUtilRange')
      : 'Optimal utilization must be between 1 and 10000 bps (0.01%–100%).';
  }
  if (
    !Number.isInteger(config.maxRateBps) ||
    config.maxRateBps <= 0 ||
    config.maxRateBps > MAX_RATE_BPS_CAP
  ) {
    return t
      ? t('maxRateRange', { max: MAX_RATE_BPS_CAP.toString() })
      : `Max rate must be between 1 and ${MAX_RATE_BPS_CAP} bps.`;
  }
  if (!Number.isInteger(config.baseRateBps) || config.baseRateBps < 0) {
    return t ? t('baseRateNonNegative') : 'Base rate must be a non-negative integer (bps).';
  }
  if (config.baseRateBps > config.maxRateBps) {
    return t ? t('baseRateExceedsMax') : 'Base rate cannot exceed the max rate.';
  }
  for (const [name, slope] of [
    ['Slope 1', config.slope1Bps],
    ['Slope 2', config.slope2Bps],
  ] as const) {
    if (!Number.isInteger(slope) || slope < 0 || slope > MAX_RATE_BPS_CAP) {
      return t
        ? t('slopeRange', { name, max: MAX_RATE_BPS_CAP.toString() })
        : `${name} must be between 0 and ${MAX_RATE_BPS_CAP} bps.`;
    }
  }
  return null;
}
