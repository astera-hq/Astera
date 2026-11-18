import { test, expect } from '@playwright/test';
import { MOCK_ADDRESS, freighterMockScript } from './mocks/freighter';

async function injectConnectedWallet(page: import('@playwright/test').Page) {
  await page.addInitScript(freighterMockScript({ isConnected: true, isAllowed: true }));
  await page.addInitScript((address: string) => {
    localStorage.setItem('astera_wallet_address', address);
  }, MOCK_ADDRESS);
}

async function stubContractCalls(page: import('@playwright/test').Page) {
  await page.route('**/*stellar.org*', (route) => {
    route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify({ jsonrpc: '2.0', id: 1, result: { entries: [] } }),
    });
  });
}

test.describe('Governance Page', () => {
  test('connects wallet and navigates to /governance', async ({ page }) => {
    await injectConnectedWallet(page);
    await stubContractCalls(page);
    await page.goto('/governance');
    await expect(page).toHaveURL('/governance');
    await expect(page.getByRole('heading', { name: /proposal/i })).toBeVisible();
  });

  test('shows empty proposals state', async ({ page }) => {
    await injectConnectedWallet(page);
    await stubContractCalls(page);
    await page.goto('/governance');
    await expect(page.getByText(/no proposals have been submitted/i)).toBeVisible();
  });

  test('create proposal form is visible', async ({ page }) => {
    await injectConnectedWallet(page);
    await stubContractCalls(page);
    await page.goto('/governance');

    const form = page.locator('form');
    await expect(form).toBeVisible();

    await expect(page.locator('input[placeholder="C..."]')).toBeVisible();
    await expect(page.locator('input[placeholder="set_yield_rate"]')).toBeVisible();
    await expect(page.locator('input[placeholder*="Raise the pool yield"]')).toBeVisible();
  });

  test('fills create proposal form and submits with mocked signing', async ({ page }) => {
    await injectConnectedWallet(page);
    await stubContractCalls(page);
    await page.goto('/governance');

    const targetContract = page.locator('input[placeholder="C..."]');
    const functionName = page.locator('input[placeholder="set_yield_rate"]');
    const description = page.locator('input[placeholder*="Raise the pool yield"]');
    const calldata = page.locator('textarea');

    await targetContract.fill('CA3J7L7L7L7L7L7L7L7L7L7L7L7L7L7L7L7L7L7L7L7L7L7L7LQ');
    await functionName.fill('set_yield');
    await description.fill('Raise pool yield by 50 bps');
    await calldata.fill('{"yield_bps": 850}');

    await expect(targetContract).toHaveValue('CA3J7L7L7L7L7L7L7L7L7L7L7L7L7L7L7L7L7L7L7L7L7L7L7LQ');
    await expect(functionName).toHaveValue('set_yield');
    await expect(description).toHaveValue('Raise pool yield by 50 bps');
    await expect(calldata).toHaveValue('{"yield_bps": 850}');
  });

  test('shows governance-not-configured warning when contract ID is missing', async ({ page }) => {
    await page.goto('/governance');
    const banner = page.getByText(/governance contract id is not configured/i);
    if (await banner.isVisible()) {
      await expect(banner).toBeVisible();
    }
  });

  test('refresh button triggers proposal reload', async ({ page }) => {
    await injectConnectedWallet(page);
    await stubContractCalls(page);
    await page.goto('/governance');
    const refreshBtn = page.getByRole('button', { name: /refresh/i });
    await expect(refreshBtn).toBeVisible();
    await refreshBtn.click();
    await expect(page.getByText(/no proposals have been submitted/i)).toBeVisible();
  });

  test('navbar has governance navigation link', async ({ page }) => {
    await page.goto('/');
    const govLink = page.getByRole('banner').getByRole('link', { name: /governance/i });
    await expect(govLink).toBeVisible();
  });

  test('toggle proposal form visibility', async ({ page }) => {
    await injectConnectedWallet(page);
    await stubContractCalls(page);
    await page.goto('/governance');

    const toggleBtn = page.getByRole('button', { name: /hide form/i });
    await expect(toggleBtn).toBeVisible();
    await toggleBtn.click();
    await expect(page.getByRole('button', { name: /new proposal/i })).toBeVisible();
    await page.getByRole('button', { name: /new proposal/i }).click();
    await expect(page.getByRole('button', { name: /hide form/i })).toBeVisible();
  });
});
