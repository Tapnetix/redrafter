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

// ── Connections (A7/B7b) ──
/**
 * A stored provider connection, mirroring `connections.rs`'s `Connection`
 * (`#[serde(rename_all = "camelCase")]`, so the Rust snake_case fields cross
 * the wire as camelCase, matched here).
 *
 * `availableModels` and `keyRef` are B7b additions over A7's shape:
 * `availableModels` is the last set of model ids `connection_refresh_models`
 * discovered (or a manually-entered id from `model_add_manual`);
 * `enabledModels` is the user-curated subset of those actually usable for
 * refining. `keyRef` is `null`/absent for a keyless connection (e.g. local
 * Ollama) and a stable (non-secret) handle once an API key has been set —
 * the key material itself never crosses the wire.
 */
export interface Connection {
  id: string;
  providerKind: string;
  baseUrl: string;
  enabledModels: string[];
  availableModels: string[];
  keyRef?: string | null;
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

export interface ConnectionEditArgs extends Record<string, unknown> {
  id: string;
  /** Omit to leave the base URL unchanged. */
  baseUrl?: string;
  /** Omit to leave the stored key unchanged; pass `''` to clear it. */
  apiKey?: string;
  /** Omit to leave the enabled-models curation unchanged. */
  enabledModels?: string[];
}

/**
 * Updates an existing connection's base URL, API key, and/or enabled-model
 * curation. Backed by the `connection_edit` Tauri command; unlike
 * `connectionAdd`, this does not re-verify reachability (that's
 * `connectionTest`'s job).
 */
export function connectionEdit(args: ConnectionEditArgs): Promise<Connection> {
  return invoke<Connection>('connection_edit', args);
}

/** Deletes a connection (and its stored key). Backed by `connection_remove`. */
export function connectionRemove(id: string): Promise<void> {
  return invoke('connection_remove', { id });
}

export interface ConnectionTestArgs extends Record<string, unknown> {
  providerKind: string;
  baseUrl: string;
  apiKey: string;
}

/**
 * Probes reachability/auth for a provider without persisting anything.
 * Backed by the `connection_test` Tauri command: resolves on success,
 * rejects with a human-readable error message on failure.
 */
export function connectionTest(args: ConnectionTestArgs): Promise<void> {
  return invoke('connection_test', args);
}

/** Outcome of `connectionRefreshModels`: either the refreshed connection
 * (with its newly discovered `availableModels`), or a reason the caller
 * should fall back to the manual model-id control (`modelAddManual`). */
export type ConnectionRefreshResult =
  | { status: 'discovered'; connection: Connection }
  | { status: 'manual_required'; reason: string };

/**
 * Re-runs model discovery for an existing connection. Backed by the
 * `connection_refresh_models` Tauri command, which resolves with the
 * refreshed connection on success and rejects with a reason string both for
 * `DiscoveryResult::ManualEntryRequired` (no list endpoint, empty result,
 * etc.) and genuine failures (e.g. an unknown connection id) — this wrapper
 * normalizes both into a `'manual_required'` result so the UI always has a
 * fallback rather than a raw thrown rejection.
 */
export async function connectionRefreshModels(id: string): Promise<ConnectionRefreshResult> {
  try {
    const connection = await invoke<Connection>('connection_refresh_models', { id });
    return { status: 'discovered', connection };
  } catch (err) {
    return { status: 'manual_required', reason: err instanceof Error ? err.message : String(err) };
  }
}

/**
 * Accepts a manually-entered model id for a connection (the fallback path
 * when discovery degrades to manual entry). Backed by `model_add_manual`.
 */
export function modelAddManual(id: string, modelId: string): Promise<Connection> {
  return invoke<Connection>('model_add_manual', { id, modelId });
}

/**
 * Where API keys are kept at rest: the encrypted config file (default) or
 * the OS keychain. Backed by the `secrets_set` Tauri command — the real
 * backend (`secrets.rs`) is B10's job; this wrapper (and the Connections
 * screen's key-storage control that calls it) exists now so B10/B14's e2e
 * specs have something to drive.
 */
export type KeyStorageLocation = 'encrypted_file' | 'keychain';

/** Sets where API keys are stored at rest. Backed by `secrets_set`. */
export function secretsSet(location: KeyStorageLocation): Promise<void> {
  return invoke('secrets_set', { location });
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
 * Rejection strings `refine`/`tray_refine` reject with for those two
 * specific failure modes. Mirrors the constants of the same name in
 * `app/src-tauri/src/lib.rs` (`NO_ACTIVE_MODEL_ERROR`/
 * `PERMISSION_DENIED_ERROR`) — keep both sides in sync if either changes.
 */
export const NO_ACTIVE_MODEL_ERROR = 'no_active_model';
export const PERMISSION_DENIED_ERROR = 'permission_denied';

/**
 * Runs the default refine pipeline (A9/A5): captures the current selection,
 * calls the active model, and blind-injects the result in place, returning
 * the original (for restore), the refined text, and the model used. Rejects
 * with `NO_ACTIVE_MODEL_ERROR` or `PERMISSION_DENIED_ERROR` for those specific
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
