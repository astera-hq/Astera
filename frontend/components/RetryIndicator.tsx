'use client';

interface RetryIndicatorProps {
  error: unknown;
  isValidating?: boolean;
  onRetry: () => void;
  className?: string;
}

// #800: subtle "Retrying…" state while SWR's automatic backoff is in
// flight, falling back to an explicit error + manual "Retry" button once
// the automatic retries are exhausted. Renders nothing when there is no
// error.
export function RetryIndicator({
  error,
  isValidating,
  onRetry,
  className = '',
}: RetryIndicatorProps) {
  if (!error) return null;

  if (isValidating) {
    return (
      <p className={`text-xs text-[var(--muted)] ${className}`.trim()} role="status">
        Retrying…
      </p>
    );
  }

  return (
    <div
      className={`flex items-center gap-2 text-xs text-yellow-600 dark:text-yellow-300 ${className}`.trim()}
      role="alert"
    >
      <span>Failed to load data.</span>
      <button type="button" onClick={onRetry} className="underline hover:no-underline">
        Retry
      </button>
    </div>
  );
}
