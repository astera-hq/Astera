import { rpc as StellarRpc } from '@stellar/stellar-sdk';
import { BaseClient, nativeToScVal, scValToNative, Address, xdr } from './base';
import type { ClientConfig, TransactionProgress } from '../types';
import type { Signer } from '../types';
import type {
  TranchePool,
  TrancheConfig,
  TrancheAccounting,
  InvoiceTrancheExposure,
  WaterfallSimulation,
} from '../generated/tranche';
import { TrancheClass } from '../generated/tranche';

export interface TrancheInvestorPosition {
  deposited: bigint;
  shares: bigint;
  earned: bigint;
  losses: bigint;
}

function trancheClassToScVal(tc: TrancheClass): xdr.ScVal {
  const name = tc === TrancheClass.Senior ? 'Senior' : 'Junior';
  return xdr.ScVal.scvVec([nativeToScVal(name, { type: 'symbol' })]);
}

function accountingFromRaw(r: Record<string, unknown>): TrancheAccounting {
  return {
    deposited: BigInt(String(r.deposited)),
    available: BigInt(String(r.available)),
    deployed: BigInt(String(r.deployed)),
    earned: BigInt(String(r.earned)),
    losses: BigInt(String(r.losses)),
  };
}

function poolFromRaw(raw: Record<string, unknown>): TranchePool {
  return {
    senior: accountingFromRaw(raw.senior as Record<string, unknown>),
    junior: accountingFromRaw(raw.junior as Record<string, unknown>),
    config: raw.config as TrancheConfig,
  };
}

export class TrancheClient extends BaseClient {
  constructor(config: ClientConfig) {
    super(config);
  }

  async getPool(token: string): Promise<TranchePool> {
    const sim = await this.simulate('get_pool', [new Address(token).toScVal()]);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    return poolFromRaw(scValToNative(sim.result!.retval) as Record<string, unknown>);
  }

  async getTrancheConfig(token: string): Promise<TrancheConfig> {
    const sim = await this.simulate('get_config', [new Address(token).toScVal()]);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    const raw = scValToNative(sim.result!.retval) as Record<string, unknown>;
    return {
      senior_target_yield_bps: Number(raw.senior_target_yield_bps),
      senior_advance_rate_bps: Number(raw.senior_advance_rate_bps),
      junior_first_loss_bps: Number(raw.junior_first_loss_bps),
    };
  }

  async getInvestorPosition(
    investor: string,
    token: string,
    trancheClass: TrancheClass,
  ): Promise<TrancheInvestorPosition> {
    const sim = await this.simulate('get_position', [
      new Address(investor).toScVal(),
      new Address(token).toScVal(),
      trancheClassToScVal(trancheClass),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    const raw = scValToNative(sim.result!.retval) as Record<string, unknown>;
    return {
      deposited: BigInt(String(raw.deposited ?? 0)),
      shares: BigInt(String(raw.shares ?? 0)),
      earned: BigInt(String(raw.earned ?? 0)),
      losses: BigInt(String(raw.losses ?? 0)),
    };
  }

  async getTotals(token: string, trancheClass: TrancheClass): Promise<TrancheAccounting> {
    const sim = await this.simulate('get_totals', [
      new Address(token).toScVal(),
      trancheClassToScVal(trancheClass),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    return accountingFromRaw(scValToNative(sim.result!.retval) as Record<string, unknown>);
  }

  /** Lifetime return in basis points — not annualized. See contract doc comment on get_lifetime_return_bps. */
  async getLifetimeReturnBps(token: string, trancheClass: TrancheClass): Promise<number> {
    const sim = await this.simulate('get_lifetime_return_bps', [
      new Address(token).toScVal(),
      trancheClassToScVal(trancheClass),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    return Number(scValToNative(sim.result!.retval));
  }

  async getInvoiceExposure(invoiceId: number): Promise<InvoiceTrancheExposure | null> {
    const sim = await this.simulate('get_invoice_exposure', [
      nativeToScVal(invoiceId, { type: 'u64' }),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    const raw = scValToNative(sim.result!.retval);
    if (!raw) return null;
    const r = raw as Record<string, unknown>;
    return {
      invoice_id: Number(r.invoice_id),
      senior_principal: BigInt(String(r.senior_deployed)),
      junior_principal: BigInt(String(r.junior_deployed)),
      senior_repaid: BigInt(String(r.senior_repaid ?? 0)),
      junior_repaid: BigInt(String(r.junior_repaid ?? 0)),
      senior_loss: BigInt(String(r.senior_loss ?? 0)),
      junior_loss: BigInt(String(r.junior_loss ?? 0)),
    };
  }

  async simulateWaterfall(
    token: string,
    invoiceId: number,
    hypotheticalRepayment: bigint,
    elapsedSecs: number,
  ): Promise<WaterfallSimulation> {
    const sim = await this.simulate('simulate_waterfall', [
      new Address(token).toScVal(),
      nativeToScVal(invoiceId, { type: 'u64' }),
      nativeToScVal(hypotheticalRepayment, { type: 'i128' }),
      nativeToScVal(elapsedSecs, { type: 'u64' }),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) throw new Error(`Simulation failed: ${sim.error}`);
    const [seniorAmount, juniorAmount] = scValToNative(sim.result!.retval) as [unknown, unknown];
    return {
      senior_amount: BigInt(String(seniorAmount)),
      junior_amount: BigInt(String(juniorAmount)),
      senior_cap: 0n,
      elapsed_yield: 0n,
    };
  }

  async deposit(params: {
    signer: Signer;
    investor: string;
    token: string;
    trancheClass: TrancheClass;
    amount: bigint;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.investor,
      'deposit_tranche',
      [
        new Address(params.investor).toScVal(),
        new Address(params.token).toScVal(),
        trancheClassToScVal(params.trancheClass),
        nativeToScVal(params.amount, { type: 'i128' }),
      ],
      params.onProgress,
    );
  }

  async withdraw(params: {
    signer: Signer;
    investor: string;
    token: string;
    trancheClass: TrancheClass;
    amount: bigint;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.investor,
      'withdraw_tranche',
      [
        new Address(params.investor).toScVal(),
        new Address(params.token).toScVal(),
        trancheClassToScVal(params.trancheClass),
        nativeToScVal(params.amount, { type: 'i128' }),
      ],
      params.onProgress,
    );
  }

  async setTrancheConfig(params: {
    signer: Signer;
    admin: string;
    token: string;
    seniorTargetYieldBps: number;
    seniorAdvanceRateBps: number;
    juniorFirstLossBps: number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'set_tranche_config',
      [
        new Address(params.admin).toScVal(),
        new Address(params.token).toScVal(),
        nativeToScVal(params.seniorTargetYieldBps, { type: 'u32' }),
        nativeToScVal(params.seniorAdvanceRateBps, { type: 'u32' }),
        nativeToScVal(params.juniorFirstLossBps, { type: 'u32' }),
      ],
      params.onProgress,
    );
  }
}
