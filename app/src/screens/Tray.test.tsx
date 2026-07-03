import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import Tray from './Tray';
import * as ipc from '@/lib/ipc';
import type { CuratedModel, ModelsListResult } from '@/lib/ipc';

vi.mock('@/lib/ipc', () => ({
  modelsList: vi.fn(),
  traySetActiveModel: vi.fn(),
  trayRefine: vi.fn(),
  trayQuit: vi.fn(),
  settingsGet: vi.fn(),
  trayPause: vi.fn(),
  trayResume: vi.fn(),
  checkUpdates: vi.fn(),
  setLaunchAtLogin: vi.fn(),
}));

const mockedIpc = vi.mocked(ipc);

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

function result(models: CuratedModel[], overrides: Partial<ModelsListResult> = {}): ModelsListResult {
  return {
    models,
    hasActive: models.some((m) => m.active),
    activeUnavailable: false,
    staleActiveModelId: null,
    ...overrides,
  };
}

describe('Tray', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    // Default: not paused, launch-at-login on -- matches an unset settings
    // store the same way settingsGet('anything-unset') resolves to null in
    // the real backend.
    mockedIpc.settingsGet.mockResolvedValue(null);
  });

  it('shows the active model label and no active model when none is set', async () => {
    mockedIpc.modelsList.mockResolvedValue(result([]));

    render(<Tray />);

    const row = await screen.findByTestId('tray-active-model-label');
    expect(row).toHaveTextContent('No model selected');
    expect(screen.getByTestId('tray-active-model')).toHaveAttribute('aria-expanded', 'false');
  });

  it('collapses the switcher by default, revealing only the active model label', async () => {
    mockedIpc.modelsList.mockResolvedValue(
      result([model({ modelId: 'claude-opus-4-6', active: true, favorite: true })]),
    );

    render(<Tray />);

    await screen.findByTestId('tray-active-model-label');
    expect(screen.queryByTestId('tray-model-region')).not.toBeInTheDocument();
    expect(screen.queryByTestId('tray-fav-claude-opus-4-6')).not.toBeInTheDocument();
  });

  it('expanding the active-model row reveals favorites at the top and the full grouped list', async () => {
    mockedIpc.modelsList.mockResolvedValue(
      result([
        model({ modelId: 'claude-opus-4-6', active: true, favorite: true }),
        model({ modelId: 'claude-sonnet-4-6', favorite: false }),
        model({ modelId: 'qwen3:8b', connectionId: '2', providerKind: 'ollama', favorite: true }),
      ]),
    );

    render(<Tray />);
    await screen.findByTestId('tray-active-model-label');

    fireEvent.click(screen.getByTestId('tray-active-model'));

    expect(screen.getByTestId('tray-active-model')).toHaveAttribute('aria-expanded', 'true');
    // Favorites: opus and qwen, not sonnet.
    expect(screen.getByTestId('tray-fav-claude-opus-4-6')).toBeInTheDocument();
    expect(screen.getByTestId('tray-fav-qwen3:8b')).toBeInTheDocument();
    expect(screen.queryByTestId('tray-fav-claude-sonnet-4-6')).not.toBeInTheDocument();
    // Full list: every enabled model, grouped by provider.
    expect(screen.getByTestId('tray-model-claude-opus-4-6')).toBeInTheDocument();
    expect(screen.getByTestId('tray-model-claude-sonnet-4-6')).toBeInTheDocument();
    expect(screen.getByTestId('tray-model-qwen3:8b')).toBeInTheDocument();
    // Manage-models link surfaces inside the expanded region.
    expect(screen.getByTestId('tray-manage-models')).toBeInTheDocument();
  });

  it('picking a favorite calls tray_set_active_model, updates the indicator, and collapses the switcher', async () => {
    mockedIpc.modelsList.mockResolvedValue(
      result([
        model({ modelId: 'claude-opus-4-6', active: true, favorite: true }),
        model({ modelId: 'claude-sonnet-4-6', favorite: true }),
      ]),
    );
    mockedIpc.traySetActiveModel.mockResolvedValue(
      result([
        model({ modelId: 'claude-opus-4-6', favorite: true }),
        model({ modelId: 'claude-sonnet-4-6', active: true, favorite: true }),
      ]),
    );

    render(<Tray />);
    await screen.findByTestId('tray-active-model-label');
    fireEvent.click(screen.getByTestId('tray-active-model'));

    fireEvent.click(await screen.findByTestId('tray-fav-claude-sonnet-4-6'));

    await waitFor(() =>
      expect(mockedIpc.traySetActiveModel).toHaveBeenCalledWith({ connectionId: '1', modelId: 'claude-sonnet-4-6' }),
    );
    await waitFor(() => expect(screen.getByTestId('tray-active-model-label')).toHaveTextContent('claude-sonnet-4-6'));
    expect(screen.getByTestId('tray-active-model')).toHaveAttribute('aria-expanded', 'false');
  });

  it('picking from the full grouped list also applies the switch', async () => {
    mockedIpc.modelsList.mockResolvedValue(
      result([model({ modelId: 'claude-opus-4-6', active: true }), model({ modelId: 'gpt-5.1', providerKind: 'openai', connectionId: '2' })]),
    );
    mockedIpc.traySetActiveModel.mockResolvedValue(
      result([model({ modelId: 'claude-opus-4-6' }), model({ modelId: 'gpt-5.1', providerKind: 'openai', connectionId: '2', active: true })]),
    );

    render(<Tray />);
    await screen.findByTestId('tray-active-model-label');
    fireEvent.click(screen.getByTestId('tray-active-model'));

    fireEvent.click(await screen.findByTestId('tray-model-gpt-5.1'));

    await waitFor(() =>
      expect(mockedIpc.traySetActiveModel).toHaveBeenCalledWith({ connectionId: '2', modelId: 'gpt-5.1' }),
    );
    await waitFor(() => expect(screen.getByTestId('tray-active-model-label')).toHaveTextContent('gpt-5.1'));
  });

  it('clicking Refine selection calls tray_refine', async () => {
    mockedIpc.modelsList.mockResolvedValue(result([]));
    mockedIpc.trayRefine.mockResolvedValue({ original: 'o', refined: 'r', model: 'm' });

    render(<Tray />);
    await screen.findByTestId('tray-refine');
    fireEvent.click(screen.getByTestId('tray-refine'));

    await waitFor(() => expect(mockedIpc.trayRefine).toHaveBeenCalled());
  });

  it('clicking Quit redrafter calls tray_quit', async () => {
    mockedIpc.modelsList.mockResolvedValue(result([]));
    mockedIpc.trayQuit.mockResolvedValue(undefined);

    render(<Tray />);
    await screen.findByTestId('tray-quit');
    fireEvent.click(screen.getByTestId('tray-quit'));

    await waitFor(() => expect(mockedIpc.trayQuit).toHaveBeenCalled());
  });

  it('clicking Manage models / Settings / History invoke their navigation callbacks', async () => {
    mockedIpc.modelsList.mockResolvedValue(result([model({ modelId: 'claude-opus-4-6', active: true })]));
    const onNavigateToModels = vi.fn();
    const onNavigateToSettings = vi.fn();
    const onNavigateToHistory = vi.fn();

    render(
      <Tray
        onNavigateToModels={onNavigateToModels}
        onNavigateToSettings={onNavigateToSettings}
        onNavigateToHistory={onNavigateToHistory}
      />,
    );
    await screen.findByTestId('tray-active-model-label');
    fireEvent.click(screen.getByTestId('tray-active-model'));

    fireEvent.click(await screen.findByTestId('tray-manage-models'));
    expect(onNavigateToModels).toHaveBeenCalled();

    fireEvent.click(screen.getByTestId('tray-settings'));
    expect(onNavigateToSettings).toHaveBeenCalled();

    fireEvent.click(screen.getByTestId('tray-history'));
    expect(onNavigateToHistory).toHaveBeenCalled();
  });

  describe('pause / resume / status (B17)', () => {
    it('shows Ready and the Pause control by default, and Resume is absent', async () => {
      mockedIpc.modelsList.mockResolvedValue(result([]));

      render(<Tray />);

      expect(await screen.findByTestId('tray-state-idle')).toHaveTextContent('Ready');
      expect(screen.getByTestId('tray-pause')).toBeInTheDocument();
      expect(screen.queryByTestId('tray-resume')).not.toBeInTheDocument();
    });

    it('pausing calls tray_pause, shows Paused, swaps Pause for Resume, and disables Refine', async () => {
      mockedIpc.modelsList.mockResolvedValue(result([]));
      mockedIpc.trayPause.mockResolvedValue(undefined);

      render(<Tray />);
      await screen.findByTestId('tray-pause');

      fireEvent.click(screen.getByTestId('tray-pause'));

      await waitFor(() => expect(mockedIpc.trayPause).toHaveBeenCalled());
      expect(await screen.findByTestId('tray-state-paused')).toHaveTextContent('Paused');
      expect(screen.queryByTestId('tray-pause')).not.toBeInTheDocument();
      expect(screen.getByTestId('tray-resume')).toBeInTheDocument();
      expect(screen.getByTestId('tray-refine')).toBeDisabled();
    });

    it('while paused, clicking Refine selection does not call tray_refine', async () => {
      mockedIpc.modelsList.mockResolvedValue(result([]));
      mockedIpc.trayPause.mockResolvedValue(undefined);

      render(<Tray />);
      fireEvent.click(await screen.findByTestId('tray-pause'));
      await screen.findByTestId('tray-state-paused');

      fireEvent.click(screen.getByTestId('tray-refine'));

      expect(mockedIpc.trayRefine).not.toHaveBeenCalled();
    });

    it('resuming calls tray_resume, restores Ready, and re-enables Refine', async () => {
      mockedIpc.modelsList.mockResolvedValue(result([]));
      mockedIpc.trayPause.mockResolvedValue(undefined);
      mockedIpc.trayResume.mockResolvedValue(undefined);

      render(<Tray />);
      fireEvent.click(await screen.findByTestId('tray-pause'));
      await screen.findByTestId('tray-resume');

      fireEvent.click(screen.getByTestId('tray-resume'));

      await waitFor(() => expect(mockedIpc.trayResume).toHaveBeenCalled());
      expect(await screen.findByTestId('tray-state-idle')).toBeInTheDocument();
      expect(screen.getByTestId('tray-pause')).toBeInTheDocument();
      expect(screen.getByTestId('tray-refine')).not.toBeDisabled();
    });

    it('starts paused when settingsGet("paused") resolves "true"', async () => {
      mockedIpc.modelsList.mockResolvedValue(result([]));
      mockedIpc.settingsGet.mockImplementation((key: string) =>
        Promise.resolve(key === 'paused' ? 'true' : null),
      );

      render(<Tray />);

      expect(await screen.findByTestId('tray-state-paused')).toBeInTheDocument();
      expect(screen.getByTestId('tray-resume')).toBeInTheDocument();
    });
  });

  describe('check for updates (B17)', () => {
    it('clicking Check for updates calls tray_check_updates and shows the result', async () => {
      mockedIpc.modelsList.mockResolvedValue(result([]));
      mockedIpc.checkUpdates.mockResolvedValue({ updateAvailable: true, version: 'v1.1' });

      render(<Tray />);
      fireEvent.click(await screen.findByTestId('tray-updates'));

      await waitFor(() => expect(mockedIpc.checkUpdates).toHaveBeenCalled());
      expect(await screen.findByTestId('tray-updates-available')).toHaveTextContent('v1.1');
    });

    it('shows up to date when no update is available', async () => {
      mockedIpc.modelsList.mockResolvedValue(result([]));
      mockedIpc.checkUpdates.mockResolvedValue({ updateAvailable: false });

      render(<Tray />);
      fireEvent.click(await screen.findByTestId('tray-updates'));

      expect(await screen.findByTestId('tray-updates-uptodate')).toBeInTheDocument();
    });
  });

  describe('check for updates (C7)', () => {
    it('shows a checking state while the request is in flight, then resolves to the available result', async () => {
      mockedIpc.modelsList.mockResolvedValue(result([]));
      let resolveCheck: (value: { updateAvailable: boolean; version?: string }) => void = () => {};
      mockedIpc.checkUpdates.mockReturnValue(
        new Promise((resolve) => {
          resolveCheck = resolve;
        }),
      );

      render(<Tray />);
      fireEvent.click(await screen.findByTestId('tray-updates'));

      // While the call is pending, the checking spinner state renders and
      // neither terminal state is shown yet.
      expect(await screen.findByTestId('tray-updates-checking')).toBeInTheDocument();
      expect(screen.queryByTestId('tray-updates-uptodate')).not.toBeInTheDocument();
      expect(screen.queryByTestId('tray-updates-available')).not.toBeInTheDocument();

      resolveCheck({ updateAvailable: true, version: 'v1.2' });

      expect(await screen.findByTestId('tray-updates-available')).toHaveTextContent('v1.2');
      expect(screen.queryByTestId('tray-updates-checking')).not.toBeInTheDocument();
    });

    it('reverts to the idle "Check for updates…" prompt if the check fails', async () => {
      mockedIpc.modelsList.mockResolvedValue(result([]));
      mockedIpc.checkUpdates.mockRejectedValue(new Error('network error'));

      render(<Tray />);
      const updates = await screen.findByTestId('tray-updates');
      fireEvent.click(updates);

      await waitFor(() => expect(mockedIpc.checkUpdates).toHaveBeenCalled());
      await waitFor(() => expect(updates).toHaveTextContent('Check for updates'));
      expect(screen.queryByTestId('tray-updates-checking')).not.toBeInTheDocument();
      expect(screen.queryByTestId('tray-updates-uptodate')).not.toBeInTheDocument();
      expect(screen.queryByTestId('tray-updates-available')).not.toBeInTheDocument();
    });
  });

  describe('launch at login (B17)', () => {
    it('reflects the persisted state and toggles via tray_set_launch_login', async () => {
      mockedIpc.modelsList.mockResolvedValue(result([]));
      mockedIpc.settingsGet.mockImplementation((key: string) =>
        Promise.resolve(key === 'launch_at_login' ? 'false' : null),
      );
      mockedIpc.setLaunchAtLogin.mockResolvedValue(undefined);

      render(<Tray />);

      const toggle = await screen.findByTestId('tray-launch-login');
      await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'false'));

      fireEvent.click(toggle);

      await waitFor(() => expect(mockedIpc.setLaunchAtLogin).toHaveBeenCalledWith(true));
      expect(toggle).toHaveAttribute('aria-checked', 'true');
    });
  });
});
