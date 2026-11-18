import React from 'react';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom';

// #780: the withdraw form used to sign-and-submit immediately on click, with
// no confirmation dialog in between. This test locks in the fix — a
// confirmation modal must appear before the wallet is asked to sign, and
// cancelling it must not build or submit a transaction.

const mockUseStore = jest.fn();
jest.mock('@/lib/store', () => ({
  useStore: () => mockUseStore(),
}));

jest.mock('next-intl', () => ({
  useTranslations: () => (key: string) => key,
}));

jest.mock('next/link', () => {
  const Link = ({ href, children }: { href: string; children: React.ReactNode }) => (
    <a href={href}>{children}</a>
  );
  Link.displayName = 'Link';
  return Link;
});

jest.mock('@/components/PoolStats', () => ({
  __esModule: true,
  default: () => null,
  PoolStatsSkeleton: () => null,
}));
jest.mock('@/components/APYCalculator', () => ({ APYCalculator: () => null }));
jest.mock('@/components/ScenarioModeler', () => ({ ScenarioModeler: () => null }));
jest.mock('@/components/analytics', () => ({ RateCurveChart: () => null }));

const mockBuildWithdrawTx = jest.fn();
const mockBuildDepositTx = jest.fn();
const mockSubmitTx = jest.fn();
const mockGetAcceptedTokens = jest.fn();

jest.mock('@/lib/contracts', () => ({
  getPoolConfig: jest.fn().mockResolvedValue({ yieldBps: 800 }),
  getInvestorPosition: jest.fn().mockResolvedValue({
    deposited: 1_000_0000000n,
    available: 1_000_0000000n,
    deployed: 0n,
    earned: 0n,
    depositCount: 1,
  }),
  getAcceptedTokens: (...args: unknown[]) => mockGetAcceptedTokens(...args),
  getPoolTokenTotals: jest.fn().mockResolvedValue({
    totalDeposited: 1_000_0000000n,
    totalDeployed: 0n,
    totalPaidOut: 0n,
    totalFeeRevenue: 0n,
  }),
  getTokenDepositCap: jest.fn().mockResolvedValue(0n),
  buildDepositTx: (...args: unknown[]) => mockBuildDepositTx(...args),
  buildWithdrawTx: (...args: unknown[]) => mockBuildWithdrawTx(...args),
  submitTx: (...args: unknown[]) => mockSubmitTx(...args),
  getKycRequired: jest.fn().mockResolvedValue(false),
  getInvestorKyc: jest.fn().mockResolvedValue(true),
  getRateModelConfig: jest.fn().mockResolvedValue(null),
  getCurrentRate: jest.fn().mockResolvedValue(null),
}));

const mockSignTransaction = jest.fn();
jest.mock('@stellar/freighter-api', () => ({
  signTransaction: (...args: unknown[]) => mockSignTransaction(...args),
}));

import InvestPage from '../page';

describe('InvestPage withdraw confirmation (#780)', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockGetAcceptedTokens.mockResolvedValue(['TOKEN_A']);
    mockBuildWithdrawTx.mockResolvedValue('withdraw-xdr');
    mockSubmitTx.mockResolvedValue(undefined);
    mockSignTransaction.mockResolvedValue({ signedTxXdr: 'signed-xdr', error: null });
    mockUseStore.mockReturnValue({
      wallet: { address: 'GABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789', connected: true },
      poolConfig: { yieldBps: 800 },
      setPoolConfig: jest.fn(),
      position: {
        deposited: 1_000_0000000n,
        available: 1_000_0000000n,
        deployed: 0n,
        earned: 0n,
        depositCount: 1,
      },
      setPosition: jest.fn(),
    });
  });

  async function enterWithdrawAmount() {
    render(<InvestPage />);

    // Switch to withdraw mode (exact match distinguishes it from the submit
    // button, whose accessible name also contains "modes.withdraw").
    fireEvent.click(await screen.findByRole('button', { name: 'modes.withdraw' }));

    const amountInput = screen.getByPlaceholderText('0.00');
    fireEvent.change(amountInput, { target: { value: '100' } });

    const submitButton = document.querySelector('button[type="submit"]') as HTMLButtonElement;
    // selectedToken is only populated once getAcceptedTokens() resolves, which
    // also gates the submit button — wait for it before interacting further.
    await waitFor(() => expect(submitButton).not.toBeDisabled());
    return submitButton;
  }

  it('shows a confirmation dialog instead of signing immediately', async () => {
    const submitButton = await enterWithdrawAmount();

    fireEvent.click(submitButton);

    expect(await screen.findByRole('dialog')).toBeInTheDocument();
    expect(mockBuildWithdrawTx).not.toHaveBeenCalled();
    expect(mockSignTransaction).not.toHaveBeenCalled();
  });

  it('submits no transaction when the dialog is cancelled', async () => {
    const submitButton = await enterWithdrawAmount();
    fireEvent.click(submitButton);

    const dialog = await screen.findByRole('dialog');
    fireEvent.click(screen.getByRole('button', { name: /cancel/i }));

    await waitFor(() => expect(dialog).not.toBeInTheDocument());
    expect(mockBuildWithdrawTx).not.toHaveBeenCalled();
    expect(mockSubmitTx).not.toHaveBeenCalled();
  });

  it('builds and submits the withdrawal once confirmed', async () => {
    const submitButton = await enterWithdrawAmount();
    fireEvent.click(submitButton);

    await screen.findByRole('dialog');
    fireEvent.click(screen.getByRole('button', { name: /^Withdraw$/i }));

    await waitFor(() => expect(mockBuildWithdrawTx).toHaveBeenCalledTimes(1));
    expect(mockSubmitTx).toHaveBeenCalledTimes(1);
  });
});
