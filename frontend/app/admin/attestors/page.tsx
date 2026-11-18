'use client';

import { useCallback, useEffect, useState } from 'react';
import { useTranslations } from 'next-intl';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { Skeleton } from '@/components/Skeleton';
import { parseStellarAddress } from '@/lib/types';
import type { AttestorInfo, AttestorType, Attestation } from '@/lib/types';
import {
  listActiveAttestors,
  buildRegisterAttestorTx,
  buildDeactivateAttestorTx,
  getAttestation,
  buildResolveAttestationDisputeTx,
  submitTx,
  getContractErrorMessage,
} from '@/lib/contracts';

const ATTESTOR_TYPES: AttestorType[] = [
  'BusinessRegistry',
  'CreditBureau',
  'ExternalProtocol',
  'Manual',
];

export default function AttestorsAdminPage() {
  const t = useTranslations('Admin.attestors');
  const { wallet } = useStore();
  const [attestors, setAttestors] = useState<AttestorInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [txLoading, setTxLoading] = useState(false);

  const [newAddress, setNewAddress] = useState('');
  const [newType, setNewType] = useState<AttestorType>('BusinessRegistry');
  const [newWeightBps, setNewWeightBps] = useState('10000');

  const [lookupId, setLookupId] = useState('');
  const [lookupResult, setLookupResult] = useState<Attestation | null>(null);
  const [lookupLoading, setLookupLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setAttestors(await listActiveAttestors());
    } catch (e) {
      console.error(e);
      toast.error(t('loadFailed'));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  async function signAndSubmit(xdr: string) {
    const freighter = await import('@stellar/freighter-api');
    const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
      networkPassphrase: NETWORK,
      address: wallet.address!,
    });
    if (signError) throw new Error(signError.message || 'Signing rejected.');
    await submitTx(signedTxXdr);
  }

  async function handleRegister(e: React.FormEvent) {
    e.preventDefault();
    if (!wallet.address) return;
    const weightBps = Number(newWeightBps);
    if (!Number.isFinite(weightBps) || weightBps <= 0 || weightBps > 10_000) {
      toast.error(t('weightRangeError'));
      return;
    }
    setTxLoading(true);
    try {
      const admin = parseStellarAddress(wallet.address);
      const address = parseStellarAddress(newAddress.trim());
      const xdr = await buildRegisterAttestorTx({
        admin,
        address,
        attestorType: newType,
        weightBps,
      });
      await signAndSubmit(xdr);
      toast.success(t('registerSuccess', { address: newAddress.slice(0, 8) }));
      setNewAddress('');
      setNewWeightBps('10000');
      await load();
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : t('transactionFailed');
      toast.error(getContractErrorMessage(message));
    } finally {
      setTxLoading(false);
    }
  }

  async function handleDeactivate(address: string) {
    if (!wallet.address) return;
    setTxLoading(true);
    try {
      const admin = parseStellarAddress(wallet.address);
      const xdr = await buildDeactivateAttestorTx({ admin, address });
      await signAndSubmit(xdr);
      toast.success(t('deactivateSuccess', { address: address.slice(0, 8) }));
      await load();
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : t('transactionFailed');
      toast.error(getContractErrorMessage(message));
    } finally {
      setTxLoading(false);
    }
  }

  async function handleLookup(e: React.FormEvent) {
    e.preventDefault();
    const id = Number(lookupId);
    if (!Number.isFinite(id) || id < 0) {
      toast.error(t('validIdRequired'));
      return;
    }
    setLookupLoading(true);
    try {
      setLookupResult(await getAttestation(id));
    } catch (e) {
      console.error(e);
      toast.error(t('lookupFailed'));
    } finally {
      setLookupLoading(false);
    }
  }

  async function handleResolve(upheld: boolean) {
    if (!wallet.address || lookupResult === null) return;
    setTxLoading(true);
    try {
      const admin = parseStellarAddress(wallet.address);
      const xdr = await buildResolveAttestationDisputeTx({
        admin,
        attestationId: lookupResult.id,
        upheld,
      });
      await signAndSubmit(xdr);
      toast.success(
        upheld
          ? t('upheldSuccess', { id: lookupResult.id })
          : t('notUpheldSuccess', { id: lookupResult.id }),
      );
      const refreshed = await getAttestation(lookupResult.id);
      setLookupResult(refreshed);
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : t('transactionFailed');
      toast.error(getContractErrorMessage(message));
    } finally {
      setTxLoading(false);
    }
  }

  return (
    <div className="max-w-5xl space-y-8">
      <div>
        <h1 className="text-3xl font-bold mb-2">{t('title')}</h1>
        <p className="text-brand-muted text-sm">{t('description')}</p>
      </div>

      {/* Register form */}
      <div className="p-6 bg-brand-card border border-brand-border rounded-2xl space-y-4">
        <h2 className="font-semibold">{t('registerAttestor')}</h2>
        <form onSubmit={handleRegister} className="grid grid-cols-1 sm:grid-cols-4 gap-3">
          <input
            type="text"
            placeholder={t('attestorAddress')}
            value={newAddress}
            onChange={(e) => setNewAddress(e.target.value)}
            required
            className="sm:col-span-2 bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold font-mono text-sm"
          />
          <select
            value={newType}
            onChange={(e) => setNewType(e.target.value as AttestorType)}
            className="bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white focus:outline-none focus:border-brand-gold text-sm"
          >
            {ATTESTOR_TYPES.map((type) => (
              <option key={type} value={type}>
                {t(`type${type}`)}
              </option>
            ))}
          </select>
          <input
            type="number"
            min={1}
            max={10000}
            placeholder={t('weightBps')}
            value={newWeightBps}
            onChange={(e) => setNewWeightBps(e.target.value)}
            className="bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white focus:outline-none focus:border-brand-gold text-sm"
          />
          <button
            type="submit"
            disabled={txLoading}
            className="sm:col-span-4 py-2.5 bg-brand-gold text-brand-dark rounded-xl text-sm font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50"
          >
            {txLoading ? t('processing') : t('register')}
          </button>
        </form>
      </div>

      {/* Active attestors */}
      <div className="space-y-4">
        <h2 className="text-xl font-bold">{t('activeAttestors')}</h2>
        {loading ? (
          <Skeleton className="h-32 w-full rounded-2xl" />
        ) : attestors.length === 0 ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">
            {t('noAttestors')}
          </div>
        ) : (
          <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden">
            <table className="w-full text-left text-sm">
              <thead className="bg-brand-dark border-b border-brand-border text-brand-muted">
                <tr>
                  <th className="px-6 py-4 font-medium">{t('tableAddress')}</th>
                  <th className="px-6 py-4 font-medium">{t('tableType')}</th>
                  <th className="px-6 py-4 font-medium">{t('tableWeight')}</th>
                  <th className="px-6 py-4 font-medium">{t('tableRegistered')}</th>
                  <th className="px-6 py-4 font-medium text-right">{t('tableActions')}</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-brand-border">
                {attestors.map((a) => (
                  <tr key={a.address} className="hover:bg-brand-dark/50 transition-colors">
                    <td className="px-6 py-4 font-mono text-xs">
                      {a.address.slice(0, 8)}…{a.address.slice(-6)}
                    </td>
                    <td className="px-6 py-4">{a.attestorType}</td>
                    <td className="px-6 py-4">{(a.weightBps / 100).toFixed(2)}%</td>
                    <td className="px-6 py-4 text-brand-muted">
                      {new Date(a.registeredAt * 1000).toLocaleDateString()}
                    </td>
                    <td className="px-6 py-4 text-right">
                      <button
                        onClick={() => handleDeactivate(a.address)}
                        disabled={txLoading}
                        className="px-3 py-1.5 bg-red-500/20 text-red-400 border border-red-500/30 rounded-lg text-xs font-semibold hover:bg-red-500/30 transition-colors disabled:opacity-50"
                      >
                        {t('deactivate')}
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Dispute review */}
      <div className="space-y-4">
        <h2 className="text-xl font-bold">{t('reviewDispute')}</h2>
        <div className="p-6 bg-brand-card border border-brand-border rounded-2xl space-y-4">
          <form onSubmit={handleLookup} className="flex gap-3">
            <input
              type="number"
              min={0}
              placeholder={t('attestationId')}
              value={lookupId}
              onChange={(e) => setLookupId(e.target.value)}
              className="flex-1 bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm"
            />
            <button
              type="submit"
              disabled={lookupLoading}
              className="px-5 py-2.5 bg-brand-dark border border-brand-border rounded-xl text-sm font-semibold hover:bg-brand-border transition-colors disabled:opacity-50"
            >
              {lookupLoading ? t('lookingUp') : t('lookUp')}
            </button>
          </form>

          {lookupResult && (
            <div className="border-t border-brand-border pt-4 space-y-2 text-sm">
              <p>
                <span className="text-brand-muted">{t('sme')}:</span>{' '}
                <span className="font-mono text-xs">{lookupResult.sme}</span>
              </p>
              <p>
                <span className="text-brand-muted">{t('attestor')}:</span>{' '}
                <span className="font-mono text-xs">{lookupResult.attestor}</span>
              </p>
              <p>
                <span className="text-brand-muted">
                  {t('signal', { score: lookupResult.scoreContribution })}
                </span>
              </p>
              <p>
                <span className="text-brand-muted">
                  {t('status', { status: lookupResult.status })}
                </span>
              </p>
              {lookupResult.status === 'Disputed' && (
                <div className="flex gap-3 pt-2">
                  <button
                    onClick={() => handleResolve(true)}
                    disabled={txLoading}
                    className="flex-1 py-2.5 bg-green-500/20 text-green-400 border border-green-500/30 rounded-xl text-sm font-semibold hover:bg-green-500/30 transition-colors disabled:opacity-50"
                  >
                    {t('uphold')}
                  </button>
                  <button
                    onClick={() => handleResolve(false)}
                    disabled={txLoading}
                    className="flex-1 py-2.5 bg-red-500/20 text-red-400 border border-red-500/30 rounded-xl text-sm font-semibold hover:bg-red-500/30 transition-colors disabled:opacity-50"
                  >
                    {t('notUpheld')}
                  </button>
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
