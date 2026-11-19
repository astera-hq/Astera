// Pure-function tests for the roles-admin proposal payload encoder. No React
// rendering and no lib/contracts / lib/stellar mocks are needed: the encoder
// only depends on `@/lib/types`.

import { buildActionPayload, summarizeAction } from '@/components/admin/roles/proposalPayload';
import type { ActionPayload, Role } from '@/lib/types';

/** Valid Stellar G-address (matches /^[GC][A-Z2-7]{55}$/). */
const ADDR = 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF';
/** Valid Stellar contract (C...) address of the right length/alphabet. */
const CONTRACT = `C${'A'.repeat(55)}`;

describe('buildActionPayload', () => {
  describe('boolean variants', () => {
    const tags: ActionPayload['tag'][] = [
      'SetPaused',
      'SetKycRequired',
      'SetOracleRegistryPaused',
      'SetCompliancePaused',
      'SetReferralPaused',
    ];

    it.each(tags)('%s encodes a true/false field as a boolean', (tag) => {
      expect(buildActionPayload(tag, { [`${tag}_value`]: 'true' })).toEqual({
        tag,
        values: [true],
      });
      expect(buildActionPayload(tag, { [`${tag}_value`]: 'false' })).toEqual({
        tag,
        values: [false],
      });
    });

    it('defaults a missing boolean field to false (preserving page behaviour)', () => {
      expect(buildActionPayload('SetPaused', {})).toEqual({ tag: 'SetPaused', values: [false] });
    });
  });

  describe('numeric basis-point variants', () => {
    const tags: ActionPayload['tag'][] = [
      'SetYield',
      'SetMaxUtilization',
      'SetBorrowRewardBps',
      'SetDepositRewardBps',
    ];

    it.each(tags)('%s encodes the bps field as a number', (tag) => {
      expect(buildActionPayload(tag, { bps: '750' })).toEqual({ tag, values: [750] });
    });

    it('defaults a missing bps field to 0', () => {
      expect(buildActionPayload('SetYield', {})).toEqual({ tag: 'SetYield', values: [0] });
    });
  });

  describe('address variants', () => {
    const tags: ActionPayload['tag'][] = [
      'SetTreasury',
      'SetOracleContract',
      'SetOracle',
      'AddKeeper',
      'SetOracleRegistryInvoiceContract',
      'SetReferralPool',
      'SetInvoiceAccessControl',
      'SetCreditScoreAccessControl',
      'SetOracleRegistryAccessControl',
      'SetComplianceAccessControl',
      'SetGovernanceAccessControl',
      'SetReferralAccessControl',
    ];

    it.each(tags)('%s encodes the address field as a validated Stellar address', (tag) => {
      expect(buildActionPayload(tag, { address: ADDR })).toEqual({ tag, values: [ADDR] });
    });

    it('trims surrounding whitespace before validating the address', () => {
      expect(buildActionPayload('SetTreasury', { address: `  ${ADDR}  ` })).toEqual({
        tag: 'SetTreasury',
        values: [ADDR],
      });
    });
  });

  describe('bigint variants', () => {
    it('WithdrawRevenue encodes [token, amount] with an exact bigint', () => {
      const amount = '9007199254740993'; // > Number.MAX_SAFE_INTEGER
      expect(buildActionPayload('WithdrawRevenue', { token: CONTRACT, amount })).toEqual({
        tag: 'WithdrawRevenue',
        values: [CONTRACT, 9007199254740993n],
      });
    });

    it('SetLateThreshold encodes days as a bigint', () => {
      expect(buildActionPayload('SetLateThreshold', { days: '42' })).toEqual({
        tag: 'SetLateThreshold',
        values: [42n],
      });
    });

    it.each(['SetRescreeningInterval', 'SetScreenerTimelock'] as ActionPayload['tag'][])(
      '%s encodes secs as a bigint',
      (tag) => {
        expect(buildActionPayload(tag, { secs: '3600' })).toEqual({ tag, values: [3600n] });
      },
    );

    it('defaults a missing bigint field to 0n', () => {
      expect(buildActionPayload('SetLateThreshold', {})).toEqual({
        tag: 'SetLateThreshold',
        values: [0n],
      });
    });
  });

  describe('compound / struct variants', () => {
    it('RegisterDebtor encodes [id, name, maxExposure]', () => {
      expect(
        buildActionPayload('RegisterDebtor', {
          debtorId: 'DEBT-1',
          debtorName: 'Acme Corp',
          maxExposure: '5000000',
        }),
      ).toEqual({
        tag: 'RegisterDebtor',
        values: ['DEBT-1', 'Acme Corp', 5000000n],
      });
    });

    it('SetScoreThresholds encodes four numeric thresholds', () => {
      expect(
        buildActionPayload('SetScoreThresholds', {
          excellent: '90',
          veryGood: '75',
          good: '60',
          fair: '45',
        }),
      ).toEqual({
        tag: 'SetScoreThresholds',
        values: [90, 75, 60, 45],
      });
    });

    it('RegisterAttestor encodes [address, type, weightBps]', () => {
      expect(
        buildActionPayload('RegisterAttestor', {
          address: ADDR,
          attestorType: '2',
          weightBps: '1500',
        }),
      ).toEqual({
        tag: 'RegisterAttestor',
        values: [ADDR, 2, 1500],
      });
    });

    it('SetOracleRegistryConfig encodes five mixed values', () => {
      expect(
        buildActionPayload('SetOracleRegistryConfig', {
          minStake: '100000',
          requiredVotes: '3',
          quorumBps: '6700',
          roundDurationSecs: '3600',
          deregisterCooldownSecs: '86400',
        }),
      ).toEqual({
        tag: 'SetOracleRegistryConfig',
        values: [100000n, 3, 6700, 3600n, 86400n],
      });
    });

    it('SlashOracle encodes [operator, bps, roundId, evidence]', () => {
      expect(
        buildActionPayload('SlashOracle', {
          operator: ADDR,
          bps: '250',
          roundId: '17',
          evidence: 'bad-price',
        }),
      ).toEqual({
        tag: 'SlashOracle',
        values: [ADDR, 250, 17n, 'bad-price'],
      });
    });

    it('AdminResolveRound encodes [invoiceId, approved, reason]', () => {
      expect(
        buildActionPayload('AdminResolveRound', {
          invoiceId: '99',
          approved: 'true',
          reason: 'resolved',
        }),
      ).toEqual({
        tag: 'AdminResolveRound',
        values: [99n, true, 'resolved'],
      });
      expect(
        buildActionPayload('AdminResolveRound', {
          invoiceId: '99',
          approved: 'false',
          reason: 'rejected',
        }),
      ).toEqual({
        tag: 'AdminResolveRound',
        values: [99n, false, 'rejected'],
      });
    });

    it('SetInvestorKyc encodes [investor, approved]', () => {
      expect(
        buildActionPayload('SetInvestorKyc', { investor: ADDR, approved: 'true' }),
      ).toEqual({ tag: 'SetInvestorKyc', values: [ADDR, true] });
    });
  });

  describe('role variants', () => {
    const role: Role = 'TreasuryManager';

    it.each(['AddSigner', 'RemoveSigner'] as ActionPayload['tag'][])(
      '%s encodes [role, address]',
      (tag) => {
        expect(buildActionPayload(tag, { role, address: ADDR })).toEqual({
          tag,
          values: [role, ADDR],
        });
      },
    );

    it('SetThreshold encodes [role, threshold]', () => {
      expect(buildActionPayload('SetThreshold', { role: 'SuperAdmin', threshold: '3' })).toEqual({
        tag: 'SetThreshold',
        values: ['SuperAdmin', 3],
      });
    });
  });

  describe('optional-address case', () => {
    it('SetOracleRegistryTreasury encodes [undefined] for an empty string', () => {
      const result = buildActionPayload('SetOracleRegistryTreasury', { treasury: '' });
      expect(result).toEqual({ tag: 'SetOracleRegistryTreasury', values: [undefined] });
      expect(result?.values).toHaveLength(1);
      expect(result?.values[0]).toBeUndefined();
    });

    it('SetOracleRegistryTreasury encodes [undefined] for whitespace-only input', () => {
      expect(buildActionPayload('SetOracleRegistryTreasury', { treasury: '   ' })).toEqual({
        tag: 'SetOracleRegistryTreasury',
        values: [undefined],
      });
    });

    it('SetOracleRegistryTreasury encodes the address when provided', () => {
      expect(buildActionPayload('SetOracleRegistryTreasury', { treasury: ADDR })).toEqual({
        tag: 'SetOracleRegistryTreasury',
        values: [ADDR],
      });
    });
  });

  it('UpdateGovernanceConfig emits exactly THREE values (ABI u32/u32/u64)', () => {
    const result = buildActionPayload('UpdateGovernanceConfig', {
      quorumBps: '6700',
      passBps: '5100',
      executionDelaySecs: '120',
    });
    // lib/types.ts types this as [number, number, number]; the SDK's two-value
    // type is wrong and must not drive this encoder.
    expect(result?.values).toHaveLength(3);
    expect(result).toEqual({
      tag: 'UpdateGovernanceConfig',
      values: [6700, 5100, 120],
    });
  });

  it('SetCategoryQuorum emits exactly two values', () => {
    const result = buildActionPayload('SetCategoryQuorum', { category: '2', quorumBps: '8000' });
    expect(result?.values).toHaveLength(2);
    expect(result).toEqual({ tag: 'SetCategoryQuorum', values: [2, 8000] });
  });
});

describe('summarizeAction', () => {
  it('summarizes a boolean action', () => {
    expect(summarizeAction({ tag: 'SetPaused', values: [true] })).toBe('Set Paused → true');
  });

  it('summarizes a bigint-bearing action', () => {
    expect(summarizeAction({ tag: 'WithdrawRevenue', values: [CONTRACT, 123n] })).toBe(
      `Withdraw 123 from ${CONTRACT}`,
    );
  });

  it('summarizes a role action using ROLE_LABELS', () => {
    expect(summarizeAction({ tag: 'AddSigner', values: ['RiskManager', ADDR] })).toBe(
      `Add Signer ${ADDR} to Risk Manager`,
    );
  });

  it('summarizes UpdateGovernanceConfig with all three values', () => {
    expect(summarizeAction({ tag: 'UpdateGovernanceConfig', values: [6700, 5100, 120] })).toBe(
      'Update Config → quorum 6700bps, pass 5100bps, exec delay 120s',
    );
  });

  it('summarizes an empty optional treasury as (none)', () => {
    expect(summarizeAction({ tag: 'SetOracleRegistryTreasury', values: [undefined] })).toBe(
      'Set Treasury → (none)',
    );
  });
});
