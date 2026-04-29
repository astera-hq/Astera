import { test, expect } from '@playwright/test';
import { MOCK_ADDRESS, freighterMockScript } from './mocks/freighter';

/** Inject a connected wallet state into Zustand store via localStorage. */
async function injectConnectedWallet(page: import('@playwright/test').Page) {
  await page.addInitScript(freighterMockScript({ isConnected: true, isAllowed: true }));
  await page.addInitScript((address: string) => {
    localStorage.setItem('astera_wallet_address', address);
  }, MOCK_ADDRESS);
}

test.describe('Invoice Creation', () => {
  test('invoice creation page renders heading', async ({ page }) => {
    await page.goto('/invoice/new');
    await expect(page.getByRole('heading', { name: /tokenize invoice/i })).toBeVisible();
  });

  test('shows connect-wallet prompt when wallet is not connected', async ({ page }) => {
    await page.goto('/invoice/new');
    await expect(page.getByText(/connect your wallet first/i)).toBeVisible();
  });

  test('form renders all required fields when wallet is connected', async ({ page }) => {
    await injectConnectedWallet(page);
    await page.goto('/invoice/new');
    await expect(page.locator('form')).toBeVisible({ timeout: 8000 });

    await expect(page.locator('input[name="debtor"]')).toBeVisible();
    await expect(page.locator('input[name="amount"]')).toBeVisible();
    await expect(page.locator('input[name="dueDate"]')).toBeVisible();
    await expect(page.locator('textarea[name="description"]')).toBeVisible();
  });

  test('submit button is present when wallet is connected', async ({ page }) => {
    await injectConnectedWallet(page);
    await page.goto('/invoice/new');
    await expect(page.locator('form')).toBeVisible({ timeout: 8000 });

    await expect(
      page.getByRole('button', { name: /mint invoice token|mint|token/i }),
    ).toBeVisible();
  });

  test('form prevents submission with empty required fields', async ({ page }) => {
    await injectConnectedWallet(page);
    await page.goto('/invoice/new');
    await expect(page.locator('form')).toBeVisible({ timeout: 8000 });

    // Click submit without filling in anything
    const submitBtn = page.getByRole('button', { name: /mint invoice token/i });
    await submitBtn.click({ force: true });

    // App-level validation should show required field errors.
    await expect(page.getByText(/debtor name is required/i)).toBeVisible({ timeout: 8000 });
    await expect(page.getByText(/amount is required/i)).toBeVisible({ timeout: 8000 });
    await expect(page.getByText(/due date is required/i)).toBeVisible({ timeout: 8000 });
  });

  test('navigating to /invoice/new from navbar works', async ({ page }) => {
    await page.goto('/');
    await page
      .getByRole('banner')
      .getByRole('link', { name: /new invoice/i })
      .click();
    await expect(page).toHaveURL('/invoice/new');
    await expect(page.getByRole('heading', { name: /tokenize invoice/i })).toBeVisible();
  });
});
