'use client';

import { useState, useEffect, useCallback } from 'react';
import { TrancheClass, TranchePool } from '@/../packages/sdk/src/generated/tranche';
import { getTranchePool, buildTrancheDepositTx } from '@/lib/contracts';
import { NETWORK, submitTx, toStroops } from '@/lib/stellar';
import { useStore } from '@/lib/store';
import toast from 'react-hot-toast';
import { useTranslations } from 'next-intl';

const INDEXER_URL = process.env.NEXT_PUBLIC_INDEXER_URL || 'http://localhost:3001';
const TOKENS = ['USDC', 'USDT', 'EURC'];

interface TrancheApyData {
  senior: number;
  junior: number;
}

async function fetchTrailingApy(token: string): Promise<TrancheApyData> {
  const res = await fetch(`${INDEXER_URL}/tranches/${encodeURIComponent(token)}/apy`);
  if (!res.ok) throw new Error('indexer unavailable');
  const data = await res.json();
  return {
    senior: data.senior?.realizedApyPct ?? 0,
    junior: data.junior?.realizedApyPct ?? 0,
  };
}

interface TrancheCardProps {
  token: string;
  trancheClass: TrancheClass;
  pool: TranchePool;
  targetApy: number;
  trailingApy: number;
  onDeposit: () => void;
  t: ReturnType<typeof useTranslations<'Tranches'>>;
}

function TrancheCard({ token, trancheClass, pool, targetApy, trailingApy, onDeposit, t }: TrancheCardProps) {
  const isSenior = trancheClass === TrancheClass.Senior;
  const config = pool.config;
  const accounting = isSenior ? pool.senior : pool.junior;
  const deposited = Number(accounting.deposited) / 1e7;
  const available = Number(accounting.available) / 1e7;
  const deployed = Number(accounting.deployed) / 1e7;
  const earned = Number(accounting.earned) / 1e7;
  const losses = Number(accounting.losses) / 1e7;
  const trancheName = isSenior ? t('senior') : t('junior');

  return (
    <div className="bg-white rounded-lg shadow-md p-6 border border-gray-200">
      <div className="flex justify-between items-start mb-4">
        <div>
          <h2 className="text-2xl font-bold text-gray-900">
            {trancheName} {t('trancheSuffix')}
          </h2>
          <p className="text-sm text-gray-500">{token}</p>
        </div>
        <span
          className={`px-3 py-1 rounded-full text-xs font-medium ${isSenior ? 'bg-blue-100 text-blue-800' : 'bg-purple-100 text-purple-800'}`}
        >
          {isSenior ? t('lowerRisk') : t('higherRisk')}
        </span>
      </div>

      <div className="grid grid-cols-2 gap-4 mb-6">
        <div>
          <p className="text-sm text-gray-500">{t('targetApy')}</p>
          <p className="text-2xl font-semibold text-gray-900">
            {isSenior ? `${targetApy.toFixed(1)}%` : t('residual')}
          </p>
        </div>
        <div>
          <p className="text-sm text-gray-500">{t('trailingApy')}</p>
          <p className={`text-2xl font-semibold ${trailingApy >= targetApy ? 'text-green-600' : 'text-orange-600'}`}>
            {trailingApy.toFixed(1)}%
          </p>
        </div>
      </div>

      <div className="space-y-3 mb-6">
        <div className="flex justify-between text-sm">
          <span className="text-gray-500">{t('totalDeposited')}</span>
          <span className="font-medium">${deposited.toLocaleString()}</span>
        </div>
        <div className="flex justify-between text-sm">
          <span className="text-gray-500">{t('availableToFund')}</span>
          <span className="font-medium">${available.toLocaleString()}</span>
        </div>
        <div className="flex justify-between text-sm">
          <span className="text-gray-500">{t('deployedInInvoices')}</span>
          <span className="font-medium">${deployed.toLocaleString()}</span>
        </div>
        <div className="flex justify-between text-sm">
          <span className="text-gray-500">{t('totalEarned')}</span>
          <span className="font-medium text-green-600">${earned.toLocaleString()}</span>
        </div>
        {losses > 0 && (
          <div className="flex justify-between text-sm">
            <span className="text-gray-500">{t('lossesAbsorbed')}</span>
            <span className="font-medium text-red-600">${losses.toLocaleString()}</span>
          </div>
        )}
      </div>

      <div className="bg-gray-50 rounded-lg p-4 mb-6">
        <h3 className="text-sm font-medium text-gray-900 mb-2">{t('riskMetrics')}</h3>
        {isSenior ? (
          <div className="space-y-1 text-sm text-gray-600">
            <div className="flex justify-between">
              <span>{t('advanceRateCap')}</span>
              <span>{(config.senior_advance_rate_bps / 100).toFixed(0)}%</span>
            </div>
            <div className="flex justify-between">
              <span>{t('juniorBuffer')}</span>
              <span>${(Number(pool.junior.deposited) / 1e7).toLocaleString()}</span>
            </div>
          </div>
        ) : (
          <div className="space-y-1 text-sm text-gray-600">
            <div className="flex justify-between">
              <span>{t('firstLossPosition')}</span>
              <span>{(config.junior_first_loss_bps / 100).toFixed(0)}%</span>
            </div>
            <div className="flex justify-between">
              <span>{t('residualUpside')}</span>
              <span>{t('unlimited')}</span>
            </div>
          </div>
        )}
      </div>

      <button
        onClick={onDeposit}
        disabled={available === 0}
        className="w-full bg-blue-600 text-white py-3 px-4 rounded-lg font-medium hover:bg-blue-700 disabled:bg-gray-300 disabled:cursor-not-allowed transition-colors"
      >
        {available === 0 ? t('noCapacity') : t('depositTo', { tranche: trancheName })}
      </button>
    </div>
  );
}

function RiskExplainer({ t }: { t: ReturnType<typeof useTranslations<'Tranches'>> }) {
  return (
    <div className="bg-blue-50 border border-blue-200 rounded-lg p-6">
      <h3 className="text-lg font-semibold text-blue-900 mb-3">{t('riskExplainerTitle')}</h3>
      <div className="space-y-4 text-sm text-blue-800">
        <div>
          <h4 className="font-medium mb-1">{t('seniorTitle')}</h4>
          <p className="text-blue-700">{t('seniorDesc')}</p>
        </div>
        <div>
          <h4 className="font-medium mb-1">{t('juniorTitle')}</h4>
          <p className="text-blue-700">{t('juniorDesc')}</p>
        </div>
        <div>
          <h4 className="font-medium mb-1">{t('waterfallTitle')}</h4>
          <p className="text-blue-700">{t('waterfallDesc')}</p>
        </div>
      </div>
    </div>
  );
}

export default function TranchesPage() {
  const { wallet } = useStore();
  const t = useTranslations('Tranches');
  const [selectedToken, setSelectedToken] = useState('USDC');
  const [depositAmount, setDepositAmount] = useState<number>(0);
  const [showDepositModal, setShowDepositModal] = useState(false);
  const [selectedTranche, setSelectedTranche] = useState<TrancheClass | null>(null);
  const [pool, setPool] = useState<TranchePool | null>(null);
  const [apy, setApy] = useState<TrancheApyData>({ senior: 0, junior: 0 });
  const [loading, setLoading] = useState(true);
  const [depositing, setDepositing] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [poolData, apyData] = await Promise.all([
        getTranchePool(selectedToken).catch(() => null),
        fetchTrailingApy(selectedToken).catch(() => ({ senior: 0, junior: 0 })),
      ]);
      if (poolData) setPool(poolData);
      setApy(apyData);
    } finally {
      setLoading(false);
    }
  }, [selectedToken]);

  useEffect(() => {
    load();
  }, [load]);

  const handleDeposit = (tranche: TrancheClass) => {
    setSelectedTranche(tranche);
    setShowDepositModal(true);
  };

  const confirmDeposit = async () => {
    if (!wallet.connected || !wallet.address || selectedTranche === null || depositAmount <= 0)
      return;
    setDepositing(true);
    try {
      const amount = toStroops(depositAmount);
      const xdr = await buildTrancheDepositTx(wallet.address, selectedToken, selectedTranche, amount);
      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
        networkPassphrase: NETWORK,
        address: wallet.address,
      });
      if (signError) throw new Error(signError.message);
      await submitTx(signedTxXdr);
      const trancheName = selectedTranche === TrancheClass.Senior ? t('senior') : t('junior');
      toast.success(t('toastDepositSuccess', { amount: depositAmount, token: selectedToken, tranche: trancheName }));
      setShowDepositModal(false);
      setDepositAmount(0);
      await load();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('toastDepositFailed'));
    } finally {
      setDepositing(false);
    }
  };

  const seniorTargetApy = pool ? pool.config.senior_target_yield_bps / 100 : 10;
  const selectedTrancheName = selectedTranche === TrancheClass.Senior ? t('senior') : t('junior');

  return (
    <div className="min-h-screen bg-gray-50">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        <div className="mb-8">
          <h1 className="text-3xl font-bold text-gray-900">{t('title')}</h1>
          <p className="mt-2 text-gray-600">{t('description')}</p>
        </div>

        <div className="mb-6">
          <label className="block text-sm font-medium text-gray-700 mb-2">{t('selectToken')}</label>
          <select
            value={selectedToken}
            onChange={(e) => setSelectedToken(e.target.value)}
            className="block w-full max-w-xs rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
          >
            {TOKENS.map((tk) => (
              <option key={tk} value={tk}>{tk}</option>
            ))}
          </select>
        </div>

        <div className="mb-8">
          <RiskExplainer t={t} />
        </div>

        {loading && (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-6 mb-8">
            {[0, 1].map((i) => (
              <div key={i} className="bg-white rounded-lg shadow-md p-6 border border-gray-200 animate-pulse">
                <div className="h-8 bg-gray-200 rounded w-1/2 mb-4" />
                <div className="h-16 bg-gray-100 rounded mb-4" />
                <div className="h-32 bg-gray-100 rounded" />
              </div>
            ))}
          </div>
        )}

        {!loading && pool && (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-6 mb-8">
            <TrancheCard
              token={selectedToken}
              trancheClass={TrancheClass.Senior}
              pool={pool}
              targetApy={seniorTargetApy}
              trailingApy={apy.senior}
              onDeposit={() => handleDeposit(TrancheClass.Senior)}
              t={t}
            />
            <TrancheCard
              token={selectedToken}
              trancheClass={TrancheClass.Junior}
              pool={pool}
              targetApy={0}
              trailingApy={apy.junior}
              onDeposit={() => handleDeposit(TrancheClass.Junior)}
              t={t}
            />
          </div>
        )}

        {showDepositModal && (
          <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
            <div className="bg-white rounded-lg p-6 max-w-md w-full mx-4">
              <h2 className="text-xl font-bold mb-4">
                {t('depositModalTitle', { tranche: selectedTrancheName })}
              </h2>
              {!wallet.connected && (
                <p className="text-sm text-red-600 mb-4">{t('connectWalletToDeposit')}</p>
              )}
              <div className="mb-4">
                <label className="block text-sm font-medium text-gray-700 mb-2">
                  {t('amountLabel', { token: selectedToken })}
                </label>
                <input
                  type="number"
                  value={depositAmount}
                  onChange={(e) => setDepositAmount(Number(e.target.value))}
                  className="block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                  placeholder={t('amountPlaceholder')}
                />
              </div>
              <div className="flex space-x-3">
                <button
                  onClick={confirmDeposit}
                  disabled={depositing || depositAmount <= 0 || !wallet.connected}
                  className="flex-1 bg-blue-600 text-white py-2 px-4 rounded-lg font-medium hover:bg-blue-700 disabled:bg-gray-300 disabled:cursor-not-allowed"
                >
                  {depositing ? t('submitting') : t('confirmDeposit')}
                </button>
                <button
                  onClick={() => setShowDepositModal(false)}
                  className="flex-1 bg-gray-200 text-gray-800 py-2 px-4 rounded-lg font-medium hover:bg-gray-300"
                >
                  {t('cancel')}
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
