import { rpc as StellarRpc } from '@stellar/stellar-sdk';
import { BaseClient, nativeToScVal, scValToNative, Address, xdr } from './base';
import type {
  ClientConfig,
  Signer,
  TransactionProgress,
  Role,
  MultiSigConfig,
  ProposalStatus,
  ActionPayload,
  Proposal,
} from '../types';

function roleToScVal(role: Role): xdr.ScVal {
  return xdr.ScVal.scvVec([nativeToScVal(role, { type: 'symbol' })]);
}

function roleFromNative(raw: unknown): Role {
  // A unit-variant contracttype enum decodes to either the bare tag string
  // or a one-element array wrapping it, depending on SDK version — handle
  // both so this doesn't silently break on a minor @stellar/stellar-sdk bump.
  if (Array.isArray(raw)) return raw[0] as Role;
  return raw as Role;
}

/**
 * `scValToNative` decodes our `[tag, ...fields]` vec encoding of an
 * `ActionPayload` variant into a flat native array (e.g. `['SetYield', 650]`),
 * not the `{ tag, values }` shape `ActionPayload` expects — reshape it here.
 */
function actionPayloadFromNative(raw: unknown): ActionPayload {
  const [tag, ...values] = raw as [ActionPayload['tag'], ...unknown[]];
  return { tag, values } as ActionPayload;
}

/**
 * Encodes an `ActionPayload` (mirrors contracts/access_control/src/lib.rs's
 * `ActionPayload` enum) into the ScVal shape Soroban expects for a Rust
 * `#[contracttype]` enum-with-payload: a Vec whose first element is the
 * variant name (Symbol) followed by the variant's fields in declaration
 * order.
 */
function actionPayloadToScVal(action: ActionPayload): xdr.ScVal {
  const tag = nativeToScVal(action.tag, { type: 'symbol' });
  switch (action.tag) {
    case 'SetPaused':
    case 'SetKycRequired': {
      const [paused] = action.values;
      return xdr.ScVal.scvVec([tag, nativeToScVal(paused, { type: 'bool' })]);
    }
    case 'SetYield':
    case 'SetMaxUtilization': {
      const [bps] = action.values;
      return xdr.ScVal.scvVec([tag, nativeToScVal(bps, { type: 'u32' })]);
    }
    case 'SetTreasury':
    case 'SetOracleContract':
    case 'SetOracle':
    case 'AddKeeper': {
      const [address] = action.values;
      return xdr.ScVal.scvVec([tag, new Address(address).toScVal()]);
    }
    case 'WithdrawRevenue': {
      const [token, amount] = action.values;
      return xdr.ScVal.scvVec([
        tag,
        new Address(token).toScVal(),
        nativeToScVal(amount, { type: 'i128' }),
      ]);
    }
    case 'SetInvestorKyc': {
      const [investor, approved] = action.values;
      return xdr.ScVal.scvVec([
        tag,
        new Address(investor).toScVal(),
        nativeToScVal(approved, { type: 'bool' }),
      ]);
    }
    case 'RegisterDebtor': {
      const [debtorId, debtorName, maxExposure] = action.values;
      return xdr.ScVal.scvVec([
        tag,
        nativeToScVal(debtorId, { type: 'string' }),
        nativeToScVal(debtorName, { type: 'string' }),
        nativeToScVal(maxExposure, { type: 'i128' }),
      ]);
    }
    case 'DeactivateDebtor': {
      const [debtorId] = action.values;
      return xdr.ScVal.scvVec([tag, nativeToScVal(debtorId, { type: 'string' })]);
    }
    case 'SetLateThreshold': {
      const [days] = action.values;
      return xdr.ScVal.scvVec([tag, nativeToScVal(days, { type: 'i64' })]);
    }
    case 'SetScoreThresholds': {
      const [excellent, veryGood, good, fair] = action.values;
      return xdr.ScVal.scvVec([
        tag,
        nativeToScVal(excellent, { type: 'u32' }),
        nativeToScVal(veryGood, { type: 'u32' }),
        nativeToScVal(good, { type: 'u32' }),
        nativeToScVal(fair, { type: 'u32' }),
      ]);
    }
    case 'RegisterAttestor': {
      const [address, attestorType, weightBps] = action.values;
      return xdr.ScVal.scvVec([
        tag,
        new Address(address).toScVal(),
        nativeToScVal(attestorType, { type: 'u32' }),
        nativeToScVal(weightBps, { type: 'u32' }),
      ]);
    }
    case 'AddSigner':
    case 'RemoveSigner': {
      const [role, address] = action.values;
      return xdr.ScVal.scvVec([tag, roleToScVal(role), new Address(address).toScVal()]);
    }
    case 'SetThreshold': {
      const [role, threshold] = action.values;
      return xdr.ScVal.scvVec([tag, roleToScVal(role), nativeToScVal(threshold, { type: 'u32' })]);
    }
    case 'SetOracleRegistryInvoiceContract':
    case 'SetReferralPool': {
      const [address] = action.values;
      return xdr.ScVal.scvVec([tag, new Address(address).toScVal()]);
    }
    case 'SetOracleRegistryTreasury': {
      const [treasury] = action.values;
      return xdr.ScVal.scvVec([
        tag,
        treasury === undefined ? xdr.ScVal.scvVoid() : new Address(treasury).toScVal(),
      ]);
    }
    case 'SetOracleRegistryConfig': {
      const [minStake, requiredVotes, quorumBps, roundDurationSecs, deregisterCooldownSecs] =
        action.values;
      return xdr.ScVal.scvVec([
        tag,
        nativeToScVal(minStake, { type: 'i128' }),
        nativeToScVal(requiredVotes, { type: 'u32' }),
        nativeToScVal(quorumBps, { type: 'u32' }),
        nativeToScVal(roundDurationSecs, { type: 'u64' }),
        nativeToScVal(deregisterCooldownSecs, { type: 'u64' }),
      ]);
    }
    case 'SetOracleRegistryPaused':
    case 'SetCompliancePaused':
    case 'SetReferralPaused': {
      const [paused] = action.values;
      return xdr.ScVal.scvVec([tag, nativeToScVal(paused, { type: 'bool' })]);
    }
    case 'SlashOracle': {
      const [operator, bps, roundId, evidence] = action.values;
      return xdr.ScVal.scvVec([
        tag,
        new Address(operator).toScVal(),
        nativeToScVal(bps, { type: 'u32' }),
        nativeToScVal(roundId, { type: 'u64' }),
        nativeToScVal(evidence, { type: 'string' }),
      ]);
    }
    case 'AdminResolveRound': {
      const [invoiceId, approved, reason] = action.values;
      return xdr.ScVal.scvVec([
        tag,
        nativeToScVal(invoiceId, { type: 'u64' }),
        nativeToScVal(approved, { type: 'bool' }),
        nativeToScVal(reason, { type: 'string' }),
      ]);
    }
    case 'RegisterScreener':
    case 'ConfirmScreenerRegistration':
    case 'DeregisterScreener': {
      const [screener] = action.values;
      return xdr.ScVal.scvVec([tag, new Address(screener).toScVal()]);
    }
    case 'SetRescreeningInterval':
    case 'SetScreenerTimelock': {
      const [secs] = action.values;
      return xdr.ScVal.scvVec([tag, nativeToScVal(secs, { type: 'u64' })]);
    }
    case 'UpdateGovernanceConfig': {
      const [quorumBps, passBps] = action.values;
      return xdr.ScVal.scvVec([
        tag,
        nativeToScVal(quorumBps, { type: 'u32' }),
        nativeToScVal(passBps, { type: 'u32' }),
      ]);
    }
    case 'SetCategoryQuorum': {
      const [category, quorumBps] = action.values;
      return xdr.ScVal.scvVec([
        tag,
        nativeToScVal(category, { type: 'u32' }),
        nativeToScVal(quorumBps, { type: 'u32' }),
      ]);
    }
    case 'SetBorrowRewardBps':
    case 'SetDepositRewardBps': {
      const [bps] = action.values;
      return xdr.ScVal.scvVec([tag, nativeToScVal(bps, { type: 'u32' })]);
    }
    case 'SetInvoiceAccessControl':
    case 'SetCreditScoreAccessControl':
    case 'SetOracleRegistryAccessControl':
    case 'SetComplianceAccessControl':
    case 'SetGovernanceAccessControl':
    case 'SetReferralAccessControl': {
      const [newAccessControl] = action.values;
      return xdr.ScVal.scvVec([tag, new Address(newAccessControl).toScVal()]);
    }
  }
}

export class AccessControlClient extends BaseClient {
  constructor(config: ClientConfig) {
    super(config);
  }

  async initialize(params: {
    signer: Signer;
    superAdminSigners: string[];
    superAdminThreshold: number;
    proposalExpirySecs: number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    const source = params.superAdminSigners[0];
    return this.buildAndSendTx(
      source,
      'initialize',
      [
        xdr.ScVal.scvVec(params.superAdminSigners.map((a) => new Address(a).toScVal())),
        nativeToScVal(params.superAdminThreshold, { type: 'u32' }),
        nativeToScVal(params.proposalExpirySecs, { type: 'u64' }),
      ],
      params.onProgress,
    );
  }

  async getRoleConfig(role: Role): Promise<MultiSigConfig | null> {
    const result = await this.simulate('get_role_config', [roleToScVal(role)]);
    if (StellarRpc.Api.isSimulationError(result)) {
      throw new Error(`Simulation failed: ${result.error}`);
    }
    const raw = scValToNative(result.result!.retval) as Record<string, unknown> | null;
    if (!raw) return null;
    return {
      signers: (raw.signers as string[]) ?? [],
      threshold: Number(raw.threshold ?? 0),
    };
  }

  async isSigner(role: Role, address: string): Promise<boolean> {
    const result = await this.simulate('is_signer', [roleToScVal(role), new Address(address).toScVal()]);
    if (StellarRpc.Api.isSimulationError(result)) {
      throw new Error(`Simulation failed: ${result.error}`);
    }
    return Boolean(scValToNative(result.result!.retval));
  }

  /** One past the highest issued proposal id — proposals exist for `0..getNextProposalId()`. */
  async getNextProposalId(): Promise<bigint> {
    const result = await this.simulate('get_next_proposal_id', []);
    if (StellarRpc.Api.isSimulationError(result)) {
      throw new Error(`Simulation failed: ${result.error}`);
    }
    return BigInt(scValToNative(result.result!.retval) as bigint | number);
  }

  /**
   * Fetches every proposal id in `0..getNextProposalId()`, for a simple
   * admin-page proposal queue. Not paginated — fine for the scale this
   * governance system is meant for (tens, not thousands, of proposals).
   */
  async listProposals(): Promise<Array<{ id: bigint; proposal: Proposal }>> {
    const nextId = await this.getNextProposalId();
    const results: Array<{ id: bigint; proposal: Proposal }> = [];
    for (let id = 0n; id < nextId; id += 1n) {
      const proposal = await this.getProposal(id);
      if (proposal) results.push({ id, proposal });
    }
    return results;
  }

  async getProposal(proposalId: bigint | number): Promise<Proposal | null> {
    const result = await this.simulate('get_proposal', [
      nativeToScVal(proposalId, { type: 'u64' }),
    ]);
    if (StellarRpc.Api.isSimulationError(result)) {
      throw new Error(`Simulation failed: ${result.error}`);
    }
    const raw = scValToNative(result.result!.retval) as Record<string, unknown> | null;
    if (!raw) return null;
    return {
      role: roleFromNative(raw.role),
      target: raw.target as string,
      action: actionPayloadFromNative(raw.action),
      proposer: raw.proposer as string,
      approvals: (raw.approvals as string[]) ?? [],
      createdAt: BigInt((raw.created_at as bigint | number | string) ?? 0),
      expiresAt: BigInt((raw.expires_at as bigint | number | string) ?? 0),
      status: roleFromNative(raw.status) as unknown as ProposalStatus,
    };
  }

  async proposeAction(params: {
    signer: Signer;
    role: Role;
    proposer: string;
    target: string;
    action: ActionPayload;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.proposer,
      'propose_action',
      [
        roleToScVal(params.role),
        new Address(params.proposer).toScVal(),
        new Address(params.target).toScVal(),
        actionPayloadToScVal(params.action),
      ],
      params.onProgress,
    );
  }

  async approveAction(params: {
    signer: Signer;
    signerAddress: string;
    proposalId: bigint | number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.signerAddress,
      'approve_action',
      [
        new Address(params.signerAddress).toScVal(),
        nativeToScVal(params.proposalId, { type: 'u64' }),
      ],
      params.onProgress,
    );
  }

  async revokeApproval(params: {
    signer: Signer;
    signerAddress: string;
    proposalId: bigint | number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.signerAddress,
      'revoke_approval',
      [
        new Address(params.signerAddress).toScVal(),
        nativeToScVal(params.proposalId, { type: 'u64' }),
      ],
      params.onProgress,
    );
  }

  async rejectAction(params: {
    signer: Signer;
    signerAddress: string;
    proposalId: bigint | number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.signerAddress,
      'reject_action',
      [
        new Address(params.signerAddress).toScVal(),
        nativeToScVal(params.proposalId, { type: 'u64' }),
      ],
      params.onProgress,
    );
  }

  async executeAction(params: {
    signer: Signer;
    caller: string;
    proposalId: bigint | number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.caller,
      'execute_action',
      [new Address(params.caller).toScVal(), nativeToScVal(params.proposalId, { type: 'u64' })],
      params.onProgress,
    );
  }
}
