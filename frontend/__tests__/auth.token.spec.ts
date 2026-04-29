import { POST as tokenHandler } from '@/app/api/auth/token/route';

describe('SEP-0010 token endpoint', () => {
  test('rejects invalid signature', async () => {
    const req = new Request('http://localhost', {
      method: 'POST',
      body: JSON.stringify({ signed_xdr: 'AAAAAAAAAAAA' }),
      headers: { 'Content-Type': 'application/json' },
    });

    const res = await tokenHandler(req as any);
    expect(res.status).toBe(400);
    const body = await res.json();
    expect(body.error).toBeDefined();
  });
});
