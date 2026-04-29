import { getFreighter } from '@/lib/freighter';

const TOKEN_KEY = 'astera_jwt';

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
