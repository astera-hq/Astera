import { test, expect } from '@playwright/test';
import { MOCK_ADDRESS } from './mocks/freighter';

async function injectConnectedWallet(page: import('@playwright/test').Page) {
  await page.addInitScript((address: string) => {
    localStorage.setItem(
      'astera-wallet',
      JSON.stringify({
        state: { wallet: { address, connected: true, network: 'testnet' } },
        version: 0,
      }),
    );
  }, MOCK_ADDRESS);
}

test.describe('Collateral Flow', () => {
  test.skip(!!process.env.CI, 'Collateral flows require live contract setup in CI.');
  test('SME can post collateral for an invoice', async ({ page }) => {
    await injectConnectedWallet(page);

    // Mock invoice data
    await page.route('**/api/invoices/1', (route) => {
      route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify({ id: 1, status: 'Pending', amount: 1000 }),
      });
    });

    await page.goto('/invoice/1');

    const collateralBtn = page.getByRole('button', { name: /add collateral/i });
    await expect(collateralBtn).toBeVisible();
    await collateralBtn.click();

    await page.locator('input[name="collateralAmount"]').fill('500');
    await page.getByRole('button', { name: /confirm/i }).click();

    await expect(page.getByText(/collateral posted/i)).toBeVisible();
  });

  test('Collateral is released after repayment', async ({ page }) => {
    await injectConnectedWallet(page);

    // Mock paid invoice with collateral
    await page.route('**/api/invoices/1', (route) => {
      route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify({ id: 1, status: 'Paid', amount: 1000, collateralReleased: false }),
      });
    });

    await page.goto('/invoice/1');

    const releaseBtn = page.getByRole('button', { name: /release collateral/i });
    await expect(releaseBtn).toBeVisible();
    await releaseBtn.click();

    await expect(page.getByText(/collateral released/i)).toBeVisible();
  });

  test('SME deposits collateral for an invoice above the threshold', async ({ page }) => {
    await injectConnectedWallet(page);

    // Mock invoice with large amount (above collateral threshold)
    await page.route('**/api/invoices/2', (route) => {
      route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify({
          id: 2,
          status: 'Pending',
          amount: 100000,
          owner: MOCK_ADDRESS,
          threshold: 50000,
        }),
      });
    });

    await page.goto('/invoice/2');

    // Verify collateral requirement section is visible
    const collateralSection = page.getByRole('heading', { name: /collateral/i });
    await expect(collateralSection).toBeVisible();

    // Verify the required amount is displayed
    await expect(page.getByText(/required/i)).toBeVisible();

    // Simulate deposit transaction
    const depositInput = page.locator('input[placeholder*="required" i]');
    await expect(depositInput).toBeVisible();
    await depositInput.fill('5000');

    await page.getByRole('button', { name: /post collateral/i }).click();

    // Verify success toast
    await expect(page.getByText(/collateral posted successfully/i)).toBeVisible({ timeout: 10000 });

    // Verify the posted amount is shown
    await expect(page.getByText('5000')).toBeVisible();
  });
});
