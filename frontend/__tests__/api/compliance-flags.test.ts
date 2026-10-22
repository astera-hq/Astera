/**
 * @jest-environment node
 */
// jose ships ESM-only, so (as in auth.token.spec.ts) it is mocked: a token
// "<secret>|<sub>" verifies only against the matching secret.
jest.mock('jose', () => ({
  jwtVerify: jest.fn(async (token: string, key: Uint8Array) => {
    const [secret, sub] = token.split('|');
    if (secret !== new TextDecoder().decode(key)) throw new Error('signature verification failed');
    return { payload: { sub } };
  }),
}));

import { GET } from '../../app/api/compliance/flags/route';

// #1550: the compliance admin token is attached server-side only, and only for
// SEP-10-authenticated accounts on the COMPLIANCE_ADMIN_ACCOUNTS allowlist.
describe('/api/compliance/flags route (#1550)', () => {
  const SECRET = 'test-jwt-secret';
  const ADMIN = 'GADMINACCOUNTXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX';
  const OTHER = 'GOTHERACCOUNTXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX';
  const originalFetch = global.fetch;
  const originalEnv = { ...process.env };

  async function sign(sub: string, secret = SECRET) {
    return `${secret}|${sub}`;
  }

  function request(token?: string) {
    return new Request('http://localhost/api/compliance/flags', {
      headers: token ? { Authorization: `Bearer ${token}` } : {},
    });
  }

  beforeEach(() => {
    process.env.SEP10_JWT_SECRET = SECRET;
    process.env.COMPLIANCE_ADMIN_ACCOUNTS = `${ADMIN}, GSECONDADMIN`;
    process.env.COMPLIANCE_ADMIN_TOKEN = 'server-only-token';
    process.env.COMPLIANCE_SERVICE_API_URL = 'http://compliance.test/';
    global.fetch = jest.fn().mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ alerts: [{ id: '1', pattern: 'structuring' }] }),
    }) as unknown as typeof fetch;
  });

  afterEach(() => {
    global.fetch = originalFetch;
    process.env = { ...originalEnv };
  });

  it('forwards an allowlisted admin request with the server-side admin token', async () => {
    const res = await GET(request(await sign(ADMIN)));

    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ alerts: [{ id: '1', pattern: 'structuring' }] });
    expect(global.fetch).toHaveBeenCalledWith('http://compliance.test/flags', {
      headers: { 'x-admin-token': 'server-only-token' },
      cache: 'no-store',
    });
  });

  it('rejects requests without a bearer token', async () => {
    const res = await GET(request());
    expect(res.status).toBe(401);
    expect(global.fetch).not.toHaveBeenCalled();
  });

  it('rejects a JWT signed with the wrong secret', async () => {
    const res = await GET(request(await sign(ADMIN, 'some-other-secret')));
    expect(res.status).toBe(401);
    expect(global.fetch).not.toHaveBeenCalled();
  });

  it('rejects an authenticated account that is not on the allowlist', async () => {
    const res = await GET(request(await sign(OTHER)));
    expect(res.status).toBe(403);
    expect(global.fetch).not.toHaveBeenCalled();
  });

  it('fails closed when no admin accounts are configured', async () => {
    delete process.env.COMPLIANCE_ADMIN_ACCOUNTS;
    const res = await GET(request(await sign(ADMIN)));
    expect(res.status).toBe(503);
    expect(global.fetch).not.toHaveBeenCalled();
  });

  it('returns 502 when the compliance service rejects the request', async () => {
    global.fetch = jest.fn().mockResolvedValue({ ok: false }) as unknown as typeof fetch;
    const res = await GET(request(await sign(ADMIN)));
    expect(res.status).toBe(502);
  });

  it('returns 502 when the compliance service is unreachable', async () => {
    global.fetch = jest
      .fn()
      .mockRejectedValue(new Error('ECONNREFUSED')) as unknown as typeof fetch;
    const res = await GET(request(await sign(ADMIN)));
    expect(res.status).toBe(502);
  });
});
