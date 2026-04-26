import React from 'react';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { PoolUtilizationChart } from '@/components/analytics/PoolUtilizationChart';
import type { PoolUtilizationPoint } from '@/lib/analytics';

const mockData: PoolUtilizationPoint[] = [
  { time: '2024-01-01', utilization: 50, deployed: 5000, deposited: 10000 },
  { time: '2024-01-02', utilization: 60, deployed: 6000, deposited: 10000 },
  { time: '2024-01-03', utilization: 70, deployed: 7000, deposited: 10000 },
];

describe('PoolUtilizationChart', () => {
  it('renders loading skeleton when isLoading is true', () => {
    const { container } = render(<PoolUtilizationChart data={[]} isLoading={true} />);
    expect(container.querySelector('.animate-pulse')).toBeInTheDocument();
    expect(screen.queryByText('Pool Utilization')).not.toBeInTheDocument();
  });

  it('renders the chart title', () => {
    render(<PoolUtilizationChart data={mockData} isLoading={false} />);
    expect(screen.getByText('Pool Utilization')).toBeInTheDocument();
  });

  it('renders with empty data without crashing', () => {
    const { container } = render(<PoolUtilizationChart data={[]} isLoading={false} />);
    expect(container.querySelector('.bg-brand-card')).toBeInTheDocument();
  });

  it('renders with data points', () => {
    const { container } = render(<PoolUtilizationChart data={mockData} isLoading={false} />);
    expect(container.querySelector('.bg-brand-card')).toBeInTheDocument();
    expect(screen.getByText('Pool Utilization')).toBeInTheDocument();
  });

  it('renders responsive container wrapper', () => {
    const { container } = render(<PoolUtilizationChart data={mockData} isLoading={false} />);
    const chartContainer = container.querySelector('.h-72');
    expect(chartContainer).toBeInTheDocument();
  });
});
