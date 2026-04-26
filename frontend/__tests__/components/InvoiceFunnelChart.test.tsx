import React from 'react';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { InvoiceFunnelChart } from '@/components/analytics/InvoiceFunnelChart';
import type { InvoiceFunnelData } from '@/lib/analytics';

const mockData: InvoiceFunnelData[] = [
  { stage: 'Created', count: 100, value: 500000 },
  { stage: 'Verified', count: 80, value: 400000 },
  { stage: 'Funded', count: 60, value: 300000 },
  { stage: 'Paid', count: 45, value: 225000 },
  { stage: 'Defaulted', count: 5, value: 25000 },
];

describe('InvoiceFunnelChart', () => {
  it('renders loading skeleton when isLoading is true', () => {
    const { container } = render(<InvoiceFunnelChart data={[]} isLoading={true} />);
    expect(container.querySelector('.animate-pulse')).toBeInTheDocument();
  });

  it('renders the chart title', () => {
    render(<InvoiceFunnelChart data={mockData} isLoading={false} />);
    expect(screen.getByText('Invoice Funnel')).toBeInTheDocument();
  });

  it('renders with empty data without crashing', () => {
    const { container } = render(<InvoiceFunnelChart data={[]} isLoading={false} />);
    expect(container.querySelector('.bg-brand-card')).toBeInTheDocument();
  });

  it('renders with data points', () => {
    const { container } = render(<InvoiceFunnelChart data={mockData} isLoading={false} />);
    expect(container.querySelector('.bg-brand-card')).toBeInTheDocument();
  });

  it('has correct height container', () => {
    const { container } = render(<InvoiceFunnelChart data={mockData} isLoading={false} />);
    expect(container.querySelector('.h-72')).toBeInTheDocument();
  });
});
