import { rpc as StellarRpc } from '@stellar/stellar-sdk';
import { BaseClient, nativeToScVal, scValToNative, Address } from './base';
import { Errors as OracleRegistryErrors } from '../generated/oracle_registry';
import type {
  ClientConfig,
  VerificationRound,
  OracleInfo,
  RegistryConfig,
  TransactionProgress,
} from '../types';
import type { Signer } from '../types';

export class OracleRegistryClient extends BaseClient {
  protected override readonly errors = OracleRegistryErrors;

  constructor(config: ClientConfig) {
    super(config);
  }

  async openRound(params: {
    signer: Signer;
    caller: string;
    invoiceId: bigint | number;
    oracleHash: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.caller,
      'open_verification_round',
      [
        new Address(params.caller).toScVal(),
        nativeToScVal(params.invoiceId, { type: 'u64' }),
        nativeToScVal(params.oracleHash, { type: 'string' }),
      ],
      params.onProgress,
    );
  }

  async register(params: {
    signer: Signer;
    operator: string;
    stakeAmount: bigint;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.operator,
      'register_oracle',
      [
        new Address(params.operator).toScVal(),
        nativeToScVal(params.stakeAmount, { type: 'i128' }),
      ],
      params.onProgress,
    );
  }

  async vote(params: {
    signer: Signer;
    oracle: string;
    invoiceId: bigint | number;
    approved: boolean;
    evidenceHash: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.oracle,
      'submit_vote',
      [
        new Address(params.oracle).toScVal(),
        nativeToScVal(params.invoiceId, { type: 'u64' }),
        nativeToScVal(params.approved, { type: 'bool' }),
        nativeToScVal(params.evidenceHash, { type: 'string' }),
      ],
      params.onProgress,
    );
  }

  async getRound(invoiceId: bigint | number): Promise<VerificationRound | null> {
    const sim = await this.simulate('get_verification_round', [
      nativeToScVal(invoiceId, { type: 'u64' }),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval);
    if (!raw) return null;
    const r = raw as Record<string, unknown>;
    return {
      invoiceId: BigInt(String(r.invoice_id)),
      requiredVotes: Number(r.required_votes),
      totalRegisteredOracles: Number(r.total_registered_oracles),
      weightFor: BigInt(String(r.weight_for)),
      weightAgainst: BigInt(String(r.weight_against)),
      totalStakeSnapshot: BigInt(String(r.total_stake_snapshot)),
      quorumBps: Number(r.quorum_bps),
      status: r.status as VerificationRound['status'],
      openedAt: Number(r.opened_at),
      deadline: Number(r.deadline),
      oracleHash: r.oracle_hash as string,
    };
  }

  async getRegistryConfig(): Promise<RegistryConfig> {
    const sim = await this.simulate('get_registry_config', []);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval) as Record<string, unknown>;
    return {
      minStake: BigInt(String(raw.min_stake)),
      stakeToken: raw.stake_token as string,
      requiredVotes: Number(raw.required_votes),
      quorumBps: Number(raw.quorum_bps),
      roundDurationSecs: Number(raw.round_duration_secs),
      deregisterCooldownSecs: Number(raw.deregister_cooldown_secs),
      treasury: (raw.treasury as string | null) ?? undefined,
    };
  }

  async getOracleInfo(operator: string): Promise<OracleInfo | null> {
    const sim = await this.simulate('get_oracle_info', [
      new Address(operator).toScVal(),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval);
    if (!raw) return null;
    const r = raw as Record<string, unknown>;
    return {
      address: r.address as string,
      stakeAmount: BigInt(String(r.stake_amount)),
      stakeToken: r.stake_token as string,
      isActive: Boolean(r.is_active),
      totalVerifications: Number(r.total_verifications),
      totalSlashes: Number(r.total_slashes),
      registeredAt: Number(r.registered_at),
      deregisterRequestedAt:
        r.deregister_requested_at !== undefined && r.deregister_requested_at !== null
          ? Number(r.deregister_requested_at)
          : undefined,
    };
  }
}
