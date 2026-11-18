/**
 * @jest-environment node
 */
jest.mock('jose', () => ({
  jwtVerify: jest.fn(),
  SignJWT: class SignJWT {
    constructor(_payload: unknown) {}

    setProtectedHeader(_h: unknown) {
      return this;
    }

    setIssuedAt(_iat: unknown) {
      return this;
    }

    setExpirationTime(_exp: unknown) {
      return this;
    }

    async sign(_secret: unknown) {
      return 'test-token';
    }
  },
}));

process.env.JWT_SECRET = 'test-secret';
process.env.SEP10_SERVER_SEED = 'SB2AGJ6J2YV2LQ5RZJXTOCEJ3G2M7Q3I5Y5XK6GZ7YQO3H4C5N6O7P8Q';

let tokenHandler: (request: Request) => Promise<Response>;

beforeAll(async () => {
  // Ensure env vars above are set before module evaluation.
  const mod = await import('@/app/api/auth/token/route');
  tokenHandler = mod.POST;
});

describe('SEP-0010 token endpoint', () => {
  test('rejects invalid signature', async () => {
    const req = new Request('http://localhost', {
      method: 'POST',
      body: JSON.stringify({ signed_xdr: 'AAAAAAAAAAAA' }),
      headers: { 'Content-Type': 'application/json' },
    });

    const res = await tokenHandler(req);
    expect(res.status).toBe(400);
    const body = await res.json();
    expect(body.error).toBeDefined();
  });
});
