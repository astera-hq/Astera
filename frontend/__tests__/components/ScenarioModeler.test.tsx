import React from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { ScenarioModeler } from '@/components/ScenarioModeler';

describe('ScenarioModeler', () => {
  it('renders the component header with title and description', () => {
    render(<ScenarioModeler yieldBps={800} />);
    expect(screen.getByText('Scenario Modeler')).toBeInTheDocument();
    expect(screen.getByText(/Model best \/ base \/ worst-case outcomes/)).toBeInTheDocument();
  });

  it('shows loading state when loading prop is true', () => {
    render(<ScenarioModeler yieldBps={null} loading={true} />);
    expect(screen.getByText('loading…')).toBeInTheDocument();
  });

  it('displays "unavailable" when yieldBps is null and not loading', () => {
    render(<ScenarioModeler yieldBps={null} loading={false} />);
    expect(screen.getByText('unavailable')).toBeInTheDocument();
  });

  it('displays the live yield rate when yieldBps is provided', () => {
    render(<ScenarioModeler yieldBps={800} />);
    expect(screen.getByText(/8\.00% APY/)).toBeInTheDocument();
  });

  it('shows message to enable rate override when no yield is available', () => {
    render(<ScenarioModeler yieldBps={null} />);
    expect(
      screen.getByText('Enable rate override to begin modeling.'),
    ).toBeInTheDocument();
  });

  it('renders override rate toggle switch and can toggle it', async () => {
    render(<ScenarioModeler yieldBps={800} />);
    const overrideButton = screen.getByRole('switch');
    expect(overrideButton).toHaveAttribute('aria-checked', 'false');

    await userEvent.click(overrideButton);
    expect(overrideButton).toHaveAttribute('aria-checked', 'true');
  });

  it('reveals yield rate override slider when override is enabled', async () => {
    render(<ScenarioModeler yieldBps={800} />);
    const overrideButton = screen.getByRole('switch');
    await userEvent.click(overrideButton);
    expect(screen.getByText('Yield Rate Override')).toBeInTheDocument();
  });

  it('renders input field for deposit amount', () => {
    render(<ScenarioModeler yieldBps={800} />);
    const depositInput = screen.getByDisplayValue('10000');
    expect(depositInput).toBeInTheDocument();
    expect(depositInput).toHaveAttribute('type', 'number');
    expect(depositInput).toHaveAttribute('min', '1000');
    expect(depositInput).toHaveAttribute('max', '1000000');
  });

  it('updates deposit amount when input changes', async () => {
    render(<ScenarioModeler yieldBps={800} />);
    const depositInput = screen.getByDisplayValue('10000') as HTMLInputElement;
    await userEvent.clear(depositInput);
    await userEvent.type(depositInput, '50000');
    expect(depositInput.value).toBe('50000');
  });

  it('clamps deposit amount to min value (1000)', async () => {
    render(<ScenarioModeler yieldBps={800} />);
    const depositInput = screen.getByDisplayValue('10000') as HTMLInputElement;
    await userEvent.clear(depositInput);
    await userEvent.type(depositInput, '500');
    expect(depositInput.value).toBe('1000');
  });

  it('clamps deposit amount to max value (1000000)', async () => {
    render(<ScenarioModeler yieldBps={800} />);
    const depositInput = screen.getByDisplayValue('10000') as HTMLInputElement;
    await userEvent.clear(depositInput);
    await userEvent.type(depositInput, '5000000');
    expect(depositInput.value).toBe('1000000');
  });

  it('renders all risk assumption sliders when ready', () => {
    render(<ScenarioModeler yieldBps={800} />);
    expect(screen.getByText('Pool Utilization')).toBeInTheDocument();
    expect(screen.getByText('Default Rate')).toBeInTheDocument();
    expect(screen.getByText('Collateral Recovery')).toBeInTheDocument();
  });

  it('displays best/base/worst case scenario labels when yield is available', () => {
    render(<ScenarioModeler yieldBps={800} />);
    expect(screen.getByText('BEST CASE')).toBeInTheDocument();
    expect(screen.getByText('BASE CASE')).toBeInTheDocument();
    expect(screen.getByText('WORST CASE')).toBeInTheDocument();
  });

  it('shows breakeven calculation against 5% savings account', () => {
    render(<ScenarioModeler yieldBps={800} />);
    expect(screen.getByText(/Breakeven vs 5% savings account/)).toBeInTheDocument();
  });

  it('renders disclaimer text', () => {
    render(<ScenarioModeler yieldBps={800} />);
    expect(screen.getByText(/This tool is for illustrative purposes only/)).toBeInTheDocument();
  });

  it('applies custom className prop', () => {
    const { container } = render(
      <ScenarioModeler yieldBps={800} className="custom-class" />,
    );
    const root = container.firstChild as HTMLElement;
    expect(root).toHaveClass('custom-class');
  });

  it('uses default loading state of false when not provided', () => {
    render(<ScenarioModeler yieldBps={800} />);
    expect(screen.queryByText('Loading pool rate…')).not.toBeInTheDocument();
  });

  it('updates display when sliders change', async () => {
    render(<ScenarioModeler yieldBps={800} />);
    const sliders = screen.getAllByRole('slider');
    const depositSlider = sliders[0];

    await userEvent.click(depositSlider, { clientX: 100 });
    expect(screen.getByDisplayValue(/\d+/)).toBeInTheDocument();
  });

  it('shows scenario results with proper formatting', () => {
    render(<ScenarioModeler yieldBps={800} />);
    expect(screen.getByText(/Gross yield/)).toBeInTheDocument();
    expect(screen.getByText(/Default loss/)).toBeInTheDocument();
    expect(screen.getByText(/Utilization/)).toBeInTheDocument();
  });

  it('displays annualized return percentage in scenario bars', () => {
    render(<ScenarioModeler yieldBps={800} />);
    expect(screen.getAllByText(/annualised/)).toHaveLength(3);
  });

  it('renders deposit amount slider with correct bounds', () => {
    render(<ScenarioModeler yieldBps={800} />);
    const depositInput = screen.getByDisplayValue('10000') as HTMLInputElement;
    expect(depositInput.min).toBe('1000');
    expect(depositInput.max).toBe('1000000');
    expect(depositInput.step).toBe('1000');
  });
});
