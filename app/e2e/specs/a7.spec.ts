/**
 * S18: First-run provider chooser.
 *
 * Given permission is granted and no provider is connected yet, the user
 * picks Cloud or Local on the first-run screen and connects a first
 * provider (`connection_add`), then continues into the app.
 *
 * See docs/wireframes/first-run.html and docs/controls/first-run.json for
 * the control contract this spec drives.
 */
import { test, expect } from '../fixtures/setup';

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test('S18: connecting a cloud provider calls connection_add, then continue opens the main window', async ({
  page,
}) => {
  await page.goto('/first-run');

  // Cloud is the default provider-type tab, Anthropic the default provider.
  await expect(page.getByTestId('firstrun-cloud-panel')).toBeVisible();
  await expect(page.getByTestId('firstrun-cloud')).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByTestId('firstrun-provider-anthropic')).toHaveAttribute('aria-checked', 'true');

  await page.getByTestId('firstrun-key').fill('sk-ant-test-key');
  await page.getByTestId('firstrun-connect').click();

  await expect(page.getByTestId('firstrun-connect')).toHaveText(/connected/i);

  const addCall = (await mockCalls(page)).find((c) => c.cmd === 'connection_add');
  expect(addCall).toBeTruthy();
  expect(addCall!.args.providerKind).toBe('anthropic');
  expect(addCall!.args.apiKey).toBe('sk-ant-test-key');

  await page.getByTestId('firstrun-continue').click();
  await expect(page).toHaveURL(/\/$/);
});

test('S18: switching to a different cloud provider connects with that provider', async ({ page }) => {
  await page.goto('/first-run');

  await page.getByTestId('firstrun-provider-openai').click();
  await expect(page.getByTestId('firstrun-provider-openai')).toHaveAttribute('aria-checked', 'true');
  await expect(page.getByTestId('firstrun-provider-anthropic')).toHaveAttribute('aria-checked', 'false');

  await page.getByTestId('firstrun-key').fill('sk-openai-test-key');
  await page.getByTestId('firstrun-connect').click();

  const addCall = (await mockCalls(page)).find((c) => c.cmd === 'connection_add');
  expect(addCall!.args.providerKind).toBe('openai');
});

test('S18: connecting the detected local Ollama calls connection_add', async ({ page }) => {
  await page.goto('/first-run');

  await page.getByTestId('firstrun-local').click();
  await expect(page.getByTestId('firstrun-local-panel')).toBeVisible();
  await expect(page.getByTestId('firstrun-ollama-detected')).toBeVisible();

  await page.getByTestId('firstrun-ollama-connect').click();
  await expect(page.getByTestId('firstrun-ollama-connect')).toHaveText(/connected/i);

  const addCall = (await mockCalls(page)).find((c) => c.cmd === 'connection_add');
  expect(addCall).toBeTruthy();
  expect(addCall!.args.providerKind).toBe('ollama');
  expect(addCall!.args.apiKey).toBe('');
});
