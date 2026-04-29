'use client';

import { useEffect, useRef } from 'react';
import { useStore, type AsteraStore } from '@/lib/store';
import { pushToast } from './Toast';
import { getFreighter } from '@/lib/freighter';

/**
 * Background monitor that detects when the Freighter wallet has been
 * disconnected from the site mid-session (e.g. via the extension directly).
 *
 * Strategy:
 *   - Poll `isAllowed()` every `POLL_INTERVAL_MS` while the tab is visible.
 *   - Re-check immediately when the tab regains focus.
 *   - If the app was previously "connected" but Freighter now reports the
 *     site is not allowed OR returns an error, treat it as a disconnect.
 *
 * On a detected disconnect we call `disconnect()` on the store (resetting
 * wallet state and position) and show a non-blocking toast.
 *
 * This component renders no UI.
 */
const POLL_INTERVAL_MS = 30_000;
const DISCONNECT_TOAST_ID = 'wallet-disconnected';

export default function WalletConnectionMonitor() {
  const wallet = useStore((s: AsteraStore) => s.wallet);
  const disconnectStore = useStore((s: AsteraStore) => s.disconnect);
  // Track "was connected" so we only notify on the connected → disconnected edge.
  const wasConnectedRef = useRef(wallet.connected);

  useEffect(() => {
    wasConnectedRef.current = wallet.connected;
  }, [wallet.connected]);

  useEffect(() => {
    let cancelled = false;
    let interval: ReturnType<typeof setInterval> | null = null;

    async function checkConnection() {
      if (cancelled) return;
      // Only care while the store thinks we're connected.
      if (!wasConnectedRef.current) return;

      try {
        const freighter = await getFreighter();
        const [{ isAllowed, error: allowedError }, { address, error: addressError }] =
          await Promise.all([freighter.isAllowed(), freighter.getAddress()]);

        if (cancelled) return;

        const disconnected =
          Boolean(allowedError) || Boolean(addressError) || !isAllowed || !address;

        if (disconnected) {
          handleDisconnect();
        }
      } catch (err) {
        // Freighter not installed or threw — treat as disconnected if we
        // previously thought we were connected.
        console.warn('[WalletConnectionMonitor] poll failed:', err);
        if (!cancelled && wasConnectedRef.current) {
          handleDisconnect();
        }
      }
    }

    function handleDisconnect() {
      if (!wasConnectedRef.current) return;
      wasConnectedRef.current = false;
      disconnectStore();
      pushToast({
        id: DISCONNECT_TOAST_ID,
        kind: 'warning',
        title: 'Wallet disconnected',
        description:
          'Your Freighter wallet is no longer connected to this site. Reconnect to continue.',
        durationMs: 8000,
      });
    }

    function handleFocus() {
      checkConnection();
    }

    function handleVisibility() {
      if (document.visibilityState === 'visible') checkConnection();
    }

    // Start the poll only when the store is connected. The outer useEffect's
    // dependency array re-runs this whenever `wallet.connected` changes, so
    // reconnecting starts a fresh poll.
    if (wallet.connected) {
      interval = setInterval(checkConnection, POLL_INTERVAL_MS);
      window.addEventListener('focus', handleFocus);
      document.addEventListener('visibilitychange', handleVisibility);
    }

    return () => {
      cancelled = true;
      if (interval) clearInterval(interval);
      window.removeEventListener('focus', handleFocus);
      document.removeEventListener('visibilitychange', handleVisibility);
    };
  }, [wallet.connected, disconnectStore]);

  return null;
}
