import { execFileSync, spawn, type ChildProcess } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

/**
 * OS-level desktop automation the real-surface refine-loop spec needs
 * *outside* the WebDriver session: `redrafter`'s hotkey is a genuinely
 * global OS shortcut (`tauri-plugin-global-shortcut`, default `Ctrl+Alt+R`
 * -- see `app/src-tauri/src/hotkey.rs`'s `DEFAULT_HOTKEY`) and its
 * capture/inject targets whatever *other* app currently has focus
 * (`crates/text-inject`) -- neither is reachable through a WebDriver
 * session scoped to redrafter's own window. So this module drives a
 * separate scratch text-editor window directly via the same command-line
 * tools `text-inject`'s real backends use (`xdotool`/`xclip` on X11,
 * `wtype`/`wl-copy`/`wl-paste`/`ydotool` on Wayland, PowerShell SendKeys +
 * clipboard on Windows), so the loop under test is the same one a real
 * user drives: select text in some other focused app, press the global
 * hotkey, see that app's text replaced.
 */

export type LinuxSessionType = 'x11' | 'wayland';

/** Mirrors `linux.rs`'s `SessionType::detect_from` (D1): `WAYLAND_DISPLAY`
 * being set is the primary signal, `XDG_SESSION_TYPE=wayland` the
 * secondary one. Kept in sync deliberately -- the real backend and this
 * harness must agree on which session they're driving. */
export function detectLinuxSession(): LinuxSessionType {
  const waylandDisplaySet = Boolean(process.env.WAYLAND_DISPLAY);
  const xdgSaysWayland = process.env.XDG_SESSION_TYPE === 'wayland';
  return waylandDisplaySet || xdgSaysWayland ? 'wayland' : 'x11';
}

/** A scratch, real (non-Tauri) editable text window the refine loop injects
 * into -- stands in for "some other app with a focused text field" a real
 * user would have selected text in. */
export interface ScratchTextTarget {
  /** Selects all of the target's current text (keyboard select-all), so
   * both `text-inject`'s PRIMARY-selection capture (Linux X11's AX
   * analogue) and its clipboard-paste inject fallback have something to
   * read/replace -- mirrors a user manually selecting text before pressing
   * the hotkey. */
  selectAll(): void;
  /** Reads back the target's current full contents, via select-all + copy
   * + a clipboard read (the same round trip a sighted user doing "Ctrl+A,
   * Ctrl+C" would produce). */
  readAll(): string;
  /** Closes the scratch target and removes its backing file. */
  close(): void;
}

const SCRATCH_PREFIX = 'redrafter-e2e-scratch-';

function writeScratchFile(initialText: string): string {
  const file = path.join(os.tmpdir(), `${SCRATCH_PREFIX}${Date.now()}.txt`);
  fs.writeFileSync(file, initialText, 'utf8');
  return file;
}

/** Gives a spawned GUI editor a moment to map its window and take focus
 * before any key simulation is sent at it. There's no portable
 * "wait for window" primitive across X11/Wayland/Windows editors here, so
 * this is a fixed, generous delay rather than a poll -- fine for a manual
 * real-surface run, not something this harness needs to make fast. */
function settleMs(ms: number): void {
  execFileSync(process.platform === 'win32' ? 'powershell' : 'sleep', process.platform === 'win32' ? ['-Command', `Start-Sleep -Milliseconds ${ms}`] : [String(ms / 1000)]);
}

/**
 * Opens a real, focused, editable scratch text window pre-filled with
 * `initialText`, appropriate to the current OS/session, for the refine
 * loop to select-all + hotkey + verify against.
 *
 * The editor binary is overridable via `REDRAFTER_E2E_EDITOR` (Linux/macOS)
 * in case the default (`gedit`) isn't installed -- any GUI text editor that
 * supports plain Ctrl+A select-all and Ctrl+V paste works.
 */
export function openScratchTextTarget(initialText: string): ScratchTextTarget {
  if (process.platform === 'win32') {
    return openWindowsScratchTarget(initialText);
  }
  return openLinuxScratchTarget(initialText);
}

function openLinuxScratchTarget(initialText: string): ScratchTextTarget {
  const file = writeScratchFile(initialText);
  const editor = process.env.REDRAFTER_E2E_EDITOR ?? 'gedit';
  const proc: ChildProcess = spawn(editor, [file], { stdio: 'ignore', detached: true });
  settleMs(1000);

  const session = detectLinuxSession();

  return {
    selectAll() {
      if (session === 'wayland') {
        // `wtype` drives the `virtual-keyboard` protocol (same fallback
        // `linux.rs` uses when `ydotool`/`ydotoold` aren't set up).
        execFileSync('wtype', ['-M', 'ctrl', '-k', 'a', '-m', 'ctrl']);
      } else {
        execFileSync('xdotool', ['key', '--clearmodifiers', 'ctrl+a']);
      }
    },
    readAll() {
      if (session === 'wayland') {
        execFileSync('wtype', ['-M', 'ctrl', '-k', 'c', '-m', 'ctrl']);
        return execFileSync('wl-paste', ['--no-newline']).toString('utf8');
      }
      execFileSync('xdotool', ['key', '--clearmodifiers', 'ctrl+c']);
      return execFileSync('xclip', ['-selection', 'clipboard', '-o']).toString('utf8');
    },
    close() {
      proc.kill();
      fs.rmSync(file, { force: true });
    },
  };
}

function openWindowsScratchTarget(initialText: string): ScratchTextTarget {
  const file = path.join(os.tmpdir(), `${SCRATCH_PREFIX}${Date.now()}.txt`);
  fs.writeFileSync(file, initialText, 'utf8');
  const proc: ChildProcess = spawn('notepad.exe', [file], { stdio: 'ignore', detached: true });
  settleMs(1000);

  function sendKeys(keys: string): void {
    // SendKeys delivers to whatever window currently has focus -- the
    // caller is responsible for Notepad being that window (it was just
    // spawned and given focus above).
    execFileSync('powershell', [
      '-NoProfile',
      '-Command',
      `Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait('${keys}')`,
    ]);
  }

  return {
    selectAll() {
      sendKeys('^a');
    },
    readAll() {
      sendKeys('^c');
      return execFileSync('powershell', ['-NoProfile', '-Command', 'Get-Clipboard']).toString('utf8');
    },
    close() {
      proc.kill();
      fs.rmSync(file, { force: true });
    },
  };
}

/**
 * Sends `redrafter`'s default global hotkey (`Ctrl+Alt+R`, see
 * `hotkey::DEFAULT_HOTKEY`) at the OS level, regardless of which window
 * currently has focus -- exactly like a real user pressing it. Whichever
 * scratch target from {@link openScratchTextTarget} currently has focus is
 * what `text-inject`'s capture/inject will act on.
 */
export function pressGlobalHotkey(): void {
  if (process.platform === 'win32') {
    execFileSync('powershell', [
      '-NoProfile',
      '-Command',
      // ^ = Ctrl, % = Alt in SendKeys syntax.
      "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait('^%r')",
    ]);
    return;
  }

  if (detectLinuxSession() === 'wayland') {
    // `ydotool` needs its `ydotoold` daemon + `uinput` access (see
    // `linux.rs`'s module docs); keycodes are keyboard-layout dependent --
    // 29=ctrl, 56=alt, 19=r on a standard US layout.
    execFileSync('ydotool', ['key', '29:1', '56:1', '19:1', '19:0', '56:0', '29:0']);
    return;
  }

  execFileSync('xdotool', ['key', '--clearmodifiers', 'ctrl+alt+r']);
}
