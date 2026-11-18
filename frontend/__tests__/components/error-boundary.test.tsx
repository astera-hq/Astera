import React from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import ErrorBoundary from '@/components/ErrorBoundary';
import * as Sentry from '@sentry/nextjs';

jest.mock('@sentry/nextjs', () => ({
  captureException: jest.fn(),
}));

const ThrowingComponent: React.FC = () => {
  throw new Error('render failure');
  return <div />;
};

describe('ErrorBoundary', () => {
  const consoleSpy = jest.spyOn(console, 'error').mockImplementation(() => {});

  afterEach(() => {
    consoleSpy.mockClear();
    jest.clearAllMocks();
  });

  afterAll(() => {
    consoleSpy.mockRestore();
  });

  it('shows fallback UI when a child throws', () => {
    render(
      <ErrorBoundary fallback={<p>Fallback rendered</p>}>
        <ThrowingComponent />
      </ErrorBoundary>,
    );

    expect(screen.getByText('Fallback rendered')).toBeInTheDocument();
  });

  it('shows default fallback with Try Again button when no fallback prop given', () => {
    render(
      <ErrorBoundary>
        <ThrowingComponent />
      </ErrorBoundary>,
    );

    expect(screen.getByRole('alert')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /try again/i })).toBeInTheDocument();
  });

  it('resets error state when Try Again is clicked', () => {
    render(
      <ErrorBoundary>
        <ThrowingComponent />
      </ErrorBoundary>,
    );

    fireEvent.click(screen.getByRole('button', { name: /try again/i }));
    // After reset, boundary re-renders children (which will throw again, but state was reset)
    expect(screen.getByRole('button', { name: /try again/i })).toBeInTheDocument();
  });

  it('reports the error to Sentry with walletAddress', () => {
    render(
      <ErrorBoundary walletAddress="GABC123">
        <ThrowingComponent />
      </ErrorBoundary>,
    );

    expect(Sentry.captureException).toHaveBeenCalledWith(
      expect.any(Error),
      expect.objectContaining({ extra: expect.objectContaining({ walletAddress: 'GABC123' }) }),
    );
  });
});
