/**
 * useSseEvents Hook (Issue #157)
 *
 * React hook that starts/stops the SSE polling service based on component
 * lifecycle and user role. Handles cleanup on unmount to prevent memory leaks.
 */

'use client';

import { useEffect, useCallback, useSyncExternalStore } from 'react';
import { sseEventsService, type UserRole } from '../lib/sse-events';
import { useStore, type AsteraStore } from '../lib/store';

interface UseSseEventsOptions {
  /** User role for event filtering (default: 'Admin') */
  role?: UserRole;
  /** Polling interval in milliseconds (default: 15000) */
  intervalMs?: number;
  /** Whether to enable polling (default: true) */
  enabled?: boolean;
}

// External store snapshot for isPolling state
function getSnapshot(): boolean {
  return sseEventsService.isActive();
}

function subscribe(callback: () => void): () => void {
  // The service manages its own lifecycle; subscribe for changes
  // Since we can't easily subscribe to the singleton, return a no-op
  // The component will re-render when other state changes
  return () => {};
}

/**
 * Hook to manage SSE event polling for real-time contract updates.
 *
 * - Starts polling when mounted and enabled
 * - Stops polling on unmount (cleanup)
 * - Pauses when page is hidden (visibilitychange)
 * - Configurable interval and role-based filtering
 */
export function useSseEvents(options: UseSseEventsOptions = {}): {
  isPolling: boolean;
  intervalMs: number;
  setPollingInterval: (ms: number) => void;
} {
  const { role = 'Admin', intervalMs = 15_000, enabled = true } = options;
  const isPolling = useSyncExternalStore(subscribe, getSnapshot);
  const setPollingInterval = useStore((s: AsteraStore) => s.setPollingInterval);

  // Keep role in sync with the service
  useEffect(() => {
    sseEventsService.setRole(role);
  }, [role]);

  // Start/stop polling based on enabled state
  useEffect(() => {
    if (!enabled || typeof window === 'undefined') {
      return;
    }

    if (sseEventsService.isActive()) {
      return;
    }

    sseEventsService.start({ intervalMs, role });
    setPollingInterval(intervalMs);

    // Cleanup on unmount
    return () => {
      sseEventsService.stop();
    };
  }, [enabled, intervalMs, role, setPollingInterval]);

  // Expose interval setter
  const changeInterval = useCallback(
    (ms: number) => {
      sseEventsService.setPollInterval(ms);
      setPollingInterval(ms);
    },
    [setPollingInterval],
  );

  return {
    isPolling,
    intervalMs,
    setPollingInterval: changeInterval,
  };
}

export default useSseEvents;
