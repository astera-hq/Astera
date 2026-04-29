import { test, expect } from '@playwright/test';
import { MOCK_ADDRESS } from './mocks/freighter';

async function injectAdminSession(page: import('@playwright/test').Page) {
  await page.addInitScript((address: string) => {
    localStorage.setItem(
      'astera-wallet',
      JSON.stringify({
        state: { wallet: { address, connected: true, network: 'testnet', isAdmin: true } },
        version: 0,
      }),
    );
  }, MOCK_ADDRESS);
}

test.describe('Default Flow', () => {
  test.skip(!!process.env.CI, 'Admin flows require live contract + permissions in CI.');
  test('Admin can mark invoice as defaulted', async ({ page }) => {
    await injectAdminSession(page);

    // Mock overdue invoice
    await page.route('**/api/admin/overdue-invoices', (route) => {
      route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify([
          { id: 123, debtor: 'Bad SME', amount: 5000, dueDate: Date.now() - 86400000 },
        ]),
      });
    });

    await page.goto('/admin/defaults');

    await expect(page.getByText('Bad SME')).toBeVisible();

    const defaultBtn = page.getByRole('button', { name: /mark defaulted/i });
    await defaultBtn.click();

    // Verify UI update
    await expect(page.getByText(/status: defaulted/i)).toBeVisible();
  });
});
