// #864: mirrors contracts/access_control/src/lib.rs's `AccessControlError`
// #[contracterror] enum so JS consumers can decode/report the same codes.
export const Errors = {
  0: { message: 'AlreadyInitialized' },
  1: { message: 'NotInitialized' },
  2: { message: 'NotASigner' },
  3: { message: 'AlreadyApproved' },
  4: { message: 'ThresholdNotMet' },
  5: { message: 'ProposalNotFound' },
  6: { message: 'ProposalExpired' },
  7: { message: 'ProposalNotPending' },
  8: { message: 'ProposalNotApproved' },
  9: { message: 'InvalidThreshold' },
  10: { message: 'DuplicateSigner' },
  11: { message: 'RoleNotConfigured' },
  12: { message: 'SelfManagementRequiresSuperAdmin' },
  13: { message: 'InvalidExpiryWindow' },
  14: { message: 'SignerNotFound' },
  15: { message: 'NoApprovalToRevoke' },
} as const;

export type AccessControlErrorCode = keyof typeof Errors;
export type AccessControlErrorMessage = (typeof Errors)[AccessControlErrorCode]['message'];
