'use client';

import type { TopSme } from '@/lib/analytics';
import { truncateAddress } from '@/lib/analytics';

interface TopSmesTableProps {
  data: TopSme[];
  isLoading: boolean;
}

export function TopSmesTable({ data, isLoading }: TopSmesTableProps) {
  if (isLoading) {
    return (
      <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
        <div className="bg-brand-dark/50 animate-pulse rounded h-6 w-40 mb-4" />
        <div className="space-y-3">
          {Array.from({ length: 5 }).map((_, i) => (
            <div key={i} className="flex items-center gap-4">
              <div className="bg-brand-dark/50 animate-pulse rounded h-5 w-8" />
              <div className="bg-brand-dark/50 animate-pulse rounded h-5 w-32" />
              <div className="bg-brand-dark/50 animate-pulse rounded h-5 w-16 ml-auto" />
              <div className="bg-brand-dark/50 animate-pulse rounded h-5 w-16 ml-auto" />
              <div className="bg-brand-dark/50 animate-pulse rounded h-5 w-16 ml-auto" />
            </div>
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
      <h3 className="text-lg font-bold text-white mb-4 flex items-center gap-2">
        <span className="w-2 h-6 bg-indigo-500 rounded-full" />
        Top SMEs by Volume
      </h3>

      {data.length === 0 ? (
        <div className="text-center py-8 text-brand-muted">
          <p className="text-sm">No SME data available yet.</p>
        </div>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-left">
            <thead>
              <tr className="border-b border-brand-border">
                <th className="px-4 py-3 text-[10px] font-bold uppercase tracking-widest text-brand-muted">
                  #
                </th>
                <th className="px-4 py-3 text-[10px] font-bold uppercase tracking-widest text-brand-muted">
                  SME Address
                </th>
                <th className="px-4 py-3 text-[10px] font-bold uppercase tracking-widest text-brand-muted text-right">
                  Invoices
                </th>
                <th className="px-4 py-3 text-[10px] font-bold uppercase tracking-widest text-brand-muted text-right">
                  Total Value
                </th>
                <th className="px-4 py-3 text-[10px] font-bold uppercase tracking-widest text-brand-muted text-right">
                  Default Rate
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-brand-border/50">
              {data.map((sme, index) => (
                <tr key={sme.address} className="hover:bg-white/5 transition-colors">
                  <td className="px-4 py-3 text-sm text-brand-muted font-mono">{index + 1}</td>
                  <td className="px-4 py-3 text-sm text-white font-mono">
                    {truncateAddress(sme.address)}
                  </td>
                  <td className="px-4 py-3 text-sm text-white text-right">{sme.invoiceCount}</td>
                  <td className="px-4 py-3 text-sm text-brand-gold text-right font-medium">
                    ${sme.totalValue.toLocaleString()}
                  </td>
                  <td className="px-4 py-3 text-sm text-right">
                    <span
                      className={`px-2 py-0.5 rounded-full text-xs font-bold ${
                        sme.defaultRate === 0
                          ? 'bg-green-500/20 text-green-500'
                          : sme.defaultRate <= 10
                            ? 'bg-yellow-500/20 text-yellow-500'
                            : 'bg-red-500/20 text-red-500'
                      }`}
                    >
                      {sme.defaultRate}%
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
