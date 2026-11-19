import { getUserFriendlyError } from '@/lib/errorHandling';
import {
  validateCreateInvoiceParams,
  assertSufficientNativeBalance,
} from '@/lib/contracts';
import { Account, Keypair } from '@stellar/stellar-sdk';

/**
 * The old buildCreateInvoiceTx/buildDepositTx/buildCommitToInvoiceTx builders
 * were deleted in the SDK migration — their write paths now go through the
 * SDK clients. This suite keeps the *validation* assertions (#687) by testing
 * the extracted, exported validators that the SDK-backed write paths rely on.
 */

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

describe('create-invoice validation (#687)', () => {
  const owner = Keypair.random().publicKey();
  const futureDueDate = Math.floor(Date.now() / 1000) + 86_400; // +1 day

  function validParams(overrides: Partial<Parameters<typeof validateCreateInvoiceParams>[0]> = {}) {
    return {
      debtor: 'Acme Corp',
      amount: 1_000_000n,
      dueDate: futureDueDate,
      description: 'Test invoice',
      ...overrides,
    };
  }

  it('rejects an empty debtor name', () => {
    expect(() => validateCreateInvoiceParams(validParams({ debtor: '' }))).toThrow(/debtor/i);
    expect(() => validateCreateInvoiceParams(validParams({ debtor: '   ' }))).toThrow(/debtor/i);
  });

  it('rejects a non-positive amount', () => {
    expect(() => validateCreateInvoiceParams(validParams({ amount: 0n }))).toThrow(
      /amount must be greater than zero/i,
    );
    expect(() => validateCreateInvoiceParams(validParams({ amount: -5n }))).toThrow(
      /amount must be greater than zero/i,
    );
  });

  it('rejects a due date in the past', () => {
    const pastDueDate = Math.floor(Date.now() / 1000) - 3_600; // -1 hour
    expect(() => validateCreateInvoiceParams(validParams({ dueDate: pastDueDate }))).toThrow(
      /due date must be in the future/i,
    );
  });

  it('accepts valid parameters', () => {
    expect(() => validateCreateInvoiceParams(validParams())).not.toThrow();
  });
});

describe('pre-write native balance guard', () => {
  it('rejects deposit before simulation when the native balance is insufficient', () => {
    const owner = Keypair.random().publicKey();
    const account = new Account(owner, '0');
    Object.assign(account, {
      balances: [{ asset_type: 'native', balance: '0.0000001' }],
    });
    expect(() => assertSufficientNativeBalance(account as never)).toThrow(/insufficient balance/i);
  });

  it('rejects commit-to-invoice before simulation when the wallet cannot cover the fee', () => {
    const owner = Keypair.random().publicKey();
    const account = new Account(owner, '0');
    Object.assign(account, {
      balances: [{ asset_type: 'native', balance: '0.0000000' }],
    });
    expect(() => assertSufficientNativeBalance(account as never)).toThrow(/insufficient balance/i);
  });
});
