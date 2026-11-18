import { rpc as StellarRpc } from '@stellar/stellar-sdk';
import { BaseClient, nativeToScVal, scValToNative, Address } from './base';
import { Errors as ShareErrors } from '../generated/share';
import type { ShareTokenMetadata } from '../generated/share';
import type { ClientConfig, Signer, TransactionProgress } from '../types';

/**
 * #1557: client for the `share` contract (contracts/share/src/lib.rs), the
 * SEP-41-style token representing pool shares. Covers the token interface
 * (balances, allowances, transfers), admin operations (mint, pause, admin
 * rotation) and the `balance_at` checkpoint read used for governance voting
 * weight.
 *
 * As with the other clients, write methods use `config.signer` to sign; the
 * `signer` param is accepted for API consistency with the rest of the SDK.
 */
export class ShareClient extends BaseClient {
  protected override readonly errors = ShareErrors;

  constructor(config: ClientConfig) {
    super(config);
  }

  // ---- Reads ----

  async balance(id: string): Promise<bigint> {
    const sim = await this.simulate('balance', [new Address(id).toScVal()]);
    return toBigInt(this.retval(sim));
  }

  /** `id`'s balance as of `timestamp` (Unix seconds, inclusive), from on-chain checkpoints. */
  async balanceAt(id: string, timestamp: number | bigint): Promise<bigint> {
    const sim = await this.simulate('balance_at', [
      new Address(id).toScVal(),
      nativeToScVal(timestamp, { type: 'u64' }),
    ]);
    return toBigInt(this.retval(sim));
  }

  async totalSupply(): Promise<bigint> {
    const sim = await this.simulate('total_supply', []);
    return toBigInt(this.retval(sim));
  }

  /** Current allowance; `0n` once `expiration_ledger` has passed. */
  async allowance(owner: string, spender: string): Promise<bigint> {
    const sim = await this.simulate('allowance', [
      new Address(owner).toScVal(),
      new Address(spender).toScVal(),
    ]);
    return toBigInt(this.retval(sim));
  }

  /** Ledger sequence at which the `owner` → `spender` allowance expires (`0` if none). */
  async getAllowanceExpiration(owner: string, spender: string): Promise<number> {
    const sim = await this.simulate('get_allowance_expiration', [
      new Address(owner).toScVal(),
      new Address(spender).toScVal(),
    ]);
    return Number(this.retval(sim) ?? 0);
  }

  async decimals(): Promise<number> {
    const sim = await this.simulate('decimals', []);
    return Number(this.retval(sim) ?? 0);
  }

  async name(): Promise<string> {
    const sim = await this.simulate('name', []);
    return String(this.retval(sim) ?? '');
  }

  async symbol(): Promise<string> {
    const sim = await this.simulate('symbol', []);
    return String(this.retval(sim) ?? '');
  }

  /** Convenience wrapper reading `name`, `symbol` and `decimals` together. */
  async metadata(): Promise<ShareTokenMetadata> {
    const [name, symbol, decimals] = await Promise.all([
      this.name(),
      this.symbol(),
      this.decimals(),
    ]);
    return { name, symbol, decimals };
  }

  async isPaused(): Promise<boolean> {
    const sim = await this.simulate('is_paused', []);
    return Boolean(this.retval(sim));
  }

  async admin(): Promise<string> {
    const sim = await this.simulate('admin', []);
    return String(this.retval(sim));
  }

  // ---- Token writes ----

  async transfer(params: {
    signer: Signer;
    from: string;
    to: string;
    amount: bigint;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.from,
      'transfer',
      [
        new Address(params.from).toScVal(),
        new Address(params.to).toScVal(),
        nativeToScVal(params.amount, { type: 'i128' }),
      ],
      params.onProgress,
    );
  }

  async transferFrom(params: {
    signer: Signer;
    spender: string;
    from: string;
    to: string;
    amount: bigint;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.spender,
      'transfer_from',
      [
        new Address(params.spender).toScVal(),
        new Address(params.from).toScVal(),
        new Address(params.to).toScVal(),
        nativeToScVal(params.amount, { type: 'i128' }),
      ],
      params.onProgress,
    );
  }

  async approve(params: {
    signer: Signer;
    owner: string;
    spender: string;
    amount: bigint;
    expirationLedger: number;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.owner,
      'approve',
      [
        new Address(params.owner).toScVal(),
        new Address(params.spender).toScVal(),
        nativeToScVal(params.amount, { type: 'i128' }),
        nativeToScVal(params.expirationLedger, { type: 'u32' }),
      ],
      params.onProgress,
    );
  }

  async increaseAllowance(params: {
    signer: Signer;
    owner: string;
    spender: string;
    amount: bigint;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.owner,
      'increase_allowance',
      [
        new Address(params.owner).toScVal(),
        new Address(params.spender).toScVal(),
        nativeToScVal(params.amount, { type: 'i128' }),
      ],
      params.onProgress,
    );
  }

  async decreaseAllowance(params: {
    signer: Signer;
    owner: string;
    spender: string;
    amount: bigint;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.owner,
      'decrease_allowance',
      [
        new Address(params.owner).toScVal(),
        new Address(params.spender).toScVal(),
        nativeToScVal(params.amount, { type: 'i128' }),
      ],
      params.onProgress,
    );
  }

  async burn(params: {
    signer: Signer;
    from: string;
    amount: bigint;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.from,
      'burn',
      [new Address(params.from).toScVal(), nativeToScVal(params.amount, { type: 'i128' })],
      params.onProgress,
    );
  }

  async burnFrom(params: {
    signer: Signer;
    spender: string;
    from: string;
    amount: bigint;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.spender,
      'burn_from',
      [
        new Address(params.spender).toScVal(),
        new Address(params.from).toScVal(),
        nativeToScVal(params.amount, { type: 'i128' }),
      ],
      params.onProgress,
    );
  }

  // ---- Admin writes ----

  async initialize(params: {
    signer: Signer;
    admin: string;
    decimals: number;
    name: string;
    symbol: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'initialize',
      [
        new Address(params.admin).toScVal(),
        nativeToScVal(params.decimals, { type: 'u32' }),
        nativeToScVal(params.name, { type: 'string' }),
        nativeToScVal(params.symbol, { type: 'string' }),
      ],
      params.onProgress,
    );
  }

  /** Mints `amount` to `to`. Requires the stored admin's auth, so `admin` is the tx source. */
  async mint(params: {
    signer: Signer;
    admin: string;
    to: string;
    amount: bigint;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'mint',
      [new Address(params.to).toScVal(), nativeToScVal(params.amount, { type: 'i128' })],
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

  /** Rotates the admin. Requires the current admin's auth, so `admin` is the tx source. */
  async setAdmin(params: {
    signer: Signer;
    admin: string;
    newAdmin: string;
    onProgress?: (progress: TransactionProgress) => void;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'set_admin',
      [new Address(params.newAdmin).toScVal()],
      params.onProgress,
    );
  }

  private retval(sim: StellarRpc.Api.SimulateTransactionResponse): unknown {
    if (!('result' in sim) || !sim.result) {
      throw new Error('Simulation returned no result');
    }
    return scValToNative(sim.result.retval);
  }
}

function toBigInt(value: unknown): bigint {
  if (value === null || value === undefined) return 0n;
  return BigInt(value as bigint | number | string);
}
