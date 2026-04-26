import React from 'react';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { CreditScoreDistributionChart } from '@/components/analytics/CreditScoreDistributionChart';
import type { CreditScoreBucket } from '@/lib/analytics';

const mockData: CreditScoreBucket[] = [
  { range: '0-300', count: 5 },
  { range: '300-500', count: 10 },
  { range: '500-650', count: 25 },
  { range: '650-750', count: 40 },
  { range: '750-850', count: 30 },
  { range: '850+', count: 15 },
];

describe('CreditScoreDistributionChart', () => {
  it('renders loading skeleton when isLoading is true', () => {
    const { container } = render(<CreditScoreDistributionChart data={[]} isLoading={true} />);
    expect(container.querySelector('.animate-pulse')).toBeInTheDocument();
  });

  it('renders the chart title', () => {
    render(<CreditScoreDistributionChart data={mockData} isLoading={false} />);
    expect(screen.getByText('Credit Score Distribution')).toBeInTheDocument();
  });

  it('renders with empty data without crashing', () => {
    const { container } = render(<CreditScoreDistributionChart data={[]} isLoading={false} />);
    expect(container.querySelector('.bg-brand-card')).toBeInTheDocument();
  });

  it('renders with data points', () => {
    const { container } = render(
      <CreditScoreDistributionChart data={mockData} isLoading={false} />,
    );
    expect(container.querySelector('.bg-brand-card')).toBeInTheDocument();
  });

  it('has correct height container', () => {
    const { container } = render(
      <CreditScoreDistributionChart data={mockData} isLoading={false} />,
    );
    expect(container.querySelector('.h-72')).toBeInTheDocument();
  });
});
