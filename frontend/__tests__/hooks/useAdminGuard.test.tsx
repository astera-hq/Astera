import React from 'react';
import { render, waitFor } from '@testing-library/react';
import { useRouter } from 'next/navigation';
import { useStore } from '@/lib/store';
import { useAdminGuard } from '@/hooks/useAdminGuard';

jest.mock('next/navigation', () => ({
  useRouter: jest.fn(),
}));

jest.mock('@/lib/store', () => ({
  useStore: jest.fn(),
}));

const TestComponent = () => {
  const { status, isAdmin } = useAdminGuard();
  return (
    <div>
      <span data-testid="status">{status}</span>
      <span data-testid="is-admin">{String(isAdmin)}</span>
    </div>
  );
};

describe('useAdminGuard', () => {
  beforeEach(() => {
    jest.resetAllMocks();
  });

  it('redirects a non-admin wallet to home', async () => {
    const replaceMock = jest.fn();
    (useRouter as jest.Mock).mockReturnValue({ replace: replaceMock } as any);
    (useStore as unknown as jest.Mock).mockReturnValue({
      wallet: {
        address: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA',
        connected: true,
        network: 'testnet',
      },
      poolConfig: {
        invoiceContract: 'C' + 'A'.repeat(55),
        admin: 'G' + 'B'.repeat(55),
        yieldBps: 0,
        factoringFeeBps: 0,
        compoundInterest: false,
        proposedYieldBps: 0,
        yieldProposalAt: 0,
        yieldTimelockSecs: 0,
        maxSingleInvestorBps: 0,
      },
      setPoolConfig: jest.fn(),
    } as any);

    const screen = render(<TestComponent />);

    await waitFor(() => expect(replaceMock).toHaveBeenCalledWith('/'));
    expect(screen.getByTestId('status').textContent).not.toBe('authorized');
  });

  it('allows an admin wallet to render as authorized', async () => {
    const replaceMock = jest.fn();
    (useRouter as jest.Mock).mockReturnValue({ replace: replaceMock } as any);
    (useStore as unknown as jest.Mock).mockReturnValue({
      wallet: { address: 'G' + 'A'.repeat(55), connected: true, network: 'testnet' },
      poolConfig: {
        invoiceContract: 'C' + 'A'.repeat(55),
        admin: 'G' + 'A'.repeat(55),
        yieldBps: 0,
        factoringFeeBps: 0,
        compoundInterest: false,
        proposedYieldBps: 0,
        yieldProposalAt: 0,
        yieldTimelockSecs: 0,
        maxSingleInvestorBps: 0,
      },
      setPoolConfig: jest.fn(),
    } as any);

    const screen = render(<TestComponent />);

    await waitFor(() => expect(screen.getByTestId('status').textContent).toBe('authorized'));
    expect(screen.getByTestId('is-admin').textContent).toBe('true');
    expect(replaceMock).not.toHaveBeenCalled();
  });
});
