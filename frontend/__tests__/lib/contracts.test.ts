import { getUserFriendlyError } from '@/lib/errorHandling';

// Partially mock the stellar helpers so we can stub the RPC round-trips
// (account fetch + simulation) while keeping the real scVal/Address helpers.
jest.mock('@/lib/stellar', () => {
  const actual = jest.requireActual('@/lib/stellar');
  return {
    __esModule: true,
    ...actual,
    rpcExecute: jest.fn(),
    INVOICE_CONTRACT_ID: 'CAAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQC526',
    POOL_CONTRACT_ID: 'CAAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQC526',
  };
});

// Keep the real SDK (TransactionBuilder, Contract, Account, Keypair, …) but stub
// the simulation/assembly helpers so a valid build resolves to a known XDR.
jest.mock('@stellar/stellar-sdk', () => {
  const actual = jest.requireActual('@stellar/stellar-sdk');
  return {
    __esModule: true,
    ...actual,
    rpc: {
      ...actual.rpc,
      Api: { ...actual.rpc.Api, isSimulationError: jest.fn(() => false) },
      assembleTransaction: jest.fn(() => ({
        build: () => ({ toXDR: () => 'BASE64_XDR_STRING' }),
      })),
    },
  };
});

import * as stellar from '@/lib/stellar';
import { Account, Keypair } from '@stellar/stellar-sdk';
import { buildCreateInvoiceTx, buildDepositTx, buildCommitToInvoiceTx } from '@/lib/contracts';

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

describe('buildCreateInvoiceTx validation (#687)', () => {
  const owner = Keypair.random().publicKey();
  const futureDueDate = Math.floor(Date.now() / 1000) + 86_400; // +1 day
  const rpcExecute = stellar.rpcExecute as jest.Mock;

  function validParams(overrides: Partial<Parameters<typeof buildCreateInvoiceTx>[0]> = {}) {
    return {
      owner,
      debtor: 'Acme Corp',
      amount: 1_000_000n,
      dueDate: futureDueDate,
      description: 'Test invoice',
      ...overrides,
    };
  }

  beforeEach(() => {
    rpcExecute.mockReset();
  });

  it('rejects an empty debtor name', async () => {
    await expect(buildCreateInvoiceTx(validParams({ debtor: '' }))).rejects.toThrow(/debtor/i);
    await expect(buildCreateInvoiceTx(validParams({ debtor: '   ' }))).rejects.toThrow(/debtor/i);
    // Validation should short-circuit before any RPC call is made.
    expect(rpcExecute).not.toHaveBeenCalled();
  });

  it('rejects a non-positive amount', async () => {
    await expect(buildCreateInvoiceTx(validParams({ amount: 0n }))).rejects.toThrow(
      /amount must be greater than zero/i,
    );
    await expect(buildCreateInvoiceTx(validParams({ amount: -5n }))).rejects.toThrow(
      /amount must be greater than zero/i,
    );
    expect(rpcExecute).not.toHaveBeenCalled();
  });

  it('rejects a due date in the past', async () => {
    const pastDueDate = Math.floor(Date.now() / 1000) - 3_600; // -1 hour
    await expect(buildCreateInvoiceTx(validParams({ dueDate: pastDueDate }))).rejects.toThrow(
      /due date must be in the future/i,
    );
    expect(rpcExecute).not.toHaveBeenCalled();
  });

  it('returns a base64 XDR string for a valid call', async () => {
    rpcExecute
      // 1st call: getAccount → return a real Account so TransactionBuilder works.
      .mockResolvedValueOnce(new Account(owner, '0'))
      // 2nd call: simulateTransaction → return a (mocked-as-successful) sim.
      .mockResolvedValueOnce({});

    const xdr = await buildCreateInvoiceTx(validParams());

    expect(typeof xdr).toBe('string');
    expect(xdr).toBe('BASE64_XDR_STRING');
    expect(rpcExecute).toHaveBeenCalledTimes(2);
  });

  it('rejects deposit before simulation when the native balance is insufficient', async () => {
    const token = 'CC4UBX4HPQ6N4UJ5F5GX2KQJ3E6ZLZ5JQ6P5T3X4VY3QYJ2NTQJX5V5X';
    const account = new Account(owner, '0');
    Object.assign(account, {
      balances: [{ asset_type: 'native', balance: '0.0000001' }],
    });

    rpcExecute.mockResolvedValueOnce(account);

    await expect(buildDepositTx(owner, token, 1_000_000n)).rejects.toThrow(/insufficient balance/i);
    expect(rpcExecute).toHaveBeenCalledTimes(1);
  });

  it('rejects commit-to-invoice before simulation when the wallet cannot cover the fee', async () => {
    const account = new Account(owner, '0');
    Object.assign(account, {
      balances: [{ asset_type: 'native', balance: '0.0000000' }],
    });

    rpcExecute.mockResolvedValueOnce(account);

    await expect(
      buildCommitToInvoiceTx({ investor: owner, invoiceId: 7, amount: 1_000n }),
    ).rejects.toThrow(/insufficient balance/i);
    expect(rpcExecute).toHaveBeenCalledTimes(1);
  });
});
