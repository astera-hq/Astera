import {
  rpc as StellarRpc,
  scValToNative,
  nativeToScVal,
  Address,
  xdr,
  TransactionBuilder,
  BASE_FEE,
  Contract,
} from '@stellar/stellar-sdk';

export async function simulateTx(
  server: StellarRpc.Server,
  network: string,
  contractId: string,
  method: string,
  args: xdr.ScVal[],
  sourceAddress: string,
): Promise<StellarRpc.Api.SimulateTransactionResponse> {
  const account = await server.getAccount(sourceAddress);
  const contract = new Contract(contractId);

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: network,
  })
    .addOperation(contract.call(method, ...args))
    .setTimeout(30)
    .build();

  return server.simulateTransaction(tx);
}

export { nativeToScVal, scValToNative, Address, xdr };
