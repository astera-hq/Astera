export const Errors = {
  0: { message: 'AlreadyInitialized' },
  1: { message: 'Unauthorized' },
  2: { message: 'AlreadyRegistered' },
  3: { message: 'SelfReferral' },
  4: { message: 'InvalidBps' },
  5: { message: 'ContractPaused' },
  6: { message: 'AccessControlNotConfigured' },
} as const;

export type ReferralErrorCode = keyof typeof Errors;
export type ReferralErrorMessage = (typeof Errors)[ReferralErrorCode]['message'];

export interface ReferralStats {
  referrer: string;
  referral_count: number;
}

export interface LeaderboardEntry {
  referrer: string;
  referral_count: number;
}
