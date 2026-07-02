/**
 * Shared test data for E2E tests.
 *
 * This is a scaffold: `TestData` intentionally starts near-empty. Later
 * scenario tasks (see app/e2e/specs/*.spec.ts) extend this shape and the
 * corresponding command handlers in ../mocks/tauri-mock.ts as they wire up
 * real Tauri commands, rather than speculating on the full contract here.
 */

export interface TestData {
  appName: string;
  /** Canned response for the `connection_add` command. Defaults to echoing
   * back the caller's args (see `../mocks/tauri-mock.ts`) when unset. */
  connectionAdd?: {
    id: string;
    providerKind: string;
    baseUrl: string;
    enabledModels: string[];
  };
}

export const DEFAULT_TEST_DATA: TestData = {
  appName: 'redrafter',
};
