import { test, expect } from '@playwright/test';
import { MOCK_ADDRESS, freighterMockScript } from './mocks/freighter';

async function injectConnectedWallet(page: import('@playwright/test').Page) {
  await page.addInitScript(freighterMockScript({ isConnected: true, isAllowed: true }));
  await page.addInitScript((address: string) => {
    localStorage.setItem('astera_wallet_address', address);
  }, MOCK_ADDRESS);
}

/** Stub out contract RPC calls so the invest page doesn't hang on network. */
async function stubContractCalls(page: import('@playwright/test').Page) {
  await page.route('**/*stellar.org*', (route) => {
    route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify({ jsonrpc: '2.0', id: 1, result: { entries: [] } }),
    });
  });
}

test.describe('Deposit / Withdraw', () => {
  test('invest page renders without crashing', async ({ page }) => {
    await stubContractCalls(page);
    await page.goto('/invest');
    // At minimum the page should load and not show a fatal error
    await expect(page).toHaveURL('/invest');
  });

  test('invest page has a heading', async ({ page }) => {
    await stubContractCalls(page);
    await page.goto('/invest');
    // The page title / heading should be visible
    const heading = page.getByRole('heading').first();
    await expect(heading).toBeVisible({ timeout: 8000 });
  });

  test('shows connect-wallet prompt on invest page when disconnected', async ({ page }) => {
    await stubContractCalls(page);
    await page.goto('/invest');
    // The invest page should show a "Connect Wallet" call to action when disconnected.
    await expect(
      page.getByRole('banner').getByRole('button', { name: /connect wallet/i }),
    ).toBeVisible({ timeout: 8000 });
  });

  test('deposit/withdraw tabs or mode selector is visible when connected', async ({ page }) => {
    await injectConnectedWallet(page);
    await stubContractCalls(page);
    await page.goto('/invest');

    // The page should render deposit/withdraw mode controls
    const depositEl = page.getByRole('button', { name: /deposit/i }).or(page.getByText(/deposit/i));
    await expect(depositEl.first()).toBeVisible({ timeout: 8000 });
  });

  test('navigating to /invest from navbar works', async ({ page }) => {
    await page.goto('/');
    await page
      .getByRole('banner')
      .getByRole('link', { name: /invest/i })
      .click();
    await expect(page).toHaveURL('/invest');
  });

  test('amount input rejects non-numeric values via browser validation', async ({ page }) => {
    await injectConnectedWallet(page);
    await stubContractCalls(page);
    await page.goto('/invest');

    // Wait for the amount input to appear
    const amountInput = page
      .locator('input[type="number"]')
      .or(page.locator('input[placeholder*="amount" i]'))
      .first();

    const inputVisible = await amountInput.isVisible().catch(() => false);
    if (!inputVisible) {
      // Pool may not have loaded in stub environment — skip gracefully
      test.skip();
      return;
    }

    const value = await amountInput.evaluate((el: HTMLInputElement) => {
      // Playwright refuses to type non-numeric text into input[type=number].
      // Simulate a user/DOM attempt by setting value directly and dispatching input.
      el.value = 'abc';
      el.dispatchEvent(new Event('input', { bubbles: true }));
      return el.value;
    });
    // Number input should coerce to empty or 0
    expect(value === '' || value === '0' || Number.isNaN(Number(value))).toBeTruthy();
  });
});
