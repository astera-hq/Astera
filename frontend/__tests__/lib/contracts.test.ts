import { getUserFriendlyError } from '@/lib/errorHandling';

describe('contract error handling', () => {
  it('maps wallet rejection to user-facing message', () => {
    expect(getUserFriendlyError(new Error('USER_DECLINED_ACCESS'))).toBe(
      'Transaction cancelled by user',
    );
  });

  it('maps RPC timeout to network error message', () => {
    expect(getUserFriendlyError(new Error('Request timeout while calling RPC'))).toBe(
      'Network error, please try again',
    );
  });

  it('maps insufficient balance errors', () => {
    expect(getUserFriendlyError(new Error('InsufficientFunds: not enough balance'))).toBe(
      'Insufficient balance for this transaction',
    );
  });

  it('maps paused contract errors', () => {
    expect(getUserFriendlyError(new Error('ContractPaused'))).toBe('Protocol is currently paused');
  });

  it('maps already initialized errors', () => {
    expect(getUserFriendlyError(new Error('already initialized'))).toBe(
      'Contract is already initialized',
    );
  });
});
