import { getFreighter } from '@/lib/freighter';

const TOKEN_KEY = 'astera_jwt';

// How far before expiry we proactively refresh (5 minutes).
export const REFRESH_MARGIN_MS = 5 * 60 * 1000;

export function setToken(token: string) {
  if (typeof window !== 'undefined') localStorage.setItem(TOKEN_KEY, token);
}

export function getToken(): string | null {
  if (typeof window === 'undefined') return null;
  return localStorage.getItem(TOKEN_KEY);
}

export function clearToken() {
  if (typeof window !== 'undefined') localStorage.removeItem(TOKEN_KEY);
}

/**
 * Decode the `exp` Unix timestamp from a JWT payload without verifying the
 * signature. Safe for client-side use — we only need the expiry to know when
 * to refresh; the server verifies the signature on every authenticated call.
 */
export function getTokenExpiry(token: string): number | null {
  try {
    const part = token.split('.')[1];
    if (!part) return null;
    const b64 = part.replace(/-/g, '+').replace(/_/g, '/');
    const json = typeof atob !== 'undefined' ? atob(b64) : Buffer.from(b64, 'base64').toString();
    const payload = JSON.parse(json) as Record<string, unknown>;
    return typeof payload.exp === 'number' ? payload.exp : null;
  } catch {
    return null;
  }
}

export function isTokenExpired(token: string): boolean {
  const exp = getTokenExpiry(token);
  return exp === null || exp <= Math.floor(Date.now() / 1000);
}

export function isTokenExpiringSoon(token: string, marginMs = REFRESH_MARGIN_MS): boolean {
  const exp = getTokenExpiry(token);
  if (exp === null) return true;
  return exp * 1000 - Date.now() < marginMs;
}

export async function requestChallenge(account: string) {
  const res = await fetch('/api/auth/challenge', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ account }),
  });
  return res.json();
}

export async function exchangeToken(signedXDR: string) {
  const res = await fetch('/api/auth/token', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ signed_xdr: signedXDR }),
  });
  return res.json();
}

export async function verifyToken(token: string | null) {
  if (!token) return { authenticated: false };
  const res = await fetch('/api/auth/me', { headers: { Authorization: `Bearer ${token}` } });
  return res.json();
}

export async function ensureAuthWithFreighter(address: string) {
  try {
    const challenge = await requestChallenge(address);
    if (!challenge || !challenge.transaction) return { error: 'no_challenge' };

    // ask freighter to sign
    const freighter = await getFreighter();
    const { signed_envelope_xdr, error } = await freighter
      .signTransaction(challenge.transaction, {
        networkPassphrase: String(challenge.network_passphrase ?? ''),
        address,
      })
      .catch((e) => ({ error: String(e) }) as any);

    if (error || !signed_envelope_xdr) return { error: 'sign_failed' };

    const tokenResp = await exchangeToken(signed_envelope_xdr);
    if (tokenResp?.token) {
      setToken(tokenResp.token);
      return { token: tokenResp.token };
    }
    return { error: 'exchange_failed', detail: tokenResp };
  } catch (err) {
    return { error: String(err) };
  }
}

/**
 * Drop-in replacement for `fetch` on authenticated endpoints.
 *
 * - Proactively re-runs the SEP-10 flow if the stored token is within
 *   REFRESH_MARGIN_MS of its expiry.
 * - On a 401 response it attempts a single re-authentication and retries
 *   the original request with the fresh token.
 * - If re-auth fails (wallet disconnected, key changed) the 401 response
 *   is returned as-is so the caller can redirect to the connect-wallet flow.
 */
export async function authenticatedFetch(
  url: string,
  opts: RequestInit = {},
  address: string,
): Promise<Response> {
  let token = getToken();

  if (token && isTokenExpiringSoon(token)) {
    const refreshed = await ensureAuthWithFreighter(address).catch(() => null);
    if (refreshed && 'token' in refreshed) {
      token = refreshed.token as string;
      setToken(token);
    }
  }

  const headers = new Headers(opts.headers as HeadersInit | undefined);
  if (token) headers.set('Authorization', `Bearer ${token}`);

  const res = await fetch(url, { ...opts, headers });

  if (res.status === 401) {
    const refreshed = await ensureAuthWithFreighter(address).catch(() => null);
    if (refreshed && 'token' in refreshed) {
      const freshToken = refreshed.token as string;
      setToken(freshToken);
      const retryHeaders = new Headers(opts.headers as HeadersInit | undefined);
      retryHeaders.set('Authorization', `Bearer ${freshToken}`);
      return fetch(url, { ...opts, headers: retryHeaders });
    }
  }

  return res;
}
