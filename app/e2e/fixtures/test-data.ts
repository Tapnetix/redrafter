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
  /** Backs the `permission_status` command's `granted` field. */
  permissionGranted: boolean;
  /** Backs `settings_get`/`settings_set`: key -> stored value. */
  settings: Record<string, string>;
}

export const DEFAULT_TEST_DATA: TestData = {
  appName: 'redrafter',
  permissionGranted: true,
  settings: {},
};
