// #861: mirrors contracts/oracle_registry/src/lib.rs's `OracleRegistryError`
// #[contracterror] enum so JS consumers can decode/report the same codes.
export const Errors = {
  0: { message: 'AlreadyInitialized' },
  1: { message: 'NotInitialized' },
  2: { message: 'Unauthorized' },
  3: { message: 'ContractPaused' },
  4: { message: 'InvalidAmount' },
  5: { message: 'InsufficientStake' },
  6: { message: 'AlreadyRegistered' },
  7: { message: 'NotRegistered' },
  8: { message: 'DeregisterHasPendingVotes' },
  9: { message: 'DeregisterCooldownActive' },
  10: { message: 'InvalidBps' },
  11: { message: 'NoActiveOracles' },
  12: { message: 'RoundAlreadyOpen' },
  13: { message: 'RoundNotFound' },
  14: { message: 'RoundNotOpen' },
  15: { message: 'RoundExpired' },
  16: { message: 'RoundNotExpired' },
  17: { message: 'AlreadyVoted' },
  18: { message: 'InvoiceContractNotSet' },
  19: { message: 'InvoiceCallFailed' },
  20: { message: 'InvalidConfig' },
} as const;

export type OracleRegistryErrorCode = keyof typeof Errors;
export type OracleRegistryErrorMessage = (typeof Errors)[OracleRegistryErrorCode]['message'];
