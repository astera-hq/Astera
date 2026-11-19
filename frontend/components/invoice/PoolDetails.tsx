'use client';

import { formatUSDC, truncateAddress } from '@/lib/stellar';
import { formatApyPercent } from '@/lib/apy';
import type { Invoice, PoolConfig, FundedInvoice } from '@/lib/types';

/** Pool configuration + funded-principal details. Returns null when absent. */
interface PoolDetailsProps {
  invoice: Invoice;
  poolConfig: PoolConfig | null;
  fundedInvoice: FundedInvoice | null;
}

export default function PoolDetails({ invoice, poolConfig, fundedInvoice }: PoolDetailsProps) {
  if (!poolConfig) return null;

  return (
    <div className="p-6 bg-brand-card border border-brand-border rounded-2xl mb-6">
      <h2 className="text-lg font-semibold mb-4">Pool Details</h2>
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 text-sm">
        <div>
          <p className="text-brand-muted mb-1">Pool Contract</p>
          <p className="font-mono text-xs break-all">{invoice.poolContract || '—'}</p>
        </div>
        <div>
          <p className="text-brand-muted mb-1">Pool Admin</p>
          <p className="font-mono text-xs break-all">{truncateAddress(poolConfig.admin)}</p>
        </div>
        <div>
          <p className="text-brand-muted mb-1">APY</p>
          <p>{formatApyPercent(poolConfig.yieldBps)}%</p>
        </div>
        <div>
          <p className="text-brand-muted mb-1">Factoring Fee</p>
          <p>{(poolConfig.factoringFeeBps / 100).toFixed(2)}%</p>
        </div>
      </div>
      {fundedInvoice && (
        <div className="mt-4 border-t border-brand-border pt-4 text-sm grid grid-cols-1 sm:grid-cols-2 gap-4">
          <div>
            <p className="text-brand-muted mb-1">Funding Token</p>
            <p className="font-mono text-xs break-all">
              {truncateAddress(fundedInvoice.token)}
            </p>
          </div>
          <div>
            <p className="text-brand-muted mb-1">Principal</p>
            <p>{formatUSDC(fundedInvoice.principal)}</p>
          </div>
        </div>
      )}
    </div>
  );
}
