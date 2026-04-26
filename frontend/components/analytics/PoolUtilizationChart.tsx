'use client';

import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Legend,
} from 'recharts';
import type { PoolUtilizationPoint } from '@/lib/analytics';

interface PoolUtilizationChartProps {
  data: PoolUtilizationPoint[];
  isLoading: boolean;
}

export function PoolUtilizationChart({ data, isLoading }: PoolUtilizationChartProps) {
  if (isLoading) {
    return (
      <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
        <div className="bg-brand-dark/50 animate-pulse rounded h-6 w-40 mb-4" />
        <div className="bg-brand-dark/50 animate-pulse rounded h-64 w-full" />
      </div>
    );
  }

  return (
    <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
      <h3 className="text-lg font-bold text-white mb-4 flex items-center gap-2">
        <span className="w-2 h-6 bg-brand-gold rounded-full" />
        Pool Utilization
      </h3>
      <div className="h-72">
        <ResponsiveContainer width="100%" height="100%">
          <LineChart data={data} margin={{ top: 5, right: 20, left: 10, bottom: 5 }}>
            <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.06)" />
            <XAxis
              dataKey="time"
              tick={{ fill: 'rgba(255,255,255,0.4)', fontSize: 11 }}
              tickFormatter={(val: string) => val.slice(5)} // MM-DD
            />
            <YAxis
              tick={{ fill: 'rgba(255,255,255,0.4)', fontSize: 11 }}
              tickFormatter={(val: number) => `${val}%`}
              domain={[0, 100]}
            />
            <Tooltip
              contentStyle={{
                backgroundColor: '#1a1a2e',
                border: '1px solid rgba(255,255,255,0.1)',
                borderRadius: '8px',
                color: '#fff',
              }}
              formatter={(value: number, name: string) => {
                if (name === 'utilization') return `${value}%`;
                if (name === 'deployed') return `$${value.toLocaleString()}`;
                if (name === 'deposited') return `$${value.toLocaleString()}`;
                return value;
              }}
            />
            <Legend wrapperStyle={{ fontSize: '12px', color: 'rgba(255,255,255,0.6)' }} />
            <Line
              type="monotone"
              dataKey="utilization"
              name="Utilization %"
              stroke="#D4A843"
              strokeWidth={2}
              dot={false}
              activeDot={{ r: 4, fill: '#D4A843' }}
            />
            <Line
              type="monotone"
              dataKey="deployed"
              name="Deployed ($)"
              stroke="#10B981"
              strokeWidth={2}
              dot={false}
              activeDot={{ r: 4, fill: '#10B981' }}
            />
            <Line
              type="monotone"
              dataKey="deposited"
              name="Deposited ($)"
              stroke="#6366F1"
              strokeWidth={2}
              dot={false}
              activeDot={{ r: 4, fill: '#6366F1' }}
            />
          </LineChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
