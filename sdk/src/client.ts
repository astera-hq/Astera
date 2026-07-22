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
  CoFundingRound,
} from './types';

// #860: `open_co_funding` takes a single OpenCoFundingRequest struct rather
// than individual scalar params. Soroban encodes named-field #[contracttype]
// structs as an ScMap keyed by field-name Symbols in alphabetical order —
// NOT declaration order — so the entries below are deliberately sorted
// (due_date, funding_deadline, invoice_id, max_investor_bps, min_commitment,
// sme, target_principal, token).
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

    // ---- #860: multi-investor co-funding rounds ----

    openCoFunding: async (params: {
      signer: (txXdr: string) => Promise<string>;
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
    }): Promise<string> => {
      const account = await this.server.getAccount(params.admin);
      const contract = new Contract(this.config.poolContractId);

      const tx = new TransactionBuilder(account, {
        fee: BASE_FEE,
        networkPassphrase: this.config.network,
      })
        .addOperation(
          contract.call(
            'open_co_funding',
            new Address(params.admin).toScVal(),
            openCoFundingRequestToScVal(params),
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

    commitToInvoice: async (params: {
      signer: (txXdr: string) => Promise<string>;
      investor: string;
      invoiceId: bigint | number;
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
            'commit_to_invoice',
            new Address(params.investor).toScVal(),
            nativeToScVal(params.invoiceId, { type: 'u64' }),
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

    finalizeCoFunding: async (params: {
      signer: (txXdr: string) => Promise<string>;
      caller: string;
      invoiceId: bigint | number;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> => {
      const account = await this.server.getAccount(params.caller);
      const contract = new Contract(this.config.poolContractId);

      const tx = new TransactionBuilder(account, {
        fee: BASE_FEE,
        networkPassphrase: this.config.network,
      })
        .addOperation(
          contract.call(
            'finalize_co_funding',
            new Address(params.caller).toScVal(),
            nativeToScVal(params.invoiceId, { type: 'u64' }),
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

    withdrawCommitment: async (params: {
      signer: (txXdr: string) => Promise<string>;
      investor: string;
      invoiceId: bigint | number;
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
            'withdraw_co_funding_commitment',
            new Address(params.investor).toScVal(),
            nativeToScVal(params.invoiceId, { type: 'u64' }),
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

    transferCoFundShare: async (params: {
      signer: (txXdr: string) => Promise<string>;
      from: string;
      invoiceId: bigint | number;
      token: string;
      to: string;
      bps: number;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> => {
      const account = await this.server.getAccount(params.from);
      const contract = new Contract(this.config.poolContractId);

      const tx = new TransactionBuilder(account, {
        fee: BASE_FEE,
        networkPassphrase: this.config.network,
      })
        .addOperation(
          contract.call(
            'transfer_co_fund_share',
            new Address(params.from).toScVal(),
            nativeToScVal(params.invoiceId, { type: 'u64' }),
            new Address(params.token).toScVal(),
            new Address(params.to).toScVal(),
            nativeToScVal(params.bps, { type: 'u32' }),
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

    getCoFundingRound: async (invoiceId: bigint | number): Promise<CoFundingRound | null> => {
      const sim = await simulateTx(
        this.server,
        this.config.network,
        this.config.poolContractId,
        'get_co_funding_round',
        [nativeToScVal(invoiceId, { type: 'u64' })],
        'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
      );
      if (StellarRpc.Api.isSimulationError(sim)) {
        throw new Error(`Simulation failed: ${sim.error}`);
      }
      const raw = scValToNative(sim.result!.retval);
      if (!raw) return null;
      return coFundingRoundFromScVal(raw as Record<string, unknown>);
    },

    listCoFundingRounds: async (): Promise<bigint[]> => {
      const sim = await simulateTx(
        this.server,
        this.config.network,
        this.config.poolContractId,
        'list_co_funding_rounds',
        [],
        'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
      );
      if (StellarRpc.Api.isSimulationError(sim)) {
        throw new Error(`Simulation failed: ${sim.error}`);
      }
      const raw = scValToNative(sim.result!.retval) as unknown[];
      return (raw ?? []).map((id) => BigInt(String(id)));
    },

    getInvestorCoFundPositions: async (
      investor: string,
    ): Promise<Array<{ invoiceId: bigint; bps: number }>> => {
      const sim = await simulateTx(
        this.server,
        this.config.network,
        this.config.poolContractId,
        'get_investor_co_fund_positions',
        [new Address(investor).toScVal()],
        'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
      );
      if (StellarRpc.Api.isSimulationError(sim)) {
        throw new Error(`Simulation failed: ${sim.error}`);
      }
      const raw = scValToNative(sim.result!.retval) as [bigint | string, number][];
      return (raw ?? []).map(([invoiceId, bps]) => ({
        invoiceId: BigInt(String(invoiceId)),
        bps,
      }));
    },

    getCoFundShare: async (invoiceId: bigint | number, investor: string): Promise<number> => {
      const sim = await simulateTx(
        this.server,
        this.config.network,
        this.config.poolContractId,
        'get_co_fund_share',
        [nativeToScVal(invoiceId, { type: 'u64' }), new Address(investor).toScVal()],
        'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',
      );
      if (StellarRpc.Api.isSimulationError(sim)) {
        throw new Error(`Simulation failed: ${sim.error}`);
      }
      return Number(scValToNative(sim.result!.retval));
    },
  };

  private async submitTx(
    signedXDR: string,
    onProgress?: (progress: TransactionProgress) => void,
  ): Promise<{ hash: string } & StellarRpc.Api.GetTransactionResponse> {
    const tx = TransactionBuilder.fromXDR(signedXDR, this.config.network);
    const response = await this.server.sendTransaction(tx);
    const hash = response.hash;

    if (response.status === 'ERROR') {
      const error = `Transaction failed: ${JSON.stringify(response)}`;
      onProgress?.({ status: 'failed', hash, error });
      throw new Error(error);
    }

    onProgress?.({ status: 'pending', hash });
    let result = await this.server.getTransaction(hash);
    let attempts = 0;

    while (
      (String(result.status) === 'NOT_FOUND' || String(result.status) === 'PENDING') &&
      attempts < 20
    ) {
      onProgress?.({ status: 'pending', hash });
      await new Promise((r) => setTimeout(r, 1500));
      result = await this.server.getTransaction(hash);
      attempts++;
    }

    if (String(result.status) === 'FAILED') {
      const error = 'Transaction failed on-chain';
      onProgress?.({ status: 'failed', hash, error });
      throw new Error(error);
    }

    onProgress?.({ status: 'confirmed', hash });
    return Object.assign(result, { hash });
  }
}
