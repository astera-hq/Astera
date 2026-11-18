import { Monitor } from './monitor';
import type { ComplianceConfig } from './types';

describe('Monitor', () => {
  const config: ComplianceConfig = {
    rpcUrl: 'https://example.invalid/rpc',
    horizonUrl: 'https://example.invalid/horizon',
    networkPassphrase: 'Test SDF Network ; September 2015',
    screenerSecretKey: 'SAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA1',
    complianceContractId: 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA',
    poolContractId: 'CBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB',
    invoiceContractId: 'CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC',
    healthPort: 8081,
    adminToken: 'test-token',
    structuringThreshold: 10n,
    structuringWindowMs: 60_000,
    structuringMaxCount: 3,
    screeningProviderUrl: '',
    screeningProviderApiKey: '',
    screeningProviderTimeoutMs: 5000,
    rescreenCheckIntervalMs: 60000,
  };

  it('calls request_review with the expected arguments when a flagged pattern is detected', async () => {
    const monitor = new Monitor(config);
    const getAccount = jest.spyOn((monitor as any).server, 'getAccount').mockResolvedValue({
      sequence: '0',
      account_id: 'GA123',
      signers: [],
      thresholds: { low_threshold: 0, med_threshold: 0, high_threshold: 0 },
      balances: [],
      paging_token: '0',
      flags: { auth_required: false, auth_revocable: false, auth_immutable: false },
      data: {},
      home_domain: undefined,
      last_modified_ledger: 0,
      subentry_count: 0,
    });
    const mockSimulate = jest.spyOn((monitor as any).server, 'simulateTransaction').mockResolvedValue({
      result: { retval: { type: 'scv_void' } },
    });
    const mockSend = jest.spyOn((monitor as any).server, 'sendTransaction').mockResolvedValue({ hash: 'tx-hash-123' });

    await monitor.flag('GTESTADDRESSXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX', 'structuring', 'reason');

    expect(getAccount).toHaveBeenCalledTimes(1);
    expect(mockSimulate).toHaveBeenCalledTimes(1);
    expect(mockSend).toHaveBeenCalledTimes(1);
    expect(mockSend.mock.calls[0][0].hash).toBe('tx-hash-123');
  });

  it('handles a failed request_review call without crashing', async () => {
    const monitor = new Monitor(config);
    jest.spyOn((monitor as any).server, 'getAccount').mockRejectedValue(new Error('rpc unavailable'));

    await expect(monitor.flag('GTESTADDRESSXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX', 'rapid_cycle', 'reason')).resolves.toBeUndefined();
  });

  it('does not invoke the on-chain review path when no alert is raised', async () => {
    const monitor = new Monitor(config);
    const spy = jest.spyOn((monitor as any).server, 'getAccount');

    await monitor.recordDeposit('GTESTADDRESSXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX', 100n);

    expect(spy).not.toHaveBeenCalled();
  });
});
