import React from 'react';
import { render, screen, act, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import WalletConnect from '@/components/WalletConnect';
import { useStore } from '@/lib/store';

// The component dynamically imports '@stellar/freighter-api' — mock both the
// real module and any dynamic-import requests to the same specifier.
const mockFreighter = {
  isConnected: jest.fn(),
  isAllowed: jest.fn(),
  setAllowed: jest.fn(),
  getAddress: jest.fn(),
};
jest.mock('@stellar/freighter-api', () => mockFreighter);

// Don't actually make contract calls anywhere downstream.
jest.mock('@/lib/contracts', () => ({}));

// Mock react-hot-toast to prevent actual toast rendering in unit tests.
jest.mock('react-hot-toast', () => ({
  toast: Object.assign(jest.fn(), {
    error: jest.fn(),
    success: jest.fn(),
    loading: jest.fn(),
    dismiss: jest.fn(),
  }),
}));

describe('WalletConnect', () => {
  beforeEach(() => {
    // Reset Zustand store between tests.
    act(() => {
      useStore.getState().disconnect();
    });
    jest.clearAllMocks();
  });

  it('shows the connect button when disconnected', () => {
    render(<WalletConnect />);
    expect(screen.getByRole('button', { name: /connect/i })).toBeInTheDocument();
  });

  it('shows the truncated address and a disconnect button when connected', () => {
    act(() => {
      useStore.getState().setWallet({
        address: 'GABCDEF1234567890ABCDEF1234567890ABCDEF1234567890ABCDXYZ',
        connected: true,
        network: 'testnet',
      });
    });

    render(<WalletConnect />);
    // Truncated format is `${first6}...${last4}` — assert on both ends.
    expect(screen.getByText(/GABCDE.+DXYZ/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /disconnect/i })).toBeInTheDocument();
  });

  it('connects successfully through the freighter flow and updates the store', async () => {
    mockFreighter.isConnected.mockResolvedValue({ isConnected: true });
    mockFreighter.isAllowed.mockResolvedValue({ isAllowed: true });
    mockFreighter.getAddress.mockResolvedValue({ address: 'GTEST', error: undefined });

    render(<WalletConnect />);
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /connect/i }));

    // After the flow resolves, the store should reflect the connected state.
    expect(useStore.getState().wallet).toMatchObject({
      address: 'GTEST',
      connected: true,
    });
    expect(localStorage.getItem('astera-wallet-connected')).toBe('true');
    expect(localStorage.getItem('astera-wallet-address')).toBe('GTEST');
  });

  it('silently reconnects when a stored session is still allowed', async () => {
    localStorage.setItem('astera-wallet-connected', 'true');
    localStorage.setItem('astera-wallet-address', 'GRETURNING');
    mockFreighter.isAllowed.mockResolvedValue({ isAllowed: true });
    mockFreighter.getAddress.mockResolvedValue({ address: 'GRETURNING', error: undefined });

    render(<WalletConnect />);

    await waitFor(() => {
      expect(useStore.getState().wallet).toMatchObject({
        address: 'GRETURNING',
        connected: true,
      });
    });
    expect(mockFreighter.isAllowed).toHaveBeenCalled();
    expect(mockFreighter.getAddress).toHaveBeenCalled();
  });

  it('reports a clear error when Freighter is not installed', async () => {
    mockFreighter.isConnected.mockResolvedValue({ isConnected: false });

    render(<WalletConnect />);
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /connect/i }));
    // Wait for async connect flow to settle.
    await act(async () => {});

    expect(useStore.getState().wallet.connected).toBe(false);
  });
});
