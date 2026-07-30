import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import Behavior from './Behavior';
import * as ipc from '@/lib/ipc';

vi.mock('@/lib/ipc', () => ({
  settingsGet: vi.fn(),
  settingsSet: vi.fn(),
  // Backs the fallback dropdown, which offers the user's real models.
  modelsList: vi.fn(),
}));

const mockedIpc = vi.mocked(ipc);

describe('Behavior settings: progress feedback (S35) and history retention', () => {
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

  it('defaults the feedback cues to spinner+sound on, HUD off', async () => {
    render(<Behavior />);

    const spinner = await screen.findByTestId('feedback-spinner');
    expect(spinner).toBeChecked();
    expect(screen.getByTestId('feedback-hud')).not.toBeChecked();
    expect(screen.getByTestId('feedback-sound')).toBeChecked();
  });

  it('toggles the menu-bar spinner off and persists it via settings_set', async () => {
    render(<Behavior />);

    const spinner = await screen.findByTestId('feedback-spinner');
    fireEvent.click(spinner);

    await waitFor(() => expect(mockedIpc.settingsSet).toHaveBeenCalledWith('feedback.spinner', 'false'));
    expect(spinner).not.toBeChecked();
  });

  it('toggles the cursor HUD on and persists it via settings_set', async () => {
    render(<Behavior />);

    const hud = await screen.findByTestId('feedback-hud');
    fireEvent.click(hud);

    await waitFor(() => expect(mockedIpc.settingsSet).toHaveBeenCalledWith('feedback.hud', 'true'));
    expect(hud).toBeChecked();
  });

  it('toggles the completion sound off and persists it via settings_set', async () => {
    render(<Behavior />);

    const sound = await screen.findByTestId('feedback-sound');
    fireEvent.click(sound);

    await waitFor(() => expect(mockedIpc.settingsSet).toHaveBeenCalledWith('feedback.sound', 'false'));
    expect(sound).not.toBeChecked();
  });

  it('restores previously persisted feedback toggles from settings_get', async () => {
    mockedIpc.settingsGet.mockImplementation((key: string) =>
      Promise.resolve(
        {
          'feedback.spinner': 'false',
          'feedback.hud': 'true',
          'feedback.sound': 'false',
        }[key] ?? null,
      ),
    );

    render(<Behavior />);

    await waitFor(() => expect(screen.getByTestId('feedback-hud')).toBeChecked());
    expect(screen.getByTestId('feedback-spinner')).not.toBeChecked();
    expect(screen.getByTestId('feedback-sound')).not.toBeChecked();
  });

  it('defaults history retention to 50 entries / 7 days and persists a change to each', async () => {
    render(<Behavior />);

    const count = await screen.findByTestId('retention-count');
    const days = screen.getByTestId('retention-days');
    expect(count).toHaveValue('50');
    expect(days).toHaveValue('7');

    fireEvent.change(count, { target: { value: '200' } });
    await waitFor(() => expect(mockedIpc.settingsSet).toHaveBeenCalledWith('history.retention_count', '200'));

    fireEvent.change(days, { target: { value: '30' } });
    await waitFor(() => expect(mockedIpc.settingsSet).toHaveBeenCalledWith('history.retention_days', '30'));
  });

  it('restores previously persisted retention settings from settings_get', async () => {
    mockedIpc.settingsGet.mockImplementation((key: string) =>
      Promise.resolve(
        {
          'history.retention_count': 'Unlimited',
          'history.retention_days': '0',
        }[key] ?? null,
      ),
    );

    render(<Behavior />);

    await waitFor(() => expect(screen.getByTestId('retention-count')).toHaveValue('Unlimited'));
    expect(screen.getByTestId('retention-days')).toHaveValue('0');
  });
});
