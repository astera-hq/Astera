import { test, expect } from '@playwright/test';

const ONBOARDING_STORAGE_KEY = 'astera-onboarding-completed';

test.describe('Onboarding flow', () => {
  test.beforeEach(async ({ page }) => {
    // Start every test as a genuine first-time visitor.
    await page.addInitScript((key) => {
      window.localStorage.removeItem(key);
    }, ONBOARDING_STORAGE_KEY);
  });

  test('shows the onboarding modal with role selection on first visit', async ({ page }) => {
    await page.goto('/dashboard');

    const dialog = page.getByRole('dialog', { name: /get started/i });
    await expect(dialog).toBeVisible();
    await expect(
      dialog.getByRole('button', { name: /I am an SME seeking invoice financing/i }),
    ).toBeVisible();
    await expect(
      dialog.getByRole('button', { name: /I am an investor looking to earn yield/i }),
    ).toBeVisible();
  });

  test('completes the full borrower (SME) onboarding flow and highlights real UI elements', async ({
    page,
  }) => {
    await page.goto('/dashboard');
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible();

    await dialog.getByRole('button', { name: /I am an SME seeking invoice financing/i }).click();
    await expect(dialog.getByText('Welcome to Astera')).toBeVisible();
    await expect(page.getByText('Step 1 of 5', { exact: false })).toBeVisible();

    // Step 2: Connect Your Wallet — should spotlight the real connect button.
    await dialog.getByRole('button', { name: /^next$/i }).click();
    await expect(dialog.getByText('Connect Your Wallet')).toBeVisible();
    const walletTarget = page.locator('[data-onboarding-id="wallet-connect"]');
    await expect(walletTarget).toBeVisible();

    // Step 3: Create Your First Invoice — spotlights the invoice nav link.
    await dialog.getByRole('button', { name: /^next$/i }).click();
    await expect(dialog.getByText('Create Your First Invoice')).toBeVisible();
    await expect(page.locator('[data-onboarding-id="nav-invoice"]').first()).toBeVisible();

    // Step 4 and 5.
    await dialog.getByRole('button', { name: /^next$/i }).click();
    await expect(dialog.getByText('Wait for Verification')).toBeVisible();
    await dialog.getByRole('button', { name: /^next$/i }).click();
    await expect(dialog.getByText('Receive Your Funds')).toBeVisible();

    await dialog.getByRole('button', { name: /get started/i }).click();
    await expect(dialog).not.toBeVisible();

    const completed = await page.evaluate(
      (key) => window.localStorage.getItem(key),
      ONBOARDING_STORAGE_KEY,
    );
    expect(completed).toBe('true');

    // Reloading must not show the tour again for a completed user.
    await page.reload();
    await expect(page.getByRole('dialog')).toHaveCount(0);
  });

  test('"Skip tour" dismisses onboarding and it does not reappear on reload', async ({ page }) => {
    await page.goto('/dashboard');
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible();

    await dialog.getByRole('button', { name: /skip tour/i }).click();
    await expect(dialog).not.toBeVisible();

    await page.reload();
    await expect(page.getByRole('dialog')).toHaveCount(0);
  });
});
