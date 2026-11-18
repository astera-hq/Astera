import {
  TransactionBuilder,
  BASE_FEE,
  Contract,
  rpc as StellarRpc,
  xdr,
} from '@stellar/stellar-sdk';
import { rpcExecute, NETWORK } from './stellar';

export interface FeeEstimate {
  minResourceFee: number;
  instructions: number;
  feeInXlm: number;
}

export async function simulateContractCall(
  contractId: string,
  method: string,
  args: xdr.ScVal[],
  sourceAddress: string,
): Promise<FeeEstimate> {
  return rpcExecute(async (server) => {
    const account = await server.getAccount(sourceAddress);
    const contract = new Contract(contractId);

    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: NETWORK,
    })
      .addOperation(contract.call(method, ...args))
      .setTimeout(30)
      .build();

    const sim = await server.simulateTransaction(tx);

    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(sim.error);
    }

    const minResourceFee = Number((sim as any).minResourceFee ?? 0);
    const instructions = Number(
      (sim as StellarRpc.Api.SimulateTransactionSuccessResponse).result?.instructions ?? 0,
    );

    return {
      minResourceFee,
      instructions,
      feeInXlm: minResourceFee / 10_000_000,
    };
  });
}
