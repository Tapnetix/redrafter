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
  return invoke('permission_status');
}

// ── Settings key-value store (A4) ──
/** Reads a settings value by key, or `null` if it has never been set. */
export function settingsGet(key: string): Promise<string | null> {
  return invoke('settings_get', { key });
}

/** Upserts a settings value by key. */
export function settingsSet(key: string, value: string): Promise<void> {
  return invoke('settings_set', { key, value });
}

// ── Connections (A7) ──
/** A stored provider connection, mirroring `connections.rs`'s `Connection`. */
export interface Connection {
  id: string;
  providerKind: string;
  baseUrl: string;
  enabledModels: string[];
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
