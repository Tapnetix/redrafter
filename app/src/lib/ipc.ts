/**
 * Thin wrapper around @tauri-apps/api's `invoke`, so screens call one typed
 * surface instead of importing @tauri-apps/api directly. Each screen task
 * adds the commands it needs; A14 (`app/src-tauri/src/lib.rs` +
 * `app/src/App.tsx`) is the composition root that registers every command
 * on the Rust side and owns this file's routing-adjacent additions
 * (`theme.ts` wiring, etc.) — it extends rather than replaces what earlier
 * tasks add here.
 */
import { invoke } from '@tauri-apps/api/core';

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
 * Connects a provider (the backend verifies reachability) and persists it
 * with a default enabled model. Backed by the `connection_add` Tauri
 * command in `connections.rs`.
 */
export function connectionAdd(args: ConnectionAddArgs): Promise<Connection> {
  return invoke<Connection>('connection_add', args);
}
