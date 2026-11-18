import { test, expect } from '@playwright/test';
import { freighterMockScript, MOCK_ADDRESS } from './mocks/freighter';

test.describe('Admin route guard (#393)', () => {
  test.skip(!!process.env.CI, 'Admin role checks need live contract config in CI.');

  test('non-admin wallet visiting /admin is redirected to /dashboard', async ({ page }) => {
    // Connect a non-admin wallet via the Freighter mock and auto-reconnect.
    await page.addInitScript(
      freighterMockScript({ isConnected: true, isAllowed: true, address: MOCK_ADDRESS }),
    );
    await page.addInitScript((addr: string) => {
      // WalletConnect auto-reconnects when an address is stored.
      localStorage.setItem('astera_wallet_address', addr);
    }, MOCK_ADDRESS);
    await page.route('**/*freighter-api*', (route) => {
      route.fulfill({
        contentType: 'application/javascript',
        body: `
          export const isConnected = () => Promise.resolve({ isConnected: true });
          export const isAllowed = () => Promise.resolve({ isAllowed: true });
          export const setAllowed = () => Promise.resolve({ isAllowed: true });
          export const getAddress = () => Promise.resolve({ address: '${MOCK_ADDRESS}', error: null });
          export const signTransaction = (xdr) => Promise.resolve({ signedTxXdr: xdr + '_signed', error: null });
          export const getNetwork = () => Promise.resolve({ network: 'TESTNET', networkPassphrase: 'Test SDF Network ; September 2015' });
        `,
      });
    });

    await page.goto('/admin');

    // MOCK_ADDRESS is not the pool admin, so useAdminGuard must redirect away
    // from /admin (to /dashboard) before any admin UI renders.
    await expect(page).toHaveURL(/\/dashboard$/, { timeout: 15_000 });
  });
});
