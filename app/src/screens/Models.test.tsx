import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import Models from './Models';
import * as ipc from '@/lib/ipc';
import type { CuratedModel, ModelsListResult } from '@/lib/ipc';

vi.mock('@/lib/ipc', () => ({
  modelsList: vi.fn(),
  modelSetActive: vi.fn(),
  modelDisable: vi.fn(),
  modelToggleFavorite: vi.fn(),
  ollamaPull: vi.fn(),
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

describe('Models', () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  it('shows the empty state with no enabled models, linking back to Connections', async () => {
    mockedIpc.modelsList.mockResolvedValue(result([]));
    const onNavigateToConnections = vi.fn();

    render(<Models onNavigateToConnections={onNavigateToConnections} />);

    await screen.findByTestId('models-empty');
    expect(screen.queryByTestId('models-table')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('models-connections-link'));
    expect(onNavigateToConnections).toHaveBeenCalled();
  });

  it('lists every enabled model with the active one checked', async () => {
    mockedIpc.modelsList.mockResolvedValue(
      result([
        model({ modelId: 'claude-opus-4-6', active: true }),
        model({ modelId: 'claude-sonnet-4-6' }),
      ]),
    );

    render(<Models />);

    const opusRadio = await screen.findByTestId('model-active-radio-claude-opus-4-6');
    const sonnetRadio = screen.getByTestId('model-active-radio-claude-sonnet-4-6');
    expect(opusRadio).toHaveAttribute('aria-checked', 'true');
    expect(sonnetRadio).toHaveAttribute('aria-checked', 'false');
    expect(screen.queryByTestId('models-no-active-banner')).not.toBeInTheDocument();
    expect(screen.queryByTestId('model-active-unavailable')).not.toBeInTheDocument();
  });

  it('shows the no-active-model banner when nothing is active yet', async () => {
    mockedIpc.modelsList.mockResolvedValue(result([model({ modelId: 'claude-opus-4-6' })]));

    render(<Models />);

    await screen.findByTestId('models-no-active-banner');
  });

  it('shows the active-unavailable banner with the stale model id', async () => {
    mockedIpc.modelsList.mockResolvedValue(
      result([model({ modelId: 'claude-sonnet-4-6' })], {
        activeUnavailable: true,
        staleActiveModelId: 'claude-opus-4-6',
      }),
    );

    render(<Models />);

    const banner = await screen.findByTestId('model-active-unavailable');
    expect(banner).toHaveTextContent('claude-opus-4-6');
    expect(screen.queryByTestId('models-no-active-banner')).not.toBeInTheDocument();
  });

  it('clicking a radio calls model_set_active with the connection/model id and refreshes the list', async () => {
    mockedIpc.modelsList.mockResolvedValue(
      result([
        model({ modelId: 'claude-opus-4-6', active: true }),
        model({ modelId: 'claude-sonnet-4-6' }),
      ]),
    );
    mockedIpc.modelSetActive.mockResolvedValue(
      result([
        model({ modelId: 'claude-opus-4-6' }),
        model({ modelId: 'claude-sonnet-4-6', active: true }),
      ]),
    );

    render(<Models />);
    await screen.findByTestId('model-active-radio-claude-sonnet-4-6');

    fireEvent.click(screen.getByTestId('model-active-radio-claude-sonnet-4-6'));

    await waitFor(() =>
      expect(mockedIpc.modelSetActive).toHaveBeenCalledWith({ connectionId: '1', modelId: 'claude-sonnet-4-6' }),
    );
    await waitFor(() =>
      expect(screen.getByTestId('model-active-radio-claude-sonnet-4-6')).toHaveAttribute('aria-checked', 'true'),
    );
    expect(screen.getByTestId('model-active-radio-claude-opus-4-6')).toHaveAttribute('aria-checked', 'false');
  });

  it('clicking the star toggles favorite via model_toggle_favorite', async () => {
    mockedIpc.modelsList.mockResolvedValue(result([model({ modelId: 'claude-opus-4-6' })]));
    mockedIpc.modelToggleFavorite.mockResolvedValue(result([model({ modelId: 'claude-opus-4-6', favorite: true })]));

    render(<Models />);
    const star = await screen.findByTestId('model-favorite-claude-opus-4-6');
    expect(star).toHaveAttribute('aria-pressed', 'false');

    fireEvent.click(star);

    await waitFor(() =>
      expect(mockedIpc.modelToggleFavorite).toHaveBeenCalledWith({ connectionId: '1', modelId: 'claude-opus-4-6' }),
    );
    await waitFor(() => expect(screen.getByTestId('model-favorite-claude-opus-4-6')).toHaveAttribute('aria-pressed', 'true'));
  });

  it('clicking disable calls model_disable and removes the row from the list', async () => {
    mockedIpc.modelsList.mockResolvedValue(
      result([model({ modelId: 'claude-opus-4-6' }), model({ modelId: 'claude-sonnet-4-6' })]),
    );
    mockedIpc.modelDisable.mockResolvedValue(result([model({ modelId: 'claude-sonnet-4-6' })]));

    render(<Models />);
    await screen.findByTestId('model-disable-claude-opus-4-6');

    fireEvent.click(screen.getByTestId('model-disable-claude-opus-4-6'));

    await waitFor(() =>
      expect(mockedIpc.modelDisable).toHaveBeenCalledWith({ connectionId: '1', modelId: 'claude-opus-4-6' }),
    );
    await waitFor(() => expect(screen.queryByTestId('model-active-radio-claude-opus-4-6')).not.toBeInTheDocument());
  });

  it('filters the table by the search term (model id or provider)', async () => {
    mockedIpc.modelsList.mockResolvedValue(
      result([
        model({ modelId: 'claude-opus-4-6', providerKind: 'anthropic' }),
        model({ modelId: 'gpt-5.1', providerKind: 'openai', connectionId: '2' }),
      ]),
    );

    render(<Models />);
    await screen.findByTestId('model-active-radio-claude-opus-4-6');

    fireEvent.change(screen.getByTestId('models-search'), { target: { value: 'gpt' } });

    expect(screen.queryByTestId('model-active-radio-claude-opus-4-6')).not.toBeInTheDocument();
    expect(screen.getByTestId('model-active-radio-gpt-5.1')).toBeInTheDocument();
  });

  it('pulling an Ollama model shows the done state on success', async () => {
    mockedIpc.modelsList.mockResolvedValue(result([]));
    mockedIpc.ollamaPull.mockResolvedValue({ status: 'success', digest: null, total: null, completed: null, error: null });

    render(<Models />);
    await screen.findByTestId('ollama-pull-idle');

    fireEvent.change(screen.getByTestId('ollama-pull-field'), { target: { value: 'llama3.2' } });
    fireEvent.click(screen.getByTestId('ollama-pull-start'));

    await waitFor(() => expect(mockedIpc.ollamaPull).toHaveBeenCalledWith('llama3.2'));
    await waitFor(() => expect(screen.getByTestId('ollama-pull-done')).toHaveTextContent('llama3.2'));
  });

  it('a failing Ollama pull shows the error state with the message', async () => {
    mockedIpc.modelsList.mockResolvedValue(result([]));
    mockedIpc.ollamaPull.mockRejectedValue(new Error('model not found in registry'));

    render(<Models />);
    await screen.findByTestId('ollama-pull-idle');

    fireEvent.change(screen.getByTestId('ollama-pull-field'), { target: { value: 'nope' } });
    fireEvent.click(screen.getByTestId('ollama-pull-start'));

    await waitFor(() => expect(screen.getByTestId('ollama-pull-error')).toHaveTextContent('model not found in registry'));
  });
});
