'use client';

/**
 * #863: kinked interest-rate curve chart — utilization on the x-axis, rate
 * (APY) on the y-axis, with the pool's current operating point highlighted.
 * The curve itself is computed client-side from the on-chain
 * `RateModelConfig` via the shared `computeCurrentRateBps` port, so rendering
 * costs zero RPC round-trips.
 */

import { useMemo } from 'react';
import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  ReferenceDot,
  ReferenceLine,
} from 'recharts';
import type { RateModelConfig } from '@/lib/types';
import { computeCurrentRateBps, sampleRateCurve } from '@/lib/rate-model';

interface RateCurveChartProps {
  config: RateModelConfig;
  /** Current pool utilization in bps; the marker is omitted when undefined. */
  currentUtilizationBps?: number;
  /** Current live rate in bps; falls back to the curve value at the marker. */
  currentRateBps?: number;
  title?: string;
}

export function RateCurveChart({
  config,
  currentUtilizationBps,
  currentRateBps,
  title = 'Interest Rate Curve',
}: RateCurveChartProps) {
  const data = useMemo(
    () =>
      sampleRateCurve(config).map((p) => ({
        utilization: p.utilizationBps / 100, // percent
        rate: p.rateBps / 100, // percent APY
      })),
    [config],
  );

  const hasMarker = currentUtilizationBps !== undefined;
  const markerUtilization = hasMarker ? currentUtilizationBps / 100 : 0;
  // The marker's y comes from the live rate when provided, otherwise the
  // curve evaluated at the exact current utilization.
  const markerRate = hasMarker
    ? (currentRateBps ?? computeCurrentRateBps(currentUtilizationBps, config)) / 100
    : 0;

  return (
    <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
      <div className="flex items-center justify-between mb-1">
        <h3 className="text-lg font-bold text-white flex items-center gap-2">
          <span className="w-2 h-6 bg-brand-gold rounded-full" />
          {title}
        </h3>
        {hasMarker && (
          <p className="text-xs text-brand-muted">
            now: {markerUtilization.toFixed(1)}% util → {markerRate.toFixed(2)}% APY
          </p>
        )}
      </div>
      <p className="text-xs text-brand-muted mb-4">
        Kink at {(config.optimalUtilizationBps / 100).toFixed(0)}% utilization — rates rise steeply
        beyond it to attract deposits and slow draw-downs.
      </p>
      <div className="h-72 overflow-x-hidden">
        <ResponsiveContainer width="100%" height="100%">
          <LineChart data={data} margin={{ top: 5, right: 20, left: 10, bottom: 5 }}>
            <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.06)" />
            <XAxis
              dataKey="utilization"
              type="number"
              domain={[0, 100]}
              tick={{ fill: 'rgba(255,255,255,0.4)', fontSize: 11 }}
              tickFormatter={(val: number) => `${val}%`}
            />
            <YAxis
              dataKey="rate"
              type="number"
              domain={[0, 'auto']}
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
              formatter={(value) => [`${Number(value ?? 0).toFixed(2)}%`, 'APY']}
              labelFormatter={(label) => `Utilization: ${Number(label).toFixed(1)}%`}
            />
            <ReferenceLine
              x={config.optimalUtilizationBps / 100}
              stroke="rgba(255,255,255,0.25)"
              strokeDasharray="4 4"
              label={{
                value: 'kink',
                fill: 'rgba(255,255,255,0.4)',
                fontSize: 10,
                position: 'top',
              }}
            />
            <Line
              type="monotone"
              dataKey="rate"
              stroke="#C9A84C"
              strokeWidth={2}
              dot={false}
              isAnimationActive={false}
            />
            {hasMarker && (
              <ReferenceDot
                x={markerUtilization}
                y={markerRate}
                r={6}
                fill="#C9A84C"
                stroke="#fff"
                strokeWidth={2}
              />
            )}
          </LineChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
