// Thin wrapper around `@tauri-apps/api`'s `invoke` for the Tauri commands
// the frontend calls. Screens import from here rather than calling
// `invoke` directly, so the command names/payload shapes live in one place
// and stay in sync with `controls/index.json` and the backend
// (`app/src-tauri/src/*.rs`). A14's composition root is the one that
// registers these commands in the Tauri builder; every command below
// already exists on the backend (A4: settings, permission, hotkey).
//
// Extended by later screen tasks as they need more commands — this file is
// intentionally not exhaustive yet.

import { invoke } from '@tauri-apps/api/core';

export interface PermissionStatus {
  granted: boolean;
}

/** Reports whether the macOS Accessibility permission is currently granted. */
export function getPermissionStatus(): Promise<PermissionStatus> {
  return invoke('permission_status');
}

/** Reads a settings value by key, or `null` if it has never been set. */
export function settingsGet(key: string): Promise<string | null> {
  return invoke('settings_get', { key });
}

/** Upserts a settings value by key. */
export function settingsSet(key: string, value: string): Promise<void> {
  return invoke('settings_set', { key, value });
}
