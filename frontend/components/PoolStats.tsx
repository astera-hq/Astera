import type { PoolConfig, PoolTokenTotals } from '@/lib/types';
import { formatUSDC } from '@/lib/stellar';

interface Props {
  config: PoolConfig;
  tokenTotals: PoolTokenTotals | null;
  tokenLabel: string;
}

export default function PoolStats({ config, tokenTotals, tokenLabel }: Props) {
  const deposited = tokenTotals?.totalDeposited ?? 0n;
  const deployed = tokenTotals?.totalDeployed ?? 0n;
  const paidOut = tokenTotals?.totalPaidOut ?? 0n;
  const available = deposited - deployed;

  const utilizationRate = deposited > 0n ? Number((deployed * 100n) / deposited) : 0;

  const apy = (config.yieldBps / 100).toFixed(1);

  return (
    <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
      <h2 className="text-lg font-semibold mb-1">Pool Overview</h2>
      <p className="text-xs text-brand-muted mb-6">Showing {tokenLabel} liquidity</p>

      <div className="grid grid-cols-2 gap-4 mb-6">
        <Stat label="Total Deposited" value={formatUSDC(deposited)} />
        <Stat label="Deployed" value={formatUSDC(deployed)} />
        <Stat label="Available" value={formatUSDC(available)} highlight />
        <Stat label="Total Paid In" value={formatUSDC(paidOut)} />
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
