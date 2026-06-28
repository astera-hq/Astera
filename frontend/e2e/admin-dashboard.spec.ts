import { test, expect } from '@playwright/test';
import { freighterMockScript, MOCK_ADDRESS } from './mocks/freighter';
import { stubAdminDashboardContracts } from './mocks/soroban-admin-dashboard';

async function injectAdminWallet(page: import('@playwright/test').Page) {
  await page.addInitScript(freighterMockScript({ isConnected: true, isAllowed: true }));
  await page.addInitScript((address: string) => {
    localStorage.setItem('astera_wallet_address', address);
  }, MOCK_ADDRESS);
}

test.describe('Admin dashboard (#242)', () => {
  test.skip(!!process.env.CI, 'Admin dashboard contract mocks need local Soroban stubs in CI.');

  test('loads protocol stats and requires confirmation before pause toggle', async ({ page }) => {
    await injectAdminWallet(page);
    await stubAdminDashboardContracts(page, MOCK_ADDRESS);

    await page.goto('/admin/dashboard');

    await expect(page.getByRole('heading', { name: 'Protocol Dashboard' })).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByText('Total Value Locked')).toBeVisible();
    await expect(page.getByText('Active Invoices')).toBeVisible();
    await expect(page.getByTestId('protocol-pause-control')).toBeVisible();
    await expect(page.getByTestId('protocol-pause-status')).toHaveText('Status: Active');

    await page.getByTestId('protocol-pause-toggle').click();
    await expect(page.getByRole('dialog')).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Pause protocol?' })).toBeVisible();

    await page.getByRole('button', { name: 'Cancel' }).click();
    await expect(page.getByRole('dialog')).not.toBeVisible();
    await expect(page.getByTestId('protocol-pause-status')).toHaveText('Status: Active');
  });
});
