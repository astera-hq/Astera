/**
 * @jest-environment node
 */
import { GET } from '../../app/api/health/route';

describe('/api/health route (#969)', () => {
  const originalFetch = global.fetch;

  afterEach(() => {
    global.fetch = originalFetch;
  });

  it('queries stellar_rpc, oracle_service, and compliance_service', async () => {
    global.fetch = jest.fn().mockImplementation((url: string | URL) => {
      const urlString = String(url);
      if (urlString.includes('soroban-testnet') || urlString.includes('8000')) {
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({ result: { status: 'healthy' } }),
        });
      }
      if (urlString.includes('8080') || urlString.includes('oracle')) {
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({ status: 'ok', processed: 5 }),
        });
      }
      if (urlString.includes('8081') || urlString.includes('compliance')) {
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({ status: 'ok', processed: 10 }),
        });
      }
      return Promise.resolve({ ok: true, json: () => Promise.resolve({}) });
    }) as any;

    const res = await GET();
    expect(res.status).toBe(200);

    const body = await res.json();
    expect(body.status).toBe('ok');
    expect(body.checks.stellar_rpc.status).toBe('ok');
    expect(body.checks.oracle_service.status).toBe('ok');
    expect(body.checks.compliance_service.status).toBe('ok');
  });

  it('returns 503 down status when an off-chain service fails', async () => {
    global.fetch = jest.fn().mockImplementation((url: string | URL) => {
      const urlString = String(url);
      if (urlString.includes('8080') || urlString.includes('oracle')) {
        return Promise.reject(new Error('Connection refused'));
      }
      return Promise.resolve({ ok: true, json: () => Promise.resolve({}) });
    }) as any;

    const res = await GET();
    expect(res.status).toBe(503);

    const body = await res.json();
    expect(body.status).toBe('down');
    expect(body.checks.oracle_service.status).toBe('down');
    expect(body.checks.oracle_service.detail).toBe('Connection refused');
  });
});
