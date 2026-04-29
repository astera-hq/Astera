'use client';

import { useEffect, useState, useCallback } from 'react';
import { useStore } from '@/lib/store';
import { monitorService, ContractEvent } from '@/lib/monitoring';
import { notificationService, NotificationAlert } from '@/lib/notifications';
import type { InvoiceTtlWarning } from '@/lib/types';
import { buildRenewInvoiceTtlTx, submitTx, getAcceptedTokens, getPoolTokenTotals } from '@/lib/contracts';
import { stablecoinLabel } from '@/lib/stellar';
import type { PoolTokenTotals } from '@/lib/types';

interface TokenUtilization {
  token: string;
  label: string;
  utilizationBps: number;
}

function UtilizationGauge({ token, label, utilizationBps }: TokenUtilization) {
  const pct = Math.min(100, Math.round(utilizationBps / 100));
  const color = pct >= 90 ? 'bg-red-500' : pct >= 80 ? 'bg-yellow-400' : 'bg-green-500';
  const textColor = pct >= 90 ? 'text-red-400' : pct >= 80 ? 'text-yellow-400' : 'text-green-400';
  return (
    <div className="bg-brand-card border border-brand-border rounded-xl p-4">
      <div className="flex items-center justify-between mb-2">
        <span className="text-sm font-medium text-white">{label}</span>
        <span className={`text-sm font-bold ${textColor}`}>{pct}%</span>
      </div>
      <div className="h-3 rounded-full bg-brand-border overflow-hidden">
        <div className={`h-full rounded-full transition-all duration-500 ${color}`} style={{ width: `${pct}%` }} />
      </div>
      {pct >= 90 && (
        <p className="text-xs text-red-400 mt-2 font-medium">
          ⚠ Pool is at {pct}% utilization — consider pausing new commitments
        </p>
      )}
      {pct >= 80 && pct < 90 && (
        <p className="text-xs text-yellow-400 mt-2">High utilization — monitor closely</p>
      )}
    </div>
  );
}

export default function MonitoringPage() {
  const { wallet } = useStore();
  const [events, setEvents] = useState<ContractEvent[]>([]);
  const [alerts, setAlerts] = useState<NotificationAlert[]>([]);
  const [ttlWarnings, setTtlWarnings] = useState<InvoiceTtlWarning[]>([]);
  const [utilizations, setUtilizations] = useState<TokenUtilization[]>([]);
  const [isPolling, setIsPolling] = useState(false);
  const [lastCheck, setLastCheck] = useState<Date | null>(null);
  const [autoPoll, setAutoPoll] = useState(true);
  const [renewingId, setRenewingId] = useState<number | null>(null);

  const fetchUtilizations = useCallback(async () => {
    try {
      const tokens = await getAcceptedTokens();
      const rows = await Promise.all(
        tokens.map(async (token) => {
          const totals: PoolTokenTotals = await getPoolTokenTotals(token);
          const deposited = Number(totals.totalDeposited);
          const deployed = Number(totals.totalDeployed);
          const utilizationBps = deposited > 0 ? Math.round((deployed / deposited) * 10_000) : 0;
          return { token, label: stablecoinLabel(token), utilizationBps };
        }),
      );
      setUtilizations(rows);
    } catch {
      // non-fatal
    }
  }, []);

  const fetchEvents = useCallback(async () => {
    setIsPolling(true);
    try {
      const newEvents = await monitorService.pollEvents();
      const warnings = await monitorService.getInvoiceTtlWarnings();
      if (newEvents.length > 0) {
        setEvents((prev) => [...newEvents, ...prev].slice(0, 100));
      }
      setTtlWarnings(warnings);
      setLastCheck(new Date());
      await fetchUtilizations();
    } catch (error) {
      console.error('Polling error:', error);
    } finally {
      setIsPolling(false);
    }
  }, [fetchUtilizations]);

  async function renewInvoice(invoiceId: number) {
    if (!wallet.connected || !wallet.address) return;
    setRenewingId(invoiceId);
    try {
      const xdr = await buildRenewInvoiceTtlTx({ operator: wallet.address, invoiceId });
      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error } = await freighter.signTransaction(xdr, {
        networkPassphrase: 'Test SDF Network ; September 2015',
        address: wallet.address,
      });
      if (error) throw new Error(error.message);
      await submitTx(signedTxXdr);
      await fetchEvents();
    } catch (error) {
      console.error('[Astera Monitor] Failed to renew invoice TTL:', error);
    } finally {
      setRenewingId(null);
    }
  }

  useEffect(() => {
    fetchEvents();
    const unsubscribe = notificationService.subscribe((alert: NotificationAlert) => {
      setAlerts((prev: NotificationAlert[]) => [alert, ...prev].slice(0, 50));
    });
    return () => unsubscribe();
  }, [fetchEvents]);

  useEffect(() => {
    let interval: NodeJS.Timeout;
    if (autoPoll) {
      interval = setInterval(fetchEvents, 30000);
    }
    return () => clearInterval(interval);
  }, [autoPoll, fetchEvents]);

  return (
    <div className="space-y-8 animate-in fade-in duration-700">
      {/* Header & Controls */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div>
          <h1 className="text-3xl font-bold text-white mb-2">Contract Monitoring</h1>
          <p className="text-brand-muted">
            Real-time surveillance of Astera protocol events and security alerts.
          </p>
        </div>
        <div className="flex items-center gap-3">
          <div className="flex items-center gap-2 bg-brand-card border border-brand-border px-4 py-2 rounded-xl">
            <div className={`w-2 h-2 rounded-full ${isPolling ? 'bg-brand-gold animate-pulse' : 'bg-green-500'}`} />
            <span className="text-sm font-medium text-brand-muted">
              {isPolling ? 'Polling...' : 'System Active'}
            </span>
          </div>
          <button
            onClick={() => setAutoPoll(!autoPoll)}
            className={`px-4 py-2 rounded-xl text-sm font-bold transition-all ${
              autoPoll
                ? 'bg-brand-gold/10 text-brand-gold border border-brand-gold/30'
                : 'bg-brand-card text-brand-muted border border-brand-border'
            }`}
          >
            {autoPoll ? 'Auto-Poll: ON' : 'Auto-Poll: OFF'}
          </button>
          <button
            onClick={fetchEvents}
            disabled={isPolling}
            className="bg-brand-gold hover:bg-brand-gold-light disabled:opacity-50 text-brand-dark px-4 py-2 rounded-xl text-sm font-bold transition-all"
          >
            Manual Check
          </button>
        </div>
      </div>

      {/* Stats Grid */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
        <div className="bg-brand-card border border-brand-border p-6 rounded-2xl">
          <p className="text-brand-muted text-sm font-medium mb-1">Events Monitored</p>
          <p className="text-3xl font-bold text-white">{events.length}</p>
          <p className="text-xs text-brand-muted mt-2">Total tracked in current session</p>
        </div>
        <div className="bg-brand-card border border-brand-border p-6 rounded-2xl">
          <p className="text-brand-muted text-sm font-medium mb-1">Active Alerts</p>
          <p className={`text-3xl font-bold ${alerts.length > 0 ? 'text-red-500' : 'text-white'}`}>
            {alerts.length}
          </p>
          <p className="text-xs text-brand-muted mt-2">Critical/High/Medium priority</p>
        </div>
        <div className="bg-brand-card border border-brand-border p-6 rounded-2xl">
          <p className="text-brand-muted text-sm font-medium mb-1">Last Heartbeat</p>
          <p className="text-xl font-bold text-white">
            {lastCheck ? lastCheck.toLocaleTimeString() : 'Never'}
          </p>
          <p className="text-xs text-brand-muted mt-2">Next check in ~30 seconds</p>
        </div>
      </div>

      {/* #275: Pool Utilization Gauges */}
      {utilizations.length > 0 && (
        <div className="bg-brand-card border border-brand-border rounded-2xl p-6">
          <h2 className="text-xl font-bold text-white mb-4">Pool Utilization</h2>
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
            {utilizations.map((u) => (
              <UtilizationGauge key={u.token} {...u} />
            ))}
          </div>
        </div>
      )}

      <div className="bg-brand-card border border-brand-border rounded-2xl p-6">
        <div className="flex items-center justify-between gap-4 mb-4">
          <div>
            <h2 className="text-xl font-bold text-white">Storage TTL Watchlist</h2>
            <p className="text-sm text-brand-muted">
              Estimated invoices whose persistent storage is approaching expiry.
            </p>
          </div>
          <span className="text-xs font-bold uppercase tracking-widest text-brand-gold">
            {ttlWarnings.length} at risk
          </span>
        </div>

        {ttlWarnings.length === 0 ? (
          <div className="text-sm text-brand-muted">
            No invoices are currently within the 30-day renewal window.
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
            {ttlWarnings.map((warning) => (
              <div
                key={warning.id}
                className={`rounded-xl border p-4 ${
                  warning.severity === 'high'
                    ? 'border-red-500/40 bg-red-500/10'
                    : warning.severity === 'medium'
                      ? 'border-orange-500/40 bg-orange-500/10'
                      : 'border-yellow-500/30 bg-yellow-500/10'
                }`}
              >
                <div className="flex items-center justify-between gap-3 mb-2">
                  <span className="text-sm font-bold text-white">Invoice #{warning.id}</span>
                  <span className="text-[10px] uppercase tracking-widest text-brand-muted">
                    {warning.severity}
                  </span>
                </div>
                <p className="text-xs text-brand-muted mb-1">Status: {warning.status}</p>
                <p className="text-sm text-white font-medium">
                  Expires in {warning.remainingDays} day{warning.remainingDays === 1 ? '' : 's'}
                </p>
                <p className="text-[10px] text-brand-muted mt-1 font-mono">
                  Ledger {warning.expiryLedger}
                </p>
                <button
                  onClick={() => void renewInvoice(warning.id)}
                  disabled={!wallet.connected || renewingId === warning.id}
                  className="mt-3 w-full rounded-lg border border-brand-border px-3 py-2 text-xs font-semibold text-white disabled:opacity-50"
                >
                  {renewingId === warning.id ? 'Renewing...' : 'Renew TTL'}
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-8">
        {/* Security Alerts List */}
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <h2 className="text-xl font-bold text-white flex items-center gap-2">
              <span className="w-2 h-6 bg-red-500 rounded-full" />
              Security Alerts
            </h2>
            {alerts.length > 0 && (
              <button onClick={() => setAlerts([])} className="text-xs text-brand-muted hover:text-white">
                Clear All
              </button>
            )}
          </div>

          <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden min-h-[400px]">
            {alerts.length === 0 ? (
              <div className="h-full flex flex-col items-center justify-center text-center p-8">
                <div className="w-12 h-12 bg-green-500/10 rounded-full flex items-center justify-center text-green-500 mb-4">✓</div>
                <p className="text-brand-muted">No security alerts detected.</p>
                <p className="text-xs text-brand-muted/60 mt-1">System is monitoring for unusual activity.</p>
              </div>
            ) : (
              <div className="divide-y divide-brand-border h-[500px] overflow-y-auto custom-scrollbar">
                {alerts.map((alert) => (
                  <div key={alert.id} className="p-4 bg-red-500/5 hover:bg-red-500/10 transition-colors">
                    <div className="flex items-start justify-between mb-1">
                      <span className={`text-[10px] font-bold px-2 py-0.5 rounded-full ${
                        alert.priority === 'CRITICAL' ? 'bg-red-500 text-white'
                          : alert.priority === 'HIGH' ? 'bg-orange-500 text-white'
                          : 'bg-yellow-500 text-brand-dark'
                      }`}>
                        {alert.priority}
                      </span>
                      <span className="text-[10px] text-brand-muted">
                        {new Date(alert.timestamp).toLocaleTimeString()}
                      </span>
                    </div>
                    <p className="text-sm font-bold text-white mb-1">{alert.message}</p>
                    {typeof alert.data?.txHash === 'string' && (
                      <a
                        href={`https://stellar.expert/explorer/testnet/tx/${alert.data.txHash}`}
                        target="_blank"
                        rel="noreferrer"
                        className="text-[10px] text-brand-gold hover:underline"
                      >
                        View Transaction ↗
                      </a>
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>

        {/* Live Event Feed */}
        <div className="space-y-4">
          <h2 className="text-xl font-bold text-white flex items-center gap-2">
            <span className="w-2 h-6 bg-brand-gold rounded-full" />
            On-Chain Events
          </h2>

          <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden h-[500px] flex flex-col">
            {events.length === 0 ? (
              <div className="flex-1 flex flex-col items-center justify-center text-center p-8">
                <div className="w-8 h-8 border-2 border-brand-gold border-t-transparent rounded-full animate-spin mb-4" />
                <p className="text-brand-muted">Waiting for events...</p>
              </div>
            ) : (
              <div className="overflow-x-auto overflow-y-auto flex-1 custom-scrollbar">
                <table className="w-full text-left border-collapse">
                  <thead className="sticky top-0 bg-brand-card shadow-sm z-10">
                    <tr className="border-b border-brand-border">
                      <th className="px-4 py-3 text-[10px] font-bold uppercase tracking-widest text-brand-muted">Type</th>
                      <th className="px-4 py-3 text-[10px] font-bold uppercase tracking-widest text-brand-muted">Contract</th>
                      <th className="px-4 py-3 text-[10px] font-bold uppercase tracking-widest text-brand-muted">Details</th>
                      <th className="px-4 py-3 text-[10px] font-bold uppercase tracking-widest text-brand-muted">Ledger</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-brand-border/50">
                    {events.map((event) => {
                      const [contract, type] = event.topic;
                      return (
                        <tr key={event.id} className="hover:bg-white/5 transition-colors group">
                          <td className="px-4 py-3">
                            <span className={`text-[10px] font-bold px-2 py-0.5 rounded-full ${
                              type === 'default' ? 'bg-red-500/20 text-red-500'
                                : type === 'funded' || type === 'paid' ? 'bg-green-500/20 text-green-500'
                                : 'bg-brand-gold/20 text-brand-gold'
                            }`}>
                              {type.toUpperCase()}
                            </span>
                          </td>
                          <td className="px-4 py-3 text-xs text-brand-muted font-mono">{contract}</td>
                          <td className="px-4 py-3 text-xs text-white max-w-[200px] truncate group-hover:whitespace-normal group-hover:break-words">
                            {JSON.stringify(event.value)}
                          </td>
                          <td className="px-4 py-3 text-xs text-brand-muted font-mono">{event.ledger}</td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}


export default function MonitoringPage() {
  const { wallet } = useStore();
  const [events, setEvents] = useState<ContractEvent[]>([]);
  const [alerts, setAlerts] = useState<NotificationAlert[]>([]);
  const [ttlWarnings, setTtlWarnings] = useState<InvoiceTtlWarning[]>([]);
  const [isPolling, setIsPolling] = useState(false);
  const [lastCheck, setLastCheck] = useState<Date | null>(null);
  const [autoPoll, setAutoPoll] = useState(true);
  const [renewingId, setRenewingId] = useState<number | null>(null);

  const fetchEvents = useCallback(async () => {
    setIsPolling(true);
    try {
      const newEvents = await monitorService.pollEvents();
      const warnings = await monitorService.getInvoiceTtlWarnings();
      if (newEvents.length > 0) {
        setEvents((prev) => [...newEvents, ...prev].slice(0, 100));
      }
      setTtlWarnings(warnings);
      setLastCheck(new Date());
    } catch (error) {
      console.error('Polling error:', error);
    } finally {
      setIsPolling(false);
    }
  }, []);

  async function renewInvoice(invoiceId: number) {
    if (!wallet.connected || !wallet.address) {
      return;
    }
    setRenewingId(invoiceId);
    try {
      const xdr = await buildRenewInvoiceTtlTx({
        operator: wallet.address,
        invoiceId,
      });
      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error } = await freighter.signTransaction(xdr, {
        networkPassphrase: 'Test SDF Network ; September 2015',
        address: wallet.address,
      });
      if (error) throw new Error(error.message);
      await submitTx(signedTxXdr);
      await fetchEvents();
    } catch (error) {
      console.error('[Astera Monitor] Failed to renew invoice TTL:', error);
    } finally {
      setRenewingId(null);
    }
  }

  useEffect(() => {
    // Initial fetch
    fetchEvents();

    // Subscribe to new alerts
    const unsubscribe = notificationService.subscribe((alert: NotificationAlert) => {
      setAlerts((prev: NotificationAlert[]) => [alert, ...prev].slice(0, 50));
    });

    return () => unsubscribe();
  }, [fetchEvents]);

  useEffect(() => {
    let interval: NodeJS.Timeout;
    if (autoPoll) {
      interval = setInterval(fetchEvents, 30000); // 30s
    }
    return () => clearInterval(interval);
  }, [autoPoll, fetchEvents]);

  return (
    <div className="space-y-8 animate-in fade-in duration-700">
      {/* Header & Controls */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div>
          <h1 className="text-3xl font-bold text-white mb-2">Contract Monitoring</h1>
          <p className="text-brand-muted">
            Real-time surveillance of Astera protocol events and security alerts.
          </p>
        </div>
        <div className="flex items-center gap-3">
          <div className="flex items-center gap-2 bg-brand-card border border-brand-border px-4 py-2 rounded-xl">
            <div
              className={`w-2 h-2 rounded-full ${isPolling ? 'bg-brand-gold animate-pulse' : 'bg-green-500'}`}
            />
            <span className="text-sm font-medium text-brand-muted">
              {isPolling ? 'Polling...' : 'System Active'}
            </span>
          </div>
          <button
            onClick={() => setAutoPoll(!autoPoll)}
            className={`px-4 py-2 rounded-xl text-sm font-bold transition-all ${
              autoPoll
                ? 'bg-brand-gold/10 text-brand-gold border border-brand-gold/30'
                : 'bg-brand-card text-brand-muted border border-brand-border'
            }`}
          >
            {autoPoll ? 'Auto-Poll: ON' : 'Auto-Poll: OFF'}
          </button>
          <button
            onClick={fetchEvents}
            disabled={isPolling}
            className="bg-brand-gold hover:bg-brand-gold-light disabled:opacity-50 text-brand-dark px-4 py-2 rounded-xl text-sm font-bold transition-all"
          >
            Manual Check
          </button>
        </div>
      </div>

      {/* Stats Grid */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
        <div className="bg-brand-card border border-brand-border p-6 rounded-2xl">
          <p className="text-brand-muted text-sm font-medium mb-1">Events Monitored</p>
          <p className="text-3xl font-bold text-white">{events.length}</p>
          <p className="text-xs text-brand-muted mt-2">Total tracked in current session</p>
        </div>
        <div className="bg-brand-card border border-brand-border p-6 rounded-2xl">
          <p className="text-brand-muted text-sm font-medium mb-1">Active Alerts</p>
          <p className={`text-3xl font-bold ${alerts.length > 0 ? 'text-red-500' : 'text-white'}`}>
            {alerts.length}
          </p>
          <p className="text-xs text-brand-muted mt-2">Critical/High/Medium priority</p>
        </div>
        <div className="bg-brand-card border border-brand-border p-6 rounded-2xl">
          <p className="text-brand-muted text-sm font-medium mb-1">Last Heartbeat</p>
          <p className="text-xl font-bold text-white">
            {lastCheck ? lastCheck.toLocaleTimeString() : 'Never'}
          </p>
          <p className="text-xs text-brand-muted mt-2">Next check in ~30 seconds</p>
        </div>
      </div>

      <div className="bg-brand-card border border-brand-border rounded-2xl p-6">
        <div className="flex items-center justify-between gap-4 mb-4">
          <div>
            <h2 className="text-xl font-bold text-white">Storage TTL Watchlist</h2>
            <p className="text-sm text-brand-muted">
              Estimated invoices whose persistent storage is approaching expiry.
            </p>
          </div>
          <span className="text-xs font-bold uppercase tracking-widest text-brand-gold">
            {ttlWarnings.length} at risk
          </span>
        </div>

        {ttlWarnings.length === 0 ? (
          <div className="text-sm text-brand-muted">
            No invoices are currently within the 30-day renewal window.
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
            {ttlWarnings.map((warning) => (
              <div
                key={warning.id}
                className={`rounded-xl border p-4 ${
                  warning.severity === 'high'
                    ? 'border-red-500/40 bg-red-500/10'
                    : warning.severity === 'medium'
                      ? 'border-orange-500/40 bg-orange-500/10'
                      : 'border-yellow-500/30 bg-yellow-500/10'
                }`}
              >
                <div className="flex items-center justify-between gap-3 mb-2">
                  <span className="text-sm font-bold text-white">Invoice #{warning.id}</span>
                  <span className="text-[10px] uppercase tracking-widest text-brand-muted">
                    {warning.severity}
                  </span>
                </div>
                <p className="text-xs text-brand-muted mb-1">Status: {warning.status}</p>
                <p className="text-sm text-white font-medium">
                  Expires in {warning.remainingDays} day{warning.remainingDays === 1 ? '' : 's'}
                </p>
                <p className="text-[10px] text-brand-muted mt-1 font-mono">
                  Ledger {warning.expiryLedger}
                </p>
                <button
                  onClick={() => void renewInvoice(warning.id)}
                  disabled={!wallet.connected || renewingId !== null}
                  className="mt-3 w-full rounded-lg border border-brand-border px-3 py-2 text-xs font-semibold text-white disabled:opacity-50"
                >
                  {renewingId === warning.id
                    ? 'Renewing...'
                    : renewingId !== null
                      ? 'Transaction in progress...'
                      : 'Renew TTL'}
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-8">
        {/* Security Alerts List */}
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <h2 className="text-xl font-bold text-white flex items-center gap-2">
              <span className="w-2 h-6 bg-red-500 rounded-full" />
              Security Alerts
            </h2>
            {alerts.length > 0 && (
              <button
                onClick={() => setAlerts([])}
                className="text-xs text-brand-muted hover:text-white"
              >
                Clear All
              </button>
            )}
          </div>

          <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden min-h-[400px]">
            {alerts.length === 0 ? (
              <div className="h-full flex flex-col items-center justify-center text-center p-8">
                <div className="w-12 h-12 bg-green-500/10 rounded-full flex items-center justify-center text-green-500 mb-4">
                  ✓
                </div>
                <p className="text-brand-muted">No security alerts detected.</p>
                <p className="text-xs text-brand-muted/60 mt-1">
                  System is monitoring for unusual activity.
                </p>
              </div>
            ) : (
              <div className="divide-y divide-brand-border h-[500px] overflow-y-auto custom-scrollbar">
                {alerts.map((alert) => (
                  <div
                    key={alert.id}
                    className="p-4 bg-red-500/5 hover:bg-red-500/10 transition-colors"
                  >
                    <div className="flex items-start justify-between mb-1">
                      <span
                        className={`text-[10px] font-bold px-2 py-0.5 rounded-full ${
                          alert.priority === 'CRITICAL'
                            ? 'bg-red-500 text-white'
                            : alert.priority === 'HIGH'
                              ? 'bg-orange-500 text-white'
                              : 'bg-yellow-500 text-brand-dark'
                        }`}
                      >
                        {alert.priority}
                      </span>
                      <span className="text-[10px] text-brand-muted">
                        {new Date(alert.timestamp).toLocaleTimeString()}
                      </span>
                    </div>
                    <p className="text-sm font-bold text-white mb-1">{alert.message}</p>
                    {typeof alert.data?.txHash === 'string' && (
                      <a
                        href={`https://stellar.expert/explorer/testnet/tx/${alert.data.txHash}`}
                        target="_blank"
                        rel="noreferrer"
                        className="text-[10px] text-brand-gold hover:underline"
                      >
                        View Transaction ↗
                      </a>
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>

        {/* Live Event Feed */}
        <div className="space-y-4">
          <h2 className="text-xl font-bold text-white flex items-center gap-2">
            <span className="w-2 h-6 bg-brand-gold rounded-full" />
            On-Chain Events
          </h2>

          <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden h-[500px] flex flex-col">
            {events.length === 0 ? (
              <div className="flex-1 flex flex-col items-center justify-center text-center p-8">
                <div className="w-8 h-8 border-2 border-brand-gold border-t-transparent rounded-full animate-spin mb-4" />
                <p className="text-brand-muted">Waiting for events...</p>
              </div>
            ) : (
              <div className="overflow-x-auto overflow-y-auto flex-1 custom-scrollbar">
                <table className="w-full text-left border-collapse">
                  <thead className="sticky top-0 bg-brand-card shadow-sm z-10">
                    <tr className="border-b border-brand-border">
                      <th className="px-4 py-3 text-[10px] font-bold uppercase tracking-widest text-brand-muted">
                        Type
                      </th>
                      <th className="px-4 py-3 text-[10px] font-bold uppercase tracking-widest text-brand-muted">
                        Contract
                      </th>
                      <th className="px-4 py-3 text-[10px] font-bold uppercase tracking-widest text-brand-muted">
                        Details
                      </th>
                      <th className="px-4 py-3 text-[10px] font-bold uppercase tracking-widest text-brand-muted">
                        Ledger
                      </th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-brand-border/50">
                    {events.map((event) => {
                      const [contract, type] = event.topic;
                      return (
                        <tr key={event.id} className="hover:bg-white/5 transition-colors group">
                          <td className="px-4 py-3">
                            <span
                              className={`text-[10px] font-bold px-2 py-0.5 rounded-full ${
                                type === 'default'
                                  ? 'bg-red-500/20 text-red-500'
                                  : type === 'funded' || type === 'paid'
                                    ? 'bg-green-500/20 text-green-500'
                                    : 'bg-brand-gold/20 text-brand-gold'
                              }`}
                            >
                              {type.toUpperCase()}
                            </span>
                          </td>
                          <td className="px-4 py-3 text-xs text-brand-muted font-mono">
                            {contract}
                          </td>
                          <td className="px-4 py-3 text-xs text-white max-w-[200px] truncate group-hover:whitespace-normal group-hover:break-words">
                            {JSON.stringify(event.value)}
                          </td>
                          <td className="px-4 py-3 text-xs text-brand-muted font-mono">
                            {event.ledger}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
