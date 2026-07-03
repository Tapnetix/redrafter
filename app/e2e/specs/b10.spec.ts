import { test, expect } from '../fixtures/setup';

/**
 * S21: Secure key storage — the user picks where API keys are kept at rest
 * (encrypted config file vs. OS keychain) on the Connections screen, and
 * that choice is persisted via `secretsSet`/`secrets_set` (B7's control,
 * B10's backend). No command ever hands a raw key back to the frontend --
 * only a `key_ref` handle (`Connection::key_ref`, B7b).
 * (design-redrafter.md, wireframes/connections.html, controls/connections.json)
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

test.describe('Secure key storage (S21)', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      connections: [],
    },
  });

  test('S21: choosing a key-storage backend persists via secrets_set, and no raw key is ever returned', async ({
    page,
  }) => {
    await page.goto('/connections');

    const encryptedBtn = page.getByTestId('key-storage-encrypted');
    const keychainBtn = page.getByTestId('key-storage-keychain');

    // Default backend is the encrypted config file (plan: encrypted-file
    // by default, keychain opt-in).
    await expect(encryptedBtn).toHaveAttribute('aria-checked', 'true');
    await expect(keychainBtn).toHaveAttribute('aria-checked', 'false');
    expect((await mockCalls(page)).some((c) => c.cmd === 'secrets_set')).toBe(false);

    // Choosing the OS keychain flips the control and persists the choice.
    await keychainBtn.click();
    await expect(keychainBtn).toHaveAttribute('aria-checked', 'true');
    await expect(encryptedBtn).toHaveAttribute('aria-checked', 'false');

    await expect
      .poll(async () => (await mockCalls(page)).filter((c) => c.cmd === 'secrets_set').length)
      .toBe(1);
    let secretsSetCalls = (await mockCalls(page)).filter((c) => c.cmd === 'secrets_set');
    expect(secretsSetCalls[0].args.location).toBe('keychain');

    // Switching back to the encrypted file persists that choice too.
    await encryptedBtn.click();
    await expect(encryptedBtn).toHaveAttribute('aria-checked', 'true');
    await expect(keychainBtn).toHaveAttribute('aria-checked', 'false');

    await expect
      .poll(async () => (await mockCalls(page)).filter((c) => c.cmd === 'secrets_set').length)
      .toBe(2);
    secretsSetCalls = (await mockCalls(page)).filter((c) => c.cmd === 'secrets_set');
    expect(secretsSetCalls[1].args.location).toBe('encrypted_file');

    // Adding a connection with an API key: the call *to* the backend
    // necessarily carries the key (that's how it gets stored) -- the
    // security property under test is what comes *back*.
    const secretKey = 'sk-super-secret-e2e-marker-key';
    await page.getByTestId('connections-empty-cta').click();
    await page.getByTestId('conn-api-key').fill(secretKey);
    await page.getByTestId('conn-test').click();
    await expect(page.getByTestId('conn-test-ok')).toBeVisible();
    await page.getByTestId('connection-save').click();

    await expect
      .poll(async () => (await mockCalls(page)).some((c) => c.cmd === 'connection_add'))
      .toBe(true);
    const addCall = (await mockCalls(page)).find((c) => c.cmd === 'connection_add');
    expect(addCall!.args.apiKey).toBe(secretKey);

    // Directly inspect what connection_add/connection_list actually
    // resolve with -- the strongest check that no returned `Connection`
    // ever carries the raw key, only `keyRef` (mirrors `connections.rs`'s
    // `Connection` struct, which has no `api_key` field to serialize).
    const listed = await page.evaluate(() =>
      (window as unknown as { __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Promise<unknown> } })
        .__TAURI_INTERNALS__.invoke('connection_list', {}),
    );
    const serializedListed = JSON.stringify(listed);
    expect(serializedListed).not.toContain(secretKey);
    expect((listed as { keyRef?: string }[])[0]?.keyRef).toBeTruthy();

    // Nothing rendered on screen leaks the raw key either.
    await expect(page.locator('body')).not.toContainText(secretKey);
  });
});
