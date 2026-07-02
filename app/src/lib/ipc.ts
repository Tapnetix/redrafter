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

// ── Permission open-settings (A9/A13) ──
/**
 * Opens macOS System Settings so the user can re-grant Accessibility. Mirrors
 * `permission_status` above; the caller re-polls that to see the change.
 */
export function permissionOpenSettings(): Promise<void> {
  return invoke('permission_open_settings');
}

// ── Refine pipeline + restore + tray (A9) ──
/** Mirrors `RefineOutcome` from src-tauri/src/orchestrator.rs. */
export interface RefineOutcome {
  original: string;
  refined: string;
  model: string;
}

/**
 * Runs the default refine pipeline (A9/A5): captures the current selection,
 * calls the active model, and blind-injects the result in place, returning
 * the original (for restore), the refined text, and the model used. Rejects
 * with `'no_active_model'` or `'permission_denied'` for those specific
 * failure modes (A11/A13); other failures reject with a generic message.
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

/**
 * Quits the app via the tray. The embedded tray preview in Capture.tsx is
 * otherwise display-only in Phase A (see Capture.tsx's carve-out note); this
 * is the one control wired to a real command.
 */
export function trayQuit(): Promise<void> {
  return invoke('tray_quit');
}
