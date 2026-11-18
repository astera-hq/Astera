import { rpc as StellarRpc } from '@stellar/stellar-sdk';
import { BaseClient, nativeToScVal, scValToNative, Address, xdr } from './base';
import { Errors as ArbitrationErrors } from '../generated/arbitration';
import type {
  ClientConfig,
  DisputeCase,
  DisputeResolution,
  EvidenceEntry,
  JurorInfo,
  JurorVote,
  ArbitrationConfig,
  TransactionProgress,
} from '../types';

/// #1043: `vote_choice` maps onto arbitration's on-chain convention — `true`
/// means "in favor of the debtor", `false` means "in favor of the SME" —
/// matching `DisputeResolution`'s `InFavorOfDebtor`/`InFavorOfSME` outcome.
export async function computeCommitHash(vote: boolean, salt: Uint8Array): Promise<Uint8Array> {
  const preimage = new Uint8Array(1 + salt.length);
  preimage[0] = vote ? 1 : 0;
  preimage.set(salt, 1);
  const digest = await crypto.subtle.digest('SHA-256', preimage);
  return new Uint8Array(digest);
}

/// Generates a fresh random 32-byte salt for a commit-reveal vote. Callers
/// are responsible for persisting it (e.g. `localStorage`, keyed by case id)
/// until the reveal phase — losing it means the committed vote can never be
/// revealed and correctly counted.
export function generateSalt(): Uint8Array {
  const salt = new Uint8Array(32);
  crypto.getRandomValues(salt);
  return salt;
}

function decodeCase(raw: Record<string, unknown>): DisputeCase {
  return {
    id: BigInt(String(raw.id)),
    invoiceId: BigInt(String(raw.invoice_id)),
    claimant: raw.claimant as string,
    respondent: raw.respondent as string,
    amount: BigInt(String(raw.amount)),
    openedAt: Number(raw.opened_at),
    evidenceDeadline: Number(raw.evidence_deadline),
    commitDeadline: Number(raw.commit_deadline),
    revealDeadline: Number(raw.reveal_deadline),
    jurors: (raw.jurors as string[]) ?? [],
    status: raw.status as DisputeCase['status'],
    resolution: raw.resolution as DisputeResolution,
    retryCount: Number(raw.retry_count),
  };
}

export class ArbitrationClient extends BaseClient {
  protected override readonly errors = ArbitrationErrors;

  constructor(config: ClientConfig) {
    super(config);
  }

  async registerJuror(params: {
    signer: import('../types').Signer;
    operator: string;
    stakeAmount: bigint;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.operator,
      'register_juror',
      [
        new Address(params.operator).toScVal(),
        nativeToScVal(params.stakeAmount, { type: 'i128' }),
      ],
      params.onProgress,
    );
  }

  async deregisterJuror(params: {
    signer: import('../types').Signer;
    operator: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.operator,
      'deregister_juror',
      [new Address(params.operator).toScVal()],
      params.onProgress,
    );
  }

  async submitEvidence(params: {
    signer: import('../types').Signer;
    caseId: bigint | number;
    submitter: string;
    evidenceHash: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.submitter,
      'submit_evidence',
      [
        nativeToScVal(params.caseId, { type: 'u64' }),
        new Address(params.submitter).toScVal(),
        nativeToScVal(params.evidenceHash, { type: 'string' }),
      ],
      params.onProgress,
    );
  }

  /// Permissionless tick — `caller` just needs to sign/pay the transaction
  /// fee, it isn't checked for any particular role on-chain.
  async selectJurors(params: {
    signer: import('../types').Signer;
    caller: string;
    caseId: bigint | number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.caller,
      'select_jurors',
      [nativeToScVal(params.caseId, { type: 'u64' })],
      params.onProgress,
    );
  }

  async commitVote(params: {
    signer: import('../types').Signer;
    juror: string;
    caseId: bigint | number;
    commitHash: Uint8Array;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.juror,
      'commit_vote',
      [
        nativeToScVal(params.caseId, { type: 'u64' }),
        new Address(params.juror).toScVal(),
        nativeToScVal(Buffer.from(params.commitHash), { type: 'bytes' }),
      ],
      params.onProgress,
    );
  }

  async revealVote(params: {
    signer: import('../types').Signer;
    juror: string;
    caseId: bigint | number;
    voteChoice: boolean;
    salt: Uint8Array;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.juror,
      'reveal_vote',
      [
        nativeToScVal(params.caseId, { type: 'u64' }),
        new Address(params.juror).toScVal(),
        nativeToScVal(params.voteChoice, { type: 'bool' }),
        nativeToScVal(Buffer.from(params.salt), { type: 'bytes' }),
      ],
      params.onProgress,
    );
  }

  /// Permissionless tick, same caller-just-pays-fees shape as `selectJurors`.
  async finalizeCase(params: {
    signer: import('../types').Signer;
    caller: string;
    caseId: bigint | number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.caller,
      'finalize_case',
      [nativeToScVal(params.caseId, { type: 'u64' })],
      params.onProgress,
    );
  }

  async adminResolveNoQuorum(params: {
    signer: import('../types').Signer;
    admin: string;
    caseId: bigint | number;
    resolution: DisputeResolution;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'admin_resolve_no_quorum',
      [
        new Address(params.admin).toScVal(),
        nativeToScVal(params.caseId, { type: 'u64' }),
        // `DisputeResolution` is a unit-variant Rust enum — same encoding
        // shape as `access_control.ts`'s `roleToScVal`/`ActionPayload` tags.
        xdr.ScVal.scvVec([nativeToScVal(params.resolution, { type: 'symbol' })]),
      ],
      params.onProgress,
    );
  }

  async getCase(caseId: bigint | number): Promise<DisputeCase | null> {
    const sim = await this.simulate('get_case', [nativeToScVal(caseId, { type: 'u64' })]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval);
    if (!raw) return null;
    return decodeCase(raw as Record<string, unknown>);
  }

  async getEvidence(caseId: bigint | number): Promise<EvidenceEntry[]> {
    const sim = await this.simulate('get_evidence', [nativeToScVal(caseId, { type: 'u64' })]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval) as Record<string, unknown>[];
    return raw.map((r) => ({
      submitter: r.submitter as string,
      party: r.party as EvidenceEntry['party'],
      evidenceHash: r.evidence_hash as string,
      submittedAt: Number(r.submitted_at),
    }));
  }

  async getVote(caseId: bigint | number, juror: string): Promise<JurorVote | null> {
    const sim = await this.simulate('get_vote', [
      nativeToScVal(caseId, { type: 'u64' }),
      new Address(juror).toScVal(),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval);
    if (!raw) return null;
    const r = raw as Record<string, unknown>;
    return {
      commitHash: r.commit_hash ? Buffer.from(r.commit_hash as Uint8Array).toString('hex') : undefined,
      revealedVote: r.revealed_vote !== undefined && r.revealed_vote !== null ? Boolean(r.revealed_vote) : undefined,
    };
  }

  async getJuror(operator: string): Promise<JurorInfo | null> {
    const sim = await this.simulate('get_juror', [new Address(operator).toScVal()]);
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
      casesServed: Number(r.cases_served),
      timesSlashed: Number(r.times_slashed),
      nonRevealStrikes: Number(r.non_reveal_strikes),
      registeredAt: Number(r.registered_at),
      deregisterRequestedAt:
        r.deregister_requested_at !== undefined && r.deregister_requested_at !== null
          ? Number(r.deregister_requested_at)
          : undefined,
    };
  }

  async listActiveJurors(): Promise<string[]> {
    const sim = await this.simulate('list_active_jurors', []);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    return (scValToNative(sim.result!.retval) as string[]) ?? [];
  }

  async getJurorCases(operator: string): Promise<bigint[]> {
    const sim = await this.simulate('get_juror_cases', [new Address(operator).toScVal()]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = (scValToNative(sim.result!.retval) as unknown[]) ?? [];
    return raw.map((v) => BigInt(String(v)));
  }

  async getConfig(): Promise<ArbitrationConfig> {
    const sim = await this.simulate('get_config', []);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval) as Record<string, unknown>;
    return {
      minStake: BigInt(String(raw.min_stake)),
      stakeToken: raw.stake_token as string,
      committeeSize: Number(raw.committee_size),
      quorumFloor: Number(raw.quorum_floor),
      evidenceWindowSecs: Number(raw.evidence_window_secs),
      commitWindowSecs: Number(raw.commit_window_secs),
      revealWindowSecs: Number(raw.reveal_window_secs),
      deregisterCooldownSecs: Number(raw.deregister_cooldown_secs),
      slashBps: Number(raw.slash_bps),
      nonRevealSlashBps: Number(raw.non_reveal_slash_bps),
      lopsidedConfidenceBps: Number(raw.lopsided_confidence_bps),
      maxRetries: Number(raw.max_retries),
    };
  }
}
