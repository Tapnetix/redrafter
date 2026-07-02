/**
 * Tauri IPC Mock Layer for Playwright E2E tests.
 *
 * Intercepts window.__TAURI_INTERNALS__.invoke() and
 * window.__TAURI_INTERNALS__.transformCallback() so that all
 * @tauri-apps/api calls resolve with canned test data, without a real
 * Tauri backend running.
 *
 * Usage: import and call getTauriMockScript(testData) to get a string
 * that can be passed to page.addInitScript(). See ../fixtures/setup.ts
 * for the shared Playwright fixture that wires this in for every spec.
 *
 * Command handling starts minimal; each screen adds `case` branches for the
 * commands it actually invokes.
 */

import type { TestData } from '../fixtures/test-data';

export function getTauriMockScript(data: TestData): string {
  const serialized = JSON.stringify(data);

  return `
    (() => {
      const TEST_DATA = ${serialized};

      // Mutable mock state, seeded from TEST_DATA. Commands that simulate a
      // stateful backend (e.g. a permission being granted mid-test) mutate
      // this rather than TEST_DATA itself.
      const state = {
        permissionGranted: !!TEST_DATA.permissionGranted,
      };

      // Callback registry for event listeners (used by listen/transformCallback)
      const callbacks = {};
      let callbackId = 0;

      // Event listener registry: eventName -> Set<callbackId>
      const eventListeners = {};

      // Command handler map: command name -> handler function
      function handleCommand(cmd, args) {
        switch (cmd) {
          // ── Permission (A4/A6/A12) ──
          case 'permission_status':
            // Mirrors src-tauri/src/permission.rs's PermissionStatus shape.
            return { granted: state.permissionGranted };
          case 'permission_open_settings':
            // The real command just opens System Settings; it doesn't grant
            // the permission itself. The mock simulates the user granting it
            // in System Settings so the next permission_status poll picks it
            // up, without needing a real OS-level Accessibility prompt.
            state.permissionGranted = true;
            return null;

          // ── Settings key-value store (A4), used by General (A12) et al ──
          case 'settings_get': {
            const key = args && args.key;
            const settings = TEST_DATA.settings || {};
            return Object.prototype.hasOwnProperty.call(settings, key) ? settings[key] : null;
          }
          case 'settings_set':
            return null;

          // ── Connections (A7), used by FirstRun ──
          case 'connection_add':
            return (
              TEST_DATA.connectionAdd ?? {
                id: '1',
                providerKind: args && args.providerKind,
                baseUrl: args && args.baseUrl,
                enabledModels: ['default'],
              }
            );

          default:
            console.warn('[tauri-mock] Unhandled command:', cmd, args);
            return undefined;
        }
      }

      // Every invoked command (name + args) is recorded here so specs can
      // assert which backend commands a user action triggered, e.g.:
      //   const calls = await page.evaluate(() => window.__TAURI_MOCK_CALLS__);
      window.__TAURI_MOCK_CALLS__ = [];

      // Install the mock __TAURI_INTERNALS__ before any code runs
      window.__TAURI_INTERNALS__ = {
        invoke(cmd, args) {
          window.__TAURI_MOCK_CALLS__.push({ cmd, args });
          return new Promise((resolve) => {
            // Use setTimeout(0) to simulate async IPC
            setTimeout(() => resolve(handleCommand(cmd, args)), 0);
          });
        },

        transformCallback(callback, once) {
          const id = callbackId++;
          callbacks[id] = { callback, once: !!once };
          return id;
        },

        metadata: {
          currentWindow: { label: 'main' },
          currentWebview: { label: 'main' },
        },
      };

      // The @tauri-apps/api/event module uses transformCallback + invoke
      // ('plugin:event|listen', ...); intercept the event plugin commands too.
      const originalInvoke = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = function (cmd, args) {
        if (cmd === 'plugin:event|listen') {
          const event = args?.event;
          const handler = args?.handler;
          if (event && handler !== undefined) {
            if (!eventListeners[event]) eventListeners[event] = new Set();
            eventListeners[event].add(handler);
          }
          return Promise.resolve(handler);
        }
        if (cmd === 'plugin:event|unlisten') {
          const event = args?.event;
          const handler = args?.handler;
          if (event && eventListeners[event]) {
            eventListeners[event].delete(handler);
          }
          return Promise.resolve();
        }
        return originalInvoke.call(this, cmd, args);
      };

      // Helper to emit mock events from test code
      window.__TAURI_MOCK_EMIT__ = function (eventName, payload) {
        const listeners = eventListeners[eventName];
        if (listeners) {
          listeners.forEach((handlerId) => {
            const entry = callbacks[handlerId];
            if (entry && entry.callback) {
              entry.callback({ event: eventName, id: 0, payload });
              if (entry.once) {
                delete callbacks[handlerId];
                listeners.delete(handlerId);
              }
            }
          });
        }
      };
    })();
  `;
}
