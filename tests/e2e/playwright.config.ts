import { defineConfig } from '@playwright/test';

// One box stack, so tests run serially; each spec resets the box it needs (helpers.ts).
export default defineConfig({
  testDir: './specs',
  workers: 1,
  globalTeardown: './global-teardown.ts',
  fullyParallel: false,
  timeout: 120_000,
  expect: { timeout: 15_000 },
  reporter: [['list'], ['json', { outputFile: 'results.json' }]],
  use: {
    ignoreHTTPSErrors: true, // the staff console uses the box's own certificate (A-124)
    trace: 'retain-on-failure',
    // A missing element fails within seconds instead of holding the test until its own timeout.
    actionTimeout: 15_000,
    navigationTimeout: 30_000,
  },
});
