import { test, expect } from '@playwright/test';
import {
  rpc,
  Contract,
  TransactionBuilder,
  BASE_FEE,
  Networks,
  Account,
  Keypair,
  scValToNative,
} from '@stellar/stellar-sdk';
import { INVOICE_CONTRACT_ID, hasRealContractIds } from './contract-ids';

const RPC_URL = process.env.NEXT_PUBLIC_STELLAR_RPC_URL ?? 'https://soroban-testnet.stellar.org';

/**
 * Real contract integration check (#395).
 *
 * Runs only when real testnet contract IDs are configured. It calls a real
 * read-only method (`get_invoice_count`) on the deployed invoice contract via
 * Soroban RPC simulation and asserts a concrete result — so XDR/method-name
 * regressions against the real contract are actually caught.
 */
test.describe('Deployed contract interaction (#395)', () => {
  test.skip(!hasRealContractIds, 'Requires real testnet contract IDs (see e2e/contract-ids.ts).');

  test('invoice get_invoice_count returns a numeric count from the deployed contract', async () => {
    const server = new rpc.Server(RPC_URL);
    const source = new Account(Keypair.random().publicKey(), '0');
    const contract = new Contract(INVOICE_CONTRACT_ID as string);

    const tx = new TransactionBuilder(source, {
      fee: BASE_FEE,
      networkPassphrase: Networks.TESTNET,
    })
      .addOperation(contract.call('get_invoice_count'))
      .setTimeout(30)
      .build();

    const sim = await server.simulateTransaction(tx);

    expect(rpc.Api.isSimulationError(sim)).toBe(false);
    expect(rpc.Api.isSimulationSuccess(sim)).toBe(true);

    const success = sim as rpc.Api.SimulateTransactionSuccessResponse;
    expect(success.result?.retval).toBeDefined();

    const count = scValToNative(success.result!.retval);
    // u64 decodes to a bigint; it must be a non-negative count.
    expect(typeof count).toBe('bigint');
    expect(count >= 0n).toBe(true);
  });
});
