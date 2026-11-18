export const Errors = {
  0: { message: 'AlreadyInitialized' },
  1: { message: 'NotInitialized' },
  2: { message: 'Unauthorized' },
  3: { message: 'ContractPaused' },
  4: { message: 'InvalidAmount' },
  5: { message: 'InvalidCoverageBps' },
  6: { message: 'InvalidPremiumConfig' },
  7: { message: 'InvalidMinCoverageRatio' },
  8: { message: 'CoverageRatioFloorBreached' },
  9: { message: 'AlreadyCovered' },
  10: { message: 'NoCoverageFound' },
  11: { message: 'InvoiceNotDefaulted' },
  12: { message: 'NoShortfall' },
  13: { message: 'AlreadyClaimed' },
  14: { message: 'AmountOverflow' },
  15: { message: 'FundedInvoiceNotFound' },
  16: { message: 'PoolCallFailed' },
  17: { message: 'InsufficientReserves' },
} as const;

export type InsuranceErrorCode = keyof typeof Errors;
export type InsuranceErrorMessage = (typeof Errors)[InsuranceErrorCode]['message'];

/** A single credit-score band and its risk multiplier (bps, 10_000 = 1.0x). */
export interface InsuranceRiskTier {
  min_score: number;
  max_score: number;
  risk_multiplier_bps: number;
}

export interface PremiumConfig {
  base_rate_bps: number;
  tenor_bps_per_day: number;
  risk_tiers: InsuranceRiskTier[];
  default_risk_multiplier_bps: number;
  min_premium_bps: number;
  max_premium_bps: number;
  default_coverage_bps: number;
}

/** Per-token reserve solvency state. */
export interface ReserveFund {
  total_reserves: bigint;
  total_premiums_collected: bigint;
  total_claims_paid: bigint;
  total_covered_exposure: bigint;
  coverage_ratio_bps: number;
  min_coverage_ratio_bps: number;
}

export interface CoverageRecord {
  invoice_id: bigint;
  token: string;
  principal: bigint;
  premium_paid: bigint;
  coverage_bps: number;
  purchased_at: bigint;
  claimed: boolean;
}

/** A single historical claim entry recording the payout details. */
export interface ClaimHistoryItem {
  invoice_id: bigint;
  token: string;
  payout: bigint;
  shortfalls: bigint;
  timestamp: bigint;
}

/** Health status of a token's reserve, returned by `check_reserve_health`. */
export interface ReserveHealth {
  token: string;
  total_reserves: bigint;
  coverage_ratio_bps: number;
  min_reserve_amount: bigint;
  is_healthy: boolean;
  needs_top_up: boolean;
}
