import React from 'react';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { YieldPerformanceChart } from '@/components/analytics/YieldPerformanceChart';
import type { YieldPoint } from '@/lib/analytics';

const mockData: YieldPoint[] = [
  { time: '2024-01-01', apy: 8.0, interest: 7.6 },
  { time: '2024-01-02', apy: 8.2, interest: 7.8 },
  { time: '2024-01-03', apy: 7.9, interest: 7.5 },
];

describe('YieldPerformanceChart', () => {
  it('renders loading skeleton when isLoading is true', () => {
    const { container } = render(<YieldPerformanceChart data={[]} isLoading={true} />);
    expect(container.querySelector('.animate-pulse')).toBeInTheDocument();
  });

  it('renders the chart title', () => {
    render(<YieldPerformanceChart data={mockData} isLoading={false} />);
    expect(screen.getByText('Yield Performance')).toBeInTheDocument();
  });

  it('renders with empty data without crashing', () => {
    const { container } = render(<YieldPerformanceChart data={[]} isLoading={false} />);
    expect(container.querySelector('.bg-brand-card')).toBeInTheDocument();
  });

  it('renders with data points', () => {
    const { container } = render(<YieldPerformanceChart data={mockData} isLoading={false} />);
    expect(container.querySelector('.bg-brand-card')).toBeInTheDocument();
  });

  it('has correct height container', () => {
    const { container } = render(<YieldPerformanceChart data={mockData} isLoading={false} />);
    expect(container.querySelector('.h-72')).toBeInTheDocument();
  });
});
