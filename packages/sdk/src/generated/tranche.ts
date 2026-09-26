export const Errors = {
  0: { message: 'AlreadyInitialized' },
  1: { message: 'Unauthorized' },
  2: { message: 'PoolNotFound' },
  3: { message: 'InvalidAmount' },
  4: { message: 'InsufficientBalance' },
  5: { message: 'AdvanceRateExceeded' },
  10: { message: 'ReentrancyDetected' },
} as const;

export type TrancheErrorCode = keyof typeof Errors;
export type TrancheErrorMessage = (typeof Errors)[TrancheErrorCode]['message'];

export enum TrancheClass {
  Senior = 0,
  Junior = 1,
}

export interface TrancheConfig {
  senior_target_yield_bps: number;
  senior_advance_rate_bps: number;
  junior_first_loss_bps: number;
}

export interface TrancheAccounting {
  deposited: bigint;
  available: bigint;
  deployed: bigint;
  earned: bigint;
  losses: bigint;
}

export interface TranchePool {
  senior: TrancheAccounting;
  junior: TrancheAccounting;
  config: TrancheConfig;
}

export interface InvoiceTrancheExposure {
  invoice_id: number;
  senior_principal: bigint;
  junior_principal: bigint;
  senior_repaid: bigint;
  junior_repaid: bigint;
  senior_loss: bigint;
  junior_loss: bigint;
}

export interface WaterfallSimulation {
  senior_amount: bigint;
  junior_amount: bigint;
  senior_cap: bigint;
  elapsed_yield: bigint;
}
