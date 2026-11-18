import React from 'react';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';

// Simple test component
const TestButton = ({
  onClick,
  children,
  disabled = false,
}: {
  onClick?: () => void;
  children: React.ReactNode;
  disabled?: boolean;
}) => (
  <button onClick={onClick} disabled={disabled} data-testid="test-button">
    {children}
  </button>
);

describe('TestButton', () => {
  it('renders with text', () => {
    render(<TestButton>Click me</TestButton>);
    expect(screen.getByTestId('test-button')).toHaveTextContent('Click me');
  });

  it('is disabled when disabled prop is true', () => {
    render(<TestButton disabled>Disabled</TestButton>);
    expect(screen.getByTestId('test-button')).toBeDisabled();
  });

  it('is not disabled by default', () => {
    render(<TestButton>Enabled</TestButton>);
    expect(screen.getByTestId('test-button')).not.toBeDisabled();
  });
});

// Form validation tests
describe('Form Validation Helpers', () => {
  const validateAmount = (amount: string): boolean => {
    const num = parseFloat(amount);
    return !isNaN(num) && num > 0;
  };

  const validateAddress = (address: string): boolean => {
    // Basic Stellar address validation (starts with G and is 56 chars)
    return /^G[A-Z0-9]{55}$/.test(address);
  };

  it('validates positive amounts', () => {
    expect(validateAmount('100')).toBe(true);
    expect(validateAmount('0.001')).toBe(true);
    expect(validateAmount('1000000')).toBe(true);
  });

  it('rejects invalid amounts', () => {
    expect(validateAmount('0')).toBe(false);
    expect(validateAmount('-100')).toBe(false);
    expect(validateAmount('abc')).toBe(false);
    expect(validateAmount('')).toBe(false);
  });

  it('validates Stellar addresses', () => {
    expect(validateAddress('GDUMMY12345678901234567890123456789012345678901234567890')).toBe(true);
    expect(validateAddress('GDUMMY')).toBe(false);
    expect(validateAddress('')).toBe(false);
    expect(validateAddress('SDUMMY12345678901234567890123456789012345678901234567890')).toBe(false);
  });
});
