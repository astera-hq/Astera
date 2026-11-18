import { defineConfig, devices } from '@playwright/test';
import { assertRealContractIds } from './e2e/contract-ids';

// #395: in CI, refuse to run against placeholder contract IDs. Locally this is
// allowed so mock-based specs can run without a deployment.
if (process.env.CI) {
  assertRealContractIds();
}

const defaultBaseURL = 'http://localhost:3000';
const parsedBaseURL = new URL(process.env.PLAYWRIGHT_BASE_URL ?? defaultBaseURL);
const baseURLPort = Number.parseInt(parsedBaseURL.port || '3000', 10);
parsedBaseURL.port = String(baseURLPort);
const baseURL = parsedBaseURL.toString().replace(/\/$/, '');

export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: process.env.CI ? 'github' : 'list',

  use: {
    baseURL,
    trace: 'on-first-retry',
  },

  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],

  webServer: {
    command: `npm run build && npm run start -- -p ${baseURLPort}`,
    url: baseURL,
    reuseExistingServer: !process.env.CI,
    timeout: 240_000,
  },
});
