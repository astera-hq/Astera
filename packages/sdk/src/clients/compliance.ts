import { rpc as StellarRpc } from '@stellar/stellar-sdk';
import { BaseClient, nativeToScVal, scValToNative, Address, xdr } from './base';
import type {
  ClientConfig,
  ComplianceStatus,
  ComplianceRecord,
  RiskTier,
  ScreeningHistoryEntry,
  TransactionProgress,
} from '../types';
import type { Signer } from '../types';

function statusToScVal(status: ComplianceStatus): xdr.ScVal {
  return xdr.ScVal.scvVec([nativeToScVal(status, { type: 'symbol' })]);
}

function riskTierToScVal(tier: RiskTier): xdr.ScVal {
  return xdr.ScVal.scvVec([nativeToScVal(tier, { type: 'symbol' })]);
}

export class ComplianceClient extends BaseClient {
  constructor(config: ClientConfig) {
    super(config);
  }

  async isCleared(address: string): Promise<boolean> {
    const result = await this.simulate('is_cleared', [
      new Address(address).toScVal(),
    ]);
    if (StellarRpc.Api.isSimulationError(result)) {
      throw new Error(`Simulation failed: ${result.error}`);
    }
    return Boolean(scValToNative(result.result!.retval));
  }

  async getRecord(address: string): Promise<ComplianceRecord | null> {
    const result = await this.simulate('get_compliance_record', [
      new Address(address).toScVal(),
    ]);
    if (StellarRpc.Api.isSimulationError(result)) {
      throw new Error(`Simulation failed: ${result.error}`);
    }
    const raw = scValToNative(result.result!.retval) as Record<string, unknown> | null;
    if (!raw) return null;
    return {
      address: raw.address as string,
      status: (raw.status as ComplianceStatus) ?? 'Unscreened',
      reasonCode: Number(raw.reason_code ?? 0),
      riskTier: (raw.risk_tier as RiskTier) ?? 'Low',
      screenedAt: Number(raw.screened_at ?? 0),
      screenedBy: raw.screened_by as string,
      expiresAt: Number(raw.expires_at ?? 0),
      notesHash: String(raw.notes_hash ?? ''),
    };
  }

  async getHistory(address: string): Promise<ScreeningHistoryEntry[]> {
    const result = await this.simulate('get_screening_history', [
      new Address(address).toScVal(),
    ]);
    if (StellarRpc.Api.isSimulationError(result)) {
      throw new Error(`Simulation failed: ${result.error}`);
    }
    const raw = (scValToNative(result.result!.retval) as Record<string, unknown>[]) ?? [];
    return raw.map((e) => ({
      status: (e.status as ComplianceStatus) ?? 'Unscreened',
      reasonCode: Number(e.reason_code ?? 0),
      riskTier: (e.risk_tier as RiskTier) ?? 'Low',
      screenedAt: Number(e.screened_at ?? 0),
      screenedBy: e.screened_by as string,
      expiresAt: Number(e.expires_at ?? 0),
      notesHash: String(e.notes_hash ?? ''),
    }));
  }

  async listFlagged(): Promise<string[]> {
    const result = await this.simulate('list_flagged', []);
    if (StellarRpc.Api.isSimulationError(result)) {
      throw new Error(`Simulation failed: ${result.error}`);
    }
    return (scValToNative(result.result!.retval) as string[]) ?? [];
  }

  async listPendingReview(): Promise<string[]> {
    const result = await this.simulate('list_pending_review', []);
    if (StellarRpc.Api.isSimulationError(result)) {
      throw new Error(`Simulation failed: ${result.error}`);
    }
    return (scValToNative(result.result!.retval) as string[]) ?? [];
  }

  async submitScreeningResult(params: {
    signer: Signer;
    screener: string;
    address: string;
    status: ComplianceStatus;
    reasonCode: number;
    riskTier: RiskTier;
    expiresAt: number;
    notesHash: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.screener,
      'submit_screening_result',
      [
        new Address(params.screener).toScVal(),
        new Address(params.address).toScVal(),
        statusToScVal(params.status),
        nativeToScVal(params.reasonCode, { type: 'u32' }),
        riskTierToScVal(params.riskTier),
        nativeToScVal(params.expiresAt, { type: 'u64' }),
        nativeToScVal(params.notesHash, { type: 'string' }),
      ],
      params.onProgress,
    );
  }

  async requestReview(params: {
    signer: Signer;
    caller: string;
    address: string;
    reason: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.caller,
      'request_review',
      [
        new Address(params.caller).toScVal(),
        new Address(params.address).toScVal(),
        nativeToScVal(params.reason, { type: 'string' }),
      ],
      params.onProgress,
    );
  }
}
