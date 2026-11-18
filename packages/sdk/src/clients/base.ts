import {
  rpc as StellarRpc,
  TransactionBuilder,
  BASE_FEE,
  Contract,
  Keypair,
} from '@stellar/stellar-sdk';
import { simulateTx } from '../stellar';
import { parseContractError } from '../errors';
import type { ClientConfig, Signer, TransactionProgress } from '../types';

export { simulateTx } from '../stellar';
export { nativeToScVal, scValToNative, Address, xdr } from '../stellar';
export { ContractError, parseContractError } from '../errors';

export class BaseClient {
  public readonly server: StellarRpc.Server;
  public readonly config: ClientConfig;
  /** Contract-specific `#[contracterror]` map used to type simulation/tx failures. Set by subclasses. */
  protected readonly errors: Record<number, { message: string }> = {};

  constructor(config: ClientConfig) {
    this.server = new StellarRpc.Server(config.rpcUrl);
    this.config = config;
  }

  get contractId(): string {
    return this.config.contractId;
  }

  get network(): string {
    return this.config.network;
  }

  get signer(): Signer | undefined {
    return this.config.signer;
  }

  protected async buildAndSendTx(
    caller: string,
    method: string,
    args: import('@stellar/stellar-sdk').xdr.ScVal[],
    onProgress?: (progress: TransactionProgress) => void,
  ): Promise<string> {
    const account = await this.server.getAccount(caller);
    const contract = new Contract(this.contractId);

    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: this.network,
    })
      .addOperation(contract.call(method, ...args))
      .setTimeout(30)
      .build();

    const sim = await this.server.simulateTransaction(tx);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw parseContractError(sim.error, this.errors);
    }

    const prepared = StellarRpc.assembleTransaction(tx, sim).build();
    const signedXdr = await this.resolveSigner(prepared.toXDR());
    return this.submitTx(signedXdr, onProgress);
  }

  protected async simulate(
    method: string,
    args: import('@stellar/stellar-sdk').xdr.ScVal[],
  ): Promise<StellarRpc.Api.SimulateTransactionResponse> {
    const sim = await simulateTx(this.server, this.network, this.contractId, method, args, SIMULATION_SOURCE);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw parseContractError(sim.error, this.errors);
    }
    return sim;
  }

  private async resolveSigner(txXdr: string): Promise<string> {
    if (this.signer) {
      return this.signer(txXdr);
    }
    throw new Error('No signer configured. Provide a signer or use a Keypair-based client.');
  }

  private async submitTx(
    signedXdr: string,
    onProgress?: (progress: TransactionProgress) => void,
  ): Promise<string> {
    const tx = TransactionBuilder.fromXDR(signedXdr, this.network);
    const result = await this.server.sendTransaction(tx);

    if (result.status === 'PENDING' || result.status === 'DUPLICATE') {
      onProgress?.({ status: 'pending', hash: result.hash });
      let status = await this.server.getTransaction(result.hash);
      const maxWait = 30_000;
      const pollInterval = 1_000;
      let waited = 0;

      while (status.status === 'NOT_FOUND' && waited < maxWait) {
        await new Promise((r) => setTimeout(r, pollInterval));
        waited += pollInterval;
        status = await this.server.getTransaction(result.hash);
      }

      if (status.status === 'SUCCESS') {
        onProgress?.({ status: 'confirmed', hash: result.hash });
      } else {
        onProgress?.({
          status: 'failed',
          hash: result.hash,
          error: `Transaction ${status.status}`,
        });
      }
      return result.hash;
    }

    throw new Error(`Transaction submission failed: ${result.status}`);
  }
}

const SIMULATION_SOURCE = 'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN';
