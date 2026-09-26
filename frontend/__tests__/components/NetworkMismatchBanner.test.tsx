import React, { act } from 'react';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';

import NetworkMismatchBanner from '@/components/NetworkMismatchBanner';
import { useStore } from '@/lib/store';

jest.mock('next-intl', () => ({
  useTranslations: (namespace: string) => (key: string) => `${namespace}.${key}`,
}));

describe('NetworkMismatchBanner', () => {
  beforeEach(() => {
    act(() => {
      useStore.getState().setNetworkMismatch({
        isMismatched: true,
        walletNetwork: 'PUBLIC',
        appNetwork: 'TESTNET',
      });
    });
  });

  afterEach(() => {
    act(() => {
      useStore.getState().setNetworkMismatch({
        isMismatched: false,
        walletNetwork: null,
        appNetwork: null,
      });
    });
  });

  it('renders network mismatch copy from next-intl keys', () => {
    render(<NetworkMismatchBanner />);

    expect(screen.getByText('Notifications.networkMismatch.title')).toBeInTheDocument();
    expect(screen.getByText(/Notifications\.networkMismatch\.messageStart/)).toBeInTheDocument();
    expect(
      screen.getByRole('link', {
        name: /Notifications\.networkMismatch\.helpLink/,
      }),
    ).toHaveAttribute('href', 'https://www.freighter.app/help#how-do-i-switch-networks');
    expect(
      screen.getByRole('button', {
        name: 'Notifications.networkMismatch.dismiss',
      }),
    ).toBeInTheDocument();
  });
});
