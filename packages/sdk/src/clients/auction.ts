import { rpc as StellarRpc } from '@stellar/stellar-sdk';
import { BaseClient, nativeToScVal, scValToNative, Address, xdr } from './base';
import { Errors as AuctionErrors } from '../generated/auction';
import type { ClientConfig, CollateralRiskConfig, CollateralSale, TransactionProgress } from '../types';
import type { Signer } from '../types';

// #1036: collateral-liquidation Dutch auction + risk-response satellite —
// reads pool's public getters, tracks the at-risk flag in its own storage,
// and drives pool's trusted `risk_liquidate_collateral` entrypoint to seize
// cross-asset collateral positions based on live, oracle-priced ratios (see
// contracts/auction).

function collateralSaleParamsToScVal(params: {
  seller: string;
  token: string;
  amount: bigint;
  proceedsToken: string;
  proceedsRecipient: string;
  startPrice: bigint;
  floorPrice: bigint;
  durationSecs: number;
}): xdr.ScVal {
  const entry = (key: string, val: xdr.ScVal) =>
    new xdr.ScMapEntry({ key: nativeToScVal(key, { type: 'symbol' }), val });
  return xdr.ScVal.scvMap([
    entry('amount', nativeToScVal(params.amount, { type: 'i128' })),
    entry('duration_secs', nativeToScVal(params.durationSecs, { type: 'u64' })),
    entry('floor_price', nativeToScVal(params.floorPrice, { type: 'i128' })),
    entry('proceeds_recipient', new Address(params.proceedsRecipient).toScVal()),
    entry('proceeds_token', new Address(params.proceedsToken).toScVal()),
    entry('seller', new Address(params.seller).toScVal()),
    entry('start_price', nativeToScVal(params.startPrice, { type: 'i128' })),
    entry('token', new Address(params.token).toScVal()),
  ]);
}

function collateralSaleFromRaw(raw: Record<string, unknown>): CollateralSale {
  return {
    saleId: BigInt(String(raw.sale_id)),
    seller: raw.seller as string,
    token: raw.token as string,
    amount: BigInt(String(raw.amount)),
    proceedsToken: raw.proceeds_token as string,
    proceedsRecipient: raw.proceeds_recipient as string,
    startPrice: BigInt(String(raw.start_price)),
    floorPrice: BigInt(String(raw.floor_price)),
    openedAt: Number(raw.opened_at),
    durationSecs: Number(raw.duration_secs),
    status: (Array.isArray(raw.status) ? raw.status[0] : raw.status) as CollateralSale['status'],
    taker: (raw.taker as string | undefined) ?? undefined,
    settledPrice: raw.settled_price != null ? BigInt(String(raw.settled_price)) : undefined,
  };
}

export class AuctionClient extends BaseClient {
  protected override readonly errors = AuctionErrors;

  constructor(config: ClientConfig) {
    super(config);
  }

  /** One-time setup: registers the admin and the pool contract this satellite
   * reads collateral/invoice data and oracle prices from. */
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

  async getPoolContract(): Promise<string | null> {
    const sim = await this.simulate('get_pool_contract', []);
    if (StellarRpc.Api.isSimulationError(sim)) return null;
    const raw = scValToNative(sim.result!.retval);
    return raw ?? null;
  }

  /** Admin-gated: danger_bps must exceed 10_000 (100%) so a position is
   * flagged with an early-warning buffer above pool's funding-time floor. */
  async setCollateralRiskConfig(params: {
    signer: Signer;
    admin: string;
    dangerBps: number;
    gracePeriodSecs: number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'set_collateral_risk_config',
      [
        new Address(params.admin).toScVal(),
        nativeToScVal(params.dangerBps, { type: 'u32' }),
        nativeToScVal(params.gracePeriodSecs, { type: 'u64' }),
      ],
      params.onProgress,
    );
  }

  async getCollateralRiskConfig(): Promise<CollateralRiskConfig> {
    const sim = await this.simulate('get_collateral_risk_config', []);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval) as Record<string, unknown>;
    return {
      dangerBps: Number(raw.danger_bps),
      gracePeriodSecs: Number(raw.grace_period_secs),
    };
  }

  /** Read-only: the live, oracle-priced collateral ratio (bps) for a funded
   * invoice's posted collateral — 10_000 = exactly covers the requirement at
   * today's prices. Requires the invoice to already be funded. */
  async getLiveCollateralRatio(invoiceId: bigint | number): Promise<number> {
    const sim = await this.simulate('get_live_collateral_ratio', [
      nativeToScVal(invoiceId, { type: 'u64' }),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    return Number(scValToNative(sim.result!.retval));
  }

  /** Read-only: the ledger timestamp a position was first flagged at-risk,
   * or null if not currently flagged. Tracked entirely in this contract's
   * own storage — it's monitoring state, not fund-movement state, so pool's
   * CollateralDeposit doesn't carry it. */
  async getAtRiskSince(invoiceId: bigint | number): Promise<number | null> {
    const sim = await this.simulate('get_at_risk_since', [
      nativeToScVal(invoiceId, { type: 'u64' }),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval);
    return raw != null ? Number(raw) : null;
  }

  /** Permissionless keeper call: recomputes the live ratio and flips the
   * at-risk flag. Returns the tx hash — read the updated state back via
   * getLiveCollateralRatio / getAtRiskSince. */
  async checkCollateralRisk(params: {
    signer: Signer;
    caller: string;
    invoiceId: bigint | number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.caller,
      'check_collateral_risk',
      [new Address(params.caller).toScVal(), nativeToScVal(params.invoiceId, { type: 'u64' })],
      params.onProgress,
    );
  }

  /** Permissionless keeper call: seizes a deposit that's been at-risk for at
   * least the configured grace period and is still below the danger threshold
   * on a fresh price recheck. Reverts with GracePeriodNotElapsed or
   * OraclePriceUnavailable if not yet eligible; succeeds (without seizing) if
   * the position recovered since being flagged. */
  async liquidateCollateral(params: {
    signer: Signer;
    caller: string;
    invoiceId: bigint | number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.caller,
      'liquidate_collateral',
      [new Address(params.caller).toScVal(), nativeToScVal(params.invoiceId, { type: 'u64' })],
      params.onProgress,
    );
  }

  // ── Dutch/declining-price collateral sale ─────────────────────────────

  /** Consigns `amount` of `token` for sale, decaying in price from
   * `startPrice` to `floorPrice` (in `proceedsToken`) over `durationSecs`. */
  async openCollateralSale(params: {
    signer: Signer;
    seller: string;
    token: string;
    amount: bigint;
    proceedsToken: string;
    proceedsRecipient: string;
    startPrice: bigint;
    floorPrice: bigint;
    durationSecs: number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.seller,
      'open_collateral_sale',
      [collateralSaleParamsToScVal(params)],
      params.onProgress,
    );
  }

  async getSale(saleId: bigint | number): Promise<CollateralSale | null> {
    const sim = await this.simulate('get_sale', [nativeToScVal(saleId, { type: 'u64' })]);
    if (StellarRpc.Api.isSimulationError(sim)) return null;
    const raw = scValToNative(sim.result!.retval);
    return raw ? collateralSaleFromRaw(raw as Record<string, unknown>) : null;
  }

  /** Read-only: the current decayed price (in `proceedsToken`). */
  async currentSalePrice(saleId: bigint | number): Promise<bigint> {
    const sim = await this.simulate('current_sale_price', [nativeToScVal(saleId, { type: 'u64' })]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    return BigInt(String(scValToNative(sim.result!.retval)));
  }

  /** Permissionless: takes an open, unexpired sale at its current decayed price. */
  async takeCollateralSale(params: {
    signer: Signer;
    taker: string;
    saleId: bigint | number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.taker,
      'take_collateral_sale',
      [new Address(params.taker).toScVal(), nativeToScVal(params.saleId, { type: 'u64' })],
      params.onProgress,
    );
  }

  /** Permissionless: once a sale's window has passed with no taker, returns
   * the consigned asset to the original seller. */
  async reclaimExpiredSale(params: {
    signer: Signer;
    caller: string;
    saleId: bigint | number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.caller,
      'reclaim_expired_sale',
      [new Address(params.caller).toScVal(), nativeToScVal(params.saleId, { type: 'u64' })],
      params.onProgress,
    );
  }
}
