import { rpc as StellarRpc } from '@stellar/stellar-sdk';
import { BaseClient, nativeToScVal, scValToNative, Address, xdr, parseContractError } from './base';
import { Errors as PoolErrors } from '../generated/pool';
import type {
  ClientConfig,
  PoolConfig,
  InvestorPosition,
  WithdrawalRequest,
  CoFundingRound,
  RateModelConfig,
  RateSnapshot,
  FundedInvoice,
  PoolTokenTotals,
  TransactionProgress,
  Listing,
  ListingKind,
  CollateralConfig,
  CollateralDeposit,
} from '../types';
import type { Signer } from '../types';

function openCoFundingRequestToScVal(params: {
  invoiceId: bigint | number;
  token: string;
  targetPrincipal: bigint;
  sme: string;
  dueDate: number;
  fundingDeadline: number;
  minCommitment: bigint;
  maxInvestorBps: number;
}): xdr.ScVal {
  const entry = (key: string, val: xdr.ScVal) =>
    new xdr.ScMapEntry({ key: nativeToScVal(key, { type: 'symbol' }), val });
  return xdr.ScVal.scvMap([
    entry('due_date', nativeToScVal(params.dueDate, { type: 'u64' })),
    entry('funding_deadline', nativeToScVal(params.fundingDeadline, { type: 'u64' })),
    entry('invoice_id', nativeToScVal(params.invoiceId, { type: 'u64' })),
    entry('max_investor_bps', nativeToScVal(params.maxInvestorBps, { type: 'u32' })),
    entry('min_commitment', nativeToScVal(params.minCommitment, { type: 'i128' })),
    entry('sme', new Address(params.sme).toScVal()),
    entry('target_principal', nativeToScVal(params.targetPrincipal, { type: 'i128' })),
    entry('token', new Address(params.token).toScVal()),
  ]);
}

function coFundingRoundFromScVal(raw: Record<string, unknown>): CoFundingRound {
  return {
    invoiceId: BigInt(String(raw.invoice_id)),
    token: raw.token as string,
    sme: raw.sme as string,
    dueDate: Number(raw.due_date),
    targetPrincipal: BigInt(String(raw.target_principal)),
    committedPrincipal: BigInt(String(raw.committed_principal)),
    fundingDeadline: Number(raw.funding_deadline),
    status: raw.status as CoFundingRound['status'],
    minCommitment: BigInt(String(raw.min_commitment)),
    maxInvestorBps: Number(raw.max_investor_bps),
    participants: (raw.participants as string[]) ?? [],
  };
}

function rateModelConfigToScVal(config: RateModelConfig): xdr.ScVal {
  const entry = (key: string, val: xdr.ScVal) =>
    new xdr.ScMapEntry({ key: nativeToScVal(key, { type: 'symbol' }), val });
  return xdr.ScVal.scvMap([
    entry('base_rate_bps', nativeToScVal(config.baseRateBps, { type: 'u32' })),
    entry('max_rate_bps', nativeToScVal(config.maxRateBps, { type: 'u32' })),
    entry('optimal_utilization_bps', nativeToScVal(config.optimalUtilizationBps, { type: 'u32' })),
    entry('slope1_bps', nativeToScVal(config.slope1Bps, { type: 'u32' })),
    entry('slope2_bps', nativeToScVal(config.slope2Bps, { type: 'u32' })),
  ]);
}

function rateModelConfigFromScVal(raw: Record<string, unknown>): RateModelConfig {
  return {
    baseRateBps: Number(raw.base_rate_bps),
    optimalUtilizationBps: Number(raw.optimal_utilization_bps),
    slope1Bps: Number(raw.slope1_bps),
    slope2Bps: Number(raw.slope2_bps),
    maxRateBps: Number(raw.max_rate_bps),
  };
}

function mapFundedInvoice(raw: unknown): FundedInvoice | null {
  if (!raw) return null;
  const r = raw as Record<string, unknown>;
  return {
    invoiceId: BigInt(String(r.invoice_id)),
    sme: r.sme as string,
    token: r.token as string,
    principal: BigInt(String(r.principal)),
    committed: BigInt(String(r.committed ?? 0)),
    fundedAt: Number(r.funded_at),
    factoringFee: BigInt(String(r.factoring_fee ?? 0)),
    dueDate: Number(r.due_date),
    repaidAmount: BigInt(String(r.repaid_amount ?? 0)),
    coFundingRoundId: r.co_funding_round_id != null ? BigInt(String(r.co_funding_round_id)) : undefined,
  };
}

export class PoolClient extends BaseClient {
  protected override readonly errors = PoolErrors;

  constructor(config: ClientConfig) {
    super(config);
  }

  async getConfig(): Promise<PoolConfig> {
    const sim = await this.simulate('get_config', []);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval) as Record<string, unknown>;
    return {
      invoiceContract: raw.invoice_contract as string,
      admin: raw.admin as string,
      yieldBps: Number(raw.yield_bps),
      factoringFeeBps: Number(raw.factoring_fee_bps ?? 0),
      compoundInterest: Boolean(raw.compound_interest),
    };
  }

  async getPosition(investor: string, token: string): Promise<InvestorPosition | null> {
    const sim = await this.simulate('get_position', [
      new Address(investor).toScVal(),
      new Address(token).toScVal(),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval);
    if (!raw) return null;
    const pos = raw as Record<string, unknown>;
    return {
      deposited: BigInt(pos.deposited as string),
      available: BigInt(pos.available as string),
      deployed: BigInt(pos.deployed as string),
      earned: BigInt(pos.earned as string),
      depositCount: Number(pos.deposit_count),
    };
  }

  async deposit(params: {
    signer: Signer;
    investor: string;
    token: string;
    amount: bigint;
    /** #992: reverts with RateBelowMinimum if the current rate has dropped
     * below this since the caller last simulated the deposit. */
    minRate?: number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.investor,
      'deposit',
      [
        new Address(params.investor).toScVal(),
        new Address(params.token).toScVal(),
        nativeToScVal(params.amount, { type: 'i128' }),
        params.minRate === undefined
          ? xdr.ScVal.scvVoid()
          : nativeToScVal(params.minRate, { type: 'u32' }),
      ],
      params.onProgress,
    );
  }

  /** #1037: permissionless liquidation trigger — any caller may invoke this once
   * the invoice is Defaulted on-chain. Not gated to admin/keeper roles. */
  async liquidateInvoice(params: {
    signer: Signer;
    caller: string;
    invoiceId: bigint | number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.caller,
      'liquidate_invoice',
      [new Address(params.caller).toScVal(), nativeToScVal(params.invoiceId, { type: 'u64' })],
      params.onProgress,
    );
  }

  /** #1037: liquidation-status query — settled=true means collateral has been seized.
   * #1036: also carries the at-risk/auction-sale fields the collateral risk
   * response needs. */
  async getCollateralDeposit(invoiceId: bigint | number): Promise<CollateralDeposit | null> {
    const sim = await this.simulate('get_collateral_deposit', [
      nativeToScVal(invoiceId, { type: 'u64' }),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw parseContractError(sim.error, this.errors);
    }
    const raw = scValToNative(sim.result!.retval) as Record<string, unknown> | null;
    if (!raw) return null;
    return {
      invoiceId: BigInt(String(raw.invoice_id)),
      depositor: raw.depositor as string,
      token: raw.token as string,
      amount: BigInt(String(raw.amount)),
      settled: Boolean(raw.settled),
      postedAt: Number(raw.posted_at),
      releasedAt: Number(raw.released_at),
      seizedAt: Number(raw.seized_at),
      collateralBpsAtDeposit: Number(raw.collateral_bps_at_deposit),
      thresholdAtDeposit: BigInt(String(raw.threshold_at_deposit)),
    };
  }

  async requestWithdrawal(params: {
    signer: Signer;
    investor: string;
    token: string;
    shares: bigint;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.investor,
      'request_withdrawal',
      [
        new Address(params.investor).toScVal(),
        new Address(params.token).toScVal(),
        nativeToScVal(params.shares, { type: 'i128' }),
      ],
      params.onProgress,
    );
  }

  async cancelWithdrawalRequest(params: {
    signer: Signer;
    investor: string;
    token: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.investor,
      'cancel_withdrawal_request',
      [
        new Address(params.investor).toScVal(),
        new Address(params.token).toScVal(),
      ],
      params.onProgress,
    );
  }

  async drainWithdrawalQueue(params: {
    signer: Signer;
    caller: string;
    token: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.caller,
      'drain_withdrawal_queue',
      [
        new Address(params.caller).toScVal(),
        new Address(params.token).toScVal(),
      ],
      params.onProgress,
    );
  }

  async getWithdrawalQueue(token: string): Promise<WithdrawalRequest[]> {
    const sim = await this.simulate('get_withdrawal_queue', [
      new Address(token).toScVal(),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval) as Record<string, unknown>[];
    return raw.map((r) => ({
      investor: r.investor as string,
      token: r.token as string,
      shares: BigInt(String(r.shares)),
      requestedAt: Number(r.requested_at),
      requestId: BigInt(String(r.request_id)),
    }));
  }

  async repay(params: {
    signer: Signer;
    payer: string;
    invoiceId: bigint | number;
    amount: bigint;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.payer,
      'repay_invoice',
      [
        nativeToScVal(params.invoiceId, { type: 'u64' }),
        new Address(params.payer).toScVal(),
        nativeToScVal(params.amount, { type: 'i128' }),
      ],
      params.onProgress,
    );
  }

  async fundInvoice(params: {
    signer: Signer;
    admin: string;
    invoiceId: bigint | number;
    principal: bigint;
    sme: string;
    dueDate: number;
    token: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'fund_invoice',
      [
        new Address(params.admin).toScVal(),
        nativeToScVal(params.invoiceId, { type: 'u64' }),
        nativeToScVal(params.principal, { type: 'i128' }),
        new Address(params.sme).toScVal(),
        nativeToScVal(params.dueDate, { type: 'u64' }),
        new Address(params.token).toScVal(),
      ],
      params.onProgress,
    );
  }

  async fundInvoicesBatch(params: {
    signer: Signer;
    admin: string;
    requests: Array<{
      invoice_id: bigint | number;
      principal: bigint;
      sme: string;
      due_date: number;
      token: string;
    }>;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    const requestsScVal = xdr.ScVal.scvVec(
      params.requests.map((r) => {
        const entry = (key: string, val: xdr.ScVal) =>
          new xdr.ScMapEntry({ key: nativeToScVal(key, { type: 'symbol' }), val });
        return xdr.ScVal.scvMap([
          entry('invoice_id', nativeToScVal(r.invoice_id, { type: 'u64' })),
          entry('principal', nativeToScVal(r.principal, { type: 'i128' })),
          entry('sme', new Address(r.sme).toScVal()),
          entry('due_date', nativeToScVal(r.due_date, { type: 'u64' })),
          entry('token', new Address(r.token).toScVal()),
        ]);
      }),
    );
    return this.buildAndSendTx(
      params.admin,
      'fund_invoices_batch',
      [new Address(params.admin).toScVal(), requestsScVal],
      params.onProgress,
    );
  }

  async openCoFunding(params: {
    signer: Signer;
    admin: string;
    invoiceId: bigint | number;
    token: string;
    targetPrincipal: bigint;
    sme: string;
    dueDate: number;
    fundingDeadline: number;
    minCommitment: bigint;
    maxInvestorBps: number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'open_co_funding',
      [new Address(params.admin).toScVal(), openCoFundingRequestToScVal(params)],
      params.onProgress,
    );
  }

  async commitToInvoice(params: {
    signer: Signer;
    investor: string;
    invoiceId: bigint | number;
    amount: bigint;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.investor,
      'commit_to_invoice',
      [
        new Address(params.investor).toScVal(),
        nativeToScVal(params.invoiceId, { type: 'u64' }),
        nativeToScVal(params.amount, { type: 'i128' }),
      ],
      params.onProgress,
    );
  }

  async finalizeCoFunding(params: {
    signer: Signer;
    caller: string;
    invoiceId: bigint | number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.caller,
      'finalize_co_funding',
      [
        new Address(params.caller).toScVal(),
        nativeToScVal(params.invoiceId, { type: 'u64' }),
      ],
      params.onProgress,
    );
  }

  async withdrawCommitment(params: {
    signer: Signer;
    investor: string;
    invoiceId: bigint | number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.investor,
      'withdraw_co_funding_commitment',
      [
        new Address(params.investor).toScVal(),
        nativeToScVal(params.invoiceId, { type: 'u64' }),
      ],
      params.onProgress,
    );
  }

  async transferCoFundShare(params: {
    signer: Signer;
    from: string;
    invoiceId: bigint | number;
    token: string;
    to: string;
    bps: number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.from,
      'transfer_co_fund_share',
      [
        new Address(params.from).toScVal(),
        nativeToScVal(params.invoiceId, { type: 'u64' }),
        new Address(params.token).toScVal(),
        new Address(params.to).toScVal(),
        nativeToScVal(params.bps, { type: 'u32' }),
      ],
      params.onProgress,
    );
  }

  async getCoFundingRound(invoiceId: bigint | number): Promise<CoFundingRound | null> {
    const sim = await this.simulate('get_co_funding_round', [
      nativeToScVal(invoiceId, { type: 'u64' }),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval);
    if (!raw) return null;
    return coFundingRoundFromScVal(raw as Record<string, unknown>);
  }

  async listCoFundingRounds(): Promise<bigint[]> {
    const sim = await this.simulate('list_co_funding_rounds', []);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval) as unknown[];
    return (raw ?? []).map((id) => BigInt(String(id)));
  }

  async getInvestorCoFundPositions(
    investor: string,
  ): Promise<Array<{ invoiceId: bigint; bps: number }>> {
    const sim = await this.simulate('get_investor_co_fund_positions', [
      new Address(investor).toScVal(),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval) as [bigint | string, number][];
    return (raw ?? []).map(([invoiceId, bps]) => ({
      invoiceId: BigInt(String(invoiceId)),
      bps,
    }));
  }

  async getCoFundShare(invoiceId: bigint | number, investor: string): Promise<number> {
    const sim = await this.simulate('get_co_fund_share', [
      nativeToScVal(invoiceId, { type: 'u64' }),
      new Address(investor).toScVal(),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    return Number(scValToNative(sim.result!.retval));
  }

  // #863: rate model methods

  async getCurrentRate(token: string): Promise<number> {
    const sim = await this.simulate('get_current_rate', [
      new Address(token).toScVal(),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    return Number(scValToNative(sim.result!.retval));
  }

  async getRateModelConfig(token: string): Promise<RateModelConfig | null> {
    const sim = await this.simulate('get_rate_model_config', [
      new Address(token).toScVal(),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval);
    if (!raw) return null;
    return rateModelConfigFromScVal(raw as Record<string, unknown>);
  }

  async getRateHistory(token: string, limit: number): Promise<RateSnapshot[]> {
    const sim = await this.simulate('get_rate_history', [
      new Address(token).toScVal(),
      nativeToScVal(limit, { type: 'u32' }),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval) as Record<string, unknown>[];
    return (raw ?? []).map((r) => ({
      timestamp: Number(r.timestamp),
      utilizationBps: Number(r.utilization_bps),
      rateBps: Number(r.rate_bps),
    }));
  }

  async previewRateAtUtilization(token: string, utilizationBps: number): Promise<number> {
    const sim = await this.simulate('preview_rate_at_utilization', [
      new Address(token).toScVal(),
      nativeToScVal(utilizationBps, { type: 'u32' }),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    return Number(scValToNative(sim.result!.retval));
  }

  async proposeRateModelChange(params: {
    signer: Signer;
    admin: string;
    token: string;
    config: RateModelConfig;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'propose_rate_model_change',
      [
        new Address(params.admin).toScVal(),
        new Address(params.token).toScVal(),
        rateModelConfigToScVal(params.config),
      ],
      params.onProgress,
    );
  }

  async executeRateModelChange(params: {
    signer: Signer;
    caller: string;
    token: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.caller,
      'execute_rate_model_change',
      [new Address(params.token).toScVal()],
      params.onProgress,
    );
  }

  async cancelRateModelChange(params: {
    signer: Signer;
    admin: string;
    token: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'cancel_rate_model_change',
      [
        new Address(params.admin).toScVal(),
        new Address(params.token).toScVal(),
      ],
      params.onProgress,
    );
  }

  // Direct data queries

  async getFundedInvoice(invoiceId: bigint | number): Promise<FundedInvoice | null> {
    const sim = await this.simulate('get_funded_invoice', [
      nativeToScVal(invoiceId, { type: 'u64' }),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    return mapFundedInvoice(scValToNative(sim.result!.retval));
  }

  /**
   * #987 — batch-read helper matching the on-chain get_funded_invoices_batch,
   * so consumers don't have to assemble the Vec<u64> call manually or fall
   * back to one RPC per id. Order of the returned array matches `invoiceIds`
   * (null entries mean that id has no funded-invoice record).
   */
  async getFundedInvoicesBatch(invoiceIds: Array<bigint | number>): Promise<Array<FundedInvoice | null>> {
    const sim = await this.simulate('get_funded_invoices_batch', [
      xdr.ScVal.scvVec(invoiceIds.map((id) => nativeToScVal(id, { type: 'u64' }))),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval) as unknown[];
    return raw.map((entry) => mapFundedInvoice(entry));
  }

  async getPoolTokenTotals(token: string): Promise<PoolTokenTotals> {
    const sim = await this.simulate('get_pool_token_totals', [
      new Address(token).toScVal(),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval) as Record<string, unknown>;
    return {
      totalDeposited: BigInt(String(raw.total_deposited)),
      totalDeployed: BigInt(String(raw.total_deployed)),
      totalPaidOut: BigInt(String(raw.total_paid_out)),
      totalFeeRevenue: BigInt(String(raw.total_fee_revenue)),
    };
  }

  // #1036: multi-asset, oracle-priced collateral risk response

  async depositCollateral(params: {
    signer: Signer;
    invoiceId: bigint | number;
    depositor: string;
    token: string;
    amount: bigint;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.depositor,
      'deposit_collateral',
      [
        nativeToScVal(params.invoiceId, { type: 'u64' }),
        new Address(params.depositor).toScVal(),
        new Address(params.token).toScVal(),
        nativeToScVal(params.amount, { type: 'i128' }),
      ],
      params.onProgress,
    );
  }

  async topUpCollateral(params: {
    signer: Signer;
    invoiceId: bigint | number;
    depositor: string;
    token: string;
    amount: bigint;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.depositor,
      'top_up_collateral',
      [
        nativeToScVal(params.invoiceId, { type: 'u64' }),
        new Address(params.depositor).toScVal(),
        new Address(params.token).toScVal(),
        nativeToScVal(params.amount, { type: 'i128' }),
      ],
      params.onProgress,
    );
  }

  async getCollateralConfig(): Promise<CollateralConfig> {
    const sim = await this.simulate('get_collateral_config', []);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval) as Record<string, unknown>;
    return {
      threshold: BigInt(String(raw.threshold)),
      collateralBps: Number(raw.collateral_bps),
    };
  }

  // #1036: get_live_collateral_ratio/check_collateral_risk/liquidateCollateral,
  // the risk-config get/set, and the at-risk flag itself all moved to the
  // collateral-risk-response satellite (contracts/auction, see AuctionClient)
  // — pool's own wasm was ~68KB over Soroban's 200KB deploy limit with that
  // logic inline. pool keeps only the trusted risk_liquidate_collateral
  // mutation entrypoint the satellite calls, gated to whichever contract is
  // registered below.

  /** Registers the trusted collateral-risk-response satellite (AuctionClient's
   * contract) — the only caller pool's risk_liquidate_collateral entrypoint
   * accepts. */
  async setRiskContract(params: {
    signer: Signer;
    admin: string;
    riskContract: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'set_risk_contract',
      [new Address(params.admin).toScVal(), new Address(params.riskContract).toScVal()],
      params.onProgress,
    );
  }

  // #1025: secondary market

  private listingFromRaw(raw: Record<string, unknown>): Listing {
    return {
      listingId: BigInt(String(raw.listing_id)),
      invoiceId: BigInt(String(raw.invoice_id)),
      seller: raw.seller as string,
      token: raw.token as string,
      kind: (Array.isArray(raw.kind) ? raw.kind[0] : raw.kind) as ListingKind,
      amountOrBps: BigInt(String(raw.amount_or_bps)),
      price: BigInt(String(raw.price)),
      createdAt: Number(raw.created_at),
      status: (Array.isArray(raw.status) ? raw.status[0] : raw.status) as Listing['status'],
    };
  }

  private listingKindToScVal(kind: ListingKind): xdr.ScVal {
    return xdr.ScVal.scvVec([nativeToScVal(kind, { type: 'symbol' })]);
  }

  /** List part or all of a position for sale on the secondary market. */
  async listPosition(params: {
    signer: Signer;
    seller: string;
    invoiceId: bigint | number;
    kind: ListingKind;
    amountOrBps: bigint;
    price: bigint;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.seller,
      'list_position',
      [
        new Address(params.seller).toScVal(),
        nativeToScVal(params.invoiceId, { type: 'u64' }),
        this.listingKindToScVal(params.kind),
        nativeToScVal(params.amountOrBps, { type: 'u64' }),
        nativeToScVal(params.price, { type: 'i128' }),
      ],
      params.onProgress,
    );
  }

  /** Cancel an open listing. Only the original seller may cancel. */
  async cancelListing(params: {
    signer: Signer;
    seller: string;
    listingId: bigint | number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.seller,
      'cancel_listing',
      [
        new Address(params.seller).toScVal(),
        nativeToScVal(params.listingId, { type: 'u64' }),
      ],
      params.onProgress,
    );
  }

  /** Buy an open listing. Buyer's available balance is debited by the price. */
  async buyListing(params: {
    signer: Signer;
    buyer: string;
    listingId: bigint | number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.buyer,
      'buy_listing',
      [
        new Address(params.buyer).toScVal(),
        nativeToScVal(params.listingId, { type: 'u64' }),
      ],
      params.onProgress,
    );
  }

  /** Fetch a single listing by ID. Returns null if not found. */
  async getListing(listingId: bigint | number): Promise<Listing | null> {
    const sim = await this.simulate('get_listing', [
      nativeToScVal(listingId, { type: 'u64' }),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) return null;
    const raw = scValToNative(sim.result!.retval);
    if (!raw) return null;
    return this.listingFromRaw(raw as Record<string, unknown>);
  }

  /** All listing IDs for a given invoice (open and closed). */
  async listListingsForInvoice(invoiceId: bigint | number): Promise<bigint[]> {
    const sim = await this.simulate('list_listings_for_invoice', [
      nativeToScVal(invoiceId, { type: 'u64' }),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) return [];
    const raw = scValToNative(sim.result!.retval) as unknown[];
    return (raw ?? []).map((id) => BigInt(String(id)));
  }

  /** All listing IDs created by a given seller (open and closed). */
  async listListingsForInvestor(seller: string): Promise<bigint[]> {
    const sim = await this.simulate('list_listings_for_investor', [
      new Address(seller).toScVal(),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) return [];
    const raw = scValToNative(sim.result!.retval) as unknown[];
    return (raw ?? []).map((id) => BigInt(String(id)));
  }
}
