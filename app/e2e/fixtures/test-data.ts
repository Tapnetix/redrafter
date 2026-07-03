/**
 * Shared test data for E2E tests.
 *
 * This is a scaffold: `TestData` intentionally starts near-empty. Later
 * scenario tasks (see app/e2e/specs/*.spec.ts) extend this shape and the
 * corresponding command handlers in ../mocks/tauri-mock.ts as they wire up
 * real Tauri commands, rather than speculating on the full contract here.
 */

/** Mirrors `RefineOutcome` from src-tauri/src/orchestrator.rs, widened
 * (B6) with the optional `status` tag `RefineFlow` (B5) adds when the
 * configured inject mode is `review` rather than `blind` — see
 * `app/src/lib/ipc.ts`'s `RefineOutcome`. Leave unset for the Phase A
 * blind-inject behavior. */
export interface RefineFixture {
  original: string;
  refined: string;
  model: string;
  status?: 'injected' | 'pending_review';
}

/** A generic (non-sentinel) `refine` failure — B6's error/retry state.
 * Optionally carries the fallback-model chain the backend was configured
 * to try, which the error state shows when present. */
export interface RefineFailureFixture {
  message: string;
  fallbackModels?: string[];
}

export interface TestData {
  appName: string;
  /** Seed for the mocked `permission_status` command's `granted` field (A6/A12). */
  permissionGranted?: boolean;
  /** Backs `settings_get`/`settings_set`: key -> stored value (A12). */
  settings?: Record<string, string>;
  /** Canned response for the `connection_add` command (A7). Defaults to echoing
   * back the caller's args (see `../mocks/tauri-mock.ts`) when unset. */
  connectionAdd?: {
    id: string;
    providerKind: string;
    baseUrl: string;
    enabledModels: string[];
    availableModels?: string[];
    keyRef?: string | null;
  };
  /** Seeds the mocked `connection_list` command (A7/A14); defaults to `[]`.
   * App.tsx/Sidebar.tsx call this on boot to resolve the active model. Also
   * the mock's stateful connection store (B7): `connection_add` appends to
   * it, `connection_edit`/`connection_remove`/`connection_refresh_models`/
   * `model_add_manual` look entries up by id and mutate it in place. */
  connections?: {
    id: string;
    providerKind: string;
    baseUrl: string;
    enabledModels: string[];
    availableModels?: string[];
    keyRef?: string | null;
  }[];
  /**
   * Seeds the mocked `connection_add`/`connection_test`'s default model
   * list (B7): when set, a fresh `connection_add` call enables the first
   * entry by default (mirroring the real backend's `connect_and_store`),
   * and `connection_refresh_models` resolves with this full list as the
   * connection's `availableModels`. Leave unset to exercise the
   * manual-entry-required fallback (`manualEntryRequired`).
   */
  discoverModels?: string[];
  /**
   * When set, the mocked `connection_test` command rejects with this
   * message instead of resolving (B7: surfaces the connection-sheet's
   * error state).
   */
  testConnectionError?: string;
  /**
   * When set, the mocked `connection_refresh_models` command rejects with
   * this reason (mirroring `DiscoveryResult::ManualEntryRequired`) instead
   * of resolving with discovered models — drives the manual model-id
   * fallback (B7/S7).
   */
  manualEntryRequired?: string;
  /** Seeds the mocked `refine` command's success response (A9). */
  refineOutcome?: RefineFixture;
  /**
   * When set, the mocked `refine` command rejects with this error code
   * instead of resolving, driving the no-active-model (A11) and
   * permission-needed (A13) capture states.
   */
  refineError?: 'no_active_model' | 'permission_denied';
  /**
   * When set, the mocked `refine` command's *first* call rejects with this
   * generic failure (B6's error/retry state) instead of resolving; every
   * call after that (e.g. the user clicking `capture-retry`) resolves with
   * `refineOutcome` instead — mirroring a fallback chain eventually
   * succeeding on retry. Mutually exclusive with `refineError` (the two
   * sentinel failure modes short-circuit before this is consulted). Set
   * `refineFailureRepeats` to have every call fail instead of just the first.
   */
  refineFailure?: RefineFailureFixture;
  /** When true, every `refine` call rejects with `refineFailure` (rather
   * than just the first), for scenarios asserting a retry that fails again. */
  refineFailureRepeats?: boolean;
  /**
   * Seeds the mocked `models_list`/`model_set_active`'s persisted active
   * model (B8): the (connection, model) pair `models_list` reports as
   * active, until `model_set_active` changes it. Leave unset to start with
   * no active model chosen.
   */
  activeModel?: { connectionId: string; modelId: string };
  /** Seeds the mocked `models_list`'s starred models (B8/B20): a list of
   * (connection, model) pairs that start out favorited. */
  favoriteModels?: { connectionId: string; modelId: string }[];
  /**
   * When set, the mocked `ollama_pull` command rejects with this message
   * instead of resolving (B22's pull-failure state).
   */
  ollamaPullError?: string;
  /** Seeds the mocked tray's initial paused state (B17): whether global
   * capturing starts paused, mirroring the `paused` settings key
   * `tray_pause`/`tray_resume` persist. Defaults to `false`. */
  paused?: boolean;
  /** Seeds the mocked tray's initial launch-at-login state (B17), mirroring
   * the `launch_at_login` settings key `tray_set_launch_login` persists.
   * Defaults to `true` (per wireframes/tray.html's default checked state). */
  launchAtLogin?: boolean;
  /** Canned response for the mocked `tray_check_updates` command (B17).
   * Defaults to `{ updateAvailable: false }` (already up to date). */
  updateCheckResult?: { updateAvailable: boolean; version?: string | null };
  /**
   * Seeds the mocked `history_list`'s stateful history store (C4): a list
   * of past refines, most recent first. `history_restore`/`history_rerefine`
   * look entries up by id; `history_rerefine` prepends a new entry (mutating
   * this in place) rather than replacing the entry it re-ran.
   */
  historyEntries?: {
    id: string;
    original: string;
    refined: string;
    model: string;
    createdAt: number;
    command?: string | null;
  }[];
  /**
   * When set, the mocked `history_rerefine` command's new entry uses this
   * as its `refined` text instead of the default `"<original> (re-refined)"`
   * placeholder (C4).
   */
  historyRerefineRefined?: string;
  /**
   * When set, a `hotkey_set` call for exactly this combo (e.g.
   * `"Ctrl+Alt+T"`) resolves with `{ ok: false, conflict: true }` instead of
   * succeeding -- mirroring `hotkey.rs`'s `apply_combo` reporting the combo
   * already registered elsewhere -- so a spec can drive the General
   * screen's rebind-conflict state (C6/S34) without a real conflicting
   * registration. Every other combo still resolves `{ ok: true, conflict:
   * false }` and is persisted (see `../mocks/tauri-mock.ts`).
   */
  hotkeyConflictCombo?: string;
}

export const DEFAULT_TEST_DATA: TestData = {
  appName: 'redrafter',
  permissionGranted: false,
  settings: {},
  refineOutcome: {
    original: 'original selection',
    refined: 'refined selection',
    model: 'test-model',
  },
};
