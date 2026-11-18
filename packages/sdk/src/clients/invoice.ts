import { Contract, TransactionBuilder, BASE_FEE, rpc as StellarRpc } from '@stellar/stellar-sdk';
import { BaseClient, nativeToScVal, scValToNative, Address, xdr } from './base';
import { Errors as InvoiceErrors } from '../generated/invoice';
import type { ClientConfig, Invoice, InvoiceMetadata, TransactionProgress } from '../types';
import type { Signer } from '../types';

export interface CreateInvoiceParams {
  signer: Signer;
  owner: string;
  debtor: string;
  amount: bigint;
  dueDate: number;
  description: string;
  verificationHash?: string;
  onProgress?: (progress: TransactionProgress) => void;
}

export interface VerifyInvoiceParams {
  signer: Signer;
  oracle: string;
  id: bigint | number;
  approved: boolean;
  reason: string;
  oracleHash: string;
  onProgress?: (progress: TransactionProgress) => void;
}

export class InvoiceClient extends BaseClient {
  protected override readonly errors = InvoiceErrors;

  constructor(config: ClientConfig) {
    super(config);
  }

  async getInvoice(id: bigint | number): Promise<Invoice> {
    const sim = await this.simulate('get_invoice', [
      nativeToScVal(id, { type: 'u64' }),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    return scValToNative(sim.result!.retval) as Invoice;
  }

  /**
   * #987 — batch-read helper matching the on-chain get_multiple_invoices, so
   * consumers don't have to assemble the Vec<u64> call manually or fall back
   * to one RPC per id. Order of the returned array matches `ids`.
   */
  async getMultipleInvoices(ids: Array<bigint | number>): Promise<Invoice[]> {
    const sim = await this.simulate('get_multiple_invoices', [
      xdr.ScVal.scvVec(ids.map((id) => nativeToScVal(id, { type: 'u64' }))),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    return scValToNative(sim.result!.retval) as Invoice[];
  }

  async getMetadata(id: bigint | number): Promise<InvoiceMetadata> {
    const sim = await this.simulate('get_metadata', [
      nativeToScVal(id, { type: 'u64' }),
    ]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    const raw = scValToNative(sim.result!.retval) as Record<string, unknown>;
    const due = raw.due_date !== undefined ? Number(raw.due_date) : Number(raw.dueDate);
    return {
      name: raw.name as string,
      description: raw.description as string,
      image: raw.image as string,
      amount: BigInt(String(raw.amount)),
      debtor: raw.debtor as string,
      dueDate: due,
      status: raw.status as any,
      symbol: raw.symbol as string,
      decimals: Number(raw.decimals),
    };
  }

  async createInvoice(params: CreateInvoiceParams): Promise<string> {
    return this.buildAndSendTx(
      params.owner,
      'create_invoice',
      [
        new Address(params.owner).toScVal(),
        nativeToScVal(params.debtor, { type: 'string' }),
        nativeToScVal(params.amount, { type: 'i128' }),
        nativeToScVal(params.dueDate, { type: 'u64' }),
        nativeToScVal(params.description, { type: 'string' }),
        nativeToScVal(params.verificationHash || '', { type: 'string' }),
      ],
      params.onProgress,
    );
  }

  async verifyInvoice(params: VerifyInvoiceParams): Promise<string> {
    return this.buildAndSendTx(
      params.oracle,
      'verify_invoice',
      [
        nativeToScVal(params.id, { type: 'u64' }),
        new Address(params.oracle).toScVal(),
        nativeToScVal(params.approved, { type: 'bool' }),
        nativeToScVal(params.reason, { type: 'string' }),
        nativeToScVal(params.oracleHash, { type: 'string' }),
      ],
      params.onProgress,
    );
  }
}
