'use client';

import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Legend,
} from 'recharts';
import type { YieldPoint } from '@/lib/analytics';

interface YieldPerformanceChartProps {
  data: YieldPoint[];
  isLoading: boolean;
}

export function YieldPerformanceChart({ data, isLoading }: YieldPerformanceChartProps) {
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
        <span className="w-2 h-6 bg-emerald-500 rounded-full" />
        Yield Performance
      </h3>
      <div className="h-72">
        <ResponsiveContainer width="100%" height="100%">
          <AreaChart data={data} margin={{ top: 5, right: 20, left: 10, bottom: 5 }}>
            <defs>
              <linearGradient id="yieldGradient" x1="0" y1="0" x2="0" y2="1">
                <stop offset="5%" stopColor="#10B981" stopOpacity={0.3} />
                <stop offset="95%" stopColor="#10B981" stopOpacity={0} />
              </linearGradient>
            </defs>
            <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.06)" />
            <XAxis
              dataKey="time"
              tick={{ fill: 'rgba(255,255,255,0.4)', fontSize: 11 }}
              tickFormatter={(val: string) => val.slice(5)}
            />
            <YAxis
              tick={{ fill: 'rgba(255,255,255,0.4)', fontSize: 11 }}
              tickFormatter={(val: number) => `${val}%`}
            />
            <Tooltip
              contentStyle={{
                backgroundColor: '#1a1a2e',
                border: '1px solid rgba(255,255,255,0.1)',
                borderRadius: '8px',
                color: '#fff',
              }}
              formatter={(value: number, name: string) => {
                if (name === 'apy') return `${value}% APY`;
                if (name === 'interest') return `${value}%`;
                return value;
              }}
            />
            <Legend wrapperStyle={{ fontSize: '12px', color: 'rgba(255,255,255,0.6)' }} />
            <Area
              type="monotone"
              dataKey="apy"
              name="APY %"
              stroke="#10B981"
              strokeWidth={2}
              fillOpacity={1}
              fill="url(#yieldGradient)"
              dot={false}
              activeDot={{ r: 4, fill: '#10B981' }}
            />
            <Area
              type="monotone"
              dataKey="interest"
              name="Interest %"
              stroke="#6366F1"
              strokeWidth={2}
              fillOpacity={0.1}
              fill="#6366F1"
              dot={false}
              activeDot={{ r: 4, fill: '#6366F1' }}
            />
          </AreaChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
