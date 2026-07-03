/**
 * S38 macOS launch-smoke: boots the built `redrafter.app`, asserts it
 * reaches the tray/onboarding without an error state, and exercises real
 * `text-inject` capture/inject (the Accessibility API, D1/D2's macOS
 * sibling that's been there since Phase 1) against a real focused app
 * (TextEdit).
 *
 * macOS has no WKWebView WebDriver (see design-redrafter.md's "Acceptance
 * Tooling"), so this is NOT a WebdriverIO/tauri-driver spec like
 * `refine-loop.e2e.ts` -- it's a plain script: launch the app, drive it and
 * a real TextEdit window with AppleScript (`osascript`)/System Events UI
 * scripting, and assert on real state (process running, no crash/error
 * dialog, TextEdit's text actually changed).
 *
 * Run with `pnpm launch-smoke:macos` from `e2e-real/` on macOS only (the
 * macOS agent) -- see README.md for the full protocol, prerequisites (build the
 * `.app`, grant Accessibility once), and how to read the output. This
 * cannot run here (no macOS).
 */
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

import { startStubModelServer } from '../helpers/stub-model-server';

const APP_BUNDLE =
  process.env.REDRAFTER_APP_BUNDLE ??
  path.resolve(__dirname, '..', '..', 'app', 'src-tauri', 'target', 'release', 'bundle', 'macos', 'redrafter.app');

const ORIGINAL_TEXT = 'the macos build still works good i think';
const REFINED_TEXT = 'The macOS build still works well.';

function osascript(script: string): string {
  return execFileSync('osascript', ['-e', script], { encoding: 'utf8' }).trim();
}

/** Seeds `redrafter`'s on-disk settings/connections SQLite stores (see
 * `app/src-tauri/src/{settings,connections,models}.rs`) with a connection
 * pointed at the local stub model server and makes it the active model,
 * so this launch-smoke can drive a real refine end-to-end without any UI
 * automation to reach the Connections/Models screens (macOS has no
 * WebDriver to do that with -- see the module doc above) and without a
 * real cloud API key. Mirrors exactly what `connection_add` +
 * `connection_refresh_models` + `model_set_active` would persist. */
/** Quotes `value` as a single-quoted SQLite string literal (doubling any
 * embedded `'`, SQLite's own escape) -- both values interpolated below
 * (`stubUrl`, JSON blobs) are this harness's own known-shape values, not
 * arbitrary user input, but this keeps the SQL text itself correct
 * regardless. */
function sqlLiteral(value: string): string {
  return `'${value.replace(/'/g, "''")}'`;
}

function seedConnectionAndActiveModel(appDataDir: string, stubUrl: string, modelName: string): void {
  fs.mkdirSync(appDataDir, { recursive: true });
  const connectionsDb = path.join(appDataDir, 'connections.sqlite3');
  const settingsDb = path.join(appDataDir, 'settings.sqlite3');

  // execFileSync (argument array, no shell) rather than a shell string
  // built via `exec`/`execSync` -- each SQL statement is its own argv
  // entry passed straight to the `sqlite3` binary, so nothing here is
  // interpreted by a shell.
  const enabledModelsJson = sqlLiteral(JSON.stringify([modelName]));
  execFileSync('sqlite3', [
    connectionsDb,
    `CREATE TABLE IF NOT EXISTS connections (
      id                INTEGER PRIMARY KEY AUTOINCREMENT,
      provider_kind     TEXT NOT NULL,
      base_url          TEXT NOT NULL,
      api_key           TEXT,
      enabled_models    TEXT NOT NULL,
      available_models  TEXT NOT NULL DEFAULT '[]'
    );`,
    'DELETE FROM connections;',
    'INSERT INTO connections (id, provider_kind, base_url, api_key, enabled_models, available_models) ' +
      `VALUES (1, 'ollama', ${sqlLiteral(stubUrl)}, NULL, ${enabledModelsJson}, ${enabledModelsJson});`,
  ]);

  const activeModelJson = sqlLiteral(JSON.stringify({ connection_id: '1', model_id: modelName }));
  execFileSync('sqlite3', [
    settingsDb,
    'CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);',
    `INSERT OR REPLACE INTO settings (key, value) VALUES ('active_model', ${activeModelJson});`,
  ]);
}

async function main(): Promise<void> {
  if (process.platform !== 'darwin') {
    throw new Error('launch-smoke.macos.ts only runs on macOS');
  }
  if (!fs.existsSync(APP_BUNDLE)) {
    throw new Error(
      `built app bundle not found at ${APP_BUNDLE} -- run \`cd app && pnpm tauri build\` first, or set REDRAFTER_APP_BUNDLE`,
    );
  }

  const stub = await startStubModelServer('stub-model', REFINED_TEXT);
  try {
    const appDataDir = path.join(os_homedir(), 'Library', 'Application Support', 'com.redrafter.app');
    seedConnectionAndActiveModel(appDataDir, stub.url, stub.modelName);

    console.log(`[launch-smoke] launching ${APP_BUNDLE}`);
    execFileSync('open', ['-n', APP_BUNDLE]);
    await sleep(3000);

    // 1. Reaches tray/onboarding without an error state: the process is
    // running and there is no crash-reporter/error dialog frontmost.
    const running = osascript(
      `tell application "System Events" to (name of processes) contains "redrafter"`,
    );
    if (running !== 'true') {
      throw new Error('redrafter process is not running after launch -- app failed to boot');
    }

    const frontAppName = osascript(`tell application "System Events" to name of first process whose frontmost is true`);
    if (/error|crash/i.test(frontAppName)) {
      throw new Error(`unexpected frontmost app after launch: ${frontAppName}`);
    }
    console.log('[launch-smoke] app reached a running state with no error dialog');

    // 2. Exercises real text-inject capture/inject against a real app
    // (TextEdit): open a scratch document, select its text, send
    // redrafter's global hotkey, and read the result back.
    const scratchFile = path.join(os_tmpdir(), `redrafter-launch-smoke-${Date.now()}.txt`);
    fs.writeFileSync(scratchFile, ORIGINAL_TEXT, 'utf8');
    execFileSync('open', ['-a', 'TextEdit', scratchFile]);
    await sleep(1500);

    osascript(`
      tell application "TextEdit" to activate
      tell application "System Events"
        keystroke "a" using {command down}
      end tell
    `);

    // redrafter's default hotkey (`hotkey::DEFAULT_HOTKEY`) is Ctrl+Alt+R;
    // System Events keystroke modifiers are {control down, option down}.
    osascript(`
      tell application "System Events"
        keystroke "r" using {control down, option down}
      end tell
    `);

    const deadline = Date.now() + 20_000;
    let finalText = '';
    while (Date.now() < deadline) {
      await sleep(500);
      finalText = osascript(`tell application "TextEdit" to get text of document 1`);
      if (finalText.trim() === REFINED_TEXT) break;
    }

    if (finalText.trim() !== REFINED_TEXT) {
      throw new Error(
        `expected TextEdit's content to become "${REFINED_TEXT}" after the hotkey refine loop, got: "${finalText}"`,
      );
    }
    console.log('[launch-smoke] real capture -> refine -> inject loop succeeded against TextEdit');

    execFileSync('osascript', ['-e', 'tell application "TextEdit" to close every document saving no']);
    fs.rmSync(scratchFile, { force: true });

    execFileSync('osascript', ['-e', 'tell application "redrafter" to quit']).toString();
    console.log('[launch-smoke] PASS');
  } finally {
    await stub.close();
  }
}

function os_homedir(): string {
  return process.env.HOME ?? '/tmp';
}
function os_tmpdir(): string {
  return process.env.TMPDIR ?? '/tmp';
}
function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

main().catch((err) => {
  console.error('[launch-smoke] FAIL:', err);
  process.exitCode = 1;
});
