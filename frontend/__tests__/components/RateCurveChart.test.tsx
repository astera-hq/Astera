import React from 'react';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { RateCurveChart } from '@/components/analytics/RateCurveChart';
import type { RateModelConfig } from '@/lib/types';

const mockConfig: RateModelConfig = {
  baseRateBps: 200,
  optimalUtilizationBps: 8000,
  slope1Bps: 500,
  slope2Bps: 3000,
  maxRateBps: 5000,
};

describe('RateCurveChart', () => {
  it('renders the chart title', () => {
    render(<RateCurveChart config={mockConfig} />);
    expect(screen.getByText('Interest Rate Curve')).toBeInTheDocument();
  });

  it('renders a custom title', () => {
    render(<RateCurveChart config={mockConfig} title="My Custom Rate Curve" />);
    expect(screen.getByText('My Custom Rate Curve')).toBeInTheDocument();
  });

  it('renders the chart card wrapper', () => {
    const { container } = render(<RateCurveChart config={mockConfig} />);
    expect(container.querySelector('.bg-brand-card')).toBeInTheDocument();
  });

  it('renders the correct height container', () => {
    const { container } = render(<RateCurveChart config={mockConfig} />);
    expect(container.querySelector('.h-72')).toBeInTheDocument();
  });

  it('does not render the marker summary when currentUtilizationBps is omitted', () => {
    render(<RateCurveChart config={mockConfig} />);
    expect(screen.queryByText(/now:/)).not.toBeInTheDocument();
  });

  it('renders the marker summary when currentUtilizationBps is provided', () => {
    render(<RateCurveChart config={mockConfig} currentUtilizationBps={7000} />);
    expect(screen.getByText(/now:.*util.*APY/)).toBeInTheDocument();
  });

  it('renders the marker summary with explicit currentRateBps', () => {
    render(
      <RateCurveChart
        config={mockConfig}
        currentUtilizationBps={7000}
        currentRateBps={1200}
      />,
    );
    expect(screen.getByText(/12\.00% APY/)).toBeInTheDocument();
  });

  it('displays the kink label text', () => {
    render(<RateCurveChart config={mockConfig} />);
    expect(screen.getByText(/Kink at/)).toBeInTheDocument();
  });

  it('renders with full config data without crashing', () => {
    const { container } = render(<RateCurveChart config={mockConfig} />);
    expect(container.querySelector('.bg-brand-card')).toBeInTheDocument();
  });
});
