import { renderHook, waitFor, act } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import {
  activeModelLabelOf,
  activeModelOf,
  NO_MODEL_LABEL,
  useModelStore,
} from './model-store';
import type { CuratedModel, ModelsListResult } from './ipc';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);

function model(overrides: Partial<CuratedModel> = {}): CuratedModel {
  return {
    connectionId: '1',
    modelId: 'claude-opus-4-6',
    providerKind: 'anthropic',
    active: false,
    favorite: false,
    ...overrides,
  };
}

function result(overrides: Partial<ModelsListResult> = {}): ModelsListResult {
  return {
    models: [],
    hasActive: false,
    activeUnavailable: false,
    staleActiveModelId: null,
    ...overrides,
  };
}

describe('activeModelOf', () => {
  it('returns the active model when one is marked active', () => {
    const active = model({ modelId: 'a', active: true });
    const res = result({ models: [model({ modelId: 'b' }), active], hasActive: true });
    expect(activeModelOf(res)).toEqual(active);
  });

  it('returns null when no model is active', () => {
    expect(activeModelOf(result({ models: [model()] }))).toBeNull();
  });
});

describe('activeModelLabelOf', () => {
  it("uses the active model's id when one is active", () => {
    const res = result({ models: [model({ modelId: 'opus', active: true })], hasActive: true });
    expect(activeModelLabelOf(res, 'fallback')).toBe('opus');
  });

  it('falls back to the first enabled connection model when none is active', () => {
    expect(activeModelLabelOf(result(), 'llama3')).toBe('llama3');
  });

  it('reports no model when neither an active model nor a fallback exists', () => {
    expect(activeModelLabelOf(result(), null)).toBe(NO_MODEL_LABEL);
  });
});

describe('useModelStore', () => {
  beforeEach(() => {
    mockedInvoke.mockReset();
  });
  afterEach(() => {
    mockedInvoke.mockReset();
  });

  function mockBackend(models: ModelsListResult | null, connections: unknown[] = []) {
    mockedInvoke.mockImplementation((cmd) => {
      if (cmd === 'models_list') return Promise.resolve(models);
      if (cmd === 'connection_list') return Promise.resolve(connections);
      return Promise.resolve(null);
    });
  }

  it('loads the active model from models_list on mount', async () => {
    mockBackend(result({ models: [model({ modelId: 'opus', active: true })], hasActive: true }));

    const { result: hook } = renderHook(() => useModelStore());

    await waitFor(() => expect(hook.current.loading).toBe(false));
    expect(hook.current.activeModel?.modelId).toBe('opus');
    expect(hook.current.activeModelLabel).toBe('opus');
  });

  it('falls back to the first enabled connection model when nothing is active', async () => {
    mockBackend(result(), [
      { id: '1', providerKind: 'ollama', baseUrl: 'http://x', enabledModels: ['llama3'] },
    ]);

    const { result: hook } = renderHook(() => useModelStore());

    await waitFor(() => expect(hook.current.loading).toBe(false));
    expect(hook.current.activeModel).toBeNull();
    expect(hook.current.activeModelLabel).toBe('llama3');
  });

  it('tolerates a null models_list response without crashing', async () => {
    mockBackend(null, []);

    const { result: hook } = renderHook(() => useModelStore());

    await waitFor(() => expect(hook.current.loading).toBe(false));
    expect(hook.current.activeModelLabel).toBe(NO_MODEL_LABEL);
    expect(hook.current.result.models).toEqual([]);
  });

  it('setActive invokes model_set_active and adopts the returned result', async () => {
    mockBackend(result());
    const activated = result({
      models: [model({ modelId: 'opus', active: true })],
      hasActive: true,
    });
    const { result: hook } = renderHook(() => useModelStore());
    await waitFor(() => expect(hook.current.loading).toBe(false));

    mockedInvoke.mockImplementation((cmd) => {
      if (cmd === 'model_set_active') return Promise.resolve(activated);
      return Promise.resolve(null);
    });

    await act(async () => {
      await hook.current.setActive({ connectionId: '1', modelId: 'opus' });
    });

    expect(mockedInvoke).toHaveBeenCalledWith('model_set_active', { connectionId: '1', modelId: 'opus' });
    expect(hook.current.activeModel?.modelId).toBe('opus');
  });

  it('setResult adopts a result a sibling command already returned', async () => {
    mockBackend(result());
    const { result: hook } = renderHook(() => useModelStore());
    await waitFor(() => expect(hook.current.loading).toBe(false));

    const pushed = result({ models: [model({ modelId: 'sonnet', active: true })], hasActive: true });
    act(() => {
      hook.current.setResult(pushed);
    });

    expect(hook.current.activeModel?.modelId).toBe('sonnet');
  });
});
