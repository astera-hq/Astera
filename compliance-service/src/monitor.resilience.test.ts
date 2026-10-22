import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { Keypair } from '@stellar/stellar-sdk';
import { Monitor } from './monitor';
import type { ComplianceConfig } from './types';

// #1552 / #1553: Horizon poll timeout and tracked-subject persistence.
describe('Monitor resilience', () => {
  const config: ComplianceConfig = {
    rpcUrl: 'https://example.invalid/rpc',
    horizonUrl: 'https://example.invalid/horizon',
    networkPassphrase: 'Test SDF Network ; September 2015',
    screenerSecretKey: Keypair.random().secret(),
    complianceContractId: 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA',
    poolContractId: '',
    invoiceContractId: '',
    healthPort: 8081,
    adminToken: 'test-token',
    structuringThreshold: 10n,
    structuringWindowMs: 60_000,
    structuringMaxCount: 3,
    screeningProviderUrl: '',
    screeningProviderApiKey: '',
    screeningProviderTimeoutMs: 5000,
    rescreenCheckIntervalMs: 60000,
    horizonTimeoutMs: 50,
    trackedSubjectsPath: path.join(os.tmpdir(), 'unused-tracked-subjects.json'),
  };

  describe('Horizon poll timeout (#1552)', () => {
    const originalFetch = global.fetch;

    afterEach(() => {
      global.fetch = originalFetch;
    });

    it('aborts a hung Horizon request instead of stalling the poll loop', async () => {
      let signal: AbortSignal | undefined;
      global.fetch = jest.fn((_url: unknown, init?: RequestInit) => {
        signal = init?.signal ?? undefined;
        return new Promise<Response>((_resolve, reject) => {
          signal?.addEventListener('abort', () => {
            const err = new Error('aborted');
            err.name = 'AbortError';
            reject(err);
          });
        });
      }) as unknown as typeof fetch;

      const monitor = new Monitor(config);
      await expect((monitor as any).pollOnce()).rejects.toThrow('Horizon poll timed out after 50ms');
      expect(signal?.aborted).toBe(true);
    });

    it('processes records returned within the timeout', async () => {
      global.fetch = jest.fn().mockResolvedValue({
        ok: true,
        json: () =>
          Promise.resolve({ _embedded: { records: [{ type: 'payment', paging_token: '42-1' }] } }),
      }) as unknown as typeof fetch;

      const monitor = new Monitor(config);
      await (monitor as any).pollOnce();

      expect(monitor.processedCount).toBe(1);
      expect((monitor as any).cursor).toBe('42-1');
    });
  });

  describe('tracked subject persistence (#1553)', () => {
    const SUBJECT_A = 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA';
    const SUBJECT_B = 'GBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB';
    let dir: string;
    let filePath: string;

    beforeEach(() => {
      dir = fs.mkdtempSync(path.join(os.tmpdir(), 'compliance-tracked-'));
      filePath = path.join(dir, 'tracked.json');
      jest.spyOn(console, 'log').mockImplementation(() => {});
      jest.spyOn(console, 'error').mockImplementation(() => {});
    });

    afterEach(() => {
      fs.rmSync(dir, { recursive: true, force: true });
      jest.restoreAllMocks();
    });

    it('persists tracked subjects so a new instance restores them', async () => {
      const first = new Monitor({ ...config, trackedSubjectsPath: filePath, complianceContractId: '' });
      first.trackSubject(SUBJECT_A);
      first.trackSubject(SUBJECT_A);
      expect(JSON.parse(fs.readFileSync(filePath, 'utf-8'))).toEqual([SUBJECT_A]);

      const restarted = new Monitor({ ...config, trackedSubjectsPath: filePath, complianceContractId: '' });
      await restarted.restoreTrackedSubjects();
      expect(restarted.listTrackedSubjects()).toEqual([SUBJECT_A]);
    });

    it("reseeds from this screener's on-chain submissions and persists the union", async () => {
      fs.writeFileSync(filePath, JSON.stringify([SUBJECT_A]));
      const monitor = new Monitor({ ...config, trackedSubjectsPath: filePath });
      const readContract = jest
        .spyOn(monitor as any, 'readContract')
        .mockResolvedValue([{ subject_address: SUBJECT_A }, { subject_address: SUBJECT_B }]);

      await monitor.restoreTrackedSubjects();

      expect(readContract).toHaveBeenCalledWith('get_screener_submissions', expect.any(Array));
      expect(monitor.listTrackedSubjects().sort()).toEqual([SUBJECT_A, SUBJECT_B]);
      expect(JSON.parse(fs.readFileSync(filePath, 'utf-8')).sort()).toEqual([SUBJECT_A, SUBJECT_B]);
    });

    it('still restores from disk when the on-chain reseed fails', async () => {
      fs.writeFileSync(filePath, JSON.stringify([SUBJECT_A]));
      const monitor = new Monitor({ ...config, trackedSubjectsPath: filePath });
      jest.spyOn(monitor as any, 'readContract').mockRejectedValue(new Error('rpc unavailable'));

      await monitor.restoreTrackedSubjects();

      expect(monitor.listTrackedSubjects()).toEqual([SUBJECT_A]);
    });

    it('ignores a corrupt tracked subjects file', async () => {
      fs.writeFileSync(filePath, '{not json');
      const monitor = new Monitor({ ...config, trackedSubjectsPath: filePath, complianceContractId: '' });

      await expect(monitor.restoreTrackedSubjects()).resolves.toBeUndefined();
      expect(monitor.listTrackedSubjects()).toEqual([]);
    });
  });
});
