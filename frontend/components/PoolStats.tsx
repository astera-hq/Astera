import type { PoolConfig, PoolTokenTotals } from '@/lib/types';
import { formatUSDC } from '@/lib/stellar';
import { Skeleton } from '@/components/Skeleton';

interface Props {
  config: PoolConfig;
  tokenTotals: PoolTokenTotals | null;
  tokenLabel: string;
}

export default function PoolStats({ config, tokenTotals, tokenLabel }: Props) {
  const deposited = tokenTotals?.totalDeposited ?? 0n;
  const deployed = tokenTotals?.totalDeployed ?? 0n;
  const paidOut = tokenTotals?.totalPaidOut ?? 0n;
  const feeRevenue = tokenTotals?.totalFeeRevenue ?? 0n;
  const available = deposited - deployed;

  const utilizationRate = deposited > 0n ? Number((deployed * 100n) / deposited) : 0;

  const apy = (config.yieldBps / 100).toFixed(1);
  const factoringFee = (config.factoringFeeBps / 100).toFixed(2);

  return (
    <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
      <h2 className="text-lg font-semibold mb-1">Pool Overview</h2>
      <p className="text-xs text-brand-muted mb-6">Showing {tokenLabel} liquidity</p>

      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 mb-6">
        <Stat label="Total Deposited" value={formatUSDC(deposited)} />
        <Stat label="Deployed" value={formatUSDC(deployed)} />
        <Stat label="Available" value={formatUSDC(available)} highlight />
        <Stat label="Total Paid In" value={formatUSDC(paidOut)} />
        <Stat label="Fee Revenue" value={formatUSDC(feeRevenue)} />
      </div>

      <div className="mb-4">
        <div className="flex justify-between text-sm mb-2">
          <span className="text-brand-muted">Utilization</span>
          <span className="font-medium">{utilizationRate}%</span>
        </div>
        <div className="h-2 bg-brand-border rounded-full overflow-hidden">
          <div
            className="h-full bg-brand-gold rounded-full transition-all"
            style={{ width: `${Math.min(utilizationRate, 100)}%` }}
          />
        </div>
      </div>

      <div className="flex items-center justify-between p-3 bg-brand-gold/10 border border-brand-gold/20 rounded-xl">
        <span className="text-sm text-brand-muted">Target APY</span>
        <span className="text-brand-gold font-bold text-lg">{apy}%</span>
      </div>

      <div className="flex items-center justify-between p-3 mt-3 bg-brand-dark rounded-xl border border-brand-border">
        <span className="text-sm text-brand-muted">Factoring Fee</span>
        <span className="text-white font-bold text-lg">{factoringFee}%</span>
      </div>
    </div>
  );
}

function Stat({ label, value, highlight }: { label: string; value: string; highlight?: boolean }) {
  return (
    <div className="p-3 bg-brand-dark rounded-xl border border-brand-border">
      <p className="text-xs text-brand-muted mb-1">{label}</p>
      <p className={`font-semibold text-sm ${highlight ? 'text-brand-gold' : 'text-white'}`}>
        {value}
      </p>
    </div>
  );
}

export function PoolStatsSkeleton() {
  return (
    <div className="p-6 bg-brand-card border border-brand-border rounded-2xl animate-pulse">
      <Skeleton className="h-5 w-32 mb-1" />
      <Skeleton className="h-3 w-48 mb-6" />

      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 mb-6">
        {[1, 2, 3, 4, 5].map((i) => (
          <div key={i} className="p-3 bg-brand-dark rounded-xl border border-brand-border">
            <Skeleton className="h-3 w-20 mb-2" />
            <Skeleton className="h-4 w-28" />
          </div>
        ))}
      </div>

      <div className="mb-4">
        <div className="flex justify-between text-sm mb-2">
          <Skeleton className="h-4 w-20" />
          <Skeleton className="h-4 w-10" />
        </div>
        <div className="h-2 bg-brand-border rounded-full overflow-hidden">
          <div className="h-full bg-brand-gold/40 rounded-full" style={{ width: '60%' }} />
        </div>
      </div>

      <div className="flex items-center justify-between p-3 bg-brand-gold/10 border border-brand-gold/20 rounded-xl">
        <Skeleton className="h-4 w-20" />
        <Skeleton className="h-6 w-14" />
      </div>

      <div className="flex items-center justify-between p-3 mt-3 bg-brand-dark rounded-xl border border-brand-border">
        <Skeleton className="h-4 w-24" />
        <Skeleton className="h-6 w-14" />
      </div>
    </div>
  );
}
