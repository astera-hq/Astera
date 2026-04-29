import {
  rpc as StellarRpc,
  TransactionBuilder,
  BASE_FEE,
  Contract,
} from '@stellar/stellar-sdk';
import { simulateTx, nativeToScVal, scValToNative, Address, xdr } from './stellar';
import type {
  AsteraConfig,
  Invoice,
  InvoiceMetadata,
  InvestorPosition,
  PoolConfig,
  PoolTokenTotals,
  FundedInvoice,
  TransactionProgress,
} from './types';

export class AsteraClient {
  private server: StellarRpc.Server;
  private config: AsteraConfig;

  constructor(config: AsteraConfig) {
    this.server = new StellarRpc.Server(config.rpcUrl);
    this.config = config;
  }

  // ---- Invoice Contract ----

  public readonly invoice = {
    get: async (id: bigint | number): Promise<Invoice> => {
      const sim = await simulateTx(
        this.server,
        this.config.network,
        this.config.invoiceContractId,
        'get_invoice',
        [nativeToScVal(id, { type: 'u64' })],
        'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
      );

      if (StellarRpc.Api.isSimulationError(sim)) {
        throw new Error(`Simulation failed: ${sim.error}`);
      }
      return scValToNative(sim.result!.retval) as Invoice;
    },

    getMetadata: async (id: bigint | number): Promise<InvoiceMetadata> => {
      const sim = await simulateTx(
        this.server,
        this.config.network,
        this.config.invoiceContractId,
        'get_metadata',
        [nativeToScVal(id, { type: 'u64' })],
        'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
      );

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
    },

    create: async (params: {
      signer: (txXdr: string) => Promise<string>;
      owner: string;
      debtor: string;
      amount: bigint;
      dueDate: number;
      description: string;
      verificationHash?: string;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> => {
      const account = await this.server.getAccount(params.owner);
      const contract = new Contract(this.config.invoiceContractId);

      const tx = new TransactionBuilder(account, {
        fee: BASE_FEE,
        networkPassphrase: this.config.network,
      })
        .addOperation(
          contract.call(
            'create_invoice',
            new Address(params.owner).toScVal(),
            nativeToScVal(params.debtor, { type: 'string' }),
            nativeToScVal(params.amount, { type: 'i128' }),
            nativeToScVal(params.dueDate, { type: 'u64' }),
            nativeToScVal(params.description, { type: 'string' }),
            nativeToScVal(params.verificationHash || '', { type: 'string' }),
          ),
        )
        .setTimeout(30)
        .build();

      const sim = await this.server.simulateTransaction(tx);
      if (StellarRpc.Api.isSimulationError(sim)) {
        throw new Error(`Simulation failed: ${sim.error}`);
      }

      const prepared = StellarRpc.assembleTransaction(tx, sim).build();
      const signedXdr = await params.signer(prepared.toXDR());
      const result = await this.submitTx(signedXdr, params.onProgress);
      return result.hash;
    },

    verify: async (params: {
      signer: (txXdr: string) => Promise<string>;
      oracle: string;
      id: bigint | number;
      approved: boolean;
      reason: string;
      oracleHash: string;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> => {
      const account = await this.server.getAccount(params.oracle);
      const contract = new Contract(this.config.invoiceContractId);

      const tx = new TransactionBuilder(account, {
        fee: BASE_FEE,
        networkPassphrase: this.config.network,
      })
        .addOperation(
          contract.call(
            'verify_invoice',
            nativeToScVal(params.id, { type: 'u64' }),
            new Address(params.oracle).toScVal(),
            nativeToScVal(params.approved, { type: 'bool' }),
            nativeToScVal(params.reason, { type: 'string' }),
            nativeToScVal(params.oracleHash, { type: 'string' }),
          ),
        )
        .setTimeout(30)
        .build();

      const sim = await this.server.simulateTransaction(tx);
      if (StellarRpc.Api.isSimulationError(sim)) {
        throw new Error(`Simulation failed: ${sim.error}`);
      }

      const prepared = StellarRpc.assembleTransaction(tx, sim).build();
      const signedXdr = await params.signer(prepared.toXDR());
      const result = await this.submitTx(signedXdr, params.onProgress);
      return result.hash;
    },
  };

  // ---- Pool Contract ----

  public readonly pool = {
    getConfig: async (): Promise<PoolConfig> => {
      const sim = await simulateTx(
        this.server,
        this.config.network,
        this.config.poolContractId,
        'get_config',
        [],
        'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
      );

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
    },

    getPosition: async (investor: string, token: string): Promise<InvestorPosition | null> => {
      const sim = await simulateTx(
        this.server,
        this.config.network,
        this.config.poolContractId,
        'get_position',
        [new Address(investor).toScVal(), new Address(token).toScVal()],
        'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
      );

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
    },

    deposit: async (params: {
      signer: (txXdr: string) => Promise<string>;
      investor: string;
      token: string;
      amount: bigint;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> => {
      const account = await this.server.getAccount(params.investor);
      const contract = new Contract(this.config.poolContractId);

      const tx = new TransactionBuilder(account, {
        fee: BASE_FEE,
        networkPassphrase: this.config.network,
      })
        .addOperation(
          contract.call(
            'deposit',
            new Address(params.investor).toScVal(),
            new Address(params.token).toScVal(),
            nativeToScVal(params.amount, { type: 'i128' }),
          ),
        )
        .setTimeout(30)
        .build();

      const sim = await this.server.simulateTransaction(tx);
      if (StellarRpc.Api.isSimulationError(sim)) {
        throw new Error(`Simulation failed: ${sim.error}`);
      }

      const prepared = StellarRpc.assembleTransaction(tx, sim).build();
      const signedXdr = await params.signer(prepared.toXDR());
      const result = await this.submitTx(signedXdr, params.onProgress);
      return result.hash;
    },

    repay: async (params: {
      signer: (txXdr: string) => Promise<string>;
      payer: string;
      invoiceId: bigint | number;
      amount: bigint;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> => {
      const account = await this.server.getAccount(params.payer);
      const contract = new Contract(this.config.poolContractId);

      const tx = new TransactionBuilder(account, {
        fee: BASE_FEE,
        networkPassphrase: this.config.network,
      })
        .addOperation(
          contract.call(
            'repay_invoice',
            nativeToScVal(params.invoiceId, { type: 'u64' }),
            new Address(params.payer).toScVal(),
            nativeToScVal(params.amount, { type: 'i128' }),
          ),
        )
        .setTimeout(30)
        .build();

      const sim = await this.server.simulateTransaction(tx);
      if (StellarRpc.Api.isSimulationError(sim)) {
        throw new Error(`Simulation failed: ${sim.error}`);
      }

      const prepared = StellarRpc.assembleTransaction(tx, sim).build();
      const signedXdr = await params.signer(prepared.toXDR());
      const result = await this.submitTx(signedXdr, params.onProgress);
      return result.hash;
    },
  };

  private async submitTx(
    signedXDR: string,
    onProgress?: (progress: TransactionProgress) => void,
  ): Promise<StellarRpc.Api.GetTransactionResponse> {
    const tx = TransactionBuilder.fromXDR(signedXDR, this.config.network);
    const response = await this.server.sendTransaction(tx);

    if (response.status === 'ERROR') {
      const error = `Transaction failed: ${JSON.stringify(response)}`;
      onProgress?.({ status: 'failed', hash: response.hash, error });
      throw new Error(error);
    }

    onProgress?.({ status: 'pending', hash: response.hash });
    let result = await this.server.getTransaction(response.hash);
    let attempts = 0;

    while (
      (String(result.status) === 'NOT_FOUND' || String(result.status) === 'PENDING') &&
      attempts < 20
    ) {
      onProgress?.({ status: 'pending', hash: response.hash });
      await new Promise((r) => setTimeout(r, 1500));
      result = await this.server.getTransaction(response.hash);
      attempts++;
    }

    if (String(result.status) === 'FAILED') {
      const error = 'Transaction failed on-chain';
      onProgress?.({ status: 'failed', hash: response.hash, error });
      throw new Error(error);
    }

    onProgress?.({ status: 'confirmed', hash: response.hash });
    return result;
  }
}
