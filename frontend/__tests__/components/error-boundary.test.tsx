import React from 'react';
import { render, screen } from '@testing-library/react';
import ErrorBoundary from '@/components/ErrorBoundary';

function ThrowingComponent() {
  throw new Error('render failure');
}

describe('ErrorBoundary', () => {
  const consoleSpy = jest.spyOn(console, 'error').mockImplementation(() => {});

  afterEach(() => {
    consoleSpy.mockClear();
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

  it('reports the error through console.error', () => {
    render(
      <ErrorBoundary fallback={<p>Fallback rendered</p>}>
        <ThrowingComponent />
      </ErrorBoundary>,
    );

    expect(consoleSpy).toHaveBeenCalled();
  });
});
