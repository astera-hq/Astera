/**
 * @jest-environment node
 *
 * Tests for the client-side auth helpers introduced in the fix for issue #576:
 * getTokenExpiry, isTokenExpired, isTokenExpiringSoon, authenticatedFetch.
 *
 * We run under the Node environment (so Response/fetch are available as Node 18+
 * globals) and shim `window` so that the auth helpers' `typeof window` guards
 * do not short-circuit to null.
 */

// Make `typeof window !== 'undefined'` true before any module code runs its
// runtime checks (getToken / setToken read from window.localStorage).
(global as unknown as Record<string, unknown>).window = global;

import {
  getTokenExpiry,
  isTokenExpired,
  isTokenExpiringSoon,
  authenticatedFetch,
  REFRESH_MARGIN_MS,
} from '@/lib/auth';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Build a minimal but structurally valid JWT with the given exp claim. */
function makeJwt(exp: number): string {
  const header = Buffer.from(JSON.stringify({ alg: 'HS256', typ: 'JWT' }))
    .toString('base64')
    .replace(/=/g, '')
    .replace(/\+/g, '-')
    .replace(/\//g, '_');
  const payload = Buffer.from(JSON.stringify({ sub: 'GTEST', exp }))
    .toString('base64')
    .replace(/=/g, '')
    .replace(/\+/g, '-')
    .replace(/\//g, '_');
  return `${header}.${payload}.fakesig`;
}

const nowSec = () => Math.floor(Date.now() / 1000);

// ---------------------------------------------------------------------------
// getTokenExpiry
// ---------------------------------------------------------------------------

describe('getTokenExpiry', () => {
  test('returns exp from a well-formed JWT', () => {
    const exp = nowSec() + 3600;
    expect(getTokenExpiry(makeJwt(exp))).toBe(exp);
  });

  test('returns null for a malformed token', () => {
    expect(getTokenExpiry('not-a-jwt')).toBeNull();
    expect(getTokenExpiry('')).toBeNull();
    expect(getTokenExpiry('a.b')).toBeNull();
  });

  test('returns null when exp field is missing', () => {
    const payload = Buffer.from(JSON.stringify({ sub: 'GTEST' }))
      .toString('base64')
      .replace(/=/g, '')
      .replace(/\+/g, '-')
      .replace(/\//g, '_');
    expect(getTokenExpiry(`hdr.${payload}.sig`)).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// isTokenExpired
// ---------------------------------------------------------------------------

describe('isTokenExpired', () => {
  test('returns false for a token that expires in the future', () => {
    expect(isTokenExpired(makeJwt(nowSec() + 3600))).toBe(false);
  });

  test('returns true for an already-expired token', () => {
    expect(isTokenExpired(makeJwt(nowSec() - 1))).toBe(true);
  });

  test('returns true for a malformed token', () => {
    expect(isTokenExpired('garbage')).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// isTokenExpiringSoon
// ---------------------------------------------------------------------------

describe('isTokenExpiringSoon', () => {
  test('returns false when expiry is well beyond the margin', () => {
    const exp = nowSec() + 3600; // 1 hour from now
    expect(isTokenExpiringSoon(makeJwt(exp), REFRESH_MARGIN_MS)).toBe(false);
  });

  test('returns true when expiry is within the margin', () => {
    const exp = nowSec() + 60; // 1 minute — inside the 5-min margin
    expect(isTokenExpiringSoon(makeJwt(exp), REFRESH_MARGIN_MS)).toBe(true);
  });

  test('returns true for an already-expired token', () => {
    expect(isTokenExpiringSoon(makeJwt(nowSec() - 1), REFRESH_MARGIN_MS)).toBe(true);
  });

  test('returns true for a malformed token', () => {
    expect(isTokenExpiringSoon('garbage', REFRESH_MARGIN_MS)).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// authenticatedFetch — 401 retry behaviour
// ---------------------------------------------------------------------------

jest.mock('@/lib/freighter', () => ({
  getFreighter: jest.fn(),
}));

// We need to spy on ensureAuthWithFreighter from within the same module.
// Jest module mocking replaces the whole module; instead we mock the
// sub-dependencies that ensureAuthWithFreighter calls.
const mockFetch = jest.fn();
global.fetch = mockFetch as unknown as typeof fetch;

// Stub localStorage — needed in Node environment (no built-in window.localStorage).
const localStorageMock = (() => {
  let store: Record<string, string> = {};
  return {
    getItem: (k: string) => store[k] ?? null,
    setItem: (k: string, v: string) => {
      store[k] = v;
    },
    removeItem: (k: string) => {
      delete store[k];
    },
    clear: () => {
      store = {};
    },
  };
})();
Object.defineProperty(global, 'localStorage', { value: localStorageMock, writable: true });

describe('authenticatedFetch', () => {
  beforeEach(() => {
    mockFetch.mockReset();
    localStorageMock.clear();
  });

  test('attaches Bearer token from localStorage on a normal request', async () => {
    const exp = nowSec() + 3600;
    const token = makeJwt(exp);
    localStorageMock.setItem('astera_jwt', token);

    mockFetch.mockResolvedValueOnce(new Response('ok', { status: 200 }));

    await authenticatedFetch('/api/test', {}, 'GADDR');

    expect(mockFetch).toHaveBeenCalledTimes(1);
    const [, opts] = mockFetch.mock.calls[0] as [string, RequestInit & { headers: Headers }];
    expect((opts.headers as Headers).get('Authorization')).toBe(`Bearer ${token}`);
  });

  test('retries with a fresh token on 401 when re-auth succeeds', async () => {
    // Token looks valid (not expiring soon) so no proactive refresh fires.
    // The server still rejects it with 401 (e.g. revoked server-side).
    const validToken = makeJwt(nowSec() + 3600);
    localStorageMock.setItem('astera_jwt', validToken);

    const freshToken = makeJwt(nowSec() + 3600);

    // Call sequence:
    //   1 — main request      → 401 (server rejects stale/revoked token)
    //   2 — challenge request → challenge envelope
    //   3 — token exchange    → fresh JWT
    //   4 — retry request     → 200 OK
    mockFetch
      .mockResolvedValueOnce(new Response('unauth', { status: 401 }))
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ transaction: 'xdr', network_passphrase: 'Test' }), {
          status: 200,
        }),
      )
      .mockResolvedValueOnce(new Response(JSON.stringify({ token: freshToken }), { status: 200 }))
      .mockResolvedValueOnce(new Response('ok', { status: 200 }));

    const { getFreighter } = await import('@/lib/freighter');
    (getFreighter as jest.Mock).mockResolvedValue({
      signTransaction: jest.fn().mockResolvedValue({ signed_envelope_xdr: 'signed' }),
    });

    const res = await authenticatedFetch('/api/secure', {}, 'GADDR');
    expect(res.status).toBe(200);
    expect(mockFetch).toHaveBeenCalledTimes(4);
  });

  test('returns 401 response when re-auth fails after 401', async () => {
    localStorageMock.setItem('astera_jwt', makeJwt(nowSec() + 3600));

    mockFetch
      .mockResolvedValueOnce(new Response('unauth', { status: 401 }))
      // challenge endpoint fails
      .mockResolvedValueOnce(new Response('{}', { status: 500 }));

    const res = await authenticatedFetch('/api/secure', {}, 'GADDR');
    expect(res.status).toBe(401);
  });

  test('proactively refreshes token when it is expiring soon', async () => {
    // Token is within the 5-min refresh margin so authenticatedFetch should
    // re-auth before the request, not wait for a 401.
    const expiringToken = makeJwt(nowSec() + 60); // 1 min left — inside margin
    const freshToken = makeJwt(nowSec() + 3600);
    localStorageMock.setItem('astera_jwt', expiringToken);

    // Call sequence:
    //   1 — challenge request  → challenge envelope  (proactive refresh)
    //   2 — token exchange     → fresh JWT
    //   3 — main request       → 200 OK (sent with fresh token)
    mockFetch
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ transaction: 'xdr', network_passphrase: 'Test' }), {
          status: 200,
        }),
      )
      .mockResolvedValueOnce(new Response(JSON.stringify({ token: freshToken }), { status: 200 }))
      .mockResolvedValueOnce(new Response('ok', { status: 200 }));

    const { getFreighter } = await import('@/lib/freighter');
    (getFreighter as jest.Mock).mockResolvedValue({
      signTransaction: jest.fn().mockResolvedValue({ signed_envelope_xdr: 'signed' }),
    });

    const res = await authenticatedFetch('/api/secure', {}, 'GADDR');
    expect(res.status).toBe(200);

    // The final request must carry the fresh token, not the expiring one.
    const lastCall = mockFetch.mock.calls[mockFetch.mock.calls.length - 1] as [
      string,
      RequestInit & { headers: Headers },
    ];
    expect((lastCall[1].headers as Headers).get('Authorization')).toBe(`Bearer ${freshToken}`);
  });

  test('makes request without Authorization header when no token is stored', async () => {
    // localStorage is cleared in beforeEach so no token is present.
    mockFetch.mockResolvedValueOnce(new Response('ok', { status: 200 }));

    await authenticatedFetch('/api/public', {}, 'GADDR');

    expect(mockFetch).toHaveBeenCalledTimes(1);
    const [, opts] = mockFetch.mock.calls[0] as [string, RequestInit & { headers: Headers }];
    expect((opts.headers as Headers).get('Authorization')).toBeNull();
  });
});
