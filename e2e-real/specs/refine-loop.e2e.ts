import { browser, expect } from '@wdio/globals';
import { openScratchTextTarget, pressGlobalHotkey, type ScratchTextTarget } from '../helpers/desktop';
import { startStubModelServer, type StubModelServer } from '../helpers/stub-model-server';

/**
 * S38: the real capture -> refine -> inject loop plus a settings flow,
 * driven against the REAL packaged `redrafter` binary via tauri-driver +
 * WebdriverIO -- the acceptance gate for the cross-platform work D1
 * (Linux text-inject), D2 (Windows text-inject), and D3 (platform-
 * conditional permission/hotkey/tray) actually did. Runs on Linux (X11 and
 * Wayland, see `wdio:x11`/`wdio:wayland` in `package.json`) and Windows
 * (`wdio:windows`); the macOS equivalent is `launch-smoke.macos.ts`
 * (no WKWebView WebDriver on macOS -- see design-redrafter.md's
 * "Acceptance Tooling").
 *
 * Two things this spec proves that `app/e2e/specs/d4.spec.ts` (the
 * Playwright/IPC-mock breadth spec) structurally cannot:
 *  1. The settings flow (add a connection, set it active) round-trips
 *     through the REAL `connection_add`/`connection_refresh_models`/
 *     `model_set_active` Tauri commands against a REAL HTTP endpoint (the
 *     local stub model server below stands in for a cloud/Ollama model per
 *     D4's "no real cloud keys needed" note).
 *  2. The refine loop's inject half actually lands in a REAL, separate,
 *     focused application window via the platform's real `text-inject`
 *     backend (X11/Wayland clipboard+XTEST/ydotool, or Windows UIA/
 *     SendInput) -- not a mocked `inject_text` IPC call.
 */

describe('S38: real-surface refine loop and settings flow', () => {
  let stub: StubModelServer;
  let target: ScratchTextTarget;

  const ORIGINAL_TEXT = 'the linux build works good on wayland i think';
  const REFINED_TEXT = 'The Linux build works well on Wayland.';

  before(async () => {
    stub = await startStubModelServer('stub-model', REFINED_TEXT);
  });

  after(async () => {
    await stub.close();
  });

  afterEach(() => {
    target?.close();
  });

  it('S38: adds a local connection at the stub endpoint and sets it as the active model', async () => {
    // Drives the REAL app window's Connections/Models screens -- the same
    // settings surface D3 kept building and running on every OS -- through
    // a real WebDriver session, not a Playwright IPC mock.
    await browser.url('/connections');
    await (await browser.$('[data-testid="add-connection"]')).click();

    await (await browser.$('[data-testid="conn-provider-type"]')).selectByAttribute('value', 'ollama');
    const baseUrlInput = await browser.$('[data-testid="conn-base-url"]');
    await baseUrlInput.setValue(stub.url);

    await (await browser.$('[data-testid="connection-save"]')).click();

    // `runSave` -> `connection_add` then `connection_refresh_models`
    // against the real stub server's `/api/tags` -- the discovered model
    // list should include `stub-model`.
    const modelCheck = await browser.$(`[data-testid="conn-model-check-stub-model"]`);
    await modelCheck.waitForExist({ timeout: 15_000 });
    if ((await modelCheck.getAttribute('aria-checked')) !== 'true') {
      await modelCheck.click();
    }

    await (await browser.$('[data-testid="connection-cancel"]')).click();

    await (await browser.$('[data-testid="connections-models-link"]')).click();
    const activeRadio = await browser.$('[data-testid="model-active-radio-stub-model"]');
    await activeRadio.waitForExist({ timeout: 15_000 });
    await activeRadio.click();
    await browser.waitUntil(async () => (await activeRadio.getAttribute('aria-checked')) === 'true', {
      timeout: 5_000,
      timeoutMsg: 'expected model-active-radio-stub-model to become the active model',
    });
  });

  it('S38: the global hotkey captures the real focused selection, refines it via the stub model, and injects it back', async () => {
    target = openScratchTextTarget(ORIGINAL_TEXT);
    target.selectAll();

    const chatCallsBefore = stub.chatCallCount();

    // The global hotkey is caught by `tauri-plugin-global-shortcut`
    // regardless of which window has focus -- the scratch target, not
    // redrafter's own window, is focused right now, exactly like a real
    // user refining text in another app.
    pressGlobalHotkey();

    await browser.waitUntil(async () => stub.chatCallCount() > chatCallsBefore, {
      timeout: 20_000,
      timeoutMsg: 'expected the real refine call to reach the stub model endpoint',
    });

    // Give the inject half of the pipeline (clipboard save/write/paste/
    // restore, or the AX write path) a moment to land before reading back.
    await browser.pause(1_000);

    const finalText = target.readAll().trim();
    expect(finalText).toBe(REFINED_TEXT);
  });
});
