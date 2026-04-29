import { test, expect } from '@playwright/test';
import { freighterMockScript, MOCK_ADDRESS } from './mocks/freighter';

test.describe('Wallet Connection', () => {
  test('Connect Wallet button is visible on the home page', async ({ page }) => {
    await page.goto('/');
    const connectBtn = page.getByRole('banner').getByRole('button', { name: /connect wallet/i });
    await expect(connectBtn).toBeVisible();
  });

  test('Connect Wallet button is visible in the navbar', async ({ page }) => {
    await page.goto('/dashboard');
    const connectBtn = page.getByRole('banner').getByRole('button', { name: /connect wallet/i });
    await expect(connectBtn).toBeVisible();
  });

  test('shows Freighter-not-detected error when extension is absent', async ({ page }) => {
    // Mock Freighter reporting isConnected = false
    await page.addInitScript(freighterMockScript({ isConnected: false }));
    await page.route('**/*freighter-api*', (route) => {
      route.fulfill({
        contentType: 'application/javascript',
        body: `
          export const isConnected = () => Promise.resolve({ isConnected: false });
          export const isAllowed = () => Promise.resolve({ isAllowed: false });
          export const setAllowed = () => Promise.resolve({ isAllowed: false });
          export const getAddress = () => Promise.resolve({ address: undefined, error: null });
          export const signTransaction = () => Promise.resolve({ signedTxXdr: '', error: null });
          export const getNetwork = () => Promise.resolve({ network: 'TESTNET', networkPassphrase: 'Test SDF Network ; September 2015' });
        `,
      });
    });
    await page.goto('/');

    await page
      .getByRole('banner')
      .getByRole('button', { name: /connect wallet/i })
      .click();

    await expect(page.getByRole('banner').getByRole('alert')).toHaveText(/freighter not detected/i);
  });

  test('shows connected address in navbar after successful connection', async ({ page }) => {
    // Patch the dynamic import so the mock freighter resolves correctly
    await page.addInitScript(freighterMockScript({ isConnected: true, isAllowed: true }));

    // Route the freighter bundle to return our mock
    await page.route('**/*freighter-api*', (route) => {
      route.fulfill({
        contentType: 'application/javascript',
        body: `
          export const isConnected = () => Promise.resolve({ isConnected: true });
          export const isAllowed = () => Promise.resolve({ isAllowed: true });
          export const setAllowed = () => Promise.resolve({ isAllowed: true });
          export const getAddress = () => Promise.resolve({ address: '${MOCK_ADDRESS}', error: null });
          export const signTransaction = (xdr) => Promise.resolve({ signedTxXdr: xdr + '_signed', error: null });
          export const getNetwork = () => Promise.resolve({ network: 'TESTNET', networkPassphrase: 'Test SDF Network ; September 2015' });
        `,
      });
    });

    await page.goto('/');
    await expect
      .poll(() => page.evaluate(() => Boolean((window as any).__MOCK_FREIGHTER_API__)))
      .toBe(true);
    await expect
      .poll(() =>
        page.evaluate(async () => {
          const api = (window as any).__MOCK_FREIGHTER_API__;
          const res = await api.isConnected();
          return Boolean(res?.isConnected);
        }),
      )
      .toBe(true);
    await page
      .getByRole('banner')
      .getByRole('button', { name: /connect wallet/i })
      .click();

    // After connection, the truncated address should appear.
    // Truncated: first 6 + last 4 chars of MOCK_ADDRESS (see `truncateAddress`)
    const truncated = `${MOCK_ADDRESS.slice(0, 6)}...${MOCK_ADDRESS.slice(-4)}`;
    await expect(page.getByRole('banner').getByText(truncated)).toBeVisible({ timeout: 8000 });
  });

  test('navigation links are accessible before wallet connection', async ({ page }) => {
    await page.goto('/');
    const nav = page.getByRole('navigation');
    await expect(nav.getByRole('link', { name: /dashboard/i })).toBeVisible();
    await expect(nav.getByRole('link', { name: /invest/i })).toBeVisible();
    await expect(nav.getByRole('link', { name: /new invoice/i })).toBeVisible();
  });

  test('Astera brand link navigates to home', async ({ page }) => {
    await page.goto('/invest');
    await page
      .getByRole('banner')
      .getByRole('link', { name: /astera/i })
      .click();
    await expect(page).toHaveURL('/');
  });
});
