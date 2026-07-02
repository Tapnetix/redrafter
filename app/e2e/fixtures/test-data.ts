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
  /** Seed for the mocked `permission_status` command's `granted` field (A6/A12). */
  permissionGranted?: boolean;
  /** Backs `settings_get`/`settings_set`: key -> stored value (A12). */
  settings?: Record<string, string>;
}

export const DEFAULT_TEST_DATA: TestData = {
  appName: 'redrafter',
  permissionGranted: false,
  settings: {},
};
