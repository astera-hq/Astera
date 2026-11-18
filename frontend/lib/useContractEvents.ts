'use client';

import { useEffect, useRef, useState } from 'react';
import { mutate } from 'swr';
import { getToken } from './auth';
import { useStore } from './store';
import type { StoreEvent } from './sse-events';

interface ContractEvent {
  id: string;
  contractId: string;
  topic: unknown[];
  value: unknown;
  ledger: number;
  txHash: string;
  ledgerCloseAt: string;
}

interface UseContractEventsOptions {
  /** Filter the stream to events for a specific invoice */
  invoiceId?: number;
  /** Disable the hook without unmounting (e.g. while unauthenticated) */
  enabled?: boolean;
  /** Optional callback fired for every received event */
  onEvent?: (event: ContractEvent) => void;
}

/**
 * Opens a persistent SSE connection to /api/events and invalidates the SWR
 * cache whenever a relevant contract event arrives.
 *
 * The browser's native EventSource reconnects automatically on drop, so no
 * manual retry logic is needed here.
 *
 * Security: Uses a short-lived SSE ticket obtained via Authorization header
 * to avoid exposing the JWT token in URL query parameters (which can leak
 * via server logs, browser history, and Referer headers).
 */
export function useContractEvents({
  invoiceId,
  enabled = true,
  onEvent,
}: UseContractEventsOptions = {}) {
  const sourceRef = useRef<EventSource | null>(null);
  const [connected, setConnected] = useState(false);
  const walletAddress = useStore((s) => s.wallet.address);
  const setRecentEvents = useStore((s) => s.setRecentEvents);

  // Keep a stable ref to onEvent so the SSE effect doesn't rerun on every render
  const onEventRef = useRef(onEvent);
  useEffect(() => {
    onEventRef.current = onEvent;
  });

  useEffect(() => {
    if (!enabled || typeof window === 'undefined') return;

    let cancelled = false;

    async function connect(): Promise<void> {
      const token = getToken();
      if (!token) return;

      const ticketRes = await fetch('/api/auth/sse-ticket', {
        headers: { Authorization: `Bearer ${token}` },
        cache: 'no-store',
      });

      if (!ticketRes.ok) {
        console.warn('[useContractEvents] Failed to obtain SSE ticket:', ticketRes.status);
        return;
      }

      const { ticket } = (await ticketRes.json()) as { ticket: string };
      if (cancelled) return;

      const url = new URL('/api/events', window.location.origin);
      url.searchParams.set('ticket', ticket);
      if (invoiceId !== undefined) {
        url.searchParams.set('invoiceId', String(invoiceId));
      }

      const source = new EventSource(url.toString());
      sourceRef.current = source;

      source.onopen = () => setConnected(true);

      source.onmessage = (ev: MessageEvent<string>) => {
        let data: ContractEvent & { type?: string };
        try {
          data = JSON.parse(ev.data) as ContractEvent & { type?: string };
        } catch {
          return;
        }

        // Skip control messages (connected / error)
        if (data.type) return;

        invalidateCaches(data, walletAddress);

        // Prepend to Zustand recent-events list (capped at 100)
        const storeEvent: StoreEvent = {
          id: data.id,
          contractId: data.contractId,
          topic: data.topic.map(String),
          value: data.value,
          ledger: data.ledger,
          ledgerCloseAt: data.ledgerCloseAt,
          txHash: data.txHash,
          receivedAt: Date.now(),
        };
        const prev = useStore.getState().recentEvents;
        setRecentEvents([storeEvent, ...prev].slice(0, 100));

        onEventRef.current?.(data);
      };

      // EventSource auto-reconnects on error — onerror is informational only
      source.onerror = () => {
        setConnected(false);
        console.warn('[useContractEvents] SSE connection error — browser will retry');
      };
    }

    void connect();

    return () => {
      cancelled = true;
      sourceRef.current?.close();
      sourceRef.current = null;
      setConnected(false);
    };
    // invoiceId and walletAddress intentionally included so the stream
    // reconnects if either changes
  }, [enabled, invoiceId, walletAddress, setRecentEvents]);

  return { connected };
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function invalidateCaches(event: ContractEvent, walletAddress: string | null): void {
  const [, eventType] = event.topic as [unknown, string];
  if (typeof eventType !== 'string') return;

  const value = event.value as unknown[] | null;
  const invoiceIdFromEvent = value?.[0] as number | undefined;

  switch (eventType) {
    case 'funded':
    case 'repaid':
    case 'default':
    case 'created':
      // Always revalidate the count; also revalidate the specific invoice if known
      void mutate('invoice-count');
      if (invoiceIdFromEvent !== undefined) {
        void mutate(['invoice', invoiceIdFromEvent]);
        void mutate(['invoice-metadata', invoiceIdFromEvent]);
        void mutate(['funded-invoice', invoiceIdFromEvent]);
      }
      break;

    case 'deposit':
    case 'withdraw':
      // Revalidate all open position and token-totals entries for this wallet
      void mutate((key) => Array.isArray(key) && key[0] === 'token-totals');
      if (walletAddress) {
        void mutate(
          (key) => Array.isArray(key) && key[0] === 'position' && key[1] === walletAddress,
        );
      }
      break;

    default:
      break;
  }
}
