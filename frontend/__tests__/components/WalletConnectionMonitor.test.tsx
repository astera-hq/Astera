import React from 'react';
import { render } from '@testing-library/react';
import '@testing-library/jest-dom';

// Mutable fake store state the component reads through selectors.
const disconnectStore = jest.fn();
let storeState: { wallet: { connected: boolean }; disconnect: () => void } = {
  wallet: { connected: false },
  disconnect: disconnectStore,
};

jest.mock('@/lib/store', () => ({
  useStore: <T,>(selector: (s: typeof storeState) => T) => selector(storeState),
}));

const pushToast = jest.fn();
jest.mock('@/components/Toast', () => ({
  pushToast: (...args: unknown[]) => pushToast(...args),
}));

// Default Freighter stub: still connected/allowed (no disconnect edge).
const getFreighter = jest.fn(async () => ({
  isAllowed: async () => ({ isAllowed: true, error: undefined }),
  getAddress: async () => ({ address: 'G' + 'A'.repeat(55), error: undefined }),
}));
jest.mock('@/lib/freighter', () => ({
  getFreighter: () => getFreighter(),
}));

jest.mock('next-intl', () => ({
  useTranslations: () => (key: string) => key,
}));

import WalletConnectionMonitor from '@/components/WalletConnectionMonitor';

describe('WalletConnectionMonitor (#692)', () => {
  beforeEach(() => {
    disconnectStore.mockClear();
    pushToast.mockClear();
    getFreighter.mockClear();
    storeState = { wallet: { connected: false }, disconnect: disconnectStore };
  });

  it('renders without crashing when the wallet is disconnected', () => {
    const { container } = render(<WalletConnectionMonitor />);
    // Renders no UI.
    expect(container).toBeEmptyDOMElement();
  });

  it('does not show a reconnect prompt when there is no active connection', () => {
    render(<WalletConnectionMonitor />);
    expect(pushToast).not.toHaveBeenCalled();
    expect(disconnectStore).not.toHaveBeenCalled();
  });

  it('renders without crashing when the wallet was previously connected', () => {
    storeState = { wallet: { connected: true }, disconnect: disconnectStore };
    const { container, unmount } = render(<WalletConnectionMonitor />);
    // Still renders no UI, and mounting the poller does not immediately
    // trigger a disconnect toast for a healthy connection.
    expect(container).toBeEmptyDOMElement();
    expect(pushToast).not.toHaveBeenCalled();
    // Cleanup tears down the interval/listeners without throwing.
    expect(() => unmount()).not.toThrow();
  });
});
