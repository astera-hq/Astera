/**
 * Simple yield projection aligned with the pool contract’s linear accrual:
 * interest = principal × yield_bps × elapsed_secs / (10_000 × 31_536_000)
 * @see contracts/pool — BPS_DENOM, SECS_PER_YEAR, repay_invoice interest calc
 */

const BPS_DENOM = 10_000n;
const SECS_PER_YEAR = 31_536_000n;
const SECS_PER_DAY = 86_400n;

/**
 * @param principalStroops — deposit in stroops (7 decimals, same as on-chain)
 * @param yieldBps — basis points from pool `get_config` (e.g. 800 = 8% annualized)
 * @param lockDays — horizon in whole days
 */
export function projectedInterestStroops(
  principalStroops: bigint,
  yieldBps: number,
  lockDays: number,
): bigint {
  if (principalStroops <= 0n || lockDays <= 0 || !Number.isFinite(lockDays)) {
    return 0n;
  }
  if (yieldBps <= 0 || !Number.isFinite(yieldBps)) {
    return 0n;
  }

  const elapsedSecs = BigInt(Math.floor(lockDays)) * SECS_PER_DAY;
  return (
    (principalStroops * BigInt(Math.floor(yieldBps)) * elapsedSecs) / (BPS_DENOM * SECS_PER_YEAR)
  );
}

export function formatApyPercent(yieldBps: number): string {
  if (!Number.isFinite(yieldBps) || yieldBps < 0) return '—';
  return (yieldBps / 100).toFixed(2);
}
