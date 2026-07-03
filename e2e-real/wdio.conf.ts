import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import path from 'node:path';

/**
 * WebdriverIO + tauri-driver config that drives the REAL packaged
 * `redrafter` binary -- not a Playwright/IPC-mock stand-in -- for the S38
 * cross-platform acceptance on Linux (X11 and Wayland) and Windows. See
 * `e2e-real/README.md` for the full per-OS run protocol (build steps,
 * `tauri-driver` install, how to invoke this on each target). This CANNOT
 * run on the headless Linux sandbox this harness was authored in: there is
 * no live X11/Wayland session, no `tauri-driver`/`WebKitWebDriver`
 * installed, and no packaged binary built -- see D4's completion notes for
 * what's deferred to the user's real machines.
 *
 * Follows the shape of the official Tauri v2 WebDriver example
 * (https://v2.tauri.app/develop/tests/webdriver/) with the `application`
 * path parameterized so the same config drives a debug build (fast local
 * iteration) or a release/bundled binary (closer to what ships) on either
 * OS.
 *
 * Linux notes:
 * - X11: run with a real `$DISPLAY` (a live desktop, or a virtual one via
 *   `xvfb-run`) so tauri-driver's WebKitWebDriver has an X server to
 *   attach to.
 * - Wayland: `$WAYLAND_DISPLAY` selects the compositor session. WebKitGTK
 *   still needs a windowing backend reachable -- either a native Wayland
 *   session (`WAYLAND_DISPLAY` set) or XWayland (both `WAYLAND_DISPLAY`
 *   and `DISPLAY` set). This mirrors exactly how `text-inject`'s own Linux
 *   backend (D1, `crates/text-inject/src/linux.rs`) detects the session
 *   type at runtime to pick its clipboard/key-sim tools.
 *
 * Windows notes: `tauri-driver` drives the WebView2 WebDriver
 * (`msedgedriver`) directly rather than shelling out to a second driver
 * process the way the Linux WebKitWebDriver path does -- `tauri-driver`
 * handles that difference internally, so this config is unchanged between
 * the two.
 */

const isWindows = process.platform === 'win32';

// The built binary to drive. Defaults to the debug build under this repo's
// standard cargo target dir; override with REDRAFTER_BINARY (e.g. to point
// at a release build, or an installed/bundled location) so the same config
// works for a dev build and a packaged release without editing this file.
const DEFAULT_BINARY = path.resolve(
  __dirname,
  '..',
  'target',
  'debug',
  isWindows ? 'redrafter.exe' : 'redrafter',
);
const APPLICATION = process.env.REDRAFTER_BINARY ?? DEFAULT_BINARY;

let tauriDriverProcess: ChildProcess | undefined;

export const config: WebdriverIO.Config = {
  specs: ['./specs/*.e2e.ts'],
  maxInstances: 1,
  hostname: '127.0.0.1',
  port: 4444,
  path: '/',
  connectionRetryTimeout: 30_000,
  connectionRetryCount: 3,
  capabilities: [
    {
      // tauri-driver's own capability, not a browserName -- see the Tauri
      // WebDriver docs. `maxInstances: 1` because a single desktop app
      // instance can't be driven by more than one session at once.
      maxInstances: 1,
      'tauri:options': {
        application: APPLICATION,
      },
    } as WebdriverIO.Capabilities,
  ],
  logLevel: 'info',
  reporters: ['spec'],
  framework: 'mocha',
  mochaOpts: {
    ui: 'bdd',
    timeout: 120_000,
  },

  // Build the binary before driving it, unless the caller already built one
  // and pointed REDRAFTER_BINARY at it (fast local iteration without a
  // rebuild every run).
  onPrepare: () => {
    if (!process.env.REDRAFTER_BINARY) {
      const result = spawnSync(
        'cargo',
        ['build', '--manifest-path', path.resolve(__dirname, '..', 'app', 'src-tauri', 'Cargo.toml')],
        { stdio: 'inherit' },
      );
      if (result.status !== 0) {
        throw new Error('cargo build of the redrafter binary failed -- see output above');
      }
    }
  },

  // `tauri-driver` must already be listening before the WebDriver session
  // starts -- there is no retry/wait-for-port logic on the wdio side, so
  // this spawns it synchronously ahead of each session and tears it down
  // after. Install it once via `cargo install tauri-driver` (see README).
  beforeSession: () => {
    tauriDriverProcess = spawn('tauri-driver', [], {
      stdio: [null, process.stdout, process.stderr],
    });
  },

  afterSession: () => {
    tauriDriverProcess?.kill();
    tauriDriverProcess = undefined;
  },
};
