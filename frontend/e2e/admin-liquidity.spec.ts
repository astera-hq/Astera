import { test, expect } from '@playwright/test';
import { freighterMockScript, MOCK_ADDRESS } from './mocks/freighter';
import { stubAdminDashboardContracts } from './mocks/soroban-admin-dashboard';

async function injectAdminWallet(page: import('@playwright/test').Page) {
  await page.addInitScript(freighterMockScript({ isConnected: true, isAllowed: true }));
  await page.addInitScript((address: string) => {
    localStorage.setItem('astera_wallet_address', address);
  }, MOCK_ADDRESS);
}

// #865: admin liquidity forecast dashboard — projected liquidity vs queued
// withdrawal demand, per token, with a manual "drain now" action.
test.describe('Admin liquidity dashboard', () => {
  test.skip(!!process.env.CI, 'Admin dashboard contract mocks need local Soroban stubs in CI.');

  test('renders per-token liquidity stats, forecast chart, and pending queue', async ({
    page,
  }) => {
    await injectAdminWallet(page);
    await stubAdminDashboardContracts(page, MOCK_ADDRESS);

    await page.goto('/admin/liquidity');

    await expect(page.getByRole('heading', { name: 'Liquidity Forecast' })).toBeVisible({
      timeout: 15_000,
    });

    // Summary stat cards for the one mocked token.
    await expect(page.getByText('Available Liquidity')).toBeVisible();
    await expect(page.getByText('Queue Depth')).toBeVisible();
    await expect(page.getByText('Projected (end of horizon)')).toBeVisible();

    // Two pending requests are mocked.
    await expect(page.getByRole('heading', { name: 'Pending requests' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Try drain now' })).toBeVisible();
    const rows = page.locator('tbody tr');
    await expect(rows).toHaveCount(2);
  });

  test('horizon selector switches between 30/60/90 day views', async ({ page }) => {
    await injectAdminWallet(page);
    await stubAdminDashboardContracts(page, MOCK_ADDRESS);

    await page.goto('/admin/liquidity');
    await expect(page.getByRole('heading', { name: 'Liquidity Forecast' })).toBeVisible({
      timeout: 15_000,
    });

    const sixtyDayButton = page.getByRole('button', { name: '60d' });
    await expect(sixtyDayButton).toBeVisible();
    await sixtyDayButton.click();

    await expect(page.getByText('60-Day Liquidity Forecast')).toBeVisible();
  });

  test('liquidity link is present in the admin nav', async ({ page }) => {
    await injectAdminWallet(page);
    await stubAdminDashboardContracts(page, MOCK_ADDRESS);

    await page.goto('/admin/liquidity');
    await expect(page.getByRole('heading', { name: 'Liquidity Forecast' })).toBeVisible({
      timeout: 15_000,
    });

    await expect(page.getByRole('link', { name: 'Liquidity' })).toBeVisible();
  });
});
