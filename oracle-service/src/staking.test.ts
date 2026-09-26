import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { StakingMetricsTracker, getStakingMetrics } from './staking.ts';

const ORACLE = 'GABCDEFEXAMPLEORACLEPUBLICKEY000000000000000000';
const OTHER = 'GOTHERORACLEPUBLICKEY0000000000000000000000000';

describe('StakingMetricsTracker', () => {
  it('ignores non-slash events', () => {
    const tracker = new StakingMetricsTracker(ORACLE);
    tracker.handleEvent('voted', [ORACLE, 100, '50', 'admin']);
    assert.deepEqual(tracker.list(), []);
  });

  it('records slashes for this oracle only', () => {
    const tracker = new StakingMetricsTracker(ORACLE);
    tracker.handleEvent('slashed', [OTHER, 500, '10', 'admin-a'], '2026-01-01T00:00:00Z');
    assert.equal(tracker.list().length, 0);

    tracker.handleEvent('slashed', [ORACLE, 250, '99', 'admin-b'], '2026-01-02T00:00:00Z');
    assert.deepEqual(tracker.list(), [
      { bps: 250, slashAmount: '99', admin: 'admin-b', occurredAt: '2026-01-02T00:00:00Z' },
    ]);
  });

  it('caps recent slashes at 20 and keeps newest first', () => {
    const tracker = new StakingMetricsTracker(ORACLE);
    for (let i = 0; i < 25; i++) {
      tracker.handleEvent('slashed', [ORACLE, i, String(i), 'admin']);
    }
    const list = tracker.list();
    assert.equal(list.length, 20);
    assert.equal(list[0].bps, 24);
    assert.equal(list[19].bps, 5);
  });
});

describe('getStakingMetrics', () => {
  it('returns unregistered snapshot when oracle info is missing', async () => {
    const tracker = new StakingMetricsTracker(ORACLE);
    tracker.handleEvent('slashed', [ORACLE, 100, '1', 'admin']);
    const client = {
      oracleRegistry: {
        getOracleInfo: async () => null,
        getRegistryConfig: async () => {
          throw new Error('should not be called');
        },
      },
    };
    const metrics = await getStakingMetrics(client as never, ORACLE, tracker);
    assert.equal(metrics.registered, false);
    assert.equal(metrics.stakeAmount, '0');
    assert.equal(metrics.recentSlashes.length, 1);
    assert.equal(metrics.deregisterCooldown, null);
  });

  it('includes deregister cooldown when requested', async () => {
    const tracker = new StakingMetricsTracker(ORACLE);
    const nowSecs = Math.floor(Date.now() / 1000);
    const client = {
      oracleRegistry: {
        getOracleInfo: async () => ({
          isActive: true,
          stakeAmount: 1000n,
          totalVerifications: 3,
          totalSlashes: 1,
          deregisterRequestedAt: nowSecs - 10,
        }),
        getRegistryConfig: async () => ({
          deregisterCooldownSecs: 100,
        }),
      },
    };
    const metrics = await getStakingMetrics(client as never, ORACLE, tracker);
    assert.equal(metrics.registered, true);
    assert.equal(metrics.stakeAmount, '1000');
    assert.equal(metrics.totalVerifications, 3);
    assert.ok(metrics.deregisterCooldown);
    assert.equal(metrics.deregisterCooldown!.cooldownSecs, 100);
    assert.ok(metrics.deregisterCooldown!.remainingSecs <= 90);
    assert.ok(metrics.deregisterCooldown!.remainingSecs >= 0);
  });
});
