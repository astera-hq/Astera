import { render, screen } from '@testing-library/react';
import EstimatedFee from '@/components/EstimatedFee';
import type { SimulationState } from '@/hooks/useTransactionSimulation';

describe('EstimatedFee', () => {
  it('renders nothing when status is idle', () => {
    const sim: SimulationState = { status: 'idle', feeEstimate: null, error: null };
    const { container } = render(<EstimatedFee simulation={sim} />);
    expect(container.innerHTML).toBe('');
  });

  it('renders spinner when status is loading', () => {
    const sim: SimulationState = { status: 'loading', feeEstimate: null, error: null };
    render(<EstimatedFee simulation={sim} />);
    expect(screen.getByText('Estimating network fee...')).toBeInTheDocument();
  });

  it('renders fee when status is success', () => {
    const sim: SimulationState = {
      status: 'success',
      feeEstimate: { minResourceFee: 12300, instructions: 4500000, feeInXlm: 0.00123 },
      error: null,
    };
    render(<EstimatedFee simulation={sim} />);
    expect(screen.getByText(/~0.001230 XLM/)).toBeInTheDocument();
  });

  it('renders inline error when status is error', () => {
    const sim: SimulationState = {
      status: 'error',
      feeEstimate: null,
      error: 'Insufficient collateral',
    };
    render(<EstimatedFee simulation={sim} />);
    expect(screen.getByText('Insufficient collateral')).toBeInTheDocument();
  });
});
