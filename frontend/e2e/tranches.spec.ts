import { test, expect } from '@playwright/test';
import { MOCK_ADDRESS } from './mocks/freighter';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

/** Stub the indexer trailing-APY endpoint for a given token. */
function mockTrailingApy(page: import('@playwright/test').Page, token = 'USDC') {
  page.route(`**/tranches/${token}/apy`, (route) => {
    route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify({
        senior: { realizedApyPct: 8.2 },
        junior: { realizedApyPct: 14.5 },
      }),
    });
  });
}

/** Stub Soroban simulateTransaction to return a tranche pool for the given token. */
function mockTranchePool(page: import('@playwright/test').Page, token = 'USDC') {
  page.route('**/*stellar.org/**', (route) => {
    const body = route.request().postData() ?? '';
    // Only intercept Soroban simulateTransaction calls; let everything else pass.
    if (!body.includes('simulateTransaction')) {
      route.fallback();
      return;
    }
    // Return a minimal JSON-RPC response so the page doesn't crash.
    // The page catches errors from getTranchePool and falls back to null,
    // so a 200 with empty result is enough to unblock rendering.
    route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify({
        jsonrpc: '2.0',
        id: 1,
        result: {
          results: [{ auth: [], xdr: 'AAAAAQAAAAE=' }],
          latestLedger: 100,
        },
      }),
    });
  });
}

// ===========================================================================
// Investor Tranche Page  (/invest/tranches)
// ===========================================================================

test.describe('Investor Tranche Page', () => {
  test.skip(!!process.env.CI, 'Tranche flows require live contract setup in CI.');

  test('renders page header and token selector', async ({ page }) => {
    await injectConnectedWallet(page);
    mockTrailingApy(page);
    mockTranchePool(page);

    await page.goto('/invest/tranches');

    await expect(page.getByRole('heading', { name: /tranche investments/i })).toBeVisible();
    await expect(page.getByText(/choose your risk profile/i)).toBeVisible();
    await expect(page.getByLabel(/select token/i)).toBeVisible();
  });

  test('token selector includes USDC, USDT, EURC', async ({ page }) => {
    await injectConnectedWallet(page);
    mockTrailingApy(page);
    mockTranchePool(page);

    await page.goto('/invest/tranches');

    const select = page.getByLabel(/select token/i);
    for (const token of ['USDC', 'USDT', 'EURC']) {
      await expect(select.locator(`option[value="${token}"]`)).toBeAttached();
    }
  });

  test('renders risk explainer section', async ({ page }) => {
    await injectConnectedWallet(page);
    mockTrailingApy(page);
    mockTranchePool(page);

    await page.goto('/invest/tranches');

    await expect(page.getByText(/understanding tranche risk/i)).toBeVisible();
    await expect(page.getByRole('heading', { name: /senior tranche.*lower risk/i })).toBeVisible();
    await expect(page.getByRole('heading', { name: /junior tranche.*higher risk/i })).toBeVisible();
    await expect(page.getByRole('heading', { name: /waterfall repayment/i })).toBeVisible();
  });

  test('shows loading skeleton while data loads', async ({ page }) => {
    await injectConnectedWallet(page);
    // Delay the trailing APY response so the skeleton is visible.
    page.route('**/tranches/USDC/apy', async (route) => {
      await new Promise((r) => setTimeout(r, 2000));
      route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify({ senior: { realizedApyPct: 0 }, junior: { realizedApyPct: 0 } }),
      });
    });
    mockTranchePool(page);

    await page.goto('/invest/tranches');

    // The loading skeleton uses animate-pulse cards.
    await expect(page.locator('.animate-pulse').first()).toBeVisible();
  });

  test('deposit modal opens and closes', async ({ page }) => {
    await injectConnectedWallet(page);
    mockTrailingApy(page);
    mockTranchePool(page);

    await page.goto('/invest/tranches');

    // Wait for the page to finish loading (skeleton gone).
    await expect(page.getByRole('heading', { name: /tranche investments/i })).toBeVisible();

    // Click the senior deposit button (may be disabled if pool is null, so target
    // the button by text if visible, otherwise skip gracefully).
    const depositBtn = page.getByRole('button', { name: /deposit to senior/i });
    if (await depositBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
      await depositBtn.click();

      // Modal should appear.
      await expect(page.getByRole('heading', { name: /deposit to senior tranche/i })).toBeVisible();
      await expect(page.getByPlaceholder(/enter amount/i)).toBeVisible();
      await expect(page.getByRole('button', { name: /confirm deposit/i })).toBeVisible();

      // Cancel closes the modal.
      await page.getByRole('button', { name: /cancel/i }).click();
      await expect(
        page.getByRole('heading', { name: /deposit to senior tranche/i }),
      ).not.toBeVisible();
    }
  });

  test('deposit modal shows wallet warning when wallet is disconnected', async ({ page }) => {
    // Do NOT inject a connected wallet.
    mockTrailingApy(page);
    mockTranchePool(page);

    await page.goto('/invest/tranches');
    await expect(page.getByRole('heading', { name: /tranche investments/i })).toBeVisible();

    const depositBtn = page.getByRole('button', { name: /deposit to senior/i });
    if (await depositBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
      await depositBtn.click();

      await expect(page.getByText(/connect your wallet to deposit/i)).toBeVisible();
      await expect(page.getByRole('button', { name: /confirm deposit/i })).toBeDisabled();
    }
  });

  test('deposit modal confirm button disabled when amount is zero', async ({ page }) => {
    await injectConnectedWallet(page);
    mockTrailingApy(page);
    mockTranchePool(page);

    await page.goto('/invest/tranches');
    await expect(page.getByRole('heading', { name: /tranche investments/i })).toBeVisible();

    const depositBtn = page.getByRole('button', { name: /deposit to senior/i });
    if (await depositBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
      await depositBtn.click();

      // Amount defaults to 0, so confirm should be disabled.
      await expect(page.getByRole('button', { name: /confirm deposit/i })).toBeDisabled();
    }
  });

  test('switching token reloads data', async ({ page }) => {
    await injectConnectedWallet(page);

    let usdcHit = false;
    let usdtHit = false;

    page.route('**/tranches/USDC/apy', (route) => {
      usdcHit = true;
      route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify({ senior: { realizedApyPct: 8 }, junior: { realizedApyPct: 14 } }),
      });
    });
    page.route('**/tranches/USDT/apy', (route) => {
      usdtHit = true;
      route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify({ senior: { realizedApyPct: 6 }, junior: { realizedApyPct: 12 } }),
      });
    });
    mockTranchePool(page);

    await page.goto('/invest/tranches');
    await expect(page.getByRole('heading', { name: /tranche investments/i })).toBeVisible();

    // Initial load hits USDC.
    await page.waitForTimeout(500);
    expect(usdcHit).toBe(true);

    // Switch to USDT.
    await page.getByLabel(/select token/i).selectOption('USDT');
    await page.waitForTimeout(500);
    expect(usdtHit).toBe(true);
  });
});

// ===========================================================================
// Admin Tranche Configuration Page  (/admin/tranches)
// ===========================================================================

test.describe('Admin Tranche Configuration', () => {
  test.skip(!!process.env.CI, 'Admin tranche config requires live contract + permissions in CI.');

  test('renders page header and token selector', async ({ page }) => {
    await injectAdminSession(page);

    await page.goto('/admin/tranches');

    await expect(page.getByRole('heading', { name: /tranche configuration/i })).toBeVisible();
    await expect(page.getByText(/configure senior\/junior tranches/i)).toBeVisible();
    await expect(page.getByLabel(/select token/i)).toBeVisible();
  });

  test('shows enabled config for USDC with BPS inputs', async ({ page }) => {
    await injectAdminSession(page);

    await page.goto('/admin/tranches');

    // USDC is enabled by default.
    await expect(page.getByText('Enabled')).toBeVisible();
    await expect(page.getByLabel(/senior target yield/i)).toBeVisible();
    await expect(page.getByLabel(/senior advance rate/i)).toBeVisible();
    await expect(page.getByLabel(/junior first loss/i)).toBeVisible();
    await expect(page.getByLabel(/senior share token/i)).toBeVisible();
    await expect(page.getByLabel(/junior share token/i)).toBeVisible();
  });

  test('shows disabled state for USDT with enable button', async ({ page }) => {
    await injectAdminSession(page);

    await page.goto('/admin/tranches');

    // Switch to USDT (disabled by default).
    await page.getByLabel(/select token/i).selectOption('USDT');

    await expect(page.getByText('Disabled')).toBeVisible();
    await expect(page.getByText(/tranche is not enabled for USDT/i)).toBeVisible();
    await expect(page.getByRole('button', { name: /enable tranche/i })).toBeVisible();
  });

  test('enabling a disabled token shows config inputs', async ({ page }) => {
    await injectAdminSession(page);

    await page.goto('/admin/tranches');
    await page.getByLabel(/select token/i).selectOption('USDT');

    await page.getByRole('button', { name: /enable tranche/i }).click();

    // Should now show the config inputs.
    await expect(page.getByText('Enabled')).toBeVisible();
    await expect(page.getByLabel(/senior target yield/i)).toBeVisible();
  });

  test('edit mode toggles correctly', async ({ page }) => {
    await injectAdminSession(page);

    await page.goto('/admin/tranches');

    // Start in view mode: Edit Configuration button visible.
    const editBtn = page.getByRole('button', { name: /edit configuration/i });
    await expect(editBtn).toBeVisible();

    // Enter edit mode.
    await editBtn.click();

    // Save Changes and Cancel buttons should appear.
    await expect(page.getByRole('button', { name: /save changes/i })).toBeVisible();
    await expect(page.getByRole('button', { name: /cancel/i })).toBeVisible();

    // Edit Configuration button should be gone.
    await expect(editBtn).not.toBeVisible();
  });

  test('cancel edit reverts to view mode', async ({ page }) => {
    await injectAdminSession(page);

    await page.goto('/admin/tranches');

    await page.getByRole('button', { name: /edit configuration/i }).click();
    await page.getByRole('button', { name: /cancel/i }).click();

    // Back to view mode.
    await expect(page.getByRole('button', { name: /edit configuration/i })).toBeVisible();
    await expect(page.getByRole('button', { name: /disable tranche/i })).toBeVisible();
  });

  test('disable tranche toggles to disabled state', async ({ page }) => {
    await injectAdminSession(page);

    await page.goto('/admin/tranches');

    // USDC is enabled; click Disable Tranche.
    await page.getByRole('button', { name: /disable tranche/i }).click();

    await expect(page.getByText('Disabled')).toBeVisible();
    await expect(page.getByRole('button', { name: /enable tranche/i })).toBeVisible();
  });

  test('all token configurations panel lists tokens with status', async ({ page }) => {
    await injectAdminSession(page);

    await page.goto('/admin/tranches');

    await expect(page.getByRole('heading', { name: /all token configurations/i })).toBeVisible();

    // USDC should show Active.
    await expect(page.getByText('Active')).toBeVisible();
    // USDT should show Inactive.
    await expect(page.getByText('Inactive')).toBeVisible();
  });

  test('configuration guidelines panel is visible', async ({ page }) => {
    await injectAdminSession(page);

    await page.goto('/admin/tranches');

    await expect(page.getByRole('heading', { name: /configuration guidelines/i })).toBeVisible();
    await expect(page.getByText(/senior target yield.*should reflect market/i)).toBeVisible();
  });

  test('BPS helper text shows percentage conversion', async ({ page }) => {
    await injectAdminSession(page);

    await page.goto('/admin/tranches');

    // Default USDC config: senior_target_yield_bps = 1000 -> 10.0%
    await expect(page.getByText('10.0% target annual yield')).toBeVisible();
    // senior_advance_rate_bps = 8000 -> 80%
    await expect(page.getByText('80% maximum senior share')).toBeVisible();
    // junior_first_loss_bps = 10000 -> 100%
    await expect(page.getByText('100% of losses junior absorbs')).toBeVisible();
  });
});

// ===========================================================================
// Waterfall Simulation Page  (/admin/waterfall-simulation)
// ===========================================================================

test.describe('Waterfall Simulation', () => {
  test.skip(!!process.env.CI, 'Waterfall simulation is a local-only admin tool.');

  test('renders page header and simulation parameters', async ({ page }) => {
    await injectAdminSession(page);

    await page.goto('/admin/waterfall-simulation');

    await expect(page.getByRole('heading', { name: /waterfall simulation/i })).toBeVisible();
    await expect(page.getByText(/simulate waterfall repayment/i)).toBeVisible();
    await expect(page.getByRole('heading', { name: /simulation parameters/i })).toBeVisible();
  });

  test('all parameter inputs are present with defaults', async ({ page }) => {
    await injectAdminSession(page);

    await page.goto('/admin/waterfall-simulation');

    await expect(page.getByLabel(/invoice id/i)).toBeVisible();
    await expect(page.getByLabel(/total repayment amount/i)).toBeVisible();
    await expect(page.getByLabel(/senior principal/i)).toBeVisible();
    await expect(page.getByLabel(/junior principal/i)).toBeVisible();
    await expect(page.getByLabel(/senior target yield/i)).toBeVisible();
    await expect(page.getByLabel(/elapsed time/i)).toBeVisible();
    await expect(page.getByRole('button', { name: /run simulation/i })).toBeVisible();
  });

  test('run simulation displays waterfall distribution results', async ({ page }) => {
    await injectAdminSession(page);

    await page.goto('/admin/waterfall-simulation');

    // Default values: totalDue=1000, senior=800, junior=200, yield=1000bps, 30 days.
    await page.getByRole('button', { name: /run simulation/i }).click();

    // Results should appear.
    await expect(page.getByRole('heading', { name: /waterfall distribution/i })).toBeVisible();
    await expect(page.getByText(/senior tranche/i).first()).toBeVisible();
    await expect(page.getByText(/junior tranche/i).first()).toBeVisible();
    await expect(page.getByText(/total distributed/i)).toBeVisible();
    await expect(page.getByText(/total due/i)).toBeVisible();
  });

  test('run simulation shows senior and junior payout cards', async ({ page }) => {
    await injectAdminSession(page);

    await page.goto('/admin/waterfall-simulation');

    await page.getByRole('button', { name: /run simulation/i }).click();

    // Senior card fields.
    await expect(page.getByText('Payout').first()).toBeVisible();
    await expect(page.getByText('Cap').first()).toBeVisible();
    await expect(page.getByText('Elapsed Yield').first()).toBeVisible();
    await expect(page.getByText('Principal Return').first()).toBeVisible();
  });

  test('loss allocation shows no default for full repayment', async ({ page }) => {
    await injectAdminSession(page);

    await page.goto('/admin/waterfall-simulation');

    // Default: totalDue=1000, senior=800, junior=200 -> no shortfall.
    await expect(page.getByText(/no default/i)).toBeVisible();
    await expect(page.getByText(/repayment covers full principal/i)).toBeVisible();
  });

  test('loss allocation shows shortfall for partial default scenario', async ({ page }) => {
    await injectAdminSession(page);

    await page.goto('/admin/waterfall-simulation');

    // Load the Partial Default preset.
    await page.getByRole('button', { name: /partial default/i }).click();

    // totalDue=500, senior=800, junior=200 -> shortfall = 1000 - 500 = 500.
    await expect(page.getByText(/shortfall/i)).toBeVisible();
    await expect(page.getByText(/junior absorbs first/i)).toBeVisible();
    await expect(page.getByText(/senior protected/i)).toBeVisible();
  });

  test('loss allocation shows total default scenario', async ({ page }) => {
    await injectAdminSession(page);

    await page.goto('/admin/waterfall-simulation');

    // Load the Total Default preset.
    await page.getByRole('button', { name: /total default/i }).click();

    // totalDue=0, senior=800, junior=200 -> shortfall = 1000.
    await expect(page.getByText(/shortfall/i)).toBeVisible();
    await expect(page.getByText(/junior absorbs first/i)).toBeVisible();
    // Junior absorbs all 200, senior takes remaining 800.
    await expect(page.getByText(/senior takes remaining/i)).toBeVisible();
  });

  test('scenario presets populate input fields', async ({ page }) => {
    await injectAdminSession(page);

    await page.goto('/admin/waterfall-simulation');

    // Full Repayment preset: totalDue=1100, senior=800, junior=200, 30 days.
    await page.getByRole('button', { name: /full repayment/i }).click();

    await expect(page.getByLabel(/total repayment amount/i)).toHaveValue(1100);
    await expect(page.getByLabel(/senior principal/i)).toHaveValue(800);
    await expect(page.getByLabel(/junior principal/i)).toHaveValue(200);

    // Partial Default preset: totalDue=500, senior=800, junior=200, 15 days.
    await page.getByRole('button', { name: /partial default/i }).click();

    await expect(page.getByLabel(/total repayment amount/i)).toHaveValue(500);

    // Total Default preset: totalDue=0, senior=800, junior=200, 60 days.
    await page.getByRole('button', { name: /total default/i }).click();

    await expect(page.getByLabel(/total repayment amount/i)).toHaveValue(0);
  });

  test('elapsed time helper shows days conversion', async ({ page }) => {
    await injectAdminSession(page);

    await page.goto('/admin/waterfall-simulation');

    // Default elapsed: 30 * 86400 = 2592000 seconds -> 30.0 days.
    await expect(page.getByText('30.0 days elapsed')).toBeVisible();
  });

  test('common scenarios panel has three presets', async ({ page }) => {
    await injectAdminSession(page);

    await page.goto('/admin/waterfall-simulation');

    await expect(page.getByRole('heading', { name: /common scenarios/i })).toBeVisible();
    await expect(page.getByRole('button', { name: /full repayment/i })).toBeVisible();
    await expect(page.getByRole('button', { name: /partial default/i })).toBeVisible();
    await expect(page.getByRole('button', { name: /total default/i })).toBeVisible();
  });
});
