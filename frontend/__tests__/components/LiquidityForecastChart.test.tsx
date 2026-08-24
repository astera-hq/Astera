import React from 'react';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { LiquidityForecastChart } from '@/components/analytics/LiquidityForecastChart';
import type { LiquidityForecastPoint } from '@/lib/types';

const mockData: LiquidityForecastPoint[] = [
  { day: 1, projectedAvailable: 50000000000n },
  { day: 7, projectedAvailable: 45000000000n },
  { day: 30, projectedAvailable: 30000000000n },
];

describe('LiquidityForecastChart', () => {
  it('renders loading skeleton when isLoading is true', () => {
    const { container } = render(
      <LiquidityForecastChart data={[]} isLoading={true} />,
    );
    expect(container.querySelector('.animate-pulse')).toBeInTheDocument();
  });

  it('does not render chart title during loading', () => {
    render(<LiquidityForecastChart data={[]} isLoading={true} />);
    expect(screen.queryByText('Projected Liquidity')).not.toBeInTheDocument();
  });

  it('renders null when data is empty', () => {
    const { container } = render(
      <LiquidityForecastChart data={[]} isLoading={false} />,
    );
    expect(container.innerHTML).toBe('');
  });

  it('renders the chart title', () => {
    render(<LiquidityForecastChart data={mockData} isLoading={false} />);
    expect(screen.getByText('Projected Liquidity')).toBeInTheDocument();
  });

  it('renders a custom title', () => {
    render(
      <LiquidityForecastChart
        data={mockData}
        isLoading={false}
        title="Pool Liquidity Forecast"
      />,
    );
    expect(screen.getByText('Pool Liquidity Forecast')).toBeInTheDocument();
  });

  it('renders the chart card wrapper', () => {
    const { container } = render(
      <LiquidityForecastChart data={mockData} isLoading={false} />,
    );
    expect(container.querySelector('.bg-brand-card')).toBeInTheDocument();
  });

  it('shows queued demand when provided', () => {
    render(
      <LiquidityForecastChart
        data={mockData}
        isLoading={false}
        queuedDemand={1000000000n}
      />,
    );
    expect(screen.getByText(/queued/)).toBeInTheDocument();
  });

  it('hides queued demand when zero', () => {
    render(
      <LiquidityForecastChart
        data={mockData}
        isLoading={false}
        queuedDemand={0n}
      />,
    );
    expect(screen.queryByText(/queued/)).not.toBeInTheDocument();
  });

  it('hides queued demand when undefined', () => {
    render(<LiquidityForecastChart data={mockData} isLoading={false} />);
    expect(screen.queryByText(/queued/)).not.toBeInTheDocument();
  });
});
