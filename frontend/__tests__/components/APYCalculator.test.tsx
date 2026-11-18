import React from 'react';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { APYCalculator } from '@/components/APYCalculator';
import { usePoolConfig } from '@/lib/cache';

jest.mock('@/lib/cache', () => ({
  usePoolConfig: jest.fn(),
}));

const mockedUsePoolConfig = usePoolConfig as jest.MockedFunction<typeof usePoolConfig>;

describe('APYCalculator', () => {
  beforeEach(() => {
    mockedUsePoolConfig.mockReset();
  });

  it('renders live APY from pool config', () => {
    mockedUsePoolConfig.mockReturnValue({
      data: { yieldBps: 500 } as ReturnType<typeof usePoolConfig>['data'],
      error: undefined,
      isLoading: false,
    } as ReturnType<typeof usePoolConfig>);

    render(<APYCalculator />);

    expect(screen.getByText(/5\.00% APY/i)).toBeInTheDocument();
    expect(screen.queryByText(/fallback APY/i)).not.toBeInTheDocument();
  });

  it('uses fallback APY with warning when pool config loading fails', () => {
    mockedUsePoolConfig.mockReturnValue({
      data: undefined,
      error: new Error('Contract fetch failed'),
      isLoading: false,
    } as ReturnType<typeof usePoolConfig>);

    render(<APYCalculator />);

    expect(screen.getByText(/8\.00% APY/i)).toBeInTheDocument();
    expect(screen.getByText(/fallback APY/i)).toBeInTheDocument();
  });

  it('shows loading skeleton while config is fetching on initial load', () => {
    mockedUsePoolConfig.mockReturnValue({
      data: undefined,
      error: undefined,
      isLoading: true,
    } as ReturnType<typeof usePoolConfig>);

    render(<APYCalculator />);

    expect(screen.getByRole('status', { name: 'Loading earnings calculator' })).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: /Earnings calculator/ })).not.toBeInTheDocument();
  });

  it('renders calculator body when data is available (yieldBps fetched, not hardcoded)', () => {
    mockedUsePoolConfig.mockReturnValue({
      data: { yieldBps: 1200 } as ReturnType<typeof usePoolConfig>['data'],
      error: undefined,
      isLoading: false,
    } as ReturnType<typeof usePoolConfig>);

    render(<APYCalculator />);

    expect(screen.getByText(/12\.00% APY/i)).toBeInTheDocument();
    expect(screen.getByText(/Earnings calculator/i)).toBeInTheDocument();
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });
});
