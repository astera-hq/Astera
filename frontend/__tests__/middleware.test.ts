/**
 * @jest-environment node
 */
import { NextRequest } from 'next/server';
import { middleware } from '../middleware';

// #770: double-submit-cookie CSRF protection for state-mutating /api/*
// routes. A request with no token, or a mismatched header/cookie pair, must
// be rejected before it reaches the route handler.

function makeRequest(
  path: string,
  opts: { method?: string; headers?: Record<string, string>; cookie?: string } = {},
): NextRequest {
  const headers = new Headers(opts.headers);
  if (opts.cookie) headers.set('cookie', opts.cookie);
  return new NextRequest(new URL(path, 'http://localhost:3000'), {
    method: opts.method ?? 'GET',
    headers,
  });
}

describe('CSRF middleware (#770)', () => {
  it('mints a csrf cookie on a fresh page navigation', () => {
    const res = middleware(makeRequest('/dashboard'));
    expect(res.cookies.get('csrf_token')?.value).toBeTruthy();
  });

  it('does not re-mint a cookie that already exists', () => {
    const res = middleware(makeRequest('/dashboard', { cookie: 'csrf_token=existing-token' }));
    expect(res.cookies.get('csrf_token')).toBeUndefined();
  });

  it('rejects a mutating /api request with no csrf token at all', async () => {
    const res = middleware(makeRequest('/api/notifications/preferences', { method: 'POST' }));
    expect(res.status).toBe(403);
    expect((await res.json()).error).toBe('invalid_csrf_token');
  });

  it('rejects a mutating /api request when only the cookie is present', () => {
    const res = middleware(
      makeRequest('/api/notifications/preferences', {
        method: 'POST',
        cookie: 'csrf_token=right-token',
      }),
    );
    expect(res.status).toBe(403);
  });

  it('rejects a mutating /api request when header and cookie disagree', () => {
    const res = middleware(
      makeRequest('/api/notifications/preferences', {
        method: 'POST',
        headers: { 'x-csrf-token': 'wrong-token' },
        cookie: 'csrf_token=right-token',
      }),
    );
    expect(res.status).toBe(403);
  });

  it('allows a mutating /api request when the header matches the cookie', () => {
    const res = middleware(
      makeRequest('/api/notifications/preferences', {
        method: 'POST',
        headers: { 'x-csrf-token': 'matching-token' },
        cookie: 'csrf_token=matching-token',
      }),
    );
    expect(res.status).not.toBe(403);
  });

  it('allows a GET request to a mutating-only route without a token', () => {
    const res = middleware(makeRequest('/api/auth/me', { method: 'GET' }));
    expect(res.status).not.toBe(403);
  });

  it.each(['/api/auth/challenge', '/api/auth/token'])(
    'exempts the wallet-signature-verified route %s from the csrf check',
    (path) => {
      const res = middleware(makeRequest(path, { method: 'POST' }));
      expect(res.status).not.toBe(403);
    },
  );

  describe('locale preservation on deep links (#971, #1529)', () => {
    it('sets NEXT_LOCALE cookie matching locale-prefixed deep link path', () => {
      const res = middleware(makeRequest('/fr/invoice/new'));
      expect(res.cookies.get('NEXT_LOCALE')?.value).toBe('fr');
    });

    it('sets NEXT_LOCALE cookie matching Spanish (es) locale-prefixed deep link path (#1529)', () => {
      const res = middleware(makeRequest('/es/invoice/new'));
      expect(res.cookies.get('NEXT_LOCALE')?.value).toBe('es');
    });

    it('does not overwrite NEXT_LOCALE cookie if it already matches deep link path', () => {
      const res = middleware(
        makeRequest('/fr/invoice/new', { cookie: 'NEXT_LOCALE=fr; csrf_token=test' }),
      );
      expect(res.cookies.get('NEXT_LOCALE')).toBeUndefined();
    });

    it('does not overwrite NEXT_LOCALE cookie if it already matches Spanish deep link path', () => {
      const res = middleware(
        makeRequest('/es/invoice/new', { cookie: 'NEXT_LOCALE=es; csrf_token=test' }),
      );
      expect(res.cookies.get('NEXT_LOCALE')).toBeUndefined();
    });

    it('ignores unsupported locale prefixes', () => {
      const res = middleware(makeRequest('/de/invoice/new'));
      expect(res.cookies.get('NEXT_LOCALE')).toBeUndefined();
    });
  });
});
