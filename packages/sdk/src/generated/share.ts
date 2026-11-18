// #1557: mirrors contracts/share/src/lib.rs's `ShareError`
// #[contracterror] enum so JS consumers can decode/report the same codes.
export const Errors = {
  1: { message: 'ContractPaused' },
  2: { message: 'AlreadyInitialized' },
  3: { message: 'NotInitialized' },
  4: { message: 'Unauthorized' },
  5: { message: 'InvalidDecimals' },
  6: { message: 'InvalidAmount' },
  7: { message: 'InsufficientBalance' },
  8: { message: 'AllowanceExceeded' },
  9: { message: 'AllowanceOverflow' },
  10: { message: 'AllowanceUnderflow' },
  11: { message: 'TotalSupplyOverflow' },
  12: { message: 'TotalSupplyUnderflow' },
} as const;

export type ShareErrorCode = keyof typeof Errors;
export type ShareErrorMessage = (typeof Errors)[ShareErrorCode]['message'];

/** Token metadata set once at `initialize`. */
export interface ShareTokenMetadata {
  name: string;
  symbol: string;
  decimals: number;
}
