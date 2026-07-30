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
  // `Promise.resolve` is a no-op in production (invoke already returns a
  // promise) but guards the unit-test/non-Tauri path: a mock-reset `invoke`
  // returns `undefined`, and calling `.then` on it in a mount effect would
  // throw synchronously and crash the render. Wrapping routes that case into
  // the caller's `.catch` instead. See feedback-cues for the same pattern.
  return Promise.resolve(invoke<PermissionStatus>('permission_status'));
}

// ── Settings key-value store (A4) ──
/** Reads a settings value by key, or `null` if it has never been set. */
export function settingsGet(key: string): Promise<string | null> {
  // Wrapped for the same reason as getPermissionStatus: called in mount
  // effects, so a mock-reset `undefined` must not throw synchronously.
  return Promise.resolve(invoke<string | null>('settings_get', { key }));
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
  // Wrapped for the same reason as getPermissionStatus (called during boot).
  return Promise.resolve(invoke<Connection[]>('connection_list'));
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

// ── Model curation and active selection (B8) ──
/**
 * One curated (enabled) model, aggregated across every connection.
 * Mirrors `models.rs`'s `CuratedModel`. The same `modelId` string can
 * appear more than once (e.g. the same model enabled on two different
 * Ollama endpoints) — `connectionId` disambiguates which row a given
 * action (`modelSetActive`/`modelDisable`/`modelToggleFavorite`) applies to.
 */
export interface CuratedModel {
  connectionId: string;
  modelId: string;
  providerKind: string;
  /** Whether this is the single global active model `refine` uses. */
  active: boolean;
  /** Whether this model is starred (surfaces in the tray quick-switch, B9/B20). */
  favorite: boolean;
}

/**
 * The response every model-curation command resolves with, mirroring
 * `models.rs`'s `ModelsListResult` — so the Models screen never needs a
 * second round trip to see the effect of an action.
 */
export interface ModelsListResult {
  models: CuratedModel[];
  /** Whether some model in `models` is currently active. */
  hasActive: boolean;
  /**
   * True when an active model was chosen but is no longer enabled (its
   * connection was removed, or the model itself was disabled) — drives the
   * Models screen's "active model unavailable" banner (S26).
   */
  activeUnavailable: boolean;
  /** The stale active model's id, set alongside `activeUnavailable`. */
  staleActiveModelId?: string | null;
}

/** Lists every enabled model across every connection, with active/favorite
 * state. Backed by the `models_list` Tauri command. */
export function modelsList(): Promise<ModelsListResult> {
  return invoke<ModelsListResult>('models_list');
}

export interface ModelRefArgs extends Record<string, unknown> {
  connectionId: string;
  modelId: string;
}

/**
 * Sets the single global active model — the one `refine` uses. Backed by
 * `model_set_active`, which rejects (rather than silently accepting) a
 * model that isn't currently enabled.
 */
export function modelSetActive(args: ModelRefArgs): Promise<ModelsListResult> {
  return invoke<ModelsListResult>('model_set_active', args);
}

/** Disables a model (removes it from its connection's enabled set). Backed
 * by `model_disable`. Disabling the active model leaves it stale rather
 * than clearing it, surfacing as `activeUnavailable` on the next list. */
export function modelDisable(args: ModelRefArgs): Promise<ModelsListResult> {
  return invoke<ModelsListResult>('model_disable', args);
}

/** Toggles favorite status for a model. Backed by `model_toggle_favorite`. */
export function modelToggleFavorite(args: ModelRefArgs): Promise<ModelsListResult> {
  return invoke<ModelsListResult>('model_toggle_favorite', args);
}

/**
 * Sets the active model from the menu-bar tray's quick-switch (`Tray.tsx`,
 * B9) — favorites or the expanded per-provider list. Same shape and guard
 * (rejects a model that isn't enabled) as `modelSetActive`, but a distinct
 * backend command (`tray_set_active_model`, per `controls/tray.json`) so
 * the native OS tray menu (B23/tray.rs) and this in-app preview both call
 * through the tray's own entry point rather than the Models screen's.
 */
export function traySetActiveModel(args: ModelRefArgs): Promise<ModelsListResult> {
  return invoke<ModelsListResult>('tray_set_active_model', args);
}

/** Mirrors `llm-provider`'s `PullProgress` (via `models.rs`'s `ollama_pull`):
 * the terminal line of an Ollama model pull's NDJSON progress stream. */
export interface OllamaPullProgress {
  status: string;
  digest?: string | null;
  total?: number | null;
  completed?: number | null;
  error?: string | null;
}

/**
 * Pulls (downloads) `modelId` from the first configured Ollama connection,
 * resolving once the download finishes (or rejecting on failure). Backed by
 * the `ollama_pull` Tauri command; on success the model becomes available
 * to enable/curate on the Models screen (not auto-enabled).
 */
export function ollamaPull(modelId: string): Promise<OllamaPullProgress> {
  return invoke<OllamaPullProgress>('ollama_pull', { modelId });
}

// ── Permission open-settings (A9/A13) ──
/**
 * Opens macOS System Settings so the user can re-grant Accessibility. Mirrors
 * `permission_status` above; the caller re-polls that to see the change.
 */
export function permissionOpenSettings(): Promise<void> {
  return invoke('permission_open_settings');
}

// ── Claude Code login (opt-in alternative to a Console API key) ──
/** What `claudeCodeStatus` reports about the signed-in Claude Code account. */
export interface ClaudeCodeSummary {
  /** e.g. "max" / "pro", so the user can confirm which account. */
  subscriptionType?: string | null;
  /** Whether the credential is permitted to run inference. */
  canInfer: boolean;
}

/**
 * Reports whether a usable Claude Code login exists on this machine, so the
 * Connections screen can offer the shortcut only when it would work. Rejects
 * with an actionable reason (not signed in / expired / not permitted to run
 * inference) rather than a bare false.
 */
export function claudeCodeStatus(): Promise<ClaudeCodeSummary> {
  return invoke<ClaudeCodeSummary>('claude_code_status');
}

/**
 * Adds (or refreshes) the connection that refines through the Claude Code
 * login. No token is copied into redrafter's own store — the connection just
 * records that it authenticates this way, and the credential is read from
 * Claude Code per call.
 */
export function claudeCodeConnect(): Promise<Connection> {
  return invoke<Connection>('claude_code_connect');
}

// ── External links ──
/**
 * Opens `url` in the user's real browser. A plain
 * `<a href target="_blank">` is inert inside a Tauri webview — there is no
 * tab to open and the webview won't navigate off its own origin — so every
 * "Get an API key ↗" link routes through the `open_external` command
 * instead. The backend rejects anything that isn't a plain `http(s)` URL.
 */
export function openExternal(url: string): Promise<void> {
  return invoke('open_external', { url });
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

// ── Tray status and pause (B17) ──
/**
 * Pauses global capturing: while paused, both the global hotkey and the
 * tray's own "Refine selection" stop triggering `refine`/`tray_refine`. The
 * real backend (B23) suspends the hotkey handler and persists the paused
 * flag via the settings store; the frontend reflects it in the tray's status
 * line. Backed by the `tray_pause` Tauri command.
 */
export function trayPause(): Promise<void> {
  return invoke('tray_pause');
}

/** Resumes global capturing after `trayPause`. Backed by `tray_resume`. */
export function trayResume(): Promise<void> {
  return invoke('tray_resume');
}

/** Result of a `tray_check_updates` call: whether a newer version is
 * available, and which one. */
export interface CheckUpdatesResult {
  updateAvailable: boolean;
  version?: string | null;
}

/**
 * Triggers an application-update check from the tray. Backed by the
 * `tray_check_updates` Tauri command; B23/C2 wire it to the real
 * `tauri-plugin-updater` check, this wrapper just calls the command and
 * reports its result.
 */
export function checkUpdates(): Promise<CheckUpdatesResult> {
  return invoke<CheckUpdatesResult>('tray_check_updates');
}

/**
 * Toggles whether redrafter launches automatically at login. Backed by the
 * `tray_set_launch_login` Tauri command, which persists the preference and
 * (in B23's real wiring) drives `tauri-plugin-autostart`.
 */
export function setLaunchAtLogin(enabled: boolean): Promise<void> {
  return invoke('tray_set_launch_login', { enabled });
}

// ── History (C4) ──
/**
 * One recorded past refine, mirroring `history.rs`'s `HistoryEntry`
 * (`#[serde(rename_all = "camelCase")]`, so the Rust snake_case fields cross
 * the wire as camelCase, matched here). `createdAt` is Unix epoch
 * milliseconds; `command`, when present, is the inline command/preset
 * trigger (e.g. `/formal`) that drove the original refine.
 */
export interface HistoryEntry {
  id: string;
  original: string;
  refined: string;
  model: string;
  createdAt: number;
  command?: string | null;
}

/** Lists every recorded refine, most recent first. Backed by `history_list`. */
export function historyList(): Promise<HistoryEntry[]> {
  return invoke<HistoryEntry[]>('history_list');
}

/** Returns the full detail of a single past refine. Backed by `history_get`. */
export function historyGet(id: string): Promise<HistoryEntry> {
  return invoke<HistoryEntry>('history_get', { id });
}

/**
 * Restores a past entry's original text back into the focused app (a single
 * call — the backend looks the entry up and injects it, reusing the same
 * inject path as `restoreOriginal`/`injectText`). Resolves with the restored
 * original. Backed by the `history_restore` Tauri command.
 */
export function historyRestore(id: string): Promise<string> {
  return invoke<string>('history_restore', { id });
}

export interface HistoryReRefineArgs extends Record<string, unknown> {
  id: string;
  /** Omit to re-run with the entry's own model. */
  model?: string;
}

/**
 * Re-runs refine on a past entry's original text (optionally with a
 * different model), injects the new result, and records it as a fresh
 * history entry — the past entry itself is left untouched. Backed by the
 * `history_rerefine` Tauri command.
 */
export function historyReRefine(args: HistoryReRefineArgs): Promise<HistoryEntry> {
  return invoke<HistoryEntry>('history_rerefine', args);
}

/** Looks up a past entry so the caller can copy its refined text (C12). Backed by history_copy. */
export function historyCopy(id: string): Promise<HistoryEntry> {
  return invoke<HistoryEntry>('history_copy', { id });
}

/** Clears every recorded refine (C15). Backed by history_clear. */
export function historyClear(): Promise<void> {
  return invoke('history_clear');
}

// ── Presets (C3/C3b) ──
/** A single before/after few-shot example, mirroring `presets.rs`'s
 * `PresetExample`. */
export interface PresetExample {
  before: string;
  after: string;
}

/**
 * A stored preset — the direction (and optional overrides) a `/trigger`
 * resolves to. Mirrors `presets.rs`'s `Preset` (`#[serde(rename_all =
 * "camelCase")]`). `builtin`/`overridden` are computed by the backend on
 * every read (never persisted): `builtin` is whether `trigger` names one of
 * the shipped defaults, and `overridden` is whether a user-saved row shadows
 * it — the Presets screen's badge/override-warning data (C3/C8).
 */
export interface Preset {
  trigger: string;
  direction: string;
  model?: string | null;
  lang?: string | null;
  inject?: string | null;
  examples: PresetExample[];
  builtin: boolean;
  overridden: boolean;
}

/** The result of `presetImport`, mirroring `presets.rs`'s
 * `PresetImportResult`: every trigger actually imported, and the subset of
 * those that already resolved to something (so were overwritten/overridden
 * rather than newly added). */
export interface PresetImportResult {
  imported: string[];
  conflicts: string[];
}

/** Lists every available preset (built-in + user, with override status).
 * Backed by the `preset_list` Tauri command. */
export function presetList(): Promise<Preset[]> {
  return invoke<Preset[]>('preset_list');
}

export interface PresetSaveArgs extends Record<string, unknown> {
  trigger: string;
  direction: string;
  model?: string;
  lang?: string;
  inject?: string;
  examples: PresetExample[];
}

/**
 * Creates or updates a user preset. Saving under a built-in's trigger
 * creates/updates the override that shadows it (surfaced by `overridden` on
 * the next `presetList`). Backed by the `preset_save` Tauri command.
 */
export function presetSave(args: PresetSaveArgs): Promise<Preset> {
  return invoke<Preset>('preset_save', args);
}

/** Deletes a user preset (or override) — errors for an unmodified built-in
 * (see `presetResetDefault`, the "go back to the shipped default"
 * operation). Backed by the `preset_delete` Tauri command. */
export function presetDelete(trigger: string): Promise<void> {
  return invoke('preset_delete', { trigger });
}

/** Copies the resolved preset at `trigger` (built-in, override, or user)
 * into a new user preset under `newTrigger`, leaving the original untouched
 * — the "Duplicate instead" alternative to overriding a built-in in place.
 * Backed by the `preset_duplicate` Tauri command. */
export function presetDuplicate(trigger: string, newTrigger: string): Promise<Preset> {
  return invoke<Preset>('preset_duplicate', { trigger, newTrigger });
}

/** Restores a built-in preset the user overrode back to its shipped
 * default, discarding the override. Backed by the `preset_reset_default`
 * Tauri command. */
export function presetResetDefault(trigger: string): Promise<void> {
  return invoke('preset_reset_default', { trigger });
}

/** Exports every user-saved preset (overrides and plain user presets) as a
 * portable JSON string. Backed by the `preset_export` Tauri command. */
export function presetExport(): Promise<string> {
  return invoke<string>('preset_export');
}

/** Imports a JSON array of presets (the shape `presetExport` produces),
 * merging them into the user store and flagging trigger conflicts. Backed
 * by the `preset_import` Tauri command. */
export function presetImport(json: string): Promise<PresetImportResult> {
  return invoke<PresetImportResult>('preset_import', { json });
}
