import React from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import '@testing-library/jest-dom';
import CreditScore, { CreditScoreSkeleton } from '@/components/CreditScore';

describe('CreditScore', () => {
  it('shows the base 300 score and building label when no invoices exist', () => {
    render(<CreditScore paid={0} funded={0} defaulted={0} totalVolume={0n} />);
    expect(screen.getByText('300')).toBeInTheDocument();
    expect(screen.getByText('Building')).toBeInTheDocument();
    expect(screen.getByText(/Create and repay invoices to build your score\./)).toBeInTheDocument();
  });

  it('rewards a perfect repayment record with an Excellent score', () => {
    // 10 paid, 0 defaulted, volumeBonus caps at 50 (needs totalVolume >= 5e11 stroops):
    // 300 + repaymentRate(1) * 500 + 50 = 850.
    render(<CreditScore paid={10} funded={0} defaulted={0} totalVolume={500_000_000_000n} />);
    expect(screen.getByText('850')).toBeInTheDocument();
    expect(screen.getByText('Excellent')).toBeInTheDocument();
  });

  it('penalizes defaults — mixed record lands in the Fair/Good band', () => {
    // 3 paid, 0 funded, 2 defaulted → repaymentRate 0.6 → 300 + 300 = 600 (no volume bonus)
    render(<CreditScore paid={3} funded={0} defaulted={2} totalVolume={0n} />);
    expect(screen.getByText('600')).toBeInTheDocument();
    // 600 is in the "Fair" band (>= 550, < 650).
    expect(screen.getByText('Fair')).toBeInTheDocument();
  });

  it('surfaces the total invoice count in the summary line', () => {
    render(<CreditScore paid={2} funded={1} defaulted={1} totalVolume={0n} />);
    expect(screen.getByText(/Based on 4 invoice\(s\)/)).toBeInTheDocument();
  });

  it('renders the collapsed improvement panel by default', () => {
    render(<CreditScore paid={3} funded={0} defaulted={2} totalVolume={0n} />);
    expect(screen.getByText('Improve your score')).toBeInTheDocument();
    expect(screen.queryByText('Current Stats vs. Recommendations')).not.toBeInTheDocument();
  });

  it('expands the improvement panel when toggled', () => {
    render(<CreditScore paid={3} funded={0} defaulted={2} totalVolume={0n} />);
    fireEvent.click(screen.getByText('Improve your score'));
    expect(screen.getByText('Current Stats vs. Recommendations')).toBeInTheDocument();
  });

  it('shows at least 3 actionable recommendations when expanded', () => {
    render(<CreditScore paid={3} funded={0} defaulted={2} totalVolume={0n} />);
    fireEvent.click(screen.getByText('Improve your score'));
    expect(screen.getByText('On-time payments')).toBeInTheDocument();
    expect(screen.getByText('Defaults')).toBeInTheDocument();
    expect(screen.getByText('Payment speed')).toBeInTheDocument();
    expect(screen.getByText('Invoice volume')).toBeInTheDocument();
  });

  it('shows the next milestone when not at the top tier', () => {
    render(<CreditScore paid={3} funded={0} defaulted={2} totalVolume={0n} />);
    fireEvent.click(screen.getByText('Improve your score'));
    expect(screen.getByText(/Next Milestone:/)).toBeInTheDocument();
  });

  it('shows points to next tier in progress bar', () => {
    render(<CreditScore paid={3} funded={0} defaulted={2} totalVolume={0n} />);
    expect(screen.getByText(/pts to/)).toBeInTheDocument();
  });

  it('renders CreditScoreSkeleton with pulse animation', () => {
    const { container } = render(<CreditScoreSkeleton />);
    const root = container.firstChild as HTMLElement;
    expect(root).toHaveClass('animate-pulse');
    const skeletons = container.querySelectorAll('[role="status"]');
    expect(skeletons.length).toBeGreaterThan(4);
  });
});
