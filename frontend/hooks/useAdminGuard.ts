'use client';

import { useEffect, useState } from 'react';
import { useRouter } from 'next/navigation';
import { useStore } from '@/lib/store';
import { getPoolConfig } from '@/lib/contracts';

/**
 * Result of the client-side admin role check.
 *
 * - `loading`         — the role check is still in flight.
 * - `unauthenticated` — no wallet is connected yet.
 * - `authorized`      — the connected wallet is the on-chain pool admin.
 * - `denied`          — the connected wallet is not the admin; a redirect to
 *                       {@link AdminGuardOptions.redirectTo} has been issued.
 */
export type AdminGuardStatus = 'loading' | 'unauthenticated' | 'authorized' | 'denied';

export interface AdminGuardResult {
  status: AdminGuardStatus;
  /** True only when the connected wallet is verified as the pool admin. */
  isAdmin: boolean;
}

export interface AdminGuardOptions {
  /** Where to send non-admin users. Defaults to `/`. */
  redirectTo?: string;
}

/**
 * Client-side route guard for `/admin` pages (#393).
 *
 * The admin role is read from the contract (`getPoolConfig().admin`) — never a
 * local flag — and, when available, cryptographically confirmed via the
 * SEP-0010 JWT. A non-admin (or unverifiable) wallet is redirected to
 * `redirectTo` before any admin UI renders, so admin controls never flash for
 * unauthorized users.
 *
 * Apply this in the `/admin` layout (which wraps every admin page) or directly
 * in an individual admin page component.
 */
export function useAdminGuard(options: AdminGuardOptions = {}): AdminGuardResult {
  const { redirectTo = '/' } = options;
  const { wallet, poolConfig, setPoolConfig } = useStore();
  const router = useRouter();
  const [status, setStatus] = useState<AdminGuardStatus>('loading');

  useEffect(() => {
    let cancelled = false;

    async function check() {
      setStatus('loading');
      try {
        // Admin identity comes from the contract, not a client-held flag.
        let config = poolConfig;
        if (!config) {
          config = await getPoolConfig();
          if (cancelled) return;
          setPoolConfig(config);
        }

        if (!wallet.connected || !wallet.address) {
          if (!cancelled) setStatus('unauthenticated');
          return;
        }

        let authorized = false;
        try {
          // Prefer cryptographic verification via the SEP-0010 JWT.
          const { verifyToken, getToken } = await import('@/lib/auth');
          const verified = await verifyToken(getToken());
          authorized =
            (verified?.authenticated && verified?.account === config.admin) ||
            wallet.address === config.admin;
        } catch {
          // Fall back to a raw address match if verification is unavailable.
          authorized = wallet.address === config.admin;
        }

        if (cancelled) return;

        if (authorized) {
          setStatus('authorized');
        } else {
          // Redirect before any admin UI renders.
          setStatus('denied');
          router.replace(redirectTo);
        }
      } catch (e) {
        // Treat any failure to establish admin identity as not-authorized and
        // route the user away rather than rendering admin controls.
        console.error('[useAdminGuard] admin check failed:', e);
        if (cancelled) return;
        setStatus('denied');
        router.replace(redirectTo);
      }
    }

    void check();
    return () => {
      cancelled = true;
    };
  }, [wallet.connected, wallet.address, poolConfig, setPoolConfig, router, redirectTo]);

  return { status, isAdmin: status === 'authorized' };
}
