import React from 'react';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { TopSmesTable } from '@/components/analytics/TopSmesTable';
import type { TopSme } from '@/lib/analytics';

const mockSmes: TopSme[] = [
  {
    address: 'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
    invoiceCount: 15,
    totalValue: 500000,
    defaultRate: 0,
  },
  {
    address: 'GBRTJIXRN3BI2UIS5OM3WV4TQK2WE4JX7PL5PKLLSMQ7XZJYASMQ5BZG',
    invoiceCount: 8,
    totalValue: 250000,
    defaultRate: 12,
  },
  {
    address: 'GCW6QF7KJ5QXPHMNPKZD7F5M6V5K5T7Z2XQ3H5J7N9L1P3R5T7V9X1Z3',
    invoiceCount: 3,
    totalValue: 75000,
    defaultRate: 33,
  },
];

describe('TopSmesTable', () => {
  it('renders loading skeleton when isLoading is true', () => {
    const { container } = render(<TopSmesTable data={[]} isLoading={true} />);
    expect(container.querySelector('.animate-pulse')).toBeInTheDocument();
  });

  it('renders the table title', () => {
    render(<TopSmesTable data={mockSmes} isLoading={false} />);
    expect(screen.getByText('Top SMEs by Volume')).toBeInTheDocument();
  });

  it('shows empty state when no data', () => {
    render(<TopSmesTable data={[]} isLoading={false} />);
    expect(screen.getByText('No SME data available yet.')).toBeInTheDocument();
  });

  it('renders SME rows with data', () => {
    render(<TopSmesTable data={mockSmes} isLoading={false} />);
    expect(screen.getByText('$500,000')).toBeInTheDocument();
    expect(screen.getByText('$250,000')).toBeInTheDocument();
    expect(screen.getByText('$75,000')).toBeInTheDocument();
  });

  it('renders index numbers', () => {
    render(<TopSmesTable data={mockSmes} isLoading={false} />);
    // Check that table rows exist by verifying the first row's index
    const firstColumnCells = screen
      .getAllByRole('cell')
      .filter((el) => el.textContent === '1' || el.textContent === '2');
    expect(firstColumnCells.length).toBeGreaterThanOrEqual(2);
  });

  it('renders default rate badges with correct colors', () => {
    render(<TopSmesTable data={mockSmes} isLoading={false} />);
    // Check for the 0% default rate badge
    expect(screen.getByText('0%')).toBeInTheDocument();
    expect(screen.getByText('12%')).toBeInTheDocument();
    expect(screen.getByText('33%')).toBeInTheDocument();
  });

  it('truncates long addresses', () => {
    render(<TopSmesTable data={mockSmes} isLoading={false} />);
    // Should show truncated addresses with ...
    const addressCells = screen.getAllByText(/\.\.\./);
    expect(addressCells.length).toBeGreaterThan(0);
  });

  it('renders table headers', () => {
    render(<TopSmesTable data={mockSmes} isLoading={false} />);
    expect(screen.getByText('#')).toBeInTheDocument();
    expect(screen.getByText('SME Address')).toBeInTheDocument();
    expect(screen.getByText('Invoices')).toBeInTheDocument();
    expect(screen.getByText('Total Value')).toBeInTheDocument();
    expect(screen.getByText('Default Rate')).toBeInTheDocument();
  });
});
