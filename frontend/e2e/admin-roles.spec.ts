import { test, expect } from '@playwright/test';
import {
  createAccessControlStore,
  stubAccessControlContracts,
} from './mocks/soroban-access-control';

const SIGNER_A = 'GALFWZFITZIDDSI5NQUGTC2SSR27SNPNU3QF6NZX5XAOHZB6YNBBVOHD';
const SIGNER_B = 'GAMWS4XVBMBSVVHIDMHLO4SKSX2VNL5SDX5326RSSMUKMWMAXGDXRYC3';
const SIGNER_C = 'GCAKR2ZQBYJOCAE7NO23XBE4E3HSX6N3QDB3IZPCDDKJMPHUGM7KVNHG';

/** Freighter mock that signs by returning the xdr unmodified — the shared
 * `freighterMockScript` appends a `_signed` suffix, which would corrupt the
 * transaction envelope this spec needs to actually replay through the mock
 * RPC (`extractInvocation` re-parses the signed xdr as a real envelope). */
function cleanFreighterMockScript(address: string): string {
  return `
    (function() {
      window.__MOCK_FREIGHTER_API__ = {
        isConnected: () => Promise.resolve({ isConnected: true }),
        isAllowed: () => Promise.resolve({ isAllowed: true }),
        setAllowed: () => Promise.resolve({ isAllowed: true }),
        getAddress: () => Promise.resolve({ address: '${address}', error: null }),
        signTransaction: (xdr) => Promise.resolve({ signedTxXdr: xdr, error: null }),
        getNetwork: () => Promise.resolve({ network: 'TESTNET', networkPassphrase: 'Test SDF Network ; September 2015' }),
      };
    })();
  `;
}

async function injectWallet(page: import('@playwright/test').Page, address: string) {
  await page.addInitScript(cleanFreighterMockScript(address));
  // Matches lib/store.ts's `WALLET_KEY` — WalletConnect's auto-reconnect
  // effect reads this on mount and, seeing it set, calls the (mocked)
  // Freighter API to restore `wallet.connected`/`wallet.address`.
  await page.addInitScript((addr: string) => {
    localStorage.setItem('astera_wallet_address', addr);
  }, address);
}

// #864: two-signer approval flow for the RiskManager role's 2-of-3 multisig —
// wallet A proposes a yield change, wallet B approves it (crossing the
// threshold), then either wallet executes it against the (mocked) pool
// contract. Mirrors the exact scenario required by issue #864's acceptance
// criteria.
test.describe('Admin roles & multisig access control', () => {
  test.skip(!!process.env.CI, 'Admin flows require live contract + permissions in CI.');

  test('wallet A proposes, wallet B approves, execution updates the pool yield', async ({
    browser,
  }) => {
    const store = createAccessControlStore('RiskManager', [SIGNER_A, SIGNER_B, SIGNER_C], 2);

    const contextA = await browser.newContext();
    const pageA = await contextA.newPage();
    await injectWallet(pageA, SIGNER_A);
    await stubAccessControlContracts(pageA, store, SIGNER_A);

    const contextB = await browser.newContext();
    const pageB = await contextB.newPage();
    await injectWallet(pageB, SIGNER_B);
    await stubAccessControlContracts(pageB, store, SIGNER_B);

    // Wallet A proposes a SetYield action targeting the pool contract.
    await pageA.goto('/admin/roles');
    await expect(
      pageA.getByRole('heading', { name: 'Roles & Multisig Access Control' }),
    ).toBeVisible({
      timeout: 15_000,
    });

    await pageA.getByRole('combobox').nth(0).selectOption('RiskManager');
    await pageA.getByRole('combobox').nth(1).selectOption('pool');
    await pageA.getByRole('combobox').nth(2).selectOption('SetYield');
    await pageA.getByPlaceholder('Basis points').fill('650');
    await pageA.getByRole('button', { name: 'Submit Proposal' }).click();

    await expect(pageA.getByText('Set Yield → 650 bps')).toBeVisible({ timeout: 15_000 });
    await expect(pageA.getByText('Approvals: 1 / 2')).toBeVisible();
    await expect(pageA.getByText('Pending')).toBeVisible();

    // Wallet B sees the same pending proposal and approves it.
    await pageB.goto('/admin/roles');
    await expect(pageB.getByText('Set Yield → 650 bps')).toBeVisible({ timeout: 15_000 });
    await pageB.getByRole('button', { name: 'Approve' }).click();

    await expect(pageB.getByText('Approvals: 2 / 2')).toBeVisible({ timeout: 15_000 });
    await expect(pageB.getByText('Approved')).toBeVisible();

    // Threshold met — either signer can now execute it.
    await pageB.getByRole('button', { name: 'Execute' }).click();
    await expect(pageB.getByText('Executed')).toBeVisible({ timeout: 15_000 });

    expect(store.poolYieldBps).toBe(650);
  });
});
