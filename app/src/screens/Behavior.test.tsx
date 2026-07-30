import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import Behavior, { DEFAULT_DIRECTION, fallbackModelGroups } from './Behavior';
import * as ipc from '@/lib/ipc';

vi.mock('@/lib/ipc', () => ({
  settingsGet: vi.fn(),
  settingsSet: vi.fn(),
  // Backs the fallback dropdown, which now offers the user's real models.
  modelsList: vi.fn(),
}));

const mockedIpc = vi.mocked(ipc);

describe('Behavior', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mockedIpc.settingsGet.mockResolvedValue(null);
    mockedIpc.settingsSet.mockResolvedValue(undefined);
    mockedIpc.modelsList.mockResolvedValue({
      models: [],
      hasActive: false,
      activeUnavailable: false,
      staleActiveModelId: null,
    });
  });

  it('prefills the default direction textarea with the built-in default when no setting is stored', async () => {
    render(<Behavior />);

    await waitFor(() => expect(mockedIpc.settingsGet).toHaveBeenCalledWith('refine.default_direction'));

    const textarea = await screen.findByTestId('default-direction');
    expect(textarea).toHaveValue(DEFAULT_DIRECTION);
  });

  it('prefills the textarea with a previously saved direction from settings_get', async () => {
    mockedIpc.settingsGet.mockResolvedValue('Make it punchier, keep the facts.');

    render(<Behavior />);

    const textarea = await screen.findByTestId('default-direction');
    await waitFor(() => expect(textarea).toHaveValue('Make it punchier, keep the facts.'));
  });

  it('persists an edited direction via settings_set on blur', async () => {
    render(<Behavior />);

    const textarea = await screen.findByTestId('default-direction');
    await waitFor(() => expect(mockedIpc.settingsGet).toHaveBeenCalledWith('refine.default_direction'));

    fireEvent.change(textarea, { target: { value: 'Tighten it up, no jokes.' } });
    fireEvent.blur(textarea);

    await waitFor(() =>
      expect(mockedIpc.settingsSet).toHaveBeenCalledWith('refine.default_direction', 'Tighten it up, no jokes.'),
    );
    expect(textarea).toHaveValue('Tighten it up, no jokes.');
  });

  it('falls back to the built-in default when settings_get rejects', async () => {
    mockedIpc.settingsGet.mockRejectedValue(new Error('no backend'));

    render(<Behavior />);

    const textarea = await screen.findByTestId('default-direction');
    await waitFor(() => expect(textarea).toHaveValue(DEFAULT_DIRECTION));
  });
});

// ── Regressions: the fallback picker offered fabricated models ──────────────
// It rendered a hardcoded copy of the wireframe's example dropdown
// (claude-opus-4-6 / gpt-5.1 / gemini-1.5-flash / …), so it listed models the
// user had never connected and hid the ones they had — and the default chain
// named two of them, aiming the orchestrator's retry at models that don't
// exist.
describe('fallbackModelGroups', () => {
  const model = (modelId: string, providerKind: string) => ({
    connectionId: '1',
    modelId,
    providerKind,
    active: false,
    favorite: false,
  });

  it('groups the enabled models by provider', () => {
    expect(
      fallbackModelGroups(
        [model('qwen3:32b', 'ollama'), model('claude-x', 'anthropic'), model('qwen3-coder', 'ollama')],
        [],
      ),
    ).toEqual([
      { label: 'anthropic', models: ['claude-x'] },
      { label: 'ollama', models: ['qwen3:32b', 'qwen3-coder'] },
    ]);
  });

  it('offers nothing when no model is enabled, rather than inventing examples', () => {
    expect(fallbackModelGroups([], [])).toEqual([]);
  });

  it('keeps a stored chain entry visible when it is no longer enabled', () => {
    // Otherwise the <select> would silently render a different model than the
    // one actually persisted.
    expect(fallbackModelGroups([model('qwen3:32b', 'ollama')], ['gpt-5.1', 'qwen3:32b'])).toEqual([
      { label: 'ollama', models: ['qwen3:32b'] },
      { label: 'Not currently enabled', models: ['gpt-5.1'] },
    ]);
  });

  it('de-duplicates repeated chain entries', () => {
    expect(fallbackModelGroups([], ['ghost', 'ghost'])).toEqual([
      { label: 'Not currently enabled', models: ['ghost'] },
    ]);
  });
});
