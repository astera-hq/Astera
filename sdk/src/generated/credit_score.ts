export const Errors = {
  1: { message: 'AlreadyInitialized' },
  2: { message: 'Unauthorized' },
  3: { message: 'ContractPaused' },
  4: { message: 'InvoiceAlreadyProcessed' },
  5: { message: 'InvalidThresholds' },
  6: { message: 'InvalidLateThreshold' },
  7: { message: 'PaymentHistoryLimitZero' },
  8: { message: 'UpgradeTimelockNotExpired' },
  9: { message: 'NoUpgradeProposed' },
  10: { message: 'InvalidUpgradeTimelock' },
  11: { message: 'InvalidWasmHash' },
  12: { message: 'AdminChangePending' },
  13: { message: 'AdminChangeTimelockNotExpired' },
  14: { message: 'NoAdminChangeProposed' },
  // #868: credit_score v2 — external attestations + dispute mechanism
  15: { message: 'AttestorNotFound' },
  16: { message: 'AttestorInactive' },
  17: { message: 'InvalidAttestorWeight' },
  18: { message: 'InvalidScoreContribution' },
  19: { message: 'InvalidAttestationExpiry' },
  20: { message: 'AttestationNotFound' },
  21: { message: 'AttestationNotActive' },
  22: { message: 'UnauthorizedDisputeCaller' },
  23: { message: 'DisputeNotFound' },
  24: { message: 'InvalidAttestationConfig' },
} as const;

export type CreditScoreErrorCode = keyof typeof Errors;
export type CreditScoreErrorMessage = (typeof Errors)[CreditScoreErrorCode]['message'];
