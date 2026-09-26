'use client';

import { useState } from 'react';
import useSWR from 'swr';
import toast from 'react-hot-toast';
import type { PremiumConfig } from '@/../packages/sdk/src/generated/insurance';
import {
  getAcceptedTokens,
  getPremiumConfig,
  getReserveStatus,
  checkReserveHealth,
  getMinReserveAmount,
  getInsuranceContractLink,
  buildSetPremiumConfigTx,
  buildSetMinCoverageRatioTx,
  buildSetMinReserveAmountTx,
  buildFundReserveFromTreasuryTx,
  buildSetInsuranceContractTx,
} from '@/lib/contracts';
import { submitTx, stablecoinLabel, formatUSDC, INSURANCE_CONTRACT_ID } from '@/lib/stellar';
import { useStore } from '@/lib/store';

const DEFAULT_PREMIUM_CONFIG: PremiumConfig = {
  base_rate_bps: 200,
  tenor_bps_per_day: 2,
  risk_tiers: [
    { min_score: 0, max_score: 549, risk_multiplier_bps: 20_000 },
    { min_score: 550, max_score: 699, risk_multiplier_bps: 15_000 },
    { min_score: 700, max_score: 1000, risk_multiplier_bps: 10_000 },
  ],
  default_risk_multiplier_bps: 25_000,
  min_premium_bps: 50,
  max_premium_bps: 2_000,
  default_coverage_bps: 8_000,
};

async function loadTokenData(token: string) {
  const [reserve, health, minReserve] = await Promise.all([
    getReserveStatus(token),
    checkReserveHealth(token),
    getMinReserveAmount(token),
  ]);
  return { reserve, health, minReserve };
}

export default function InsuranceAdminPage() {
  const { wallet } = useStore();
  const [selectedToken, setSelectedToken] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [fundAmount, setFundAmount] = useState('');
  const [minRatioBps, setMinRatioBps] = useState('');
  const [minReserveInput, setMinReserveInput] = useState('');
  const [editingConfig, setEditingConfig] = useState<PremiumConfig | null>(null);

  const { data: tokens } = useSWR('insurance-admin-tokens', () => getAcceptedTokens());
  const token = selectedToken ?? tokens?.[0] ?? null;

  const { data: tokenData, mutate: mutateTokenData } = useSWR(
    token ? ['insurance-admin-token-data', token] : null,
    ([, t]) => loadTokenData(t),
  );
  const { data: premiumConfig, mutate: mutatePremiumConfig } = useSWR(
    'insurance-admin-premium-config',
    () => getPremiumConfig(),
  );
  const { data: poolLink, mutate: mutatePoolLink } = useSWR('insurance-admin-pool-link', () =>
    getInsuranceContractLink(),
  );

  const activeConfig = editingConfig ?? premiumConfig ?? DEFAULT_PREMIUM_CONFIG;

  async function sign(txXdr: string): Promise<void> {
    if (!wallet.address) throw new Error('Connect your admin wallet first.');
    const freighter = await import('@stellar/freighter-api');
    const { signedTxXdr, error: signError } = await freighter.signTransaction(txXdr, {
      networkPassphrase: 'Test SDF Network ; September 2015',
      address: wallet.address,
    });
    if (signError) throw new Error(signError.message || 'Signing rejected.');
    await submitTx(signedTxXdr);
  }

  async function savePremiumConfig() {
    if (!wallet.address) {
      toast.error('Connect your admin wallet to save on-chain.');
      return;
    }
    setSaving(true);
    try {
      const txXdr = await buildSetPremiumConfigTx(wallet.address, activeConfig);
      await sign(txXdr);
      toast.success('Premium configuration saved on-chain.');
      setEditingConfig(null);
      await mutatePremiumConfig();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : 'Failed to save premium configuration.');
    } finally {
      setSaving(false);
    }
  }

  async function saveMinCoverageRatio() {
    if (!wallet.address || !token || !minRatioBps) return;
    setSaving(true);
    try {
      const txXdr = await buildSetMinCoverageRatioTx(wallet.address, token, Number(minRatioBps));
      await sign(txXdr);
      toast.success('Minimum coverage ratio updated.');
      setMinRatioBps('');
      await mutateTokenData();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : 'Failed to update minimum coverage ratio.');
    } finally {
      setSaving(false);
    }
  }

  async function saveMinReserveAmount() {
    if (!wallet.address || !token || !minReserveInput) return;
    setSaving(true);
    try {
      const txXdr = await buildSetMinReserveAmountTx(
        wallet.address,
        token,
        BigInt(minReserveInput),
      );
      await sign(txXdr);
      toast.success('Minimum reserve amount updated.');
      setMinReserveInput('');
      await mutateTokenData();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : 'Failed to update minimum reserve amount.');
    } finally {
      setSaving(false);
    }
  }

  async function fundReserve() {
    if (!wallet.address || !token || !fundAmount) return;
    setSaving(true);
    try {
      const txXdr = await buildFundReserveFromTreasuryTx(wallet.address, token, BigInt(fundAmount));
      await sign(txXdr);
      toast.success('Reserve funded from treasury.');
      setFundAmount('');
      await mutateTokenData();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : 'Failed to fund reserve.');
    } finally {
      setSaving(false);
    }
  }

  async function linkPoolToInsurance() {
    if (!wallet.address || !INSURANCE_CONTRACT_ID) return;
    setSaving(true);
    try {
      const txXdr = await buildSetInsuranceContractTx(wallet.address, INSURANCE_CONTRACT_ID);
      await sign(txXdr);
      toast.success('Pool linked to the insurance reserve.');
      await mutatePoolLink();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : 'Failed to link pool to insurance reserve.');
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="min-h-screen bg-gray-50">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        <div className="mb-8">
          <h1 className="text-3xl font-bold text-gray-900">Insurance Reserve</h1>
          <p className="mt-2 text-gray-600">
            Manage default-insurance premium pricing, per-token reserve health, and the pool ↔
            insurance link.
          </p>
        </div>

        <div className="mb-6 bg-white rounded-lg shadow-md p-6 border border-gray-200">
          <div className="flex justify-between items-center">
            <div>
              <h2 className="text-lg font-semibold text-gray-900">Pool integration</h2>
              <p className="text-sm text-gray-500 mt-1">
                {poolLink
                  ? `Pool is linked to insurance contract ${poolLink.slice(0, 8)}…${poolLink.slice(-4)}`
                  : 'Pool is not yet linked to an insurance contract — automatic coverage purchase at funding time is disabled.'}
              </p>
            </div>
            <button
              onClick={() => void linkPoolToInsurance()}
              disabled={saving || !INSURANCE_CONTRACT_ID}
              className="bg-blue-600 text-white py-2 px-4 rounded-lg font-medium hover:bg-blue-700 disabled:bg-gray-300"
            >
              Link pool
            </button>
          </div>
        </div>

        {tokens && tokens.length > 0 && (
          <div className="mb-6">
            <label className="block text-sm font-medium text-gray-700 mb-2">Select Token</label>
            <select
              value={token ?? ''}
              onChange={(e) => setSelectedToken(e.target.value)}
              className="block w-full max-w-xs rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
            >
              {tokens.map((t) => (
                <option key={t} value={t}>
                  {stablecoinLabel(t)}
                </option>
              ))}
            </select>
          </div>
        )}

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          <div className="bg-white rounded-lg shadow-md p-6 border border-gray-200">
            <div className="flex justify-between items-center mb-6">
              <h2 className="text-xl font-semibold text-gray-900">Reserve health</h2>
              {tokenData && (
                <span
                  className={`px-3 py-1 rounded-full text-xs font-medium ${
                    tokenData.health.is_healthy
                      ? 'bg-green-100 text-green-800'
                      : 'bg-red-100 text-red-800'
                  }`}
                >
                  {tokenData.health.is_healthy ? 'Healthy' : 'Needs top-up'}
                </span>
              )}
            </div>

            {tokenData ? (
              <div className="space-y-4">
                <div className="grid grid-cols-2 gap-4 text-sm">
                  <p className="text-gray-500">
                    Total reserves
                    <span className="block text-lg font-semibold text-gray-900">
                      {formatUSDC(tokenData.reserve.total_reserves)}
                    </span>
                  </p>
                  <p className="text-gray-500">
                    Covered exposure
                    <span className="block text-lg font-semibold text-gray-900">
                      {formatUSDC(tokenData.reserve.total_covered_exposure)}
                    </span>
                  </p>
                  <p className="text-gray-500">
                    Premiums collected
                    <span className="block text-lg font-semibold text-gray-900">
                      {formatUSDC(tokenData.reserve.total_premiums_collected)}
                    </span>
                  </p>
                  <p className="text-gray-500">
                    Claims paid
                    <span className="block text-lg font-semibold text-gray-900">
                      {formatUSDC(tokenData.reserve.total_claims_paid)}
                    </span>
                  </p>
                </div>
                <p className="text-sm text-gray-500">
                  Coverage ratio:{' '}
                  <span className="font-medium text-gray-900">
                    {(tokenData.reserve.coverage_ratio_bps / 100).toFixed(1)}%
                  </span>{' '}
                  (floor {(tokenData.reserve.min_coverage_ratio_bps / 100).toFixed(1)}%)
                </p>

                <div className="pt-4 border-t border-gray-200 space-y-3">
                  <div className="flex gap-2">
                    <input
                      type="number"
                      placeholder="Fund amount (stroops)"
                      value={fundAmount}
                      onChange={(e) => setFundAmount(e.target.value)}
                      className="block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                    />
                    <button
                      onClick={() => void fundReserve()}
                      disabled={saving || !fundAmount}
                      className="whitespace-nowrap bg-blue-600 text-white py-2 px-4 rounded-lg font-medium hover:bg-blue-700 disabled:bg-gray-300"
                    >
                      Fund reserve
                    </button>
                  </div>
                  <div className="flex gap-2">
                    <input
                      type="number"
                      placeholder={`Min coverage ratio bps (current ${tokenData.reserve.min_coverage_ratio_bps})`}
                      value={minRatioBps}
                      onChange={(e) => setMinRatioBps(e.target.value)}
                      className="block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                    />
                    <button
                      onClick={() => void saveMinCoverageRatio()}
                      disabled={saving || !minRatioBps}
                      className="whitespace-nowrap bg-gray-700 text-white py-2 px-4 rounded-lg font-medium hover:bg-gray-800 disabled:bg-gray-300"
                    >
                      Set floor
                    </button>
                  </div>
                  <div className="flex gap-2">
                    <input
                      type="number"
                      placeholder={`Min reserve amount (current ${tokenData.minReserve.toString()})`}
                      value={minReserveInput}
                      onChange={(e) => setMinReserveInput(e.target.value)}
                      className="block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                    />
                    <button
                      onClick={() => void saveMinReserveAmount()}
                      disabled={saving || !minReserveInput}
                      className="whitespace-nowrap bg-gray-700 text-white py-2 px-4 rounded-lg font-medium hover:bg-gray-800 disabled:bg-gray-300"
                    >
                      Set min
                    </button>
                  </div>
                </div>
              </div>
            ) : (
              <p className="text-gray-500 text-sm">Loading reserve data…</p>
            )}
          </div>

          <div className="bg-white rounded-lg shadow-md p-6 border border-gray-200">
            <div className="flex justify-between items-center mb-6">
              <h2 className="text-xl font-semibold text-gray-900">Premium configuration</h2>
              {!editingConfig ? (
                <button
                  onClick={() => setEditingConfig({ ...activeConfig })}
                  className="bg-blue-600 text-white py-2 px-4 rounded-lg font-medium hover:bg-blue-700"
                >
                  Edit
                </button>
              ) : (
                <div className="flex gap-2">
                  <button
                    onClick={() => void savePremiumConfig()}
                    disabled={saving}
                    className="bg-blue-600 text-white py-2 px-4 rounded-lg font-medium hover:bg-blue-700 disabled:bg-gray-300"
                  >
                    {saving ? 'Saving…' : 'Save'}
                  </button>
                  <button
                    onClick={() => setEditingConfig(null)}
                    className="bg-gray-200 text-gray-800 py-2 px-4 rounded-lg font-medium hover:bg-gray-300"
                  >
                    Cancel
                  </button>
                </div>
              )}
            </div>

            <div className="space-y-4">
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  Base rate (bps/year)
                </label>
                <input
                  type="number"
                  value={activeConfig.base_rate_bps}
                  disabled={!editingConfig}
                  onChange={(e) =>
                    setEditingConfig((prev) => ({
                      ...(prev ?? activeConfig),
                      base_rate_bps: Number(e.target.value),
                    }))
                  }
                  className="block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                />
              </div>
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  Tenor bps per day
                </label>
                <input
                  type="number"
                  value={activeConfig.tenor_bps_per_day}
                  disabled={!editingConfig}
                  onChange={(e) =>
                    setEditingConfig((prev) => ({
                      ...(prev ?? activeConfig),
                      tenor_bps_per_day: Number(e.target.value),
                    }))
                  }
                  className="block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                />
              </div>
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-1">
                    Min premium (bps)
                  </label>
                  <input
                    type="number"
                    value={activeConfig.min_premium_bps}
                    disabled={!editingConfig}
                    onChange={(e) =>
                      setEditingConfig((prev) => ({
                        ...(prev ?? activeConfig),
                        min_premium_bps: Number(e.target.value),
                      }))
                    }
                    className="block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-1">
                    Max premium (bps)
                  </label>
                  <input
                    type="number"
                    value={activeConfig.max_premium_bps}
                    disabled={!editingConfig}
                    onChange={(e) =>
                      setEditingConfig((prev) => ({
                        ...(prev ?? activeConfig),
                        max_premium_bps: Number(e.target.value),
                      }))
                    }
                    className="block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                  />
                </div>
              </div>
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  Default coverage (bps)
                </label>
                <input
                  type="number"
                  value={activeConfig.default_coverage_bps}
                  disabled={!editingConfig}
                  onChange={(e) =>
                    setEditingConfig((prev) => ({
                      ...(prev ?? activeConfig),
                      default_coverage_bps: Number(e.target.value),
                    }))
                  }
                  className="block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                />
                <p className="mt-1 text-xs text-gray-500">
                  {(activeConfig.default_coverage_bps / 100).toFixed(0)}% of principal insured per
                  purchase
                </p>
              </div>
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  Default risk multiplier (bps)
                </label>
                <input
                  type="number"
                  value={activeConfig.default_risk_multiplier_bps}
                  disabled={!editingConfig}
                  onChange={(e) =>
                    setEditingConfig((prev) => ({
                      ...(prev ?? activeConfig),
                      default_risk_multiplier_bps: Number(e.target.value),
                    }))
                  }
                  className="block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                />
              </div>

              <div className="pt-2">
                <h3 className="text-sm font-medium text-gray-700 mb-2">Credit-score risk tiers</h3>
                <div className="space-y-2">
                  {activeConfig.risk_tiers.map((tier, i) => (
                    <div key={i} className="grid grid-cols-3 gap-2 text-sm">
                      <input
                        type="number"
                        value={tier.min_score}
                        disabled={!editingConfig}
                        placeholder="Min score"
                        onChange={(e) => {
                          const risk_tiers = activeConfig.risk_tiers.map((t, j) =>
                            j === i ? { ...t, min_score: Number(e.target.value) } : t,
                          );
                          setEditingConfig({ ...activeConfig, risk_tiers });
                        }}
                        className="rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                      />
                      <input
                        type="number"
                        value={tier.max_score}
                        disabled={!editingConfig}
                        placeholder="Max score"
                        onChange={(e) => {
                          const risk_tiers = activeConfig.risk_tiers.map((t, j) =>
                            j === i ? { ...t, max_score: Number(e.target.value) } : t,
                          );
                          setEditingConfig({ ...activeConfig, risk_tiers });
                        }}
                        className="rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                      />
                      <input
                        type="number"
                        value={tier.risk_multiplier_bps}
                        disabled={!editingConfig}
                        placeholder="Multiplier bps"
                        onChange={(e) => {
                          const risk_tiers = activeConfig.risk_tiers.map((t, j) =>
                            j === i ? { ...t, risk_multiplier_bps: Number(e.target.value) } : t,
                          );
                          setEditingConfig({ ...activeConfig, risk_tiers });
                        }}
                        className="rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                      />
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </div>
        </div>

        <div className="mt-8 bg-yellow-50 border border-yellow-200 rounded-lg p-6">
          <h3 className="text-lg font-semibold text-yellow-900 mb-3">Guidelines</h3>
          <div className="space-y-2 text-sm text-yellow-800">
            <p>
              <strong>Coverage ratio floor:</strong> new coverage purchases are rejected once the
              reserve&apos;s coverage ratio would fall below this floor — raising it protects
              solvency at the cost of rejecting more purchases during high demand.
            </p>
            <p>
              <strong>Min reserve amount:</strong> purely informational — it drives the
              &quot;Healthy&quot;/&quot;Needs top-up&quot; signal shown here, it does not block
              purchases or claims on its own.
            </p>
            <p>
              <strong>Risk tiers:</strong> resolved by credit-score range; a score outside every
              configured tier falls back to the default risk multiplier, which should stay
              conservative (high) since missing data is itself a risk signal.
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}
