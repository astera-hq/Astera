// #1043: mirrors contracts/arbitration/src/lib.rs's `ArbitrationError`
// #[contracterror] enum so JS consumers can decode/report the same codes.
//
// Hand-authored (not run through the real ABI codegen in this environment —
// no `stellar` CLI wasm build was performed here). Once the contract is
// built for real, regenerate via `./scripts/gen-bindings.sh arbitration` and
// diff against this file rather than trusting it blindly.
export const Errors = {
  0: { message: 'AlreadyInitialized' },
  1: { message: 'NotInitialized' },
  2: { message: 'Unauthorized' },
  3: { message: 'ContractPaused' },
  4: { message: 'InvalidAmount' },
  5: { message: 'InsufficientStake' },
  6: { message: 'AlreadyRegistered' },
  7: { message: 'NotRegistered' },
  8: { message: 'DeregisterHasPendingCases' },
  9: { message: 'DeregisterCooldownActive' },
  10: { message: 'InvalidConfig' },
  11: { message: 'InvoiceContractNotSet' },
  12: { message: 'InvoiceCallFailed' },
  13: { message: 'CaseNotFound' },
  14: { message: 'InvalidCaseStatus' },
  15: { message: 'WindowNotClosed' },
  16: { message: 'WindowClosed' },
  17: { message: 'InvalidParty' },
  18: { message: 'NotEnoughActiveJurors' },
  19: { message: 'NotSelectedJuror' },
  20: { message: 'AlreadyCommitted' },
  21: { message: 'NotCommitted' },
  22: { message: 'AlreadyRevealed' },
  23: { message: 'RevealMismatch' },
  24: { message: 'RetriesNotExhausted' },
  25: { message: 'CaseNotEscalated' },
  26: { message: 'RetriesExhausted' },
} as const;

export type ArbitrationErrorCode = keyof typeof Errors;
export type ArbitrationErrorMessage = (typeof Errors)[ArbitrationErrorCode]['message'];
