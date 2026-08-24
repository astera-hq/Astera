import { rpc as StellarRpc } from '@stellar/stellar-sdk';
import { BaseClient, nativeToScVal, scValToNative, Address, xdr } from './base';
import { GovernanceError } from '../generated/governance';
import type {
  ClientConfig,
  GovernanceProposal,
  GovernanceProposalStatus,
  ProposalCategory,
  GovernanceConfig,
  TransactionProgress,
  Signer,
} from '../types';

function categoryToScVal(category: ProposalCategory): xdr.ScVal {
  return xdr.ScVal.scvVec([nativeToScVal(category, { type: 'symbol' })]);
}

const STATUS_MAP: Record<string, GovernanceProposalStatus> = {
  Active: 'Active',
  Passed: 'Passed',
  Rejected: 'Rejected',
  Executed: 'Executed',
  Cancelled: 'Cancelled',
  Expired: 'Expired',
};

function parseProposal(raw: Record<string, unknown>): GovernanceProposal {
  return {
    id: BigInt(raw.id as bigint | number),
    proposer: raw.proposer as string,
    description: raw.description as string,
    targetContract: raw.target_contract as string,
    functionName: raw.function_name as string,
    calldata: raw.calldata as string,
    votesFor: BigInt(raw.votes_for as bigint | number),
    votesAgainst: BigInt(raw.votes_against as bigint | number),
    status: STATUS_MAP[raw.status as string] ?? 'Active',
    createdAt: BigInt(raw.created_at as bigint | number),
    votingEndsAt: BigInt(raw.voting_ends_at as bigint | number),
    executionDelay: BigInt(raw.execution_delay as bigint | number),
    snapshotSupply: BigInt(raw.snapshot_supply as bigint | number),
    passedAt: BigInt(raw.passed_at as bigint | number),
    category: raw.category as ProposalCategory,
    quorumBps: Number(raw.quorum_bps),
    passBps: Number(raw.pass_bps),
  };
}

export class GovernanceClient extends BaseClient {
  protected override readonly errors = GovernanceError;

  constructor(config: ClientConfig) {
    super(config);
  }

  // ─── Read-only methods ───────────────────────────────────────────────

  async getConfig(): Promise<GovernanceConfig> {
    const result = await this.simulate('get_config', []);
    if (StellarRpc.Api.isSimulationError(result)) {
      throw new Error(`Simulation failed: ${result.error}`);
    }
    const raw = scValToNative(result.result!.retval) as Record<string, unknown>;
    return {
      admin: raw.admin as string,
      shareToken: raw.share_token as string,
      votingPeriodSecs: BigInt(raw.voting_period_secs as bigint | number),
      quorumBps: Number(raw.quorum_bps),
      passBps: Number(raw.pass_bps),
      executionDelaySecs: BigInt(raw.execution_delay_secs as bigint | number),
      minShareBalance: BigInt(raw.min_share_balance as bigint | number),
      treasuryQuorumBps: Number(raw.treasury_quorum_bps ?? 0),
      criticalQuorumBps: Number(raw.critical_quorum_bps ?? 0),
    };
  }

  async getProposal(proposalId: bigint | number): Promise<GovernanceProposal | null> {
    const result = await this.simulate('get_proposal', [
      nativeToScVal(proposalId, { type: 'u64' }),
    ]);
    if (StellarRpc.Api.isSimulationError(result)) {
      throw new Error(`Simulation failed: ${result.error}`);
    }
    const raw = scValToNative(result.result!.retval) as Record<string, unknown> | null;
    if (!raw) return null;
    return parseProposal(raw);
  }

  async listProposals(): Promise<GovernanceProposal[]> {
    const result = await this.simulate('list_proposals', []);
    if (StellarRpc.Api.isSimulationError(result)) {
      throw new Error(`Simulation failed: ${result.error}`);
    }
    const raw = (scValToNative(result.result!.retval) as Record<string, unknown>[]) ?? [];
    return raw.map(parseProposal);
  }

  async getVotingPower(proposalId: bigint | number, voter: string): Promise<bigint> {
    const result = await this.simulate('get_voting_power', [
      nativeToScVal(proposalId, { type: 'u64' }),
      new Address(voter).toScVal(),
    ]);
    if (StellarRpc.Api.isSimulationError(result)) {
      throw new Error(`Simulation failed: ${result.error}`);
    }
    return BigInt(scValToNative(result.result!.retval) as bigint | number);
  }

  async hasVoted(proposalId: bigint | number, voter: string): Promise<boolean> {
    const result = await this.simulate('has_voted', [
      nativeToScVal(proposalId, { type: 'u64' }),
      new Address(voter).toScVal(),
    ]);
    if (StellarRpc.Api.isSimulationError(result)) {
      throw new Error(`Simulation failed: ${result.error}`);
    }
    return Boolean(scValToNative(result.result!.retval));
  }

  async getCategoryQuorum(category: ProposalCategory): Promise<number> {
    const result = await this.simulate('get_category_quorum', [
      categoryToScVal(category),
    ]);
    if (StellarRpc.Api.isSimulationError(result)) {
      throw new Error(`Simulation failed: ${result.error}`);
    }
    return Number(scValToNative(result.result!.retval));
  }

  async getAccessControl(): Promise<string | null> {
    const result = await this.simulate('get_access_control', []);
    if (StellarRpc.Api.isSimulationError(result)) {
      throw new Error(`Simulation failed: ${result.error}`);
    }
    const raw = scValToNative(result.result!.retval) as Record<string, unknown> | null;
    return (raw?.address as string) ?? null;
  }

  // ─── Write methods ───────────────────────────────────────────────────

  async initialize(params: {
    signer: Signer;
    admin: string;
    shareToken: string;
    votingPeriodSecs: bigint | number;
    quorumBps: number;
    passBps: number;
    executionDelaySecs: bigint | number;
    minShareBalance: bigint | number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'initialize',
      [
        new Address(params.admin).toScVal(),
        new Address(params.shareToken).toScVal(),
        nativeToScVal(params.votingPeriodSecs, { type: 'u64' }),
        nativeToScVal(params.quorumBps, { type: 'u32' }),
        nativeToScVal(params.passBps, { type: 'u32' }),
        nativeToScVal(params.executionDelaySecs, { type: 'u64' }),
        nativeToScVal(params.minShareBalance, { type: 'i128' }),
      ],
      params.onProgress,
    );
  }

  async createProposal(params: {
    signer: Signer;
    proposer: string;
    description: string;
    targetContract: string;
    functionName: string;
    calldata: string;
    category: ProposalCategory;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<bigint> {
    const result = await this.buildAndSendTx(
      params.proposer,
      'create_proposal',
      [
        new Address(params.proposer).toScVal(),
        nativeToScVal(params.description, { type: 'string' }),
        new Address(params.targetContract).toScVal(),
        nativeToScVal(params.functionName, { type: 'string' }),
        nativeToScVal(params.calldata, { type: 'string' }),
        categoryToScVal(params.category),
      ],
      params.onProgress,
    );
    // The return value is a proposal ID encoded in the result
    return BigInt(result);
  }

  async vote(params: {
    signer: Signer;
    proposalId: bigint | number;
    voter: string;
    inFavor: boolean;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.voter,
      'vote',
      [
        nativeToScVal(params.proposalId, { type: 'u64' }),
        new Address(params.voter).toScVal(),
        nativeToScVal(params.inFavor, { type: 'bool' }),
      ],
      params.onProgress,
    );
  }

  async executeProposal(params: {
    signer: Signer;
    proposalId: bigint | number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.signer,
      'execute_proposal',
      [nativeToScVal(params.proposalId, { type: 'u64' })],
      params.onProgress,
    );
  }

  async cancelProposal(params: {
    signer: Signer;
    proposalId: bigint | number;
    caller: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.caller,
      'cancel_proposal',
      [
        nativeToScVal(params.proposalId, { type: 'u64' }),
        new Address(params.caller).toScVal(),
      ],
      params.onProgress,
    );
  }

  async updateConfig(params: {
    signer: Signer;
    caller: string;
    quorumBps: number;
    passBps: number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.caller,
      'update_config',
      [
        new Address(params.caller).toScVal(),
        nativeToScVal(params.quorumBps, { type: 'u32' }),
        nativeToScVal(params.passBps, { type: 'u32' }),
      ],
      params.onProgress,
    );
  }

  async setCategoryQuorum(params: {
    signer: Signer;
    caller: string;
    category: ProposalCategory;
    quorumBps: number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.caller,
      'set_category_quorum',
      [
        new Address(params.caller).toScVal(),
        categoryToScVal(params.category),
        nativeToScVal(params.quorumBps, { type: 'u32' }),
      ],
      params.onProgress,
    );
  }

  async setAccessControl(params: {
    signer: Signer;
    caller: string;
    accessControl: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.caller,
      'set_access_control',
      [
        new Address(params.caller).toScVal(),
        new Address(params.accessControl).toScVal(),
      ],
      params.onProgress,
    );
  }
}
