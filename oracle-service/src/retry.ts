const NON_RETRIABLE_PATTERNS = [
  'Simulation failed',
  'Transaction failed on-chain',
  'Transaction failed:',
  'Document verification failed',
];

export function isRetriableError(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return !NON_RETRIABLE_PATTERNS.some((pattern) => message.includes(pattern));
}

export interface RetryOptions {
  maxAttempts: number;
  baseDelayMs: number;
}

const DEFAULT_OPTIONS: RetryOptions = {
  maxAttempts: 3,
  baseDelayMs: 2000,
};

export async function retryWithBackoff<T>(
  fn: () => Promise<T>,
  context: string,
  options: Partial<RetryOptions> = {},
): Promise<T> {
  const { maxAttempts, baseDelayMs } = { ...DEFAULT_OPTIONS, ...options };
  let lastError: Error | null = null;

  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    try {
      return await fn();
    } catch (error) {
      const err = error instanceof Error ? error : new Error(String(error));
      lastError = err;

      if (!isRetriableError(err)) {
        throw err;
      }

      if (attempt < maxAttempts) {
        const delay = baseDelayMs * Math.pow(2, attempt - 1);
        console.warn(
          `[Retry] ${context} failed (attempt ${attempt}/${maxAttempts}), retrying in ${delay}ms: ${err.message}`,
        );
        await new Promise((resolve) => setTimeout(resolve, delay));
      }
    }
  }

  throw lastError!;
}
