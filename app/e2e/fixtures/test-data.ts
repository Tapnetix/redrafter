/**
 * Shared test data for E2E tests.
 *
 * This is a scaffold: `TestData` intentionally starts near-empty. Later
 * scenario tasks (see app/e2e/specs/*.spec.ts) extend this shape and the
 * corresponding command handlers in ../mocks/tauri-mock.ts as they wire up
 * real Tauri commands, rather than speculating on the full contract here.
 */

/** Mirrors `RefineOutcome` from src-tauri/src/orchestrator.rs. */
export interface RefineFixture {
  original: string;
  refined: string;
  model: string;
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
