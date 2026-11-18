'use client';

import { useEffect } from 'react';
import useSseEvents from '@/hooks/useSseEvents';
import { useStore } from '@/lib/store';
import type { UserRole } from '@/lib/sse-events';

/**
 * RealTimeNotificationProvider
 *
 * Activates the real-time event polling service on mount.
 * Detects user role based on wallet state and pool config,
 * then configures the polling service accordingly.
 *
 * This component enables:
 * - Real-time toast notifications for contract events (funding, repayment, defaults)
 * - Live updates in the NotificationBell component
 * - Automatic dispatch to the notification service
 */
export function RealTimeNotificationProvider() {
  const wallet = useStore((s) => s.wallet);
  const poolConfig = useStore((s) => s.poolConfig);
  const position = useStore((s) => s.position);

  // Determine user role based on wallet and pool config
  const userRole: UserRole = (() => {
    if (!wallet.connected || !wallet.address) {
      return 'Investor';
    }

    if (poolConfig?.admin === wallet.address) {
      return 'Admin';
    }

    return position ? 'Investor' : 'SME';
  })();

  // Activate polling with user role and default interval (15 seconds)
  // Only enable polling if wallet is connected to avoid spurious requests
  const shouldEnablePolling = wallet.connected;

  const { isPolling } = useSseEvents({
    role: userRole,
    intervalMs: 15_000,
    enabled: shouldEnablePolling,
  });

  // Log polling status for debugging
  useEffect(() => {
    if (isPolling) {
      console.log(`[RealTimeNotifications] Polling activated for role: ${userRole}`);
    }
  }, [isPolling, userRole]);

  // No UI needed - this component just manages the polling lifecycle
  return null;
}
