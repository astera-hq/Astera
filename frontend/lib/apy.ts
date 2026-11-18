/** Astera/frontend/lib/apy.ts
 * Yield projection aligned with the pool contract's linear accrual:
 * interest = principal × yield_bps × elapsed_secs / (10_000 × 31_536_000)
 * @see contracts/pool — BPS_DENOM, SECS_PER_YEAR, repay_invoice interest calc
 *
 * Extended in #289 to support scenario modelling with utilization and
 * default-rate assumptions.
 */

const BPS_DENOM = 10_000n;
const SECS_PER_YEAR = 31_536_000n;
const SECS_PER_DAY = 86_400n;
const PRECISION_SCALE = 1_000_000n; // 6 decimal places for float precision

// ─── Utility for safe bigint arithmetic ──────────────────────────────────────

/**
 * Multiply a bigint by a float (0–1) while preserving precision.
 * Scales the float to maintain precision without requiring Number conversion.
 * @param value bigint value to multiply
 * @param factor float factor (0–1) to apply
 * @returns result of value × factor as bigint
 */
function multiplyBigintByFactor(value: bigint, factor: number): bigint {
  if (factor <= 0) return 0n;
  if (factor >= 1) return value;
  // Scale factor to 6 decimal places to maintain precision
  const scaledFactor = BigInt(Math.round(factor * Number(PRECISION_SCALE)));
  return (value * scaledFactor) / PRECISION_SCALE;
}

// ─── Core yield math (unchanged) ─────────────────────────────────────────────

/**
 * @param principalStroops  deposit in stroops (7 decimals, same as on-chain)
 * @param yieldBps          basis points from pool `get_config` (e.g. 800 = 8% p.a.)
 * @param lockDays          horizon in whole days
 */
export function projectedInterestStroops(
  principalStroops: bigint,
  yieldBps: number,
  lockDays: number,
): bigint {
  if (principalStroops <= 0n || lockDays <= 0 || !Number.isFinite(lockDays)) return 0n;
  if (yieldBps <= 0 || !Number.isFinite(yieldBps)) return 0n;

  const elapsedSecs = BigInt(Math.floor(lockDays)) * SECS_PER_DAY;
  return (
    (principalStroops * BigInt(Math.floor(yieldBps)) * elapsedSecs) / (BPS_DENOM * SECS_PER_YEAR)
  );
}

export function formatApyPercent(yieldBps: number): string {
  if (!Number.isFinite(yieldBps) || yieldBps < 0) return '—';
  return (yieldBps / 100).toFixed(2);
}

// ─── Scenario modelling (#289) ────────────────────────────────────────────────

export interface ScenarioInputs {
  /** Principal in stroops */
  principalStroops: bigint;
  /** Annualised yield in basis points from the pool contract */
  yieldBps: number;
  /** Investment horizon in whole days */
  lockDays: number;
  /** Fraction of pool actually deployed to invoices, 0–1 (e.g. 0.8 = 80%) */
  utilizationRate: number;
  /** Expected fraction of deployed capital that defaults, 0–1 (e.g. 0.02 = 2%) */
  defaultRate: number;
  /** Recovery fraction on defaulted collateral, 0–1 (e.g. 0.7 = 70%) */
  collateralRecoveryRate: number;
}

export interface ScenarioResult {
  /** Label shown in the UI */
  label: 'Best Case' | 'Base Case' | 'Worst Case';
  /** Gross interest earned on the deployed portion, stroops */
  grossYieldStroops: bigint;
  /** Net loss from defaults after collateral recovery, stroops (always ≥ 0) */
  defaultLossStroops: bigint;
  /** Net return = grossYield − defaultLoss, may be negative */
  netReturnStroops: bigint;
  /** Net return as a percentage of principal (annualised), e.g. 6.2 */
  netReturnPct: number;
  /** Effective utilization used for this scenario */
  utilizationRate: number;
  /** Effective default rate used for this scenario */
  defaultRate: number;
}

/**
 * Compute the outcome for one scenario given explicit utilization + default rates.
 *
 * Formula:
 *   gross_yield   = projectedInterest(principal, yieldBps, lockDays) × utilization
 *   default_loss  = principal × deployed_fraction × default_rate × (1 − recovery)
 *   net_return    = gross_yield − default_loss
 *   net_pct       = net_return / principal / (lockDays / 365) × 100
 */
export function computeScenario(
  inputs: ScenarioInputs,
  label: ScenarioResult['label'],
  overrideUtilization: number,
  overrideDefaultRate: number,
): ScenarioResult {
  const { principalStroops, yieldBps, lockDays, collateralRecoveryRate } = inputs;

  const util = Math.max(0, Math.min(1, overrideUtilization));
  const defRate = Math.max(0, Math.min(1, overrideDefaultRate));
  const recovery = Math.max(0, Math.min(1, collateralRecoveryRate));

  // Gross yield: only the deployed (utilised) portion earns interest
  // Use bigint arithmetic to preserve precision for large amounts
  const fullInterest = projectedInterestStroops(principalStroops, yieldBps, lockDays);
  const grossYieldStroops = multiplyBigintByFactor(fullInterest, util);

  // Default loss: defaults occur on deployed capital; recovery reduces the loss
  // Calculate deployed amount and default loss while keeping precision
  const deployedStroops = multiplyBigintByFactor(principalStroops, util);
  const rawLoss = multiplyBigintByFactor(deployedStroops, defRate);
  const defaultLossStroops = rawLoss > 0n ? multiplyBigintByFactor(rawLoss, 1 - recovery) : 0n;

  const netReturnStroops = grossYieldStroops - defaultLossStroops;

  // Annualised net return % — only convert to Number for final percentage calculation
  const yearFraction = lockDays / 365;
  const netReturnPct =
    principalStroops > 0n && yearFraction > 0
      ? (Number(netReturnStroops) / Number(principalStroops) / yearFraction) * 100
      : 0;

  return {
    label,
    grossYieldStroops,
    defaultLossStroops,
    netReturnStroops,
    netReturnPct,
    utilizationRate: util,
    defaultRate: defRate,
  };
}

/**
 * Generate all three scenarios from a shared set of base inputs.
 *
 * The base `utilizationRate` and `defaultRate` on `inputs` are used as
 * the Base Case. Best and Worst cases apply fixed deltas:
 *
 *   Best:  utilization + 0.2 (capped at 1.0), defaultRate = 0
 *   Base:  utilization as-is, defaultRate as-is
 *   Worst: utilization − 0.2 (floored at 0), defaultRate × 4 (capped at 0.20)
 */
export function computeAllScenarios(
  inputs: ScenarioInputs,
): [ScenarioResult, ScenarioResult, ScenarioResult] {
  const bestUtil = Math.min(1, inputs.utilizationRate + 0.2);
  const worstUtil = Math.max(0, inputs.utilizationRate - 0.2);
  const worstDefault = Math.min(0.2, inputs.defaultRate * 4);

  return [
    computeScenario(inputs, 'Best Case', bestUtil, 0),
    computeScenario(inputs, 'Base Case', inputs.utilizationRate, inputs.defaultRate),
    computeScenario(inputs, 'Worst Case', worstUtil, worstDefault),
  ];
}

/**
 * Breakeven utilization: the minimum pool utilization needed for net return
 * to exceed a reference savings rate (e.g. a 5% savings account).
 *
 * Solves: gross_yield(u) − default_loss(u) ≥ savings_yield
 * Approximated by binary search (1 000 steps — runs in < 1 ms client-side).
 *
 * @returns utilization rate 0–1, or null if impossible even at 100% utilization
 */
export function breakevenUtilization(
  inputs: Omit<ScenarioInputs, 'utilizationRate'>,
  savingsRatePct: number,
): number | null {
  const savingsYield =
    (Number(inputs.principalStroops) * savingsRatePct) / 100 / (365 / inputs.lockDays);

  // Check if even 100% utilization clears the bar
  const atFull = computeScenario(
    { ...inputs, utilizationRate: 1 },
    'Base Case',
    1,
    inputs.defaultRate,
  );
  if (Number(atFull.netReturnStroops) < savingsYield) return null;

  // Binary search
  let lo = 0;
  let hi = 1;
  for (let i = 0; i < 1_000; i++) {
    const mid = (lo + hi) / 2;
    const result = computeScenario(
      { ...inputs, utilizationRate: mid },
      'Base Case',
      mid,
      inputs.defaultRate,
    );
    if (Number(result.netReturnStroops) >= savingsYield) {
      hi = mid;
    } else {
      lo = mid;
    }
  }
  return (lo + hi) / 2;
}
