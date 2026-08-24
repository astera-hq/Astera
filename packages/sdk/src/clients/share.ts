import { BaseClient, nativeToScVal, scValToNative, Address } from './base';
import type { ClientConfig, TransactionProgress, Signer } from '../types';

export interface ShareTokenMetadata {
  name: string;
  symbol: string;
  decimals: number;
  totalSupply: bigint;
}

export class ShareClient extends BaseClient {
  protected override readonly errors = {};

  constructor(config: ClientConfig) {
    super(config);
  }

  // ─── Read-only methods ──────────────────────────────────────────────

  async balance(id: string): Promise<bigint> {
    const sim = await this.simulate('balance', [new Address(id).toScVal()]);
    return scValToNative(sim.result!.retval);
  }

  async balanceAt(id: string, timestamp: number): Promise<bigint> {
    const sim = await this.simulate('balance_at', [
      new Address(id).toScVal(),
      nativeToScVal(timestamp, { type: 'u64' }),
    ]);
    return scValToNative(sim.result!.retval);
  }

  async totalSupply(): Promise<bigint> {
    const sim = await this.simulate('total_supply', []);
    return scValToNative(sim.result!.retval);
  }

  async allowance(owner: string, spender: string): Promise<bigint> {
    const sim = await this.simulate('allowance', [
      new Address(owner).toScVal(),
      new Address(spender).toScVal(),
    ]);
    return scValToNative(sim.result!.retval);
  }

  async decimals(): Promise<number> {
    const sim = await this.simulate('decimals', []);
    return Number(scValToNative(sim.result!.retval));
  }

  async name(): Promise<string> {
    const sim = await this.simulate('name', []);
    return scValToNative(sim.result!.retval);
  }

  async symbol(): Promise<string> {
    const sim = await this.simulate('symbol', []);
    return scValToNative(sim.result!.retval);
  }

  async metadata(): Promise<ShareTokenMetadata> {
    const [name, symbol, decimals, totalSupply] = await Promise.all([
      this.name(),
      this.symbol(),
      this.decimals(),
      this.totalSupply(),
    ]);
    return { name, symbol, decimals, totalSupply };
  }

  // ─── Mutation methods ───────────────────────────────────────────────

  async mint(params: {
    signer: Signer;
    admin: string;
    to: string;
    amount: bigint;
    onProgress?: TransactionProgress;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'mint',
      [new Address(params.to).toScVal(), nativeToScVal(params.amount, { type: 'i128' })],
      params.onProgress,
    );
  }

  async burn(params: {
    signer: Signer;
    admin: string;
    from: string;
    amount: bigint;
    onProgress?: TransactionProgress;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.admin,
      'burn',
      [new Address(params.from).toScVal(), nativeToScVal(params.amount, { type: 'i128' })],
      params.onProgress,
    );
  }

  async transfer(params: {
    signer: Signer;
    from: string;
    to: string;
    amount: bigint;
    onProgress?: TransactionProgress;
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

  async approve(params: {
    signer: Signer;
    owner: string;
    spender: string;
    amount: bigint;
    onProgress?: TransactionProgress;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.owner,
      'approve',
      [
        new Address(params.owner).toScVal(),
        new Address(params.spender).toScVal(),
        nativeToScVal(params.amount, { type: 'i128' }),
      ],
      params.onProgress,
    );
  }

  async increaseAllowance(params: {
    signer: Signer;
    owner: string;
    spender: string;
    addedAmount: bigint;
    onProgress?: TransactionProgress;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.owner,
      'increase_allowance',
      [
        new Address(params.owner).toScVal(),
        new Address(params.spender).toScVal(),
        nativeToScVal(params.addedAmount, { type: 'i128' }),
      ],
      params.onProgress,
    );
  }

  async decreaseAllowance(params: {
    signer: Signer;
    owner: string;
    spender: string;
    subtractedAmount: bigint;
    onProgress?: TransactionProgress;
  }): Promise<string> {
    return this.buildAndSendTx(
      params.owner,
      'decrease_allowance',
      [
        new Address(params.owner).toScVal(),
        new Address(params.spender).toScVal(),
        nativeToScVal(params.subtractedAmount, { type: 'i128' }),
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
    onProgress?: TransactionProgress;
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
}
