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
        // 'paused'/'launch_at_login' mirror the settings keys the real
        // tray_pause/tray_resume/tray_set_launch_login commands persist
        // (B17); TEST_DATA.settings can still override them directly.
        settings: Object.assign(
          {
            paused: String(!!TEST_DATA.paused),
            launch_at_login: String(TEST_DATA.launchAtLogin !== false),
          },
          TEST_DATA.settings || {},
        ),
        // Mirrors the orchestrator's restore buffer (A9): the original text
        // from the most recent successful \`refine\`, for \`restore_original\`.
        lastOriginal: null,
        // Stateful connection store (B7): seeded from TEST_DATA.connections,
        // mutated in place by connection_add/edit/remove/refresh_models/
        // model_add_manual so a spec can add a connection then immediately
        // edit/refresh/remove it by the id the mock handed back.
        connections: (TEST_DATA.connections || []).map((c) =>
          Object.assign({ availableModels: [], keyRef: null }, c),
        ),
        nextConnectionId: (TEST_DATA.connections || []).length + 1,
        // Whether \`refine\` has rejected with \`refineFailure\` at least once
        // yet (B6) — only the first call fails unless \`refineFailureRepeats\`
        // is set, so a retry (\`capture-retry\`) can simulate a fallback chain
        // eventually succeeding.
        refineFailed: false,
        // The persisted active-model reference (B8): mirrors \`models.rs\`'s
        // settings-backed \`active_model\` key. \`null\` means none chosen yet.
        activeModel: TEST_DATA.activeModel || null,
        // Favorited (connection, model) pairs (B8/B20), keyed by \`modelKey\`.
        favorites: new Set(
          (TEST_DATA.favoriteModels || []).map((f) => modelKey(f.connectionId, f.modelId)),
        ),
      };

      function findConnection(id) {
        return state.connections.find((c) => c.id === id);
      }

      // Opaque per-(connection, model) key mirroring \`models.rs\`'s
      // \`model_key\` — never parsed apart, just compared for equality.
      function modelKey(connectionId, modelId) {
        return JSON.stringify([connectionId, modelId]);
      }

      // Builds the curated cross-connection model list mirroring
      // \`models.rs\`'s \`models_list_impl\`: every enabled model on every
      // connection, with active/favorite state layered on top.
      function buildModelsList() {
        const models = [];
        state.connections.forEach((c) => {
          (c.enabledModels || []).forEach((modelId) => {
            models.push({
              connectionId: c.id,
              modelId,
              providerKind: c.providerKind,
              active:
                !!state.activeModel &&
                state.activeModel.connectionId === c.id &&
                state.activeModel.modelId === modelId,
              favorite: state.favorites.has(modelKey(c.id, modelId)),
            });
          });
        });
        const hasActive = models.some((m) => m.active);
        const activeUnavailable = !!state.activeModel && !hasActive;
        return {
          models,
          hasActive,
          activeUnavailable,
          staleActiveModelId: activeUnavailable ? state.activeModel.modelId : null,
        };
      }

      // A real Tauri IPC round-trip serializes to/from JSON, so the
      // frontend never sees the exact same object reference the backend
      // holds. The mock calls handlers directly in-process (no
      // serialization), so it must clone before returning a connection --
      // otherwise a later mutation (e.g. connection_refresh_models filling
      // in availableModels) would silently rewrite an object a screen has
      // already put in React state, without a new reference to trigger a
      // re-render.
      function cloneConnection(connection) {
        return Object.assign({}, connection, {
          enabledModels: connection.enabledModels.slice(),
          availableModels: connection.availableModels.slice(),
        });
      }

      // Log of every invoked command, exposed on window so specs can assert
      // which backend commands fired (and with what args) without a real
      // backend to observe side effects on (e.g. a blind inject).
      window.__TAURI_MOCK_CALLS__ = [];

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

          // ── Settings key-value store (A4), used by General (A12),
          // Behavior (A8) et al ──
          case 'settings_get': {
            const key = args && args.key;
            return Object.prototype.hasOwnProperty.call(state.settings, key)
              ? state.settings[key]
              : null;
          }
          case 'settings_set': {
            const key = args && args.key;
            state.settings[key] = args && args.value;
            return null;
          }

          // ── Connections (A7/B7b), used by FirstRun + Connections ──
          case 'connection_add': {
            if (TEST_DATA.connectionAdd) {
              const added = Object.assign({ availableModels: [], keyRef: null }, TEST_DATA.connectionAdd);
              state.connections.push(added);
              return cloneConnection(added);
            }
            const id = String(state.nextConnectionId++);
            const providerKind = args && args.providerKind;
            const baseUrl = args && args.baseUrl;
            const apiKey = args && args.apiKey;
            // Mirrors the real backend's connect_and_store: enables the
            // first discovered model by default (falling back to 'default'
            // when discovery isn't seeded), but leaves availableModels
            // empty until an explicit connection_refresh_models call.
            const defaultModel =
              TEST_DATA.discoverModels && TEST_DATA.discoverModels.length > 0
                ? TEST_DATA.discoverModels[0]
                : 'default';
            const added = {
              id,
              providerKind,
              baseUrl,
              enabledModels: [defaultModel],
              availableModels: [],
              keyRef: apiKey ? id : null,
            };
            state.connections.push(added);
            return cloneConnection(added);
          }
          // Boot-time list read (App.tsx/Sidebar.tsx, A14): defaults to \`[]\`
          // so an unset fixture reads as "no connected provider yet".
          case 'connection_list':
            return state.connections.map(cloneConnection);
          case 'connection_edit': {
            const id = args && args.id;
            const existing = findConnection(id);
            if (!existing) throw \`no connection with id \${id}\`;
            if (args && Object.prototype.hasOwnProperty.call(args, 'baseUrl') && args.baseUrl !== undefined) {
              existing.baseUrl = args.baseUrl;
            }
            if (args && Object.prototype.hasOwnProperty.call(args, 'apiKey') && args.apiKey !== undefined) {
              existing.keyRef = args.apiKey ? id : null;
            }
            if (
              args &&
              Object.prototype.hasOwnProperty.call(args, 'enabledModels') &&
              args.enabledModels !== undefined
            ) {
              existing.enabledModels = args.enabledModels;
            }
            return cloneConnection(existing);
          }
          case 'connection_remove': {
            const id = args && args.id;
            state.connections = state.connections.filter((c) => c.id !== id);
            return null;
          }
          case 'connection_test': {
            if (TEST_DATA.testConnectionError) {
              throw TEST_DATA.testConnectionError;
            }
            return null;
          }
          case 'connection_refresh_models': {
            const id = args && args.id;
            const existing = findConnection(id);
            if (!existing) throw \`no connection with id \${id}\`;
            if (TEST_DATA.manualEntryRequired) {
              throw TEST_DATA.manualEntryRequired;
            }
            existing.availableModels = TEST_DATA.discoverModels ?? [];
            return cloneConnection(existing);
          }
          case 'model_add_manual': {
            const id = args && args.id;
            const modelId = args && args.modelId && args.modelId.trim();
            if (!modelId) {
              throw 'model id must not be empty';
            }
            const existing = findConnection(id);
            if (!existing) throw \`no connection with id \${id}\`;
            if (existing.availableModels.indexOf(modelId) === -1) {
              existing.availableModels = existing.availableModels.concat([modelId]);
            }
            if (existing.enabledModels.indexOf(modelId) === -1) {
              existing.enabledModels = existing.enabledModels.concat([modelId]);
            }
            return cloneConnection(existing);
          }

          // ── Key storage (B10, backend TBD) ──
          case 'secrets_set':
            return null;

          // ── Model curation and active selection (B8), used by Models;
          // \`tray_set_active_model\` (B9) shares the same active-model state
          // via the menu-bar tray's own entry point ──
          case 'models_list':
            return buildModelsList();
          case 'model_set_active':
          case 'tray_set_active_model': {
            const connectionId = args && args.connectionId;
            const modelId = args && args.modelId;
            const connection = findConnection(connectionId);
            if (!connection || connection.enabledModels.indexOf(modelId) === -1) {
              throw \`model \${modelId} is not enabled on connection \${connectionId}\`;
            }
            state.activeModel = { connectionId, modelId };
            return buildModelsList();
          }
          case 'model_disable': {
            const connectionId = args && args.connectionId;
            const modelId = args && args.modelId;
            const connection = findConnection(connectionId);
            if (!connection) throw \`no connection with id \${connectionId}\`;
            connection.enabledModels = connection.enabledModels.filter((m) => m !== modelId);
            return buildModelsList();
          }
          case 'model_toggle_favorite': {
            const connectionId = args && args.connectionId;
            const modelId = args && args.modelId;
            const key = modelKey(connectionId, modelId);
            if (state.favorites.has(key)) {
              state.favorites.delete(key);
            } else {
              state.favorites.add(key);
            }
            return buildModelsList();
          }
          case 'ollama_pull': {
            if (TEST_DATA.ollamaPullError) {
              throw TEST_DATA.ollamaPullError;
            }
            const modelId = args && args.modelId;
            const connection = state.connections.find((c) => c.providerKind === 'ollama');
            if (connection && connection.availableModels.indexOf(modelId) === -1) {
              connection.availableModels = connection.availableModels.concat([modelId]);
            }
            return { status: 'success', digest: null, total: null, completed: null, error: null };
          }

          // ── Default refine pipeline (A9): capture -> prompt -> model ->
          // inject, all in one backend call. Errors drive the no-active-model
          // (A11) and permission-needed (A13) capture states. A generic
          // \`refineFailure\` (B6) drives the error/retry state instead --
          // only the first call fails (unless \`refineFailureRepeats\`), so
          // retrying (\`capture-retry\`) can simulate a fallback chain
          // eventually succeeding. \`refineOutcome.status\` (B5/B6) drives the
          // review-and-confirm state instead of a blind inject.
          case 'refine': {
            if (TEST_DATA.refineError) {
              throw TEST_DATA.refineError;
            }
            if (TEST_DATA.refineFailure && (!state.refineFailed || TEST_DATA.refineFailureRepeats)) {
              state.refineFailed = true;
              throw TEST_DATA.refineFailure;
            }
            const outcome = TEST_DATA.refineOutcome || {
              original: 'original selection',
              refined: 'refined selection',
              model: 'test-model',
            };
            state.lastOriginal = outcome.original;
            return outcome;
          }
          // Same pipeline, triggered from the tray's Refine entry instead
          // of the Capture panel's button (A9/A14) -- shares \`refineOutcome\`/
          // \`refineError\`, mirroring the real backend's shared \`run_refine\`.
          // While paused (B17), the real backend suspends both the hotkey
          // and this tray entry point -- the mock enforces that too, so a
          // spec calling this directly (bypassing the frontend's own guard)
          // still observes the suspension.
          case 'tray_refine': {
            if (state.settings.paused === 'true') {
              throw 'paused';
            }
            if (TEST_DATA.refineError) {
              throw TEST_DATA.refineError;
            }
            const outcome = TEST_DATA.refineOutcome || {
              original: 'original selection',
              refined: 'refined selection',
              model: 'test-model',
            };
            state.lastOriginal = outcome.original;
            return outcome;
          }

          // ── Restore (A9/A10): re-fetch the buffered original, then inject
          // it back in place as two explicit calls (mirrors capture-restore's
          // declared backend commands in controls/capture.json). ──
          case 'restore_original': {
            if (state.lastOriginal == null) {
              throw 'no captured original to restore';
            }
            return state.lastOriginal;
          }
          case 'inject_text':
            return null;
          // Cancels the in-flight refine (A9); nothing for the mock to track.
          case 'cancel_refine':
            return null;

          // ── Tray (A9 display-only carve-out; only tray_quit is wired) ──
          case 'tray_quit':
            return null;

          // ── Tray status and pause (B17): persists via the same settings
          // keys General/other screens use, mirroring the real backend. ──
          case 'tray_pause':
            state.settings.paused = 'true';
            return null;
          case 'tray_resume':
            state.settings.paused = 'false';
            return null;
          case 'tray_check_updates':
            return TEST_DATA.updateCheckResult || { updateAvailable: false, version: null };
          case 'tray_set_launch_login': {
            const enabled = !!(args && args.enabled);
            state.settings.launch_at_login = String(enabled);
            return null;
          }

          // ── Hotkey (A6/A14) ──
          case 'hotkey_set':
            return { ok: true, conflict: false };

          default:
            console.warn('[tauri-mock] Unhandled command:', cmd, args);
            return undefined;
        }
      }

      // Install the mock __TAURI_INTERNALS__ before any code runs
      window.__TAURI_INTERNALS__ = {
        invoke(cmd, args) {
          window.__TAURI_MOCK_CALLS__.push({ cmd, args });
          return new Promise((resolve, reject) => {
            // Use setTimeout(0) to simulate async IPC
            setTimeout(() => {
              try {
                resolve(handleCommand(cmd, args));
              } catch (err) {
                reject(err);
              }
            }, 0);
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
