import { rpc as StellarRpc } from '@stellar/stellar-sdk';
import { BaseClient, nativeToScVal, scValToNative, Address, xdr } from './base';
import { Errors as SecondaryMarketErrors } from '../generated/secondary_market';
import type {
  ClientConfig,
  WaitEstimate,
  LiquidityForecastPoint,
  Listing,
  ListingKind,
  Order,
  OrderBookLevel,
  OrderSide,
  TransactionProgress,
} from '../types';
import type { Signer } from '../types';

// #1044: split out of PoolClient when the secondary-market listing lifecycle
// and withdrawal-wait/liquidity-forecast analytics moved off `pool` onto the
// `secondary_market` satellite contract (see contracts/secondary_market).

export class SecondaryMarketClient extends BaseClient {
  protected override readonly errors = SecondaryMarketErrors;

  constructor(config: ClientConfig) {
    super(config);
  }

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

  private orderSideToScVal(side: OrderSide): xdr.ScVal {
    return xdr.ScVal.scvVec([nativeToScVal(side, { type: 'symbol' })]);
  }

  // `place_order` takes a single PlaceOrderRequest struct rather than
  // individual scalar params (clippy's too-many-arguments threshold —
  // see the same pattern for pool's OpenCoFundingRequest). Soroban encodes
  // named-field #[contracttype] structs as an ScMap keyed by field-name
  // Symbols in alphabetical order — NOT declaration order — so the entries
  // below are deliberately sorted (amount_or_bps, expires_at, invoice_id,
  // kind, price, side).
  private placeOrderRequestToScVal(params: {
    invoiceId: bigint | number;
    kind: ListingKind;
    side: OrderSide;
    amountOrBps: bigint;
    price: bigint;
    expiresAt?: bigint | number;
  }): xdr.ScVal {
    const entry = (key: string, val: xdr.ScVal) =>
      new xdr.ScMapEntry({ key: nativeToScVal(key, { type: 'symbol' }), val });
    return xdr.ScVal.scvMap([
      entry('amount_or_bps', nativeToScVal(params.amountOrBps, { type: 'u64' })),
      entry('expires_at', nativeToScVal(params.expiresAt ?? 0, { type: 'u64' })),
      entry('invoice_id', nativeToScVal(params.invoiceId, { type: 'u64' })),
      entry('kind', this.listingKindToScVal(params.kind)),
      entry('price', nativeToScVal(params.price, { type: 'i128' })),
      entry('side', this.orderSideToScVal(params.side)),
    ]);
  }

  private orderFromRaw(raw: Record<string, unknown>): Order {
    return {
      orderId: BigInt(String(raw.order_id)),
      invoiceId: BigInt(String(raw.invoice_id)),
      owner: raw.owner as string,
      token: raw.token as string,
      kind: (Array.isArray(raw.kind) ? raw.kind[0] : raw.kind) as ListingKind,
      side: (Array.isArray(raw.side) ? raw.side[0] : raw.side) as OrderSide,
      price: BigInt(String(raw.price)),
      amountOrBps: BigInt(String(raw.amount_or_bps)),
      remaining: BigInt(String(raw.remaining)),
      createdAt: Number(raw.created_at),
      expiresAt: Number(raw.expires_at),
      status: (Array.isArray(raw.status) ? raw.status[0] : raw.status) as Order['status'],
    };
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

  /**
   * Place a resting limit order (bid or ask). Matches immediately against
   * crossing resting orders on the opposite side; any unfilled remainder
   * rests on the book. `price` is per-unit, scaled by 1e7 — not a flat
   * total like `listPosition`'s `price`. `expiresAt` of `0n`/`0` means no
   * expiry.
   */
  async placeOrder(params: {
    signer: Signer;
    owner: string;
    invoiceId: bigint | number;
    kind: ListingKind;
    side: OrderSide;
    amountOrBps: bigint;
    price: bigint;
    expiresAt?: bigint | number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.owner,
      'place_order',
      [new Address(params.owner).toScVal(), this.placeOrderRequestToScVal(params)],
      params.onProgress,
    );
  }

  /** Cancel a resting or partially-filled order. Only the original owner may cancel. */
  async cancelOrder(params: {
    signer: Signer;
    owner: string;
    orderId: bigint | number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.owner,
      'cancel_order',
      [new Address(params.owner).toScVal(), nativeToScVal(params.orderId, { type: 'u64' })],
      params.onProgress,
    );
  }

  /**
   * Permissionlessly expire a resting order past its `expiresAt`. Anyone
   * may submit this — it only prunes stale book state, no funds move.
   */
  async expireOrder(params: {
    signer: Signer;
    caller: string;
    orderId: bigint | number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.caller,
      'expire_order',
      [nativeToScVal(params.orderId, { type: 'u64' })],
      params.onProgress,
    );
  }

  /** Fetch a single order by ID. Returns null if not found. */
  async getOrder(orderId: bigint | number): Promise<Order | null> {
    const sim = await this.simulate('get_order', [nativeToScVal(orderId, { type: 'u64' })]);
    if (StellarRpc.Api.isSimulationError(sim)) return null;
    const raw = scValToNative(sim.result!.retval);
    if (!raw) return null;
    return this.orderFromRaw(raw as Record<string, unknown>);
  }

  /** Resting bid/ask depth levels for an invoice's order book, as `{ bids, asks }` (#1133). */
  async getOrderBook(
    invoiceId: bigint | number,
    kind: ListingKind,
  ): Promise<{ bids: OrderBookLevel[]; asks: OrderBookLevel[] }> {
    const levelFromRaw = (raw: Record<string, unknown>): OrderBookLevel => ({
      orderId: BigInt(String(raw.order_id)),
      price: BigInt(String(raw.price)),
      quantity: BigInt(String(raw.quantity)),
    });
    const sim = await this.simulate('get_order_book', [
      nativeToScVal(invoiceId, { type: 'u64' }),
      this.listingKindToScVal(kind),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) return { bids: [], asks: [] };
    const [bids, asks] = scValToNative(sim.result!.retval) as [
      Record<string, unknown>[],
      Record<string, unknown>[],
    ];
    return {
      bids: (bids ?? []).map(levelFromRaw),
      asks: (asks ?? []).map(levelFromRaw),
    };
  }

  /** All order IDs ever placed by `owner` (any status). */
  async listOrdersForOwner(owner: string): Promise<bigint[]> {
    const sim = await this.simulate('list_orders_for_owner', [new Address(owner).toScVal()]);
    if (StellarRpc.Api.isSimulationError(sim)) return [];
    const raw = scValToNative(sim.result!.retval) as unknown[];
    return (raw ?? []).map((id) => BigInt(String(id)));
  }

  async estimateWithdrawalWait(investor: string, token: string): Promise<WaitEstimate> {
    const sim = await this.simulate('estimate_withdrawal_wait', [
      new Address(investor).toScVal(),
      new Address(token).toScVal(),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval) as Record<string, unknown>;
    return {
      queuePosition: Number(raw.queue_position),
      capitalAhead: BigInt(String(raw.capital_ahead)),
      nearestInvoiceDueDate: Number(raw.nearest_invoice_due_date),
      estimatedWaitSecs: Number(raw.estimated_wait_secs),
    };
  }

  async getLiquidityForecast(token: string, horizonDays: number): Promise<LiquidityForecastPoint[]> {
    const sim = await this.simulate('get_liquidity_forecast', [
      new Address(token).toScVal(),
      nativeToScVal(horizonDays, { type: 'u32' }),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval) as Record<string, unknown>[];
    return raw.map((r) => ({
      day: Number(r.day),
      projectedAvailable: BigInt(String(r.projected_available)),
    }));
  }
}
