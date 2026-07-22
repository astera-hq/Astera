'use client';

import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
} from 'recharts';
import type { LiquidityForecastPoint } from '@/lib/types';
import { formatUSDC } from '@/lib/stellar';

interface LiquidityForecastChartProps {
  data: LiquidityForecastPoint[];
  isLoading?: boolean;
  /** Optional queued withdrawal demand to overlay as a reference line/label. */
  queuedDemand?: bigint;
  title?: string;
}

/**
 * #865: renders `get_liquidity_forecast`'s projected-available-liquidity points as an
 * area chart. Reused by both the investor portfolio page (per-token, compact) and the
 * admin liquidity dashboard (per-token, larger, with queued-demand context).
 */
export function LiquidityForecastChart({
  data,
  isLoading,
  queuedDemand,
  title = 'Projected Liquidity',
}: LiquidityForecastChartProps) {
  if (isLoading) {
    return (
      <div className="p-4 bg-brand-card border border-brand-border rounded-2xl">
        <div className="bg-brand-dark/50 animate-pulse rounded h-5 w-32 mb-3" />
        <div className="bg-brand-dark/50 animate-pulse rounded h-40 w-full" />
      </div>
    );
  }

  if (!data || data.length === 0) {
    return null;
  }

  const chartData = data.map((p) => ({
    day: p.day,
    projected: Number(p.projectedAvailable) / 1e7,
  }));

  return (
    <div className="p-4 bg-brand-card border border-brand-border rounded-2xl">
      <div className="flex items-center justify-between mb-3">
        <h4 className="text-sm font-semibold text-white flex items-center gap-2">
          <span className="w-1.5 h-4 bg-brand-gold rounded-full" />
          {title}
        </h4>
        {queuedDemand !== undefined && queuedDemand > 0n && (
          <span className="text-xs text-brand-muted">
            {formatUSDC(queuedDemand)} queued
          </span>
        )}
      </div>
      <div className="h-40">
        <ResponsiveContainer width="100%" height="100%">
          <AreaChart data={chartData} margin={{ top: 5, right: 10, left: 0, bottom: 0 }}>
            <defs>
              <linearGradient id="liquidityForecastGradient" x1="0" y1="0" x2="0" y2="1">
                <stop offset="5%" stopColor="#D4A843" stopOpacity={0.4} />
                <stop offset="95%" stopColor="#D4A843" stopOpacity={0} />
              </linearGradient>
            </defs>
            <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.06)" />
            <XAxis
              dataKey="day"
              tick={{ fill: 'rgba(255,255,255,0.4)', fontSize: 10 }}
              tickFormatter={(d: number) => `d${d}`}
            />
            <YAxis
              tick={{ fill: 'rgba(255,255,255,0.4)', fontSize: 10 }}
              tickFormatter={(v: number) => `$${v >= 1000 ? `${(v / 1000).toFixed(0)}k` : v.toFixed(0)}`}
              width={44}
            />
            <Tooltip
              contentStyle={{
                backgroundColor: '#1a1a2e',
                border: '1px solid rgba(255,255,255,0.1)',
                borderRadius: '8px',
                color: '#fff',
                fontSize: '12px',
              }}
              labelFormatter={(d) => `Day ${d}`}
              formatter={(value) => [`$${Number(value).toLocaleString()}`, 'Projected']}
            />
            <Area
              type="monotone"
              dataKey="projected"
              name="Projected"
              stroke="#D4A843"
              strokeWidth={2}
              fill="url(#liquidityForecastGradient)"
            />
          </AreaChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
