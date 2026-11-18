import {
  isRiskTier,
  isScreenDecision,
  RISK_TIERS,
  SCREEN_DECISIONS,
  validateScreenOverride,
} from './validation';

describe('isScreenDecision', () => {
  it('accepts every ComplianceStatus variant', () => {
    for (const status of SCREEN_DECISIONS) {
      expect(isScreenDecision(status)).toBe(true);
    }
  });

  it('rejects unknown, mis-cased and non-string values', () => {
    expect(isScreenDecision('Approved')).toBe(false);
    expect(isScreenDecision('cleared')).toBe(false);
    expect(isScreenDecision('')).toBe(false);
    expect(isScreenDecision(1)).toBe(false);
    expect(isScreenDecision(null)).toBe(false);
    expect(isScreenDecision(['Cleared'])).toBe(false);
  });
});

describe('isRiskTier', () => {
  it('accepts every RiskTier variant', () => {
    for (const tier of RISK_TIERS) {
      expect(isRiskTier(tier)).toBe(true);
    }
  });

  it('rejects unknown, mis-cased and non-string values', () => {
    expect(isRiskTier('Critical')).toBe(false);
    expect(isRiskTier('low')).toBe(false);
    expect(isRiskTier(0)).toBe(false);
    expect(isRiskTier(undefined)).toBe(false);
  });
});

describe('validateScreenOverride', () => {
  it('accepts a body with no override fields', () => {
    expect(validateScreenOverride({ address: 'GABC' })).toBeNull();
  });

  it('accepts a fully valid override', () => {
    expect(
      validateScreenOverride({
        address: 'GABC',
        status: 'Blocked',
        riskTier: 'High',
        reasonCode: 9001,
        notesHash: 'abc123',
      }),
    ).toBeNull();
  });

  it('rejects an arbitrary status string', () => {
    expect(validateScreenOverride({ address: 'GABC', status: 'SuperCleared' })).toMatch(
      /invalid status/,
    );
  });

  it('rejects an arbitrary riskTier string', () => {
    expect(validateScreenOverride({ address: 'GABC', riskTier: 'None' })).toMatch(
      /invalid riskTier/,
    );
  });

  it('rejects a riskTier override even when status is absent', () => {
    expect(validateScreenOverride({ address: 'GABC', riskTier: 42 })).toMatch(/invalid riskTier/);
  });

  it('rejects a reasonCode that does not fit in a u32', () => {
    expect(validateScreenOverride({ status: 'Flagged', reasonCode: -1 })).toMatch(
      /invalid reasonCode/,
    );
    expect(validateScreenOverride({ status: 'Flagged', reasonCode: 1.5 })).toMatch(
      /invalid reasonCode/,
    );
    expect(validateScreenOverride({ status: 'Flagged', reasonCode: 2 ** 32 })).toMatch(
      /invalid reasonCode/,
    );
    expect(validateScreenOverride({ status: 'Flagged', reasonCode: '7' })).toMatch(
      /invalid reasonCode/,
    );
  });

  it('rejects a non-string notesHash', () => {
    expect(validateScreenOverride({ status: 'Cleared', notesHash: 123 })).toMatch(
      /invalid notesHash/,
    );
  });
});
