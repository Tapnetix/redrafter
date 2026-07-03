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
  };
  /** Seeds the mocked `connection_list` command (A7/A14); defaults to `[]`.
   * App.tsx/Sidebar.tsx call this on boot to resolve the active model. */
  connections?: {
    id: string;
    providerKind: string;
    baseUrl: string;
    enabledModels: string[];
  }[];
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
