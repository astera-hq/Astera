'use client';

import LoadingSpinner from '@/components/LoadingSpinner';
import type { SimulationState } from '@/hooks/useTransactionSimulation';

interface EstimatedFeeProps {
  simulation: SimulationState;
}

export default function EstimatedFee({ simulation }: EstimatedFeeProps) {
  if (simulation.status === 'idle') return null;

  if (simulation.status === 'loading') {
    return (
      <div className="flex items-center justify-center gap-2 py-2">
        <LoadingSpinner size="sm" />
        <span className="text-xs text-brand-muted">Estimating network fee...</span>
      </div>
    );
  }

  if (simulation.status === 'error') {
    return (
      <div className="py-2 px-3 bg-red-900/20 border border-red-800/50 rounded-xl text-xs text-red-300">
        {simulation.error}
      </div>
    );
  }

  if (simulation.status === 'success' && simulation.feeEstimate) {
    const { feeInXlm, instructions, minResourceFee } = simulation.feeEstimate;
    return (
      <div className="py-2 px-3 bg-brand-gold/10 border border-brand-gold/20 rounded-xl text-xs space-y-1">
        <div className="flex items-center justify-between text-brand-muted">
          <span>Estimated network fee</span>
          <span className="text-white font-medium">~{feeInXlm.toFixed(6)} XLM</span>
        </div>
        <div className="flex items-center justify-between text-brand-muted/60">
          <span>Compute units</span>
          <span>{instructions.toLocaleString()}</span>
        </div>
      </div>
    );
  }

  return null;
}
