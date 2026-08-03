import { defineConfig, devices } from '@playwright/test';
const PORT = 3199;
export default defineConfig({
  testDir: 'e2e/shots',
  reporter: [['list']],
  use: { baseURL: `http://localhost:${PORT}`, ...devices['Desktop Chrome'] },
  webServer: {
    command: `python3 -m http.server ${PORT} --directory out`,
    url: `http://localhost:${PORT}/index.html`,
    reuseExistingServer: false,
    timeout: 30_000,
  },
});
