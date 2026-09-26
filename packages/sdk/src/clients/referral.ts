import { rpc as StellarRpc } from '@stellar/stellar-sdk';
import { BaseClient, nativeToScVal, scValToNative, Address, xdr } from './base';
import { Errors as ReferralErrors } from '../generated/referral';
import type { ReferralStats, LeaderboardEntry } from '../generated/referral';
import type { ClientConfig, TransactionProgress, Signer } from '../types';

function referralStatsFromRaw(raw: Record<string, unknown>): ReferralStats {
  return {
    referrer: String(raw.referrer),
    referral_count: Number(raw.referral_count),
  };
}

function leaderboardEntryFromRaw(raw: Record<string, unknown>): LeaderboardEntry {
  return {
    referrer: String(raw.referrer),
    referral_count: Number(raw.referral_count),
  };
}

export class ReferralClient extends BaseClient {
  protected override readonly errors = ReferralErrors;

  constructor(config: ClientConfig) {
    super(config);
  }

  async initialize(params: {
    signer: Signer;
    admin: string;
    pool: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'initialize',
      [new Address(params.admin).toScVal(), new Address(params.pool).toScVal()],
      params.onProgress,
    );
  }

  async pause(params: {
    signer: Signer;
    admin: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'pause',
      [new Address(params.admin).toScVal()],
      params.onProgress,
    );
  }

  async unpause(params: {
    signer: Signer;
    admin: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'unpause',
      [new Address(params.admin).toScVal()],
      params.onProgress,
    );
  }

  async isPaused(): Promise<boolean> {
    const sim = await this.simulate('is_paused', []);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    return Boolean(scValToNative(sim.result!.retval));
  }

  async setPool(params: {
    signer: Signer;
    admin: string;
    pool: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'set_pool',
      [new Address(params.admin).toScVal(), new Address(params.pool).toScVal()],
      params.onProgress,
    );
  }

  async getPool(): Promise<string> {
    const sim = await this.simulate('get_pool', []);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    return String(scValToNative(sim.result!.retval));
  }

  async setBorrowRewardBps(params: {
    signer: Signer;
    admin: string;
    bps: number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'set_borrow_reward_bps',
      [new Address(params.admin).toScVal(), nativeToScVal(params.bps, { type: 'u32' })],
      params.onProgress,
    );
  }

  async setDepositRewardBps(params: {
    signer: Signer;
    admin: string;
    bps: number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'set_deposit_reward_bps',
      [new Address(params.admin).toScVal(), nativeToScVal(params.bps, { type: 'u32' })],
      params.onProgress,
    );
  }

  async setAccessControl(params: {
    signer: Signer;
    admin: string;
    accessControl: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'set_access_control',
      [new Address(params.admin).toScVal(), new Address(params.accessControl).toScVal()],
      params.onProgress,
    );
  }

  async getAccessControl(): Promise<string | null> {
    const sim = await this.simulate('get_access_control', []);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    const raw = scValToNative(sim.result!.retval);
    return raw ? String(raw) : null;
  }

  async getBorrowRewardBps(): Promise<number> {
    const sim = await this.simulate('get_borrow_reward_bps', []);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    return Number(scValToNative(sim.result!.retval));
  }

  async getDepositRewardBps(): Promise<number> {
    const sim = await this.simulate('get_deposit_reward_bps', []);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    return Number(scValToNative(sim.result!.retval));
  }

  async register(params: {
    signer: Signer;
    referee: string;
    referrer: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.referee,
      'register',
      [new Address(params.referee).toScVal(), new Address(params.referrer).toScVal()],
      params.onProgress,
    );
  }

  async getReferrer(referee: string): Promise<string | null> {
    const sim = await this.simulate('get_referrer', [new Address(referee).toScVal()]);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    const raw = scValToNative(sim.result!.retval);
    return raw ? String(raw) : null;
  }

  async recordActivity(params: {
    signer: Signer;
    caller: string;
    referee: string;
    kind: string;
    feeAmount: bigint;
    token: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.caller,
      'record_activity',
      [
        new Address(params.caller).toScVal(),
        new Address(params.referee).toScVal(),
        nativeToScVal(params.kind, { type: 'symbol' }),
        nativeToScVal(params.feeAmount, { type: 'i128' }),
        new Address(params.token).toScVal(),
      ],
      params.onProgress,
    );
  }

  async getPendingReward(referrer: string, token: string): Promise<bigint> {
    const sim = await this.simulate('get_pending_reward', [
      new Address(referrer).toScVal(),
      new Address(token).toScVal(),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    return BigInt(String(scValToNative(sim.result!.retval)));
  }

  async claimRewards(params: {
    signer: Signer;
    referrer: string;
    token: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.referrer,
      'claim_rewards',
      [new Address(params.referrer).toScVal(), new Address(params.token).toScVal()],
      params.onProgress,
    );
  }

  async getStats(referrer: string): Promise<ReferralStats> {
    const sim = await this.simulate('get_stats', [new Address(referrer).toScVal()]);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    return referralStatsFromRaw(scValToNative(sim.result!.retval) as Record<string, unknown>);
  }

  async getTopReferrers(limit: number): Promise<LeaderboardEntry[]> {
    const sim = await this.simulate('get_top_referrers', [
      nativeToScVal(limit, { type: 'u32' }),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    const raw = scValToNative(sim.result!.retval) as Record<string, unknown>[];
    return (raw ?? []).map(leaderboardEntryFromRaw);
  }
}
