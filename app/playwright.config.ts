import { defineConfig, devices } from '@playwright/test';

// The Tauri IPC mock (app/e2e/mocks/tauri-mock.ts) is loaded per-page via
// page.addInitScript() in the shared fixture at app/e2e/fixtures/setup.ts.
// Scenario specs under app/e2e/specs/ import { test, expect } from that
// fixture (not '@playwright/test' directly) to pick up the mock.
// Port is env-configurable so parallel git-worktree runs don't collide on a
// single hardcoded port. Set PW_PORT per run (the orchestrator gives each
// concurrent worktree a distinct one); defaults to 3100 for a solo run.
const PORT = Number(process.env.PW_PORT ?? 3100);
const BASE_URL = `http://localhost:${PORT}`;

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
    baseURL: BASE_URL,
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
    command: `pnpm dev --port ${PORT}`,
    url: BASE_URL,
    reuseExistingServer: false,
    timeout: 30_000,
  },
});
