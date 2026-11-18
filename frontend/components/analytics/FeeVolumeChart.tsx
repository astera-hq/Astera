'use client';

import {
  ComposedChart,
  Bar,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Legend,
} from 'recharts';
import type { FeeVsVolumePoint } from '@/lib/revenue';
import { formatValue } from '@/lib/analytics';

interface FeeVolumeChartProps {
  data: FeeVsVolumePoint[];
  isLoading: boolean;
}

export function FeeVolumeChart({ data, isLoading }: FeeVolumeChartProps) {
  if (isLoading) {
    return (
      <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
        <div className="bg-brand-dark/50 animate-pulse rounded h-6 w-48 mb-4" />
        <div className="bg-brand-dark/50 animate-pulse rounded h-72 w-full" />
      </div>
    );
  }

  return (
    <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
      <h3 className="text-lg font-bold text-white mb-4 flex items-center gap-2">
        <span className="w-2 h-6 bg-indigo-500 rounded-full" />
        Fee Rate vs. Funded Volume
      </h3>
      <div className="h-72 overflow-x-hidden">
        <ResponsiveContainer width="100%" height="100%">
          <ComposedChart data={data} margin={{ top: 5, right: 20, left: 10, bottom: 5 }}>
            <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.06)" />
            <XAxis
              dataKey="label"
              tick={{ fill: 'rgba(255,255,255,0.4)', fontSize: 11 }}
              interval="preserveStartEnd"
            />
            <YAxis
              yAxisId="volume"
              tick={{ fill: 'rgba(255,255,255,0.4)', fontSize: 11 }}
              tickFormatter={(val: number) => formatValue(val)}
              width={60}
            />
            <YAxis
              yAxisId="rate"
              orientation="right"
              domain={[0, 'auto']}
              tick={{ fill: 'rgba(255,255,255,0.4)', fontSize: 11 }}
              tickFormatter={(val: number) => `${val}%`}
              width={40}
            />
            <Tooltip
              cursor={{ fill: 'rgba(255,255,255,0.04)' }}
              contentStyle={{
                backgroundColor: '#1a1a2e',
                border: '1px solid rgba(255,255,255,0.1)',
                borderRadius: '8px',
                color: '#fff',
              }}
              formatter={(value, name) => {
                const n = Number(value ?? 0);
                if (name === 'fundedVolume')
                  return `$${n.toLocaleString('en-US', { maximumFractionDigits: 0 })}`;
                if (name === 'feeRatePct') return `${n}%`;
                return n;
              }}
            />
            <Legend wrapperStyle={{ fontSize: '12px', color: 'rgba(255,255,255,0.6)' }} />
            <Bar
              yAxisId="volume"
              dataKey="fundedVolume"
              name="Funded Volume"
              fill="rgba(99,102,241,0.45)"
              radius={[4, 4, 0, 0]}
            />
            <Line
              yAxisId="rate"
              type="monotone"
              dataKey="feeRatePct"
              name="Fee Rate"
              stroke="#E0A93C"
              strokeWidth={2}
              dot={{ r: 3, fill: '#E0A93C' }}
              activeDot={{ r: 5 }}
            />
          </ComposedChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
