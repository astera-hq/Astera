'use client';

import { useCallback, useEffect, useState } from 'react';
import { useTranslations } from 'next-intl';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { Skeleton } from '@/components/Skeleton';
import { parseStellarAddress } from '@/lib/types';
import {
  buildSubmitScreeningResultTx,
  fetchComplianceServiceFlags,
  getComplianceHistory,
  getComplianceRecord,
  listComplianceFlagged,
  listCompliancePendingReview,
  submitTx,
  type ComplianceRecordUi,
  type ComplianceStatusUi,
  type RiskTierUi,
} from '@/lib/contracts';

export default function AdminCompliancePage() {
  const t = useTranslations('Admin.compliance');
  const { wallet } = useStore();
  const [loading, setLoading] = useState(true);
  const [txLoading, setTxLoading] = useState(false);
  const [flagged, setFlagged] = useState<string[]>([]);
  const [pending, setPending] = useState<string[]>([]);
  const [alerts, setAlerts] = useState<
    Array<{ id: string; address: string; reason: string; at: string; pattern: string }>
  >([]);

  const [lookupAddress, setLookupAddress] = useState('');
  const [record, setRecord] = useState<ComplianceRecordUi | null>(null);
  const [history, setHistory] = useState<ComplianceRecordUi[]>([]);

  const [decisionAddress, setDecisionAddress] = useState('');
  const [decisionStatus, setDecisionStatus] = useState<ComplianceStatusUi>('Cleared');
  const [decisionReason, setDecisionReason] = useState('0');
  const [decisionTier, setDecisionTier] = useState<RiskTierUi>('Low');
  const [decisionNotes, setDecisionNotes] = useState('');

  const loadQueues = useCallback(async () => {
    setLoading(true);
    try {
      const [f, p, a] = await Promise.all([
        listComplianceFlagged().catch(() => [] as string[]),
        listCompliancePendingReview().catch(() => [] as string[]),
        fetchComplianceServiceFlags(),
      ]);
      setFlagged(f);
      setPending(p);
      setAlerts(a);
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadQueues();
  }, [loadQueues]);

  async function signAndSubmit(xdr: string) {
    const freighter = await import('@stellar/freighter-api');
    const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
      networkPassphrase: NETWORK,
      address: wallet.address!,
    });
    if (signError) throw new Error(signError.message || 'Signing rejected.');
    await submitTx(signedTxXdr);
  }

  async function handleLookup(e: React.FormEvent) {
    e.preventDefault();
    if (!lookupAddress.trim()) return;
    try {
      const addr = parseStellarAddress(lookupAddress.trim());
      const [r, h] = await Promise.all([getComplianceRecord(addr), getComplianceHistory(addr)]);
      setRecord(r);
      setHistory(h);
      setDecisionAddress(addr);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : t('lookupFailed'));
    }
  }

  async function handleDecision(e: React.FormEvent) {
    e.preventDefault();
    if (!wallet.address) {
      toast.error(t('screeningDecision'));
      return;
    }
    setTxLoading(true);
    try {
      const screener = parseStellarAddress(wallet.address);
      const address = parseStellarAddress(decisionAddress.trim());
      const expiresAt =
        decisionStatus === 'Cleared' ? Math.floor(Date.now() / 1000) + 180 * 24 * 60 * 60 : 0;
      const xdr = await buildSubmitScreeningResultTx({
        screener,
        address,
        status: decisionStatus,
        reasonCode: parseInt(decisionReason, 10) || 0,
        riskTier: decisionTier,
        expiresAt,
        notesHash: decisionNotes.slice(0, 64) || 'admin-decision',
      });
      await signAndSubmit(xdr);
      toast.success(t('submitResult'));
      await loadQueues();
      const [r, h] = await Promise.all([
        getComplianceRecord(address),
        getComplianceHistory(address),
      ]);
      setRecord(r);
      setHistory(h);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : t('transactionFailed'));
    } finally {
      setTxLoading(false);
    }
  }

  return (
    <div className="space-y-8">
      <div>
        <h1 className="text-2xl font-semibold text-white">{t('title')}</h1>
        <p className="text-sm text-brand-muted mt-1">{t('description')}</p>
      </div>

      {loading ? (
        <Skeleton className="h-40 w-full" />
      ) : (
        <div className="grid md:grid-cols-2 gap-6">
          <QueueCard title={t('pendingReview')} addresses={pending} onPick={setDecisionAddress} />
          <QueueCard title={t('flaggedBlocked')} addresses={flagged} onPick={setDecisionAddress} />
        </div>
      )}

      <section className="rounded-xl border border-brand-border bg-brand-card p-5 space-y-4">
        <h2 className="text-lg font-medium text-white">{t('lookupAddress')}</h2>
        <form onSubmit={handleLookup} className="flex flex-col sm:flex-row gap-3">
          <input
            className="flex-1 rounded-lg bg-brand-bg border border-brand-border px-3 py-2 text-sm text-white"
            placeholder={t('addressPlaceholder')}
            value={lookupAddress}
            onChange={(e) => setLookupAddress(e.target.value)}
          />
          <button
            type="submit"
            className="rounded-lg bg-brand-accent px-4 py-2 text-sm font-medium text-black"
          >
            {t('lookup')}
          </button>
        </form>
        {record && (
          <div className="text-sm text-brand-muted space-y-1">
            <p>
              {t('statusLabel', {
                status: record.status,
                risk: record.riskTier,
                reason: record.reasonCode,
              })}
            </p>
            <p>
              {t('screenedAt', { at: record.screenedAt || '—', expires: record.expiresAt || '—' })}
            </p>
            <p className="truncate">{t('screenedBy', { screener: record.screenedBy || '—' })}</p>
          </div>
        )}
        {history.length > 0 && (
          <div className="overflow-x-auto">
            <table className="w-full text-left text-sm">
              <thead className="text-brand-muted">
                <tr>
                  <th className="py-1 pr-3">{t('tableStatus')}</th>
                  <th className="py-1 pr-3">{t('tableReason')}</th>
                  <th className="py-1 pr-3">{t('tableTier')}</th>
                  <th className="py-1 pr-3">{t('tableAt')}</th>
                  <th className="py-1">{t('tableNotes')}</th>
                </tr>
              </thead>
              <tbody>
                {history.map((h, i) => (
                  <tr key={i} className="border-t border-brand-border text-white">
                    <td className="py-1 pr-3">{h.status}</td>
                    <td className="py-1 pr-3">{h.reasonCode}</td>
                    <td className="py-1 pr-3">{h.riskTier}</td>
                    <td className="py-1 pr-3">{h.screenedAt}</td>
                    <td className="py-1 truncate max-w-[12rem]">{h.notesHash}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <section className="rounded-xl border border-brand-border bg-brand-card p-5 space-y-4">
        <h2 className="text-lg font-medium text-white">{t('screeningDecision')}</h2>
        <form onSubmit={handleDecision} className="grid sm:grid-cols-2 gap-3">
          <input
            className="sm:col-span-2 rounded-lg bg-brand-bg border border-brand-border px-3 py-2 text-sm text-white"
            placeholder={t('targetAddress')}
            value={decisionAddress}
            onChange={(e) => setDecisionAddress(e.target.value)}
            required
          />
          <select
            className="rounded-lg bg-brand-bg border border-brand-border px-3 py-2 text-sm text-white"
            value={decisionStatus}
            onChange={(e) => setDecisionStatus(e.target.value as ComplianceStatusUi)}
          >
            <option value="Cleared">{t('cleared')}</option>
            <option value="Flagged">{t('flagged')}</option>
            <option value="Blocked">{t('blocked')}</option>
            <option value="PendingReview">{t('pendingReviewOption')}</option>
          </select>
          <select
            className="rounded-lg bg-brand-bg border border-brand-border px-3 py-2 text-sm text-white"
            value={decisionTier}
            onChange={(e) => setDecisionTier(e.target.value as RiskTierUi)}
          >
            <option value="Low">{t('low')}</option>
            <option value="Medium">{t('medium')}</option>
            <option value="High">{t('high')}</option>
          </select>
          <input
            className="rounded-lg bg-brand-bg border border-brand-border px-3 py-2 text-sm text-white"
            placeholder={t('reasonCode')}
            value={decisionReason}
            onChange={(e) => setDecisionReason(e.target.value)}
          />
          <input
            className="rounded-lg bg-brand-bg border border-brand-border px-3 py-2 text-sm text-white"
            placeholder={t('notesHash')}
            value={decisionNotes}
            onChange={(e) => setDecisionNotes(e.target.value)}
          />
          <button
            type="submit"
            disabled={txLoading || !wallet.address}
            className="sm:col-span-2 rounded-lg bg-brand-accent px-4 py-2 text-sm font-medium text-black disabled:opacity-50"
          >
            {txLoading ? t('submitting') : t('submitResult')}
          </button>
        </form>
      </section>

      <section className="rounded-xl border border-brand-border bg-brand-card p-5 space-y-3">
        <h2 className="text-lg font-medium text-white">{t('monitoringAlerts')}</h2>
        {alerts.length === 0 ? (
          <p className="text-sm text-brand-muted">{t('noAlerts')}</p>
        ) : (
          <ul className="space-y-2 text-sm">
            {alerts.map((a) => (
              <li
                key={a.id}
                className="rounded-lg border border-brand-border px-3 py-2 text-brand-muted"
              >
                <span className="text-white">{a.pattern}</span> · {a.address.slice(0, 12)}… ·{' '}
                {a.reason}
                <span className="block text-xs opacity-70">{a.at}</span>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

function QueueCard({
  title,
  addresses,
  onPick,
}: {
  title: string;
  addresses: string[];
  onPick: (a: string) => void;
}) {
  const t = useTranslations('Admin.compliance');
  return (
    <div className="rounded-xl border border-brand-border bg-brand-card p-5">
      <h2 className="text-lg font-medium text-white mb-3">
        {title} <span className="text-brand-muted text-sm font-normal">({addresses.length})</span>
      </h2>
      {addresses.length === 0 ? (
        <p className="text-sm text-brand-muted">{t('empty')}</p>
      ) : (
        <ul className="space-y-2">
          {addresses.map((a) => (
            <li key={a}>
              <button
                type="button"
                onClick={() => onPick(a)}
                className="w-full text-left text-sm text-brand-accent hover:underline truncate"
              >
                {a}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
