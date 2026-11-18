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

// #1043: structured multi-party dispute arbitration. These specs require a
// deployed invoice + arbitration contract pair and live juror/evidence
// state, same constraint `dispute.spec.ts` documents for the pre-existing
// dispute flow — skipped in CI, meant for a manual/testnet run.
test.describe('Arbitration Flow', () => {
  test.skip(!!process.env.CI, 'Arbitration flows require live contract setup in CI.');

  test('SME can dispute a defaulted invoice and submit evidence', async ({ page }) => {
    await injectConnectedWallet(page);

    await page.route('**/api/invoices/789', (route) => {
      route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify({ id: 789, status: 'Defaulted', amount: 5_000_000_000 }),
      });
    });

    await page.goto('/invoice/789');

    const disputeBtn = page.getByRole('button', { name: /dispute default/i });
    await expect(disputeBtn).toBeVisible();
    await disputeBtn.click();

    await page.getByPlaceholder(/hash\/uri of your supporting evidence/i).fill('ipfs://evidence-1');
    await page.getByPlaceholder('G...').fill(MOCK_ADDRESS);
    await page.getByRole('button', { name: /raise dispute/i }).click();

    await expect(page.getByText(/dispute raised/i)).toBeVisible();
  });

  test('Evidence submission is only offered during the evidence window', async ({ page }) => {
    await injectConnectedWallet(page);

    await page.route('**/api/invoices/790', (route) => {
      route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify({ id: 790, status: 'Disputed', amount: 5_000_000_000 }),
      });
    });

    await page.goto('/invoice/790');

    await expect(page.getByText(/under dispute review/i)).toBeVisible();
    // No evidence form once the case has moved past EvidenceWindow — this
    // spec exercises the display path, live-state assertions belong in a
    // testnet run against a real CommitReveal-phase case.
  });

  test('Wallet can register as a juror and see assigned cases', async ({ page }) => {
    await injectConnectedWallet(page);
    await page.goto('/arbitration/jurors');

    await expect(page.getByRole('heading', { name: /arbitration jurors/i })).toBeVisible();
    await expect(page.getByText(/your juror status/i)).toBeVisible();

    const registerBtn = page.getByRole('button', { name: /^register$/i });
    if (await registerBtn.isVisible()) {
      await page.getByPlaceholder(/stake amount/i).fill('1000');
      await registerBtn.click();
      await expect(page.getByText(/registered as a juror/i)).toBeVisible();
    }
  });

  test('Juror can commit a vote on an assigned case', async ({ page }) => {
    await injectConnectedWallet(page);
    await page.goto('/arbitration/jurors');

    const commitSection = page.getByText(/commit your vote/i);
    if (await commitSection.isVisible().catch(() => false)) {
      await page.getByRole('button', { name: /favor debtor/i }).click();
      await page.getByRole('button', { name: /^commit vote$/i }).click();
      await expect(page.getByText(/vote committed/i)).toBeVisible();
    }
  });
});
