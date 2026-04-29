import React from 'react';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import InvoiceCard, { InvoiceCardSkeleton } from '@/components/InvoiceCard';
import type { InvoiceMetadata } from '@/lib/types';

// next/link is fine in jsdom, but mock to a plain anchor to keep the test
// independent of Next's client-side router.
jest.mock('next/link', () => {
  const Link = ({
    href,
    children,
    className,
  }: {
    href: string;
    children: React.ReactNode;
    className?: string;
  }) => (
    <a href={href} className={className}>
      {children}
    </a>
  );
  Link.displayName = 'Link';
  return { __esModule: true, default: Link };
});

function makeMeta(overrides: Partial<InvoiceMetadata> = {}): InvoiceMetadata {
  return {
    name: 'Acme Invoice #42',
    description: 'Widgets delivered Q2',
    image: '',
    amount: 50_000_000n, // 5.00 USDC (7 decimals)
    debtor: 'Acme Corp',
    // 30 days in the future
    dueDate: Math.floor(Date.now() / 1000) + 30 * 86_400,
    status: 'Pending',
    symbol: 'USDC',
    decimals: 7,
    ...overrides,
  };
}

describe('InvoiceCard', () => {
  it('renders invoice name, debtor, symbol, and ID', () => {
    render(<InvoiceCard id={7} metadata={makeMeta()} />);
    expect(screen.getByText('Acme Invoice #42')).toBeInTheDocument();
    expect(screen.getByText('Acme Corp')).toBeInTheDocument();
    expect(screen.getByText(/USDC · #7/)).toBeInTheDocument();
  });

  it('renders a status badge matching metadata.status', () => {
    const { rerender } = render(<InvoiceCard id={1} metadata={makeMeta({ status: 'Funded' })} />);
    expect(screen.getByText('Funded')).toBeInTheDocument();

    rerender(<InvoiceCard id={1} metadata={makeMeta({ status: 'Paid' })} />);
    expect(screen.getByText('Paid')).toBeInTheDocument();

    rerender(<InvoiceCard id={1} metadata={makeMeta({ status: 'Defaulted' })} />);
    expect(screen.getByText('Defaulted')).toBeInTheDocument();
  });

  it('shows a days-left countdown for future due dates', () => {
    const meta = makeMeta({ dueDate: Math.floor(Date.now() / 1000) + 10 * 86_400 });
    render(<InvoiceCard id={1} metadata={meta} />);
    expect(screen.getByText(/\d+d left/)).toBeInTheDocument();
  });

  it('shows an overdue indicator for past due dates', () => {
    const meta = makeMeta({ dueDate: Math.floor(Date.now() / 1000) - 5 * 86_400 });
    render(<InvoiceCard id={1} metadata={meta} />);
    expect(screen.getByText(/overdue/)).toBeInTheDocument();
  });

  it('renders co-funding progress only when Pending with a fundedAmount', () => {
    const meta = makeMeta({ status: 'Pending', amount: 100_000_000n });
    // Pending without fundedAmount: no progress UI
    const { rerender } = render(<InvoiceCard id={1} metadata={meta} />);
    expect(screen.queryByText(/Co-funding progress/)).not.toBeInTheDocument();

    // Pending with 25% funded: progress shown at 25.0%
    rerender(<InvoiceCard id={1} metadata={meta} fundedAmount={25_000_000n} />);
    expect(screen.getByText(/Co-funding progress/)).toBeInTheDocument();
    expect(screen.getByText(/25\.0%/)).toBeInTheDocument();
  });

  it('renders InvoiceCardSkeleton with pulse animation', () => {
    const { container } = render(<InvoiceCardSkeleton />);
    const root = container.firstChild as HTMLElement;
    expect(root).toHaveClass('animate-pulse');
    const skeletons = container.querySelectorAll('[role="status"]');
    expect(skeletons.length).toBeGreaterThan(3);
  });
});
