'use client';

import { useCallback, useEffect, useState } from 'react';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { Skeleton } from '@/components/Skeleton';
import {
  getReserveStatus,
  checkReserveHealth,
  getPremiumConfig,
} from '@/lib/contracts';
import type { PremiumConfig, ReserveFund, ReserveHealth } from '@/../packages/sdk/src/generated/insurance';

const INDEXER_URL = process.env.NEXT_PUBLIC_INDEXER_URL || 'http://localhost:3001';
const TOKENS = ['USDC', 'USDT', 'EURC'];

const HEALTH_STYLES: Record<string, string> = {
  healthy: 'bg-green-500/20 text-green-400 border-green-500/30',
  warning: 'bg-amber-500/20 text-amber-400 border-amber-500/30',
  critical: 'bg-red-500/20 text-red-400 border-red-500/30',
};

const CLAIM_STATUS_STYLES: Record<string, string> = {
  Pending: 'bg-amber-500/20 text-amber-400 border-amber-500/30',
  Approved: 'bg-green-500/20 text-green-400 border-green-500/30',
  Rejected: 'bg-red-500/20 text-red-400 border-red-500/30',
  Paid: 'bg-green-500/20 text-green-400 border-green-500/30',
};

export default function InvestInsurancePage() {
  const { wallet } = useStore();
  const [reserves, setReserves] = useState<Record<string, ReserveFund>>({});
  const [health, setHealth] = useState<Record<string, ReserveHealth>>({});
  const [premiumConfig, setPremiumConfig] = useState<PremiumConfig | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [config, ...results] = await Promise.allSettled([
        getPremiumConfig(),
        ...TOKENS.map((t) => getReserveStatus(t)),
        ...TOKENS.map((t) => checkReserveHealth(t)),
      ]);

      if (config.status === 'fulfilled') {
        setPremiumConfig(config.value);
      }

      const reserveMap: Record<string, ReserveFund> = {};
      const healthMap: Record<string, ReserveHealth> = {};

      TOKENS.forEach((token, i) => {
        const reserveResult = results[i];
        const healthResult = results[TOKENS.length + i];

        if (reserveResult.status === 'fulfilled' && reserveResult.value) {
          reserveMap[token] = reserveResult.value;
        }
        if (healthResult.status === 'fulfilled' && healthResult.value) {
          healthMap[token] = healthResult.value;
        }
      });

      setReserves(reserveMap);
      setHealth(healthMap);
    } catch (e) {
      console.error(e);
      toast.error('Failed to load insurance data.');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  if (!wallet.connected) {
    return (
      <div className="max-w-5xl">
        <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">
          Connect your wallet to view insurance reserve data.
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-5xl space-y-8">
      <div>
        <h1 className="text-3xl font-bold mb-2">Insurance Reserves</h1>
        <p className="text-brand-muted text-sm">
          View the health and coverage of insurance reserves backing invoices on the platform.
          Reserves protect lenders against borrower defaults.
        </p>
      </div>

      {loading ? (
        <Skeleton className="h-64 w-full rounded-2xl" />
      ) : (
        <>
          {/* Reserve overview cards */}
          <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
            {TOKENS.map((token) => {
              const fund = reserves[token];
              const h = health[token];
              if (!fund) return null;

              const reservesNum = Number(fund.total_reserves) / 1e7;
              const exposure = Number(fund.total_covered_exposure) / 1e7;
              const premiums = Number(fund.total_premiums_collected) / 1e7;
              const claimsPaid = Number(fund.total_claims_paid) / 1e7;
              const ratio = fund.coverage_ratio_bps / 100;
              const healthStatus = h?.is_healthy ? 'healthy' : h?.needs_top_up ? 'critical' : 'warning';

              return (
                <div key={token} className="p-6 bg-brand-card border border-brand-border rounded-2xl space-y-3">
                  <div className="flex items-center justify-between">
                    <h3 className="font-semibold text-lg">{token}</h3>
                    <span className={`px-2 py-0.5 rounded-full text-xs font-semibold border ${HEALTH_STYLES[healthStatus]}`}>
                      {healthStatus}
                    </span>
                  </div>
                  <div className="grid grid-cols-2 gap-3 text-sm">
                    <div>
                      <p className="text-xs text-brand-muted">Reserves</p>
                      <p className="font-semibold">{reservesNum.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}</p>
                    </div>
                    <div>
                      <p className="text-xs text-brand-muted">Exposure</p>
                      <p className="font-semibold">{exposure.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}</p>
                    </div>
                    <div>
                      <p className="text-xs text-brand-muted">Premiums Collected</p>
                      <p className="font-semibold text-green-400">{premiums.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}</p>
                    </div>
                    <div>
                      <p className="text-xs text-brand-muted">Claims Paid</p>
                      <p className="font-semibold text-red-400">{claimsPaid.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}</p>
                    </div>
                  </div>
                  <div className="pt-2 border-t border-brand-border">
                    <div className="flex items-center justify-between text-xs">
                      <span className="text-brand-muted">Coverage Ratio</span>
                      <span className={`font-semibold ${ratio >= 100 ? 'text-green-400' : ratio >= 50 ? 'text-amber-400' : 'text-red-400'}`}>
                        {ratio.toFixed(1)}%
                      </span>
                    </div>
                    <div className="mt-1 h-1.5 bg-brand-dark rounded-full overflow-hidden">
                      <div
                        className={`h-full rounded-full ${ratio >= 100 ? 'bg-green-400' : ratio >= 50 ? 'bg-amber-400' : 'bg-red-400'}`}
                        style={{ width: `${Math.min(ratio, 100)}%` }}
                      />
                    </div>
                  </div>
                </div>
              );
            })}
          </div>

          {/* Premium config */}
          {premiumConfig && (
            <div className="p-6 bg-brand-card border border-brand-border rounded-2xl space-y-4">
              <h2 className="font-semibold">Premium Configuration</h2>
              <div className="grid grid-cols-2 sm:grid-cols-4 gap-4 text-sm">
                <div>
                  <p className="text-xs text-brand-muted">Base Rate</p>
                  <p className="font-semibold">{(premiumConfig.base_rate_bps / 100).toFixed(2)}%</p>
                </div>
                <div>
                  <p className="text-xs text-brand-muted">Tenor Rate</p>
                  <p className="font-semibold">{(premiumConfig.tenor_bps_per_day / 100).toFixed(4)}%/day</p>
                </div>
                <div>
                  <p className="text-xs text-brand-muted">Min Premium</p>
                  <p className="font-semibold">{(premiumConfig.min_premium_bps / 100).toFixed(2)}%</p>
                </div>
                <div>
                  <p className="text-xs text-brand-muted">Max Premium</p>
                  <p className="font-semibold">{(premiumConfig.max_premium_bps / 100).toFixed(2)}%</p>
                </div>
              </div>
              {premiumConfig.risk_tiers.length > 0 && (
                <div className="pt-3 border-t border-brand-border">
                  <p className="text-xs text-brand-muted mb-2">Risk Tiers</p>
                  <div className="flex gap-2 flex-wrap">
                    {premiumConfig.risk_tiers.map((tier, i) => (
                      <span key={i} className="px-3 py-1 bg-brand-dark rounded-lg text-xs">
                        Score {tier.min_score}–{tier.max_score}: {(tier.risk_multiplier_bps / 100).toFixed(0)}x
                      </span>
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}

          {/* How it works */}
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl space-y-3">
            <h2 className="font-semibold">How Insurance Works</h2>
            <ul className="space-y-2 text-sm text-brand-muted">
              <li className="flex gap-2">
                <span className="text-brand-gold font-bold">1.</span>
                <span>SMEs pay a premium when their funded invoice is verified, purchasing default coverage.</span>
              </li>
              <li className="flex gap-2">
                <span className="text-brand-gold font-bold">2.</span>
                <span>Premiums flow into the reserve fund, growing the pool available for claims.</span>
              </li>
              <li className="flex gap-2">
                <span className="text-brand-gold font-bold">3.</span>
                <span>If a borrower defaults, the claim is filed and paid from the reserve fund.</span>
              </li>
              <li className="flex gap-2">
                <span className="text-brand-gold font-bold">4.</span>
                <span>Lenders benefit from reduced default risk — covered invoices are reimbursed from reserves.</span>
              </li>
            </ul>
          </div>
        </>
      )}
    </div>
  );
}
