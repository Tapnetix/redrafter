import { defineConfig, devices } from '@playwright/test';

// The Tauri IPC mock (app/e2e/mocks/tauri-mock.ts) is loaded per-page via
// page.addInitScript() in the shared fixture at app/e2e/fixtures/setup.ts.
// Scenario specs under app/e2e/specs/ import { test, expect } from that
// fixture (not '@playwright/test' directly) to pick up the mock.
export default defineConfig({
  testDir: 'e2e/specs',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: [
    ['html', { open: 'never' }],
    ['list'],
  ],
  use: {
    baseURL: 'http://localhost:3100',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
  webServer: {
    command: 'pnpm dev --port 3100',
    url: 'http://localhost:3100',
    reuseExistingServer: false,
    timeout: 30_000,
  },
});
