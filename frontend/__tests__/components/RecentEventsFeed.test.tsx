import React from 'react';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { RecentEventsFeed } from '@/components/analytics/RecentEventsFeed';
import type { ContractEvent } from '@/lib/monitoring';

const mockEvents: ContractEvent[] = [
  {
    id: 'event-1',
    contractId: 'INV123',
    topic: ['INVOICE', 'created'],
    value: ['INV001', 'OWNER1', 1000000000n],
    ledger: 100,
    ledgerCloseAt: '2024-01-01T00:00:00Z',
    txHash: 'tx123',
  },
  {
    id: 'event-2',
    contractId: 'POOL123',
    topic: ['POOL', 'funded'],
    value: ['INV001', 'SME1', 500000000n],
    ledger: 101,
    ledgerCloseAt: '2024-01-01T00:05:00Z',
    txHash: 'tx456',
  },
  {
    id: 'event-3',
    contractId: 'INV123',
    topic: ['INVOICE', 'default'],
    value: 'INV002',
    ledger: 102,
    ledgerCloseAt: '2024-01-01T00:10:00Z',
    txHash: 'tx789',
  },
];

describe('RecentEventsFeed', () => {
  it('renders loading skeleton when isLoading is true', () => {
    const { container } = render(<RecentEventsFeed events={[]} isLoading={true} />);
    expect(container.querySelector('.animate-pulse')).toBeInTheDocument();
  });

  it('renders the feed title', () => {
    render(<RecentEventsFeed events={mockEvents} isLoading={false} />);
    expect(screen.getByText('Recent On-Chain Events')).toBeInTheDocument();
  });

  it('shows empty state when no events', () => {
    render(<RecentEventsFeed events={[]} isLoading={false} />);
    expect(screen.getByText('No recent events detected.')).toBeInTheDocument();
  });

  it('renders event items', () => {
    render(<RecentEventsFeed events={mockEvents} isLoading={false} />);
    expect(screen.getByText('CREATED')).toBeInTheDocument();
    expect(screen.getByText('FUNDED')).toBeInTheDocument();
    expect(screen.getByText('DEFAULT')).toBeInTheDocument();
  });

  it('renders ledger numbers', () => {
    render(<RecentEventsFeed events={mockEvents} isLoading={false} />);
    expect(screen.getByText('#100')).toBeInTheDocument();
    expect(screen.getByText('#101')).toBeInTheDocument();
    expect(screen.getByText('#102')).toBeInTheDocument();
  });

  it('renders contract type labels', () => {
    render(<RecentEventsFeed events={mockEvents} isLoading={false} />);
    const invoiceLabels = screen.getAllByText('Invoice Contract');
    const poolLabels = screen.getAllByText('Pool Contract');
    expect(invoiceLabels.length).toBeGreaterThanOrEqual(1);
    expect(poolLabels.length).toBeGreaterThanOrEqual(1);
  });

  it('renders with single event', () => {
    render(<RecentEventsFeed events={[mockEvents[0]]} isLoading={false} />);
    expect(screen.getByText('CREATED')).toBeInTheDocument();
    expect(screen.queryByText('FUNDED')).not.toBeInTheDocument();
  });
});
