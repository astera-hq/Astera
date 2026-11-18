// #867: mirrors contracts/compliance/src/lib.rs's `ComplianceError`
// #[contracterror] enum so JS consumers can decode/report the same codes.
export const Errors = {
  0: { message: 'AlreadyInitialized' },
  1: { message: 'NotInitialized' },
  2: { message: 'Unauthorized' },
  3: { message: 'ContractPaused' },
  4: { message: 'InvalidConfig' },
  5: { message: 'ScreenerAlreadyRegistered' },
  6: { message: 'ScreenerNotRegistered' },
  7: { message: 'ScreenerTimelockActive' },
  8: { message: 'NoPendingScreener' },
  9: { message: 'InvalidStatus' },
  10: { message: 'AddressNotFound' },
  11: { message: 'ListFull' },
  12: { message: 'InvalidReason' },
} as const;

export type ComplianceErrorCode = keyof typeof Errors;
export type ComplianceErrorMessage = (typeof Errors)[ComplianceErrorCode]['message'];
