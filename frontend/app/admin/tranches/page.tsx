'use client';

import { useState } from 'react';
import toast from 'react-hot-toast';
import { useTranslations } from 'next-intl';
import { TrancheConfig } from '@/../packages/sdk/src/generated/tranche';
import { getTrancheClient, sdkSigner } from '@/lib/sdk';
import { NETWORK, submitTx } from '@/lib/stellar';
import { useStore } from '@/lib/store';

interface TokenTrancheConfig {
  token: string;
  enabled: boolean;
  config: TrancheConfig;
  seniorShareToken: string;
  juniorShareToken: string;
}

export default function TrancheConfigPage() {
  const t = useTranslations('Admin.tranches');
  const { wallet } = useStore();
  const [selectedToken, setSelectedToken] = useState('USDC');
  const [saving, setSaving] = useState(false);
  const [configs, setConfigs] = useState<TokenTrancheConfig[]>([
    {
      token: 'USDC',
      enabled: true,
      config: {
        senior_target_yield_bps: 1000,
        senior_advance_rate_bps: 8000,
        junior_first_loss_bps: 10000,
      },
      seniorShareToken: 'sASTR-SR-USDC',
      juniorShareToken: 'sASTR-JR-USDC',
    },
    {
      token: 'USDT',
      enabled: false,
      config: {
        senior_target_yield_bps: 1000,
        senior_advance_rate_bps: 8000,
        junior_first_loss_bps: 10000,
      },
      seniorShareToken: '',
      juniorShareToken: '',
    },
  ]);

  const [editingConfig, setEditingConfig] = useState<TrancheConfig | null>(null);
  const [showSaveModal, setShowSaveModal] = useState(false);

  const handleEnableTranche = (token: string) => {
    setConfigs(configs.map((c) => (c.token === token ? { ...c, enabled: true } : c)));
  };

  const handleDisableTranche = (token: string) => {
    setConfigs(configs.map((c) => (c.token === token ? { ...c, enabled: false } : c)));
  };

  const handleEditConfig = (token: string) => {
    const config = configs.find((c) => c.token === token);
    if (config) {
      setEditingConfig({ ...config.config });
      setSelectedToken(token);
    }
  };

  const handleSaveConfig = async () => {
    if (!editingConfig) return;
    if (!wallet.connected || !wallet.address) {
      toast.error('Connect your admin wallet to save on-chain.');
      return;
    }
    setSaving(true);
    try {
      await getTrancheClient().setTrancheConfig({
        signer: await sdkSigner(wallet.address),
        admin: wallet.address,
        token: selectedToken,
        seniorTargetYieldBps: editingConfig.senior_target_yield_bps,
        seniorAdvanceRateBps: editingConfig.senior_advance_rate_bps,
        juniorFirstLossBps: editingConfig.junior_first_loss_bps,
      });

      // Reflect the persisted config in local state after the on-chain write.
      setConfigs(
        configs.map((c) =>
          c.token === selectedToken ? { ...c, config: { ...editingConfig } } : c,
        ),
      );
      setEditingConfig(null);
      setShowSaveModal(false);
      toast.success(`${selectedToken} tranche configuration saved on-chain.`);
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Failed to save configuration.');
    } finally {
      setSaving(false);
    }
  };

  const currentConfig = configs.find((c) => c.token === selectedToken);

  return (
    <div className="min-h-screen bg-gray-50">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        {/* Header */}
        <div className="mb-8">
          <h1 className="text-3xl font-bold text-gray-900">{t('title')}</h1>
          <p className="mt-2 text-gray-600">
            {t('description')}
          </p>
        </div>

        {/* Token Selector */}
        <div className="mb-6">
          <label className="block text-sm font-medium text-gray-700 mb-2">{t('selectToken')}</label>
          <select
            value={selectedToken}
            onChange={(e) => setSelectedToken(e.target.value)}
            className="block w-full max-w-xs rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
          >
            {configs.map((c) => (
              <option key={c.token} value={c.token}>
                {c.token}
              </option>
            ))}
          </select>
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Configuration Panel */}
          <div className="bg-white rounded-lg shadow-md p-6 border border-gray-200">
            <div className="flex justify-between items-center mb-6">
              <h2 className="text-xl font-semibold text-gray-900">{selectedToken} {t('configuration')}</h2>
              <span
                className={`px-3 py-1 rounded-full text-xs font-medium ${
                  currentConfig?.enabled
                    ? 'bg-green-100 text-green-800'
                    : 'bg-gray-100 text-gray-800'
                }`}
              >
                {currentConfig?.enabled ? t('enabled') : t('disabled')}
              </span>
            </div>

            {currentConfig?.enabled ? (
              <div className="space-y-6">
                {/* Senior Target Yield */}
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-2">
                    {t('seniorTargetYield')}
                  </label>
                  <input
                    type="number"
                    value={
                      editingConfig?.senior_target_yield_bps ??
                      currentConfig.config.senior_target_yield_bps
                    }
                    onChange={(e) =>
                      setEditingConfig((prev) => ({
                        ...prev!,
                        senior_target_yield_bps: Number(e.target.value),
                      }))
                    }
                    className="block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                    disabled={!editingConfig}
                  />
                  <p className="mt-1 text-xs text-gray-500">
                    {(currentConfig.config.senior_target_yield_bps / 100).toFixed(1)}% {t('targetYield')}
                  </p>
                </div>

                {/* Senior Advance Rate */}
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-2">
                    {t('seniorAdvanceRate')}
                  </label>
                  <input
                    type="number"
                    value={
                      editingConfig?.senior_advance_rate_bps ??
                      currentConfig.config.senior_advance_rate_bps
                    }
                    onChange={(e) =>
                      setEditingConfig((prev) => ({
                        ...prev!,
                        senior_advance_rate_bps: Number(e.target.value),
                      }))
                    }
                    className="block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                    disabled={!editingConfig}
                  />
                  <p className="mt-1 text-xs text-gray-500">
                    {(currentConfig.config.senior_advance_rate_bps / 100).toFixed(0)}% {t('maxSeniorShare')}
                  </p>
                </div>

                {/* Junior First Loss */}
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-2">
                    {t('juniorFirstLoss')}
                  </label>
                  <input
                    type="number"
                    value={
                      editingConfig?.junior_first_loss_bps ??
                      currentConfig.config.junior_first_loss_bps
                    }
                    onChange={(e) =>
                      setEditingConfig((prev) => ({
                        ...prev!,
                        junior_first_loss_bps: Number(e.target.value),
                      }))
                    }
                    className="block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                    disabled={!editingConfig}
                  />
                  <p className="mt-1 text-xs text-gray-500">
                    {(currentConfig.config.junior_first_loss_bps / 100).toFixed(0)}% {t('juniorAbsorbs')}
                  </p>
                </div>

                {/* Share Tokens */}
                <div className="space-y-3">
                  <div>
                    <label className="block text-sm font-medium text-gray-700 mb-2">
                      {t('seniorShareToken')}
                    </label>
                    <input
                      type="text"
                      value={currentConfig.seniorShareToken}
                      className="block w-full rounded-md border-gray-300 shadow-sm bg-gray-50"
                      disabled
                    />
                  </div>
                  <div>
                    <label className="block text-sm font-medium text-gray-700 mb-2">
                      {t('juniorShareToken')}
                    </label>
                    <input
                      type="text"
                      value={currentConfig.juniorShareToken}
                      className="block w-full rounded-md border-gray-300 shadow-sm bg-gray-50"
                      disabled
                    />
                  </div>
                </div>

                {/* Actions */}
                <div className="flex space-x-3 pt-4 border-t border-gray-200">
                  {editingConfig ? (
                    <>
                      <button
                        onClick={handleSaveConfig}
                        disabled={saving}
                        className="flex-1 bg-blue-600 text-white py-2 px-4 rounded-lg font-medium hover:bg-blue-700 disabled:bg-gray-300 disabled:cursor-not-allowed"
                      >
                        {saving ? t('saving') : t('saveChanges')}
                      </button>
                      <button
                        onClick={() => setEditingConfig(null)}
                        className="flex-1 bg-gray-200 text-gray-800 py-2 px-4 rounded-lg font-medium hover:bg-gray-300"
                      >
                        {t('cancel')}
                      </button>
                    </>
                  ) : (
                    <>
                      <button
                        onClick={() => handleEditConfig(selectedToken)}
                        className="flex-1 bg-blue-600 text-white py-2 px-4 rounded-lg font-medium hover:bg-blue-700"
                      >
                        {t('editConfiguration')}
                      </button>
                      <button
                        onClick={() => handleDisableTranche(selectedToken)}
                        className="flex-1 bg-red-100 text-red-700 py-2 px-4 rounded-lg font-medium hover:bg-red-200"
                      >
                        {t('disableTranche')}
                      </button>
                    </>
                  )}
                </div>
              </div>
            ) : (
              <div className="text-center py-12">
                <p className="text-gray-500 mb-4">{t('notEnabled')} {selectedToken}</p>
                <button
                  onClick={() => handleEnableTranche(selectedToken)}
                  className="bg-blue-600 text-white py-2 px-6 rounded-lg font-medium hover:bg-blue-700"
                >
                  {t('enableTranche')}
                </button>
              </div>
            )}
          </div>

          {/* All Tokens Overview */}
          <div className="bg-white rounded-lg shadow-md p-6 border border-gray-200">
            <h2 className="text-xl font-semibold text-gray-900 mb-4">{t('allTokens')}</h2>
            <div className="space-y-3">
              {configs.map((config) => (
                <div
                  key={config.token}
                  className={`p-4 rounded-lg border ${
                    config.token === selectedToken
                      ? 'border-blue-500 bg-blue-50'
                      : 'border-gray-200'
                  }`}
                >
                  <div className="flex justify-between items-center">
                    <div>
                      <h3 className="font-medium text-gray-900">{config.token}</h3>
                      <div className="text-xs text-gray-500 mt-1">
                        {config.enabled ? (
                          <>
                            <span>
                              {t('yield')}: {(config.config.senior_target_yield_bps / 100).toFixed(1)}%
                            </span>
                            <span className="mx-2">•</span>
                            <span>
                              {t('advance')}: {(config.config.senior_advance_rate_bps / 100).toFixed(0)}%
                            </span>
                          </>
                        ) : (
                          t('notEnabled')
                        )}
                      </div>
                    </div>
                    <span
                      className={`px-2 py-1 rounded text-xs font-medium ${
                        config.enabled ? 'bg-green-100 text-green-800' : 'bg-gray-100 text-gray-600'
                      }`}
                    >
                      {config.enabled ? t('active') : t('inactive')}
                    </span>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>

        {/* Risk Guidelines */}
        <div className="mt-8 bg-yellow-50 border border-yellow-200 rounded-lg p-6">
          <h3 className="text-lg font-semibold text-yellow-900 mb-3">{t('guidelines')}</h3>
          <div className="space-y-2 text-sm text-yellow-800">
            <p>
              <strong>{t('seniorTargetYield')}:</strong> {t('seniorYieldDesc')}
            </p>
            <p>
              <strong>{t('seniorAdvanceRate')}:</strong> {t('advanceRateDesc')}
            </p>
            <p>
              <strong>{t('juniorFirstLoss')}:</strong> {t('firstLossDesc')}
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}
