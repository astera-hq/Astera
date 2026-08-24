import { rpc as StellarRpc } from '@stellar/stellar-sdk';
import { BaseClient, nativeToScVal, scValToNative, Address, xdr } from './base';
import { Errors as InsuranceErrors } from '../generated/insurance';
import type { ClientConfig, TransactionProgress, Signer } from '../types';
import type {
  PremiumConfig,
  InsuranceRiskTier,
  ReserveFund,
  CoverageRecord,
  ClaimHistoryItem,
  ReserveHealth,
} from '../generated/insurance';

function mapEntry(key: string, val: xdr.ScVal): xdr.ScMapEntry {
  return new xdr.ScMapEntry({ key: nativeToScVal(key, { type: 'symbol' }), val });
}

function riskTierToScVal(tier: InsuranceRiskTier): xdr.ScVal {
  return xdr.ScVal.scvMap([
    mapEntry('max_score', nativeToScVal(tier.max_score, { type: 'u32' })),
    mapEntry('min_score', nativeToScVal(tier.min_score, { type: 'u32' })),
    mapEntry('risk_multiplier_bps', nativeToScVal(tier.risk_multiplier_bps, { type: 'u32' })),
  ]);
}

function riskTierFromRaw(r: Record<string, unknown>): InsuranceRiskTier {
  return {
    min_score: Number(r.min_score),
    max_score: Number(r.max_score),
    risk_multiplier_bps: Number(r.risk_multiplier_bps),
  };
}

function premiumConfigToScVal(config: PremiumConfig): xdr.ScVal {
  return xdr.ScVal.scvMap([
    mapEntry('base_rate_bps', nativeToScVal(config.base_rate_bps, { type: 'u32' })),
    mapEntry('default_coverage_bps', nativeToScVal(config.default_coverage_bps, { type: 'u32' })),
    mapEntry(
      'default_risk_multiplier_bps',
      nativeToScVal(config.default_risk_multiplier_bps, { type: 'u32' }),
    ),
    mapEntry('max_premium_bps', nativeToScVal(config.max_premium_bps, { type: 'u32' })),
    mapEntry('min_premium_bps', nativeToScVal(config.min_premium_bps, { type: 'u32' })),
    mapEntry(
      'risk_tiers',
      xdr.ScVal.scvVec(config.risk_tiers.map((tier) => riskTierToScVal(tier))),
    ),
    mapEntry('tenor_bps_per_day', nativeToScVal(config.tenor_bps_per_day, { type: 'u32' })),
  ]);
}

function premiumConfigFromRaw(raw: Record<string, unknown>): PremiumConfig {
  return {
    base_rate_bps: Number(raw.base_rate_bps),
    tenor_bps_per_day: Number(raw.tenor_bps_per_day),
    risk_tiers: ((raw.risk_tiers as Record<string, unknown>[]) ?? []).map(riskTierFromRaw),
    default_risk_multiplier_bps: Number(raw.default_risk_multiplier_bps),
    min_premium_bps: Number(raw.min_premium_bps),
    max_premium_bps: Number(raw.max_premium_bps),
    default_coverage_bps: Number(raw.default_coverage_bps),
  };
}

function reserveFundFromRaw(raw: Record<string, unknown>): ReserveFund {
  return {
    total_reserves: BigInt(String(raw.total_reserves ?? 0)),
    total_premiums_collected: BigInt(String(raw.total_premiums_collected ?? 0)),
    total_claims_paid: BigInt(String(raw.total_claims_paid ?? 0)),
    total_covered_exposure: BigInt(String(raw.total_covered_exposure ?? 0)),
    coverage_ratio_bps: Number(raw.coverage_ratio_bps),
    min_coverage_ratio_bps: Number(raw.min_coverage_ratio_bps),
  };
}

function coverageRecordFromRaw(raw: Record<string, unknown>): CoverageRecord {
  return {
    invoice_id: BigInt(String(raw.invoice_id)),
    token: raw.token as string,
    principal: BigInt(String(raw.principal)),
    premium_paid: BigInt(String(raw.premium_paid)),
    coverage_bps: Number(raw.coverage_bps),
    purchased_at: BigInt(String(raw.purchased_at)),
    claimed: Boolean(raw.claimed),
  };
}

function claimHistoryItemFromRaw(raw: Record<string, unknown>): ClaimHistoryItem {
  return {
    invoice_id: BigInt(String(raw.invoice_id)),
    token: raw.token as string,
    payout: BigInt(String(raw.payout)),
    shortfalls: BigInt(String(raw.shortfalls)),
    timestamp: BigInt(String(raw.timestamp)),
  };
}

function reserveHealthFromRaw(raw: Record<string, unknown>): ReserveHealth {
  return {
    token: raw.token as string,
    total_reserves: BigInt(String(raw.total_reserves ?? 0)),
    coverage_ratio_bps: Number(raw.coverage_ratio_bps),
    min_reserve_amount: BigInt(String(raw.min_reserve_amount ?? 0)),
    is_healthy: Boolean(raw.is_healthy),
    needs_top_up: Boolean(raw.needs_top_up),
  };
}

export class InsuranceClient extends BaseClient {
  protected override readonly errors = InsuranceErrors;

  constructor(config: ClientConfig) {
    super(config);
  }

  async initialize(params: {
    signer: Signer;
    admin: string;
    poolContract: string;
    invoiceContract: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'initialize',
      [
        new Address(params.admin).toScVal(),
        new Address(params.poolContract).toScVal(),
        new Address(params.invoiceContract).toScVal(),
      ],
      params.onProgress,
    );
  }

  async estimatePremium(
    principal: bigint,
    sme: string,
    tenorDays: number,
    token: string,
  ): Promise<bigint> {
    const sim = await this.simulate('estimate_premium', [
      nativeToScVal(principal, { type: 'i128' }),
      new Address(sme).toScVal(),
      nativeToScVal(tenorDays, { type: 'u32' }),
      new Address(token).toScVal(),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    return BigInt(String(scValToNative(sim.result!.retval)));
  }

  async purchaseCoverage(params: {
    signer: Signer;
    payer: string;
    invoiceId: bigint | number;
    principal: bigint;
    sme: string;
    dueDate: number;
    token: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.payer,
      'purchase_coverage',
      [
        new Address(params.payer).toScVal(),
        nativeToScVal(params.invoiceId, { type: 'u64' }),
        nativeToScVal(params.principal, { type: 'i128' }),
        new Address(params.sme).toScVal(),
        nativeToScVal(params.dueDate, { type: 'u64' }),
        new Address(params.token).toScVal(),
      ],
      params.onProgress,
    );
  }

  async fileClaim(params: {
    signer: Signer;
    caller: string;
    invoiceId: bigint | number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.caller,
      'file_claim',
      [new Address(params.caller).toScVal(), nativeToScVal(params.invoiceId, { type: 'u64' })],
      params.onProgress,
    );
  }

  async getReserveStatus(token: string): Promise<ReserveFund> {
    const sim = await this.simulate('get_reserve_status', [new Address(token).toScVal()]);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    return reserveFundFromRaw(scValToNative(sim.result!.retval) as Record<string, unknown>);
  }

  async getCoverageRecord(invoiceId: bigint | number): Promise<CoverageRecord | null> {
    const sim = await this.simulate('get_coverage_record', [
      nativeToScVal(invoiceId, { type: 'u64' }),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    const raw = scValToNative(sim.result!.retval);
    return raw ? coverageRecordFromRaw(raw as Record<string, unknown>) : null;
  }

  async getClaimHistory(invoiceId: bigint | number): Promise<ClaimHistoryItem[]> {
    const sim = await this.simulate('get_claim_history', [
      nativeToScVal(invoiceId, { type: 'u64' }),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    const raw = scValToNative(sim.result!.retval) as Record<string, unknown>[];
    return (raw ?? []).map(claimHistoryItemFromRaw);
  }

  async checkReserveHealth(token: string): Promise<ReserveHealth> {
    const sim = await this.simulate('check_reserve_health', [new Address(token).toScVal()]);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    return reserveHealthFromRaw(scValToNative(sim.result!.retval) as Record<string, unknown>);
  }

  async getPremiumConfig(): Promise<PremiumConfig | null> {
    const sim = await this.simulate('get_premium_config', []);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    const raw = scValToNative(sim.result!.retval);
    return raw ? premiumConfigFromRaw(raw as Record<string, unknown>) : null;
  }

  async getCreditScoreContract(): Promise<string | null> {
    const sim = await this.simulate('get_credit_score_contract', []);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    const raw = scValToNative(sim.result!.retval);
    return raw ? String(raw) : null;
  }

  async getMinReserveAmount(token: string): Promise<bigint> {
    const sim = await this.simulate('get_min_reserve_amount', [new Address(token).toScVal()]);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    return BigInt(String(scValToNative(sim.result!.retval)));
  }

  async setPremiumConfig(params: {
    signer: Signer;
    admin: string;
    config: PremiumConfig;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'set_premium_config',
      [new Address(params.admin).toScVal(), premiumConfigToScVal(params.config)],
      params.onProgress,
    );
  }

  async setMinCoverageRatio(params: {
    signer: Signer;
    admin: string;
    token: string;
    minRatioBps: number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'set_min_coverage_ratio',
      [
        new Address(params.admin).toScVal(),
        new Address(params.token).toScVal(),
        nativeToScVal(params.minRatioBps, { type: 'u32' }),
      ],
      params.onProgress,
    );
  }

  async setMinReserveAmount(params: {
    signer: Signer;
    admin: string;
    token: string;
    minAmount: bigint;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'set_min_reserve_amount',
      [
        new Address(params.admin).toScVal(),
        new Address(params.token).toScVal(),
        nativeToScVal(params.minAmount, { type: 'i128' }),
      ],
      params.onProgress,
    );
  }

  async fundReserveFromTreasury(params: {
    signer: Signer;
    admin: string;
    token: string;
    amount: bigint;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'fund_reserve_from_treasury',
      [
        new Address(params.admin).toScVal(),
        new Address(params.token).toScVal(),
        nativeToScVal(params.amount, { type: 'i128' }),
      ],
      params.onProgress,
    );
  }

  async setCreditScoreContract(params: {
    signer: Signer;
    admin: string;
    creditScoreContract: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'set_credit_score_contract',
      [new Address(params.admin).toScVal(), new Address(params.creditScoreContract).toScVal()],
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
}
