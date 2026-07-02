/**
 * Playwright test fixture that injects the Tauri IPC mock into every page
 * before React hydrates, via page.addInitScript(). Scenario specs under
 * app/e2e/specs/ should import { test, expect } from here (instead of
 * '@playwright/test') so they automatically get the mocked Tauri backend.
 */

import { test as base, expect } from '@playwright/test';
import { getTauriMockScript } from '../mocks/tauri-mock';
import { DEFAULT_TEST_DATA, type TestData } from './test-data';

type TestFixtures = {
  testData: TestData;
};

export const test = base.extend<TestFixtures>({
  testData: [DEFAULT_TEST_DATA, { option: true }],

  page: async ({ page, testData }, use) => {
    await page.addInitScript(getTauriMockScript(testData));
    await use(page);
  },
});

export { expect };
