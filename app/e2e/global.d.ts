// Ambient types for the window hooks the Tauri IPC mock installs
// (app/e2e/mocks/tauri-mock.ts), so specs can reference them from
// `page.evaluate()` callbacks without a `tsc` error.
export {};

declare global {
  interface Window {
    /** Every command invoked via the mocked `__TAURI_INTERNALS__.invoke`, in order. */
    __TAURI_MOCK_CALLS__: Array<{ cmd: string; args?: Record<string, unknown> }>;
    /** Emits a mock Tauri event to any registered `listen()` callbacks. */
    __TAURI_MOCK_EMIT__?: (eventName: string, payload: unknown) => void;
  }
}
