import { test, expect } from '@playwright/test';
import { freighterMockScript, MOCK_ADDRESS } from './mocks/freighter';
import { stubAdminDashboardContracts } from './mocks/soroban-admin-dashboard';

async function injectAdminWallet(page: import('@playwright/test').Page) {
  await page.addInitScript(freighterMockScript({ isConnected: true, isAllowed: true }));
  await page.addInitScript((address: string) => {
    localStorage.setItem('astera_wallet_address', address);
  }, MOCK_ADDRESS);
}

// #861: N-of-M staked oracle consensus network admin surface.
test.describe('Admin oracle network', () => {
  test.skip(!!process.env.CI, 'Admin dashboard contract mocks need local Soroban stubs in CI.');

  test('renders registry config and registered oracles', async ({ page }) => {
    await injectAdminWallet(page);
    await stubAdminDashboardContracts(page, MOCK_ADDRESS);

    await page.goto('/admin/oracles');

    await expect(page.getByRole('heading', { name: 'Oracle Network' })).toBeVisible({
      timeout: 15_000,
    });

    // Registry config summary stats.
    await expect(page.getByText('Min Stake')).toBeVisible();
    await expect(page.getByText('Quorum')).toBeVisible();
    await expect(page.getByText('66.0%')).toBeVisible();
    await expect(page.getByText('Required Votes')).toBeVisible();

    // Registered oracles table — mocked with 2 addresses, both resolving to
    // the same canned OracleInfo record.
    await expect(page.getByRole('heading', { name: 'Registered Oracles' })).toBeVisible();
    await expect(page.getByText(MOCK_ADDRESS)).toBeVisible();
    await expect(page.getByRole('button', { name: 'Slash' }).first()).toBeVisible();
  });

  test('slash action requires the confirmation phrase', async ({ page }) => {
    await injectAdminWallet(page);
    await stubAdminDashboardContracts(page, MOCK_ADDRESS);

    await page.goto('/admin/oracles');
    await expect(page.getByRole('heading', { name: 'Oracle Network' })).toBeVisible({
      timeout: 15_000,
    });

    await page.getByRole('button', { name: 'Slash' }).first().click();
    await expect(page.getByRole('dialog')).toBeVisible();
    await expect(page.getByRole('heading', { name: /Slash G/ })).toBeVisible();

    // Confirm button stays disabled until the exact phrase is typed —
    // ConfirmActionModal's shared destructive-action safeguard.
    const confirmButton = page.getByRole('button', { name: 'Slash Oracle' });
    await expect(confirmButton).toBeDisabled();

    await page.getByRole('button', { name: 'Cancel' }).click();
    await expect(page.getByRole('dialog')).not.toBeVisible();
  });

  test('round lookup shows a not-found state for an unknown invoice', async ({ page }) => {
    await injectAdminWallet(page);
    await stubAdminDashboardContracts(page, MOCK_ADDRESS);

    await page.goto('/admin/oracles');
    await expect(page.getByRole('heading', { name: 'Oracle Network' })).toBeVisible({
      timeout: 15_000,
    });

    await page.getByPlaceholder('Invoice ID').fill('999999');
    await page.getByRole('button', { name: 'Look up' }).click();

    await expect(
      page.getByText('No verification round found for that invoice.'),
    ).toBeVisible({ timeout: 10_000 });
  });
});
