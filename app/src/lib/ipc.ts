// Thin wrapper around `@tauri-apps/api`'s `invoke` for the Tauri commands the
// frontend calls. Screens import from here rather than calling `invoke`
// directly, so command names/payload shapes live in one place and stay in sync
// with `controls/*.json` and the backend (`app/src-tauri/src/*.rs`). A14's
// composition root registers these commands in the Tauri builder and owns this
// file's routing-adjacent additions; each screen task extends (not replaces)
// what earlier tasks add here. Intentionally not exhaustive yet.

import { invoke } from '@tauri-apps/api/core';

// ── Permission (A4) ──
export interface PermissionStatus {
  granted: boolean;
}

/** Reports whether the macOS Accessibility permission is currently granted. */
export function getPermissionStatus(): Promise<PermissionStatus> {
  return invoke('permission_status');
}

// ── Settings key-value store (A4) ──
/** Reads a settings value by key, or `null` if it has never been set. */
export function settingsGet(key: string): Promise<string | null> {
  return invoke('settings_get', { key });
}

/** Upserts a settings value by key. */
export function settingsSet(key: string, value: string): Promise<void> {
  return invoke('settings_set', { key, value });
}

// ── Hotkey (A6) ──
/** Result of a `hotkey_set`, mirroring `hotkey.rs`'s `HotkeySetResult`. */
export interface HotkeySetResult {
  ok: boolean;
  conflict: boolean;
}

/**
 * Saves `combo` (e.g. `"Ctrl+Alt+R"`) as the new global hotkey, unregistering
 * the previous one first. Resolves with `conflict: true` (rather than
 * rejecting) when the combo is already claimed elsewhere. Backed by the
 * `hotkey_set` Tauri command; C2/C6 reuse this for the rebind dialog.
 */
export function hotkeySet(combo: string): Promise<HotkeySetResult> {
  return invoke<HotkeySetResult>('hotkey_set', { combo });
}

// ── Connections (A7) ──
/** A stored provider connection, mirroring `connections.rs`'s `Connection`. */
export interface Connection {
  id: string;
  providerKind: string;
  baseUrl: string;
  enabledModels: string[];
}

export interface ConnectionAddArgs extends Record<string, unknown> {
  providerKind: string;
  baseUrl: string;
  apiKey: string;
}

/**
 * Connects a provider (the backend verifies reachability) and persists it with
 * a default enabled model. Backed by the `connection_add` Tauri command.
 */
export function connectionAdd(args: ConnectionAddArgs): Promise<Connection> {
  return invoke<Connection>('connection_add', args);
}

/** Lists every stored provider connection. Backed by `connection_list`. */
export function connectionList(): Promise<Connection[]> {
  return invoke<Connection[]>('connection_list');
}

// ── Permission open-settings (A9/A13) ──
/**
 * Opens macOS System Settings so the user can re-grant Accessibility. Mirrors
 * `permission_status` above; the caller re-polls that to see the change.
 */
export function permissionOpenSettings(): Promise<void> {
  return invoke('permission_open_settings');
}

// ── Refine pipeline + restore + tray (A9) ──
/**
 * Mirrors `RefineOutcome` from src-tauri/src/orchestrator.rs, widened
 * (B5/B6) with the optional `status` tag `RefineFlow` adds when the
 * configured inject mode is `Review` rather than `Blind` — see
 * `orchestrator.rs`'s `RefineFlow` (`#[serde(tag = "status", ...)]`). A plain
 * Phase A response (no `status` field, always immediately injected) is
 * unaffected: `status` is `undefined`, and callers treat that the same as
 * `'injected'`.
 */
export interface RefineOutcome {
  original: string;
  refined: string;
  model: string;
  status?: 'injected' | 'pending_review';
}

/**
 * Rejection strings `refine`/`tray_refine` reject with for those two
 * specific failure modes. Mirrors the constants of the same name in
 * `app/src-tauri/src/lib.rs` (`NO_ACTIVE_MODEL_ERROR`/
 * `PERMISSION_DENIED_ERROR`) — keep both sides in sync if either changes.
 */
export const NO_ACTIVE_MODEL_ERROR = 'no_active_model';
export const PERMISSION_DENIED_ERROR = 'permission_denied';

/**
 * Runs the refine pipeline (A9/A5, extended B5/B6): captures the current
 * selection, calls the active model — retrying a configured fallback chain
 * on failure — and either blind-injects the result in place or (when the
 * configured inject mode is `Review`) leaves it pending the user's
 * accept/edit/discard choice; see `RefineOutcome.status`. Resolves with the
 * original (for restore), the refined text, and the model used either way.
 * Rejects with `NO_ACTIVE_MODEL_ERROR` or `PERMISSION_DENIED_ERROR` for those
 * specific failure modes (A11/A13); other failures reject with a generic
 * message (optionally an object carrying a `fallbackModels` list — B6's
 * error/retry state shows it when present) rather than injecting anything.
 */
export function refine(): Promise<RefineOutcome> {
  return invoke('refine');
}

/**
 * Returns the pre-refine original text saved by the most recent `refine`
 * call, for restore (A9/A10). Does not itself inject anything — pair with
 * `injectText` to put it back in place.
 */
export function restoreOriginal(): Promise<string> {
  return invoke('restore_original');
}

/** Injects `text` into the focused app in place of the current selection. */
export function injectText(text: string): Promise<void> {
  return invoke('inject_text', { text });
}

/** Cancels the in-flight `refine` call, if any. Backed by `cancel_refine`. */
export function cancelRefine(): Promise<void> {
  return invoke('cancel_refine');
}

/**
 * Triggers a refine from the menu-bar tray (same pipeline as `refine`, but the
 * tray's entry point). Backed by the `tray_refine` Tauri command.
 */
export function trayRefine(): Promise<RefineOutcome> {
  return invoke<RefineOutcome>('tray_refine');
}

/**
 * Quits the app via the tray. The embedded tray preview in Capture.tsx is
 * otherwise display-only in Phase A (see Capture.tsx's carve-out note); this
 * is the one control wired to a real command.
 */
export function trayQuit(): Promise<void> {
  return invoke('tray_quit');
}
