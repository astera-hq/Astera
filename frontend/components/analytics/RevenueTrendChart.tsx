'use client';

import {
  BarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Cell,
} from 'recharts';
import { formatValue } from '@/lib/analytics';

interface RevenueTrendChartProps {
  /** Buckets with a display label and a fee value (monthly or weekly). */
  data: Array<{ label: string; fees: number }>;
  isLoading: boolean;
  /** Chart heading, e.g. "Monthly Fee Revenue". */
  title?: string;
}

export function RevenueTrendChart({
  data,
  isLoading,
  title = 'Fee Revenue',
}: RevenueTrendChartProps) {
  if (isLoading) {
    return (
      <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
        <div className="bg-brand-dark/50 animate-pulse rounded h-6 w-48 mb-4" />
        <div className="bg-brand-dark/50 animate-pulse rounded h-72 w-full" />
      </div>
    );
  }

  const hasData = data.length > 0 && data.some((d) => d.fees > 0);

  return (
    <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
      <h3 className="text-lg font-bold text-white mb-4 flex items-center gap-2">
        <span className="w-2 h-6 bg-brand-gold rounded-full" />
        {title}
      </h3>
      <div className="h-72 overflow-x-hidden">
        <ResponsiveContainer width="100%" height="100%">
          <BarChart data={data} margin={{ top: 5, right: 20, left: 10, bottom: 5 }}>
            <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.06)" />
            <XAxis
              dataKey="label"
              tick={{ fill: 'rgba(255,255,255,0.4)', fontSize: 11 }}
              interval="preserveStartEnd"
            />
            <YAxis
              tick={{ fill: 'rgba(255,255,255,0.4)', fontSize: 11 }}
              tickFormatter={(val: number) => formatValue(val)}
              width={60}
            />
            <Tooltip
              cursor={{ fill: 'rgba(255,255,255,0.04)' }}
              contentStyle={{
                backgroundColor: '#1a1a2e',
                border: '1px solid rgba(255,255,255,0.1)',
                borderRadius: '8px',
                color: '#fff',
              }}
              formatter={(value) =>
                `$${Number(value ?? 0).toLocaleString('en-US', { maximumFractionDigits: 0 })}`
              }
            />
            <Bar dataKey="fees" name="Fees" radius={[4, 4, 0, 0]}>
              {data.map((entry, index) => (
                <Cell
                  key={entry.label}
                  fill={index === data.length - 1 ? '#E0A93C' : 'rgba(224,169,60,0.45)'}
                />
              ))}
            </Bar>
          </BarChart>
        </ResponsiveContainer>
      </div>
      {!hasData && <p className="text-xs text-brand-muted mt-3">No fee revenue recorded yet.</p>}
    </div>
  );
}
