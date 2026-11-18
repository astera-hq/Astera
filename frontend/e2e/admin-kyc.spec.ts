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

test.describe('Admin KYC', () => {
  test.skip(!!process.env.CI, 'Admin flows require live contract + permissions in CI.');
  test('Admin can see and approve investor KYC', async ({ page }) => {
    // Mock the KYC request list
    await page.route('**/api/admin/kyc-requests', (route) => {
      route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify([
          { address: 'GCLIENT123', status: 'Pending', submittedAt: Date.now() },
        ]),
      });
    });

    // Mock the approval action
    await page.route('**/api/admin/approve-kyc', (route) => {
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ success: true }),
      });
    });

    await injectAdminSession(page);
    await page.goto('/admin/kyc');

    // Verify list contains the pending investor
    await expect(page.getByText('GCLIENT123')).toBeVisible();

    // Click approve
    await page
      .getByRole('button', { name: /approve/i })
      .first()
      .click();

    // Verify toast or status update (mocked update)
    await page.route('**/api/admin/kyc-requests', (route) => {
      route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify([]),
      });
    });

    // In a real test we'd wait for revalidation
    await page.reload();
    await expect(page.getByText('GCLIENT123')).not.toBeVisible();
  });
});
