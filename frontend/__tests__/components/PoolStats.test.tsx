import React from 'react';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import PoolStats, { PoolStatsSkeleton } from '@/components/PoolStats';
import type { PoolConfig, PoolTokenTotals } from '@/lib/types';

const baseConfig: PoolConfig = {
  invoiceContract: 'INVOICE_CID',
  admin: 'ADMIN',
  yieldBps: 800, // 8.0%
  factoringFeeBps: 250, // 2.50%
  compoundInterest: false,
};

function totals(deposited: bigint, deployed: bigint): PoolTokenTotals {
  return {
    totalDeposited: deposited,
    totalDeployed: deployed,
    totalPaidOut: 0n,
    totalFeeRevenue: 0n,
  };
}

function getUtilizationBar(container: HTMLElement): HTMLElement {
  // The utilization bar is the first inner bar under the "Utilization" label.
  // Its inline width style is the value under test.
  const bars = container.querySelectorAll<HTMLDivElement>('div.bg-brand-gold.rounded-full');
  // First one is the utilization bar.
  if (!bars[0]) throw new Error('Utilization bar not found');
  return bars[0];
}

describe('PoolStats', () => {
  it('renders APY and factoring fee from config', () => {
    render(<PoolStats config={baseConfig} tokenTotals={totals(0n, 0n)} tokenLabel="USDC" />);
    expect(screen.getByText('8.0%')).toBeInTheDocument();
    expect(screen.getByText('2.50%')).toBeInTheDocument();
  });

  it('renders 0% utilization when deposited is zero', () => {
    const { container } = render(
      <PoolStats config={baseConfig} tokenTotals={totals(0n, 0n)} tokenLabel="USDC" />,
    );
    expect(screen.getByText('0%')).toBeInTheDocument();
    expect(getUtilizationBar(container)).toHaveStyle({ width: '0%' });
  });

  it('renders 50% utilization width for half-deployed pool', () => {
    const { container } = render(
      <PoolStats
        config={baseConfig}
        tokenTotals={totals(200_000_000n, 100_000_000n)}
        tokenLabel="USDC"
      />,
    );
    expect(screen.getByText('50%')).toBeInTheDocument();
    expect(getUtilizationBar(container)).toHaveStyle({ width: '50%' });
  });

  it('renders 100% utilization width for fully-deployed pool', () => {
    const { container } = render(
      <PoolStats
        config={baseConfig}
        tokenTotals={totals(100_000_000n, 100_000_000n)}
        tokenLabel="USDC"
      />,
    );
    expect(screen.getByText('100%')).toBeInTheDocument();
    expect(getUtilizationBar(container)).toHaveStyle({ width: '100%' });
  });

  it('caps utilization bar width at 100% even if deployed > deposited', () => {
    const { container } = render(
      <PoolStats
        config={baseConfig}
        tokenTotals={totals(100_000_000n, 200_000_000n)}
        tokenLabel="USDC"
      />,
    );
    // Label shows the raw ratio, but bar is clamped.
    expect(getUtilizationBar(container)).toHaveStyle({ width: '100%' });
  });

  it('handles a null tokenTotals gracefully (treats as zero liquidity)', () => {
    const { container } = render(
      <PoolStats config={baseConfig} tokenTotals={null} tokenLabel="USDC" />,
    );
    expect(screen.getByText('0%')).toBeInTheDocument();
    expect(getUtilizationBar(container)).toHaveStyle({ width: '0%' });
  });

  it('renders PoolStatsSkeleton with pulse animation and placeholder content', () => {
    const { container } = render(<PoolStatsSkeleton />);
    const root = container.firstChild as HTMLElement;
    expect(root).toHaveClass('animate-pulse');
    // Skeleton uses role="status" for accessibility
    const skeletons = container.querySelectorAll('[role="status"]');
    expect(skeletons.length).toBeGreaterThan(5);
  });
});
