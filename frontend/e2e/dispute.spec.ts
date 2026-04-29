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

test.describe('Dispute Flow', () => {
  test.skip(!!process.env.CI, 'Dispute flows require live contract setup in CI.');
  test('Investor can raise a dispute on a funded invoice', async ({ page }) => {
    await injectConnectedWallet(page);

    // Mock funded invoice
    await page.route('**/api/invoices/456', (route) => {
      route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify({ id: 456, status: 'Funded', amount: 2000 }),
      });
    });

    await page.goto('/invoice/456');

    const disputeBtn = page.getByRole('button', { name: /raise dispute/i });
    await expect(disputeBtn).toBeVisible();
    await disputeBtn.click();

    await page.locator('textarea[name="reason"]').fill('Invoice data seems fraudulent.');
    await page.getByRole('button', { name: /submit dispute/i }).click();

    await expect(page.getByText(/dispute raised/i)).toBeVisible();
    await expect(page.getByText(/status: disputed/i)).toBeVisible();
  });
});
