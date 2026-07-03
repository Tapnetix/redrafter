import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import Behavior, { DEFAULT_DIRECTION } from './Behavior';
import * as ipc from '@/lib/ipc';

vi.mock('@/lib/ipc', () => ({
  settingsGet: vi.fn(),
  settingsSet: vi.fn(),
}));

const mockedIpc = vi.mocked(ipc);

describe('Behavior', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mockedIpc.settingsGet.mockResolvedValue(null);
    mockedIpc.settingsSet.mockResolvedValue(undefined);
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
