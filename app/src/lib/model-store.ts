'use client';

// A small shared frontend store for the single global active model (B23).
//
// Before this, three surfaces each derived "which model is active" their own
// way: the Tray (`Tray.tsx`) read `models_list` directly, the Sidebar
// (`Sidebar.tsx`) read the first connection with an enabled model
// (`connection_list`, the Phase A heuristic), and the Capture panel showed
// only whatever the last refine returned. `useModelStore` unifies that: it
// reads B8's `models_list` (the real curated active-model layer) as the
// source of truth, and exposes the active model + a `refresh`/`setResult`
// pair so a pick made on one surface (Models screen, or the tray's
// `tray_set_active_model`) is reflected everywhere without prop-drilling.
//
// The backend is the actual cross-window shared state (the tray is a
// separate OS window from the settings shell), so "shared" here means every
// surface reads the same `models_list` and can re-pull it after a change —
// this hook is the single client-side accessor for that, not an in-memory
// store that would desync across windows.

import { useCallback, useEffect, useState } from 'react';
import {
  connectionList,
  modelsList,
  modelSetActive,
  type CuratedModel,
  type ModelRefArgs,
  type ModelsListResult,
} from '@/lib/ipc';

/** The label shown when nothing is active and no connection offers a model. */
export const NO_MODEL_LABEL = 'No model selected';

const EMPTY_RESULT: ModelsListResult = {
  models: [],
  hasActive: false,
  activeUnavailable: false,
  staleActiveModelId: null,
};

export interface ModelStore {
  /** The full curated `models_list` result (B8), or an empty result until loaded. */
  result: ModelsListResult;
  /** The single active model, or `null` when none is active. */
  activeModel: CuratedModel | null;
  /**
   * A display label for the active model: its id when one is active,
   * otherwise the first enabled model across connections (the Phase A
   * `active_provider` heuristic the backend still falls back to), otherwise
   * [`NO_MODEL_LABEL`].
   */
  activeModelLabel: string;
  /** True when an active model was chosen but is no longer available (S26). */
  activeUnavailable: boolean;
  /** Still fetching the initial `models_list`. */
  loading: boolean;
  /** Re-pulls `models_list` from the backend (call after a mutation elsewhere). */
  refresh: () => Promise<void>;
  /**
   * Sets the active model (the Models screen's own action) and updates the
   * store from the command's returned result, no extra round trip.
   */
  setActive: (args: ModelRefArgs) => Promise<ModelsListResult>;
  /**
   * Adopts a `ModelsListResult` a sibling command already returned (e.g. the
   * tray's `tray_set_active_model`, or `model_disable`), so a surface that
   * mutated state can push it into the store without a re-fetch.
   */
  setResult: (next: ModelsListResult) => void;
}

/** The active model in a result, or `null` if none is marked active. */
export function activeModelOf(result: ModelsListResult): CuratedModel | null {
  return result.models.find((m) => m.active) ?? null;
}

/**
 * Computes the active-model label from the curated result, falling back to
 * the first enabled model across `connections` (mirrors the backend's
 * `active_provider` default), then to [`NO_MODEL_LABEL`]. Pure, so it's
 * unit-testable independent of the fetching hook.
 */
export function activeModelLabelOf(
  result: ModelsListResult,
  connectionsFirstEnabled: string | null,
): string {
  const active = activeModelOf(result);
  if (active) return active.modelId;
  if (connectionsFirstEnabled) return connectionsFirstEnabled;
  return NO_MODEL_LABEL;
}

/**
 * Reads and tracks the shared active-model state. Fetches `models_list` on
 * mount (plus `connection_list` for the label fallback) and re-fetches on
 * `refresh`. Every surface that needs the active model calls this rather
 * than deriving it independently.
 */
export function useModelStore(): ModelStore {
  const [result, setResult] = useState<ModelsListResult>(EMPTY_RESULT);
  const [firstEnabled, setFirstEnabled] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      const [models, connections] = await Promise.all([
        modelsList().catch(() => EMPTY_RESULT),
        connectionList().catch(() => []),
      ]);
      // Tolerate a backend/mock that resolves something without a `models`
      // array (e.g. a stub returning `null`): treat it as an empty result
      // rather than crashing every consumer that reads `.models`.
      setResult(models && Array.isArray(models.models) ? models : EMPTY_RESULT);
      const list = Array.isArray(connections) ? connections : [];
      const withModel = list.find((c) => c.enabledModels.length > 0);
      setFirstEnabled(withModel ? withModel.enabledModels[0] : null);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      await refresh();
      if (cancelled) {
        // A refresh that resolved after unmount already called the setters
        // guarded by React; nothing else to undo.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [refresh]);

  const setActive = useCallback(async (args: ModelRefArgs) => {
    const next = await modelSetActive(args);
    setResult(next);
    return next;
  }, []);

  return {
    result,
    activeModel: activeModelOf(result),
    activeModelLabel: activeModelLabelOf(result, firstEnabled),
    activeUnavailable: result.activeUnavailable,
    loading,
    refresh,
    setActive,
    setResult,
  };
}
