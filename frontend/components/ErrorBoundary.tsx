'use client';

import React, { ErrorInfo } from 'react';
import * as Sentry from '@sentry/nextjs';

type Props = {
  children: React.ReactNode;
  fallback?: React.ReactNode;
  walletAddress?: string;
};

type State = { hasError: boolean };

export default class ErrorBoundary extends React.Component<Props, State> {
  state: State = { hasError: false };

  static getDerivedStateFromError(): State {
    return { hasError: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    Sentry.captureException(error, {
      extra: { walletAddress: this.props.walletAddress, ...info },
    });
  }

  render() {
    if (this.state.hasError) {
      return (
        this.props.fallback ?? (
          <div role="alert" className="p-4 text-center">
            <p className="mb-2">Something went wrong.</p>
            <button
              onClick={() => this.setState({ hasError: false })}
              className="px-4 py-2 rounded bg-blue-600 text-white hover:bg-blue-700"
            >
              Try Again
            </button>
          </div>
        )
      );
    }
    return this.props.children;
  }
}
