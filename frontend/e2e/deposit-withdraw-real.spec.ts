import { test, expect } from '@playwright/test';
import {
  Account,
  Address,
  Contract,
  Keypair,
  Networks,
  TransactionBuilder,
  BASE_FEE,
  rpc,
  scValToNative,
} from '@stellar/stellar-sdk';

/**
 * Runs only when the local-network job supplies a funded test keypair and
 * deployed contract IDs. Unlike deposit-withdraw.spec.ts this deliberately
 * does not stub RPC: the page builds, signs, submits, and then reads a real
 * on-chain share balance through the same UI path a user takes.
 */
test.describe('Deposit / Withdraw on a real local network', () => {
  test.skip(process.env.E2E_REAL_NETWORK !== 'true', 'real local network is not configured');

  test('submits deposit and withdraw and observes on-chain share balance', async ({ page }) => {
    const secret = process.env.E2E_TEST_SECRET;
    const rpcUrl = process.env.NEXT_PUBLIC_SOROBAN_RPC_URL;
    const token = process.env.NEXT_PUBLIC_USDC_TOKEN_ID;
    const share = process.env.NEXT_PUBLIC_SHARE_TOKEN_ID;
    if (!secret || !rpcUrl || !token || !share) {
      test.skip(true, 'E2E_TEST_SECRET, RPC URL, token, and share IDs are required');
      return;
    }

    const keypair = Keypair.fromSecret(secret);
    const address = keypair.publicKey();
    const network = process.env.NEXT_PUBLIC_NETWORK === 'mainnet' ? Networks.PUBLIC : Networks.STANDALONE;
    const server = new rpc.Server(rpcUrl);

    await page.exposeFunction('__signRealTransaction', (xdr: string) => {
      const tx = TransactionBuilder.fromXDR(xdr, network);
      tx.sign(keypair);
      return tx.toXDR();
    });
    await page.addInitScript((walletAddress: string) => {
      window.__MOCK_FREIGHTER_API__ = {
        isConnected: async () => ({ isConnected: true }),
        isAllowed: async () => ({ isAllowed: true }),
        getAddress: async () => ({ address: walletAddress, error: null }),
        signTransaction: async (xdr: string) => ({
          signedTxXdr: await (window as unknown as { __signRealTransaction: (x: string) => Promise<string> })
            .__signRealTransaction(xdr),
          error: null,
        }),
      };
      localStorage.setItem('astera_wallet_address', walletAddress);
    }, address);

    async function shareBalance(): Promise<bigint> {
      const account = await server.getAccount(address);
      const tx = new TransactionBuilder(account, {
        fee: BASE_FEE,
        networkPassphrase: network,
      })
        .addOperation(new Contract(share).call('balance', new Address(address).toScVal()))
        .setTimeout(30)
        .build();
      const simulation = await server.simulateTransaction(tx);
      if (rpc.Api.isSimulationError(simulation)) throw new Error(simulation.error);
      return BigInt(scValToNative(simulation.result!.retval) as string | number | bigint);
    }

    const before = await shareBalance();
    await page.goto('/invest');
    const amount = page.locator('input[type="number"]').first();
    await expect(amount).toBeVisible();
    await amount.fill('1');
    await page.getByRole('button', { name: /^deposit$/i }).click();
    await expect(page.getByText(/successfully|confirmed/i)).toBeVisible({ timeout: 60_000 });
    const afterDeposit = await shareBalance();
    expect(afterDeposit).toBeGreaterThan(before);

    await page.getByRole('button', { name: /^withdraw$/i }).click();
    await amount.fill('1');
    await page.getByRole('button', { name: /withdraw/i }).last().click();
    await expect(page.getByText(/successfully|confirmed/i)).toBeVisible({ timeout: 60_000 });
    await expect.poll(shareBalance, { timeout: 60_000 }).toBeLessThan(afterDeposit);
  });
});
