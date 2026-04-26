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
import type { CreditScoreBucket } from '@/lib/analytics';

interface CreditScoreDistributionChartProps {
  data: CreditScoreBucket[];
  isLoading: boolean;
}

const SCORE_COLORS = [
  '#EF4444', // 0-300: Red
  '#F97316', // 300-500: Orange
  '#EAB308', // 500-650: Yellow
  '#84CC16', // 650-750: Lime
  '#10B981', // 750-850: Green
  '#06B6D4', // 850+: Cyan
];

export function CreditScoreDistributionChart({
  data,
  isLoading,
}: CreditScoreDistributionChartProps) {
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
        <span className="w-2 h-6 bg-cyan-500 rounded-full" />
        Credit Score Distribution
      </h3>
      <div className="h-72">
        <ResponsiveContainer width="100%" height="100%">
          <BarChart data={data} margin={{ top: 5, right: 20, left: 10, bottom: 5 }}>
            <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.06)" />
            <XAxis
              dataKey="range"
              tick={{ fill: 'rgba(255,255,255,0.4)', fontSize: 11 }}
              axisLine={{ stroke: 'rgba(255,255,255,0.1)' }}
            />
            <YAxis
              tick={{ fill: 'rgba(255,255,255,0.4)', fontSize: 11 }}
              axisLine={{ stroke: 'rgba(255,255,255,0.1)' }}
              allowDecimals={false}
            />
            <Tooltip
              contentStyle={{
                backgroundColor: '#1a1a2e',
                border: '1px solid rgba(255,255,255,0.1)',
                borderRadius: '8px',
                color: '#fff',
              }}
              formatter={(value: number) => [`${value} SMEs`, 'Count']}
            />
            <Bar dataKey="count" name="SME Count" radius={[4, 4, 0, 0]}>
              {data.map((entry, index) => (
                <Cell key={`cell-${index}`} fill={SCORE_COLORS[index] || '#6366F1'} />
              ))}
            </Bar>
          </BarChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
