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
});
