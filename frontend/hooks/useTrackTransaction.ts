import { useCallback } from 'react';
import { useStore } from '@/lib/store';
import { submitTx, type TransactionProgress } from '@/lib/stellar';

/**
 * Hook that wraps `submitTx` and automatically tracks transaction status
 * in the Zustand store. Returns the same interface as `submitTx`.
 *
 * @param label - Human-readable label shown in the status tracker (e.g. "Deposit 100 USDC")
 */
export function useTrackTransaction(label: string) {
  const addTracked = useStore((s) => s.addTrackedTransaction);
  const updateTracked = useStore((s) => s.updateTrackedTransaction);

  const trackedSubmit = useCallback(
    async (signedXDR: string) => {
      let hash = '';

      const result = await submitTx(signedXDR, (progress: TransactionProgress) => {
        if (!hash && progress.hash) {
          hash = progress.hash;
          addTracked({
            hash: progress.hash,
            status: progress.status,
            label,
            error: progress.error,
            timestamp: Date.now(),
          });
        } else if (hash) {
          updateTracked(hash, {
            status: progress.status,
            error: progress.error,
          });
        }
      });

      return result;
    },
    [label, addTracked, updateTracked],
  );

  return trackedSubmit;
}
