import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import Connections from './Connections';
import * as ipc from '@/lib/ipc';
import type { Connection } from '@/lib/ipc';

vi.mock('@/lib/ipc', () => ({
  connectionList: vi.fn(),
  connectionAdd: vi.fn(),
  connectionEdit: vi.fn(),
  connectionRemove: vi.fn(),
  connectionTest: vi.fn(),
  connectionRefreshModels: vi.fn(),
  modelAddManual: vi.fn(),
  secretsSet: vi.fn(),
  openExternal: vi.fn(),
  claudeCodeStatus: vi.fn(),
  claudeCodeConnect: vi.fn(),
}));

const mockedIpc = vi.mocked(ipc);

function anthropicConnection(overrides: Partial<Connection> = {}): Connection {
  return {
    id: '1',
    providerKind: 'anthropic',
    baseUrl: 'https://api.anthropic.com',
    enabledModels: ['claude-opus-4-6'],
    availableModels: [],
    keyRef: '1',
    ...overrides,
  };
}

describe('Connections', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mockedIpc.connectionList.mockResolvedValue([]);
    mockedIpc.connectionTest.mockResolvedValue(undefined);
    mockedIpc.secretsSet.mockResolvedValue(undefined);
    mockedIpc.openExternal.mockResolvedValue(undefined);
    // No Claude Code login by default, so the existing specs see the plain
    // key-entry sheet they were written against.
    mockedIpc.claudeCodeStatus.mockRejectedValue(new Error('no Claude Code login found'));
  });

  it('shows the empty state with no connections and opens the add sheet from its CTA', async () => {
    render(<Connections />);

    await screen.findByTestId('connections-empty');
    expect(screen.queryByTestId('connections-modal')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('connections-empty-cta'));

    expect(screen.getByTestId('connection-modal')).toBeVisible();
    expect(screen.getByTestId('conn-provider-type')).toHaveValue('anthropic');
  });

  it('lists existing connections instead of the empty state', async () => {
    mockedIpc.connectionList.mockResolvedValue([anthropicConnection()]);

    render(<Connections />);

    await screen.findByTestId('connection-row-anthropic');
    expect(screen.queryByTestId('connections-empty')).not.toBeInTheDocument();
  });

  it('conn-test shows an ok chip on success and an error chip with the message on failure', async () => {
    render(<Connections />);
    await screen.findByTestId('connections-empty');
    fireEvent.click(screen.getByTestId('connections-empty-cta'));

    fireEvent.change(screen.getByTestId('conn-api-key'), { target: { value: 'sk-ant-test' } });
    fireEvent.click(screen.getByTestId('conn-test'));

    await waitFor(() => expect(screen.getByTestId('conn-test-ok')).toBeVisible());
    expect(mockedIpc.connectionTest).toHaveBeenCalledWith({
      providerKind: 'anthropic',
      baseUrl: 'https://api.anthropic.com',
      apiKey: 'sk-ant-test',
    });

    mockedIpc.connectionTest.mockRejectedValue(new Error('bad key'));
    fireEvent.click(screen.getByTestId('conn-test'));

    await waitFor(() => expect(screen.getByTestId('conn-test-error')).toHaveTextContent('bad key'));
  });

  it('saving a new connection discovers models and enables the default one', async () => {
    const added = anthropicConnection({ availableModels: [] });
    mockedIpc.connectionAdd.mockResolvedValue(added);
    mockedIpc.connectionRefreshModels.mockResolvedValue({
      status: 'discovered',
      connection: anthropicConnection({ availableModels: ['claude-opus-4-6', 'claude-sonnet-4-6'] }),
    });

    render(<Connections />);
    await screen.findByTestId('connections-empty');
    fireEvent.click(screen.getByTestId('connections-empty-cta'));

    fireEvent.change(screen.getByTestId('conn-api-key'), { target: { value: 'sk-ant-test' } });
    fireEvent.click(screen.getByTestId('connection-save'));

    await waitFor(() =>
      expect(mockedIpc.connectionAdd).toHaveBeenCalledWith({
        providerKind: 'anthropic',
        baseUrl: 'https://api.anthropic.com',
        apiKey: 'sk-ant-test',
      }),
    );
    await waitFor(() => expect(mockedIpc.connectionRefreshModels).toHaveBeenCalledWith('1'));

    const opusCheck = await screen.findByTestId('conn-model-check-claude-opus-4-6');
    const sonnetCheck = screen.getByTestId('conn-model-check-claude-sonnet-4-6');
    expect(opusCheck).toBeChecked();
    expect(sonnetCheck).not.toBeChecked();
  });

  it('falls back to the manual model-id control when discovery requires manual entry', async () => {
    mockedIpc.connectionAdd.mockResolvedValue(anthropicConnection({ availableModels: [] }));
    mockedIpc.connectionRefreshModels.mockResolvedValue({
      status: 'manual_required',
      reason: 'provider returned no models',
    });
    mockedIpc.modelAddManual.mockResolvedValue(
      anthropicConnection({ availableModels: ['my-model'], enabledModels: ['claude-opus-4-6', 'my-model'] }),
    );

    render(<Connections />);
    await screen.findByTestId('connections-empty');
    fireEvent.click(screen.getByTestId('connections-empty-cta'));
    fireEvent.change(screen.getByTestId('conn-api-key'), { target: { value: 'sk-ant-test' } });
    fireEvent.click(screen.getByTestId('connection-save'));

    await waitFor(() => expect(screen.getByTestId('conn-discovered-list')).toHaveTextContent(/no models could be listed/i));

    fireEvent.change(screen.getByTestId('conn-add-model-manual'), { target: { value: 'my-model' } });
    fireEvent.click(screen.getByTestId('conn-add-model-manual-add'));

    await waitFor(() => expect(mockedIpc.modelAddManual).toHaveBeenCalledWith('1', 'my-model'));
    await screen.findByTestId('conn-model-check-my-model');
  });

  // ── Regressions from real-app testing: every one of these controls ran a
  // command successfully and rendered nothing, so the user read them as dead.

  it('routes the "Get an API key" link through open_external instead of an inert href', async () => {
    render(<Connections />);
    await screen.findByTestId('connections-empty');
    fireEvent.click(screen.getByTestId('connections-empty-cta'));

    fireEvent.click(screen.getByTestId('conn-get-key-link'));

    await waitFor(() =>
      expect(mockedIpc.openExternal).toHaveBeenCalledWith('https://platform.claude.com/settings/keys'),
    );
  });

  it('reports a reachable result on the row Test button, not just failures', async () => {
    mockedIpc.connectionList.mockResolvedValue([anthropicConnection()]);
    render(<Connections />);
    await screen.findByTestId('connection-row-anthropic');

    fireEvent.click(screen.getByTestId('connection-test-anthropic'));

    expect(await screen.findByTestId('connection-test-ok-anthropic')).toBeInTheDocument();
  });

  it('surfaces a failed row Test with its message', async () => {
    mockedIpc.connectionList.mockResolvedValue([anthropicConnection()]);
    mockedIpc.connectionTest.mockRejectedValue(new Error('could not connect to anthropic'));
    render(<Connections />);
    await screen.findByTestId('connection-row-anthropic');

    fireEvent.click(screen.getByTestId('connection-test-anthropic'));

    expect(await screen.findByTestId('connection-error-anthropic')).toHaveTextContent(
      'could not connect to anthropic',
    );
  });

  it('closes the edit sheet after a successful Save changes', async () => {
    mockedIpc.connectionList.mockResolvedValue([anthropicConnection()]);
    mockedIpc.connectionEdit.mockResolvedValue(anthropicConnection({ baseUrl: 'https://proxy.example' }));
    render(<Connections />);
    await screen.findByTestId('connection-row-anthropic');
    fireEvent.click(screen.getByTestId('connection-edit-anthropic'));

    fireEvent.click(screen.getByTestId('connection-save'));

    await waitFor(() => expect(screen.queryByTestId('connection-modal')).not.toBeInTheDocument());
  });

  it('keeps the sheet open and explains why when Save changes fails', async () => {
    mockedIpc.connectionList.mockResolvedValue([anthropicConnection()]);
    mockedIpc.connectionEdit.mockRejectedValue(new Error('no connection with id 1'));
    render(<Connections />);
    await screen.findByTestId('connection-row-anthropic');
    fireEvent.click(screen.getByTestId('connection-edit-anthropic'));

    fireEvent.click(screen.getByTestId('connection-save'));

    expect(await screen.findByTestId('conn-sheet-error')).toHaveTextContent('no connection with id 1');
    expect(screen.getByTestId('connection-modal')).toBeInTheDocument();
  });

  it('explains why a model checkbox did not take when connection_edit rejects', async () => {
    mockedIpc.connectionList.mockResolvedValue([
      anthropicConnection({ availableModels: ['claude-opus-4-6', 'claude-sonnet-4-6'] }),
    ]);
    mockedIpc.connectionEdit.mockRejectedValue(new Error('database is locked'));
    render(<Connections />);
    await screen.findByTestId('connection-row-anthropic');
    fireEvent.click(screen.getByTestId('connection-edit-anthropic'));

    fireEvent.click(screen.getByTestId('conn-model-check-claude-sonnet-4-6'));

    expect(await screen.findByTestId('conn-sheet-error')).toHaveTextContent('database is locked');
  });

  it('reports what Refresh models discovered', async () => {
    mockedIpc.connectionList.mockResolvedValue([anthropicConnection()]);
    mockedIpc.connectionRefreshModels.mockResolvedValue({
      status: 'discovered',
      connection: anthropicConnection({ availableModels: ['claude-opus-4-6', 'claude-sonnet-4-6'] }),
    });
    render(<Connections />);
    await screen.findByTestId('connection-row-anthropic');

    fireEvent.click(screen.getByTestId('connection-refresh-anthropic'));

    expect(await screen.findByTestId('connection-refreshed-anthropic')).toHaveTextContent('2 models found');
  });

  it('toggling a discovered model checkbox calls connection_edit with the updated curation', async () => {
    mockedIpc.connectionList.mockResolvedValue([
      anthropicConnection({ availableModels: ['claude-opus-4-6', 'claude-sonnet-4-6'] }),
    ]);
    mockedIpc.connectionEdit.mockResolvedValue(
      anthropicConnection({
        availableModels: ['claude-opus-4-6', 'claude-sonnet-4-6'],
        enabledModels: ['claude-opus-4-6', 'claude-sonnet-4-6'],
      }),
    );

    render(<Connections />);
    const row = await screen.findByTestId('connection-row-anthropic');
    fireEvent.click(screen.getByTestId('connection-edit-anthropic'));

    const sonnetCheck = screen.getByTestId('conn-model-check-claude-sonnet-4-6');
    expect(sonnetCheck).not.toBeChecked();
    fireEvent.click(sonnetCheck);

    await waitFor(() =>
      expect(mockedIpc.connectionEdit).toHaveBeenCalledWith({
        id: '1',
        enabledModels: ['claude-opus-4-6', 'claude-sonnet-4-6'],
      }),
    );
    expect(row).toBeInTheDocument();
  });

  it('removing a connection asks for confirmation, then calls connection_remove', async () => {
    mockedIpc.connectionList.mockResolvedValue([anthropicConnection()]);
    mockedIpc.connectionRemove.mockResolvedValue(undefined);

    render(<Connections />);
    await screen.findByTestId('connection-row-anthropic');
    fireEvent.click(screen.getByTestId('connection-edit-anthropic'));
    fireEvent.click(screen.getByTestId('connection-remove'));

    expect(screen.getByTestId('connection-remove-modal')).toBeVisible();
    fireEvent.click(screen.getByTestId('connection-remove-confirm'));

    await waitFor(() => expect(mockedIpc.connectionRemove).toHaveBeenCalledWith('1'));
    await waitFor(() => expect(screen.queryByTestId('connection-row-anthropic')).not.toBeInTheDocument());
  });

  it('choosing a key-storage option calls secrets_set and marks it active', async () => {
    render(<Connections />);
    await screen.findByTestId('connections-empty');

    fireEvent.click(screen.getByTestId('key-storage-keychain'));

    await waitFor(() => expect(mockedIpc.secretsSet).toHaveBeenCalledWith('keychain'));
    expect(screen.getByTestId('key-storage-keychain')).toHaveClass('active');
  });

  it('the add-connection button in the header also opens the add sheet', async () => {
    render(<Connections />);
    await screen.findByTestId('connections-empty');

    fireEvent.click(screen.getByTestId('add-connection'));

    expect(screen.getByTestId('connection-modal')).toBeVisible();
  });

  it('a row Test click calls connection_test with the row provider/base URL and reflects ok/error', async () => {
    mockedIpc.connectionList.mockResolvedValue([anthropicConnection()]);

    render(<Connections />);
    await screen.findByTestId('connection-row-anthropic');

    fireEvent.click(screen.getByTestId('connection-test-anthropic'));

    await waitFor(() =>
      expect(mockedIpc.connectionTest).toHaveBeenCalledWith({
        providerKind: 'anthropic',
        baseUrl: 'https://api.anthropic.com',
        apiKey: '',
      }),
    );

    mockedIpc.connectionTest.mockRejectedValue(new Error('unreachable'));
    fireEvent.click(screen.getByTestId('connection-test-anthropic'));

    await waitFor(() => expect(screen.getByTestId('connection-row-anthropic')).toHaveTextContent('auth error'));
  });

  it('a row Refresh click updates the row with newly discovered models', async () => {
    mockedIpc.connectionList.mockResolvedValue([anthropicConnection({ availableModels: ['claude-opus-4-6'] })]);
    mockedIpc.connectionRefreshModels.mockResolvedValue({
      status: 'discovered',
      connection: anthropicConnection({ availableModels: ['claude-opus-4-6', 'claude-sonnet-4-6'] }),
    });

    render(<Connections />);
    const row = await screen.findByTestId('connection-row-anthropic');
    expect(row).toHaveTextContent('1 models');

    fireEvent.click(screen.getByTestId('connection-refresh-anthropic'));

    await waitFor(() => expect(mockedIpc.connectionRefreshModels).toHaveBeenCalledWith('1'));
    await waitFor(() => expect(row).toHaveTextContent('2 models'));
  });

  it('a row Refresh click that requires manual entry opens Edit with the manual-entry note', async () => {
    mockedIpc.connectionList.mockResolvedValue([anthropicConnection({ availableModels: [] })]);
    mockedIpc.connectionRefreshModels.mockResolvedValue({
      status: 'manual_required',
      reason: 'provider returned no models',
    });

    render(<Connections />);
    await screen.findByTestId('connection-row-anthropic');

    fireEvent.click(screen.getByTestId('connection-refresh-anthropic'));

    await waitFor(() =>
      expect(screen.getByTestId('conn-discovered-list')).toHaveTextContent(/no models could be listed/i),
    );
  });

  it('the row switch disables a connection (clearing enabled models) and re-enables it from the last-known set', async () => {
    const connection = anthropicConnection({ availableModels: ['claude-opus-4-6'] });
    mockedIpc.connectionList.mockResolvedValue([connection]);
    mockedIpc.connectionEdit
      .mockResolvedValueOnce(anthropicConnection({ availableModels: ['claude-opus-4-6'], enabledModels: [] }))
      .mockResolvedValueOnce(connection);

    render(<Connections />);
    await screen.findByTestId('connection-row-anthropic');

    fireEvent.click(screen.getByTestId('connection-enable-anthropic'));
    await waitFor(() =>
      expect(mockedIpc.connectionEdit).toHaveBeenNthCalledWith(1, { id: '1', enabledModels: [] }),
    );

    fireEvent.click(screen.getByTestId('connection-enable-anthropic'));
    await waitFor(() =>
      expect(mockedIpc.connectionEdit).toHaveBeenNthCalledWith(2, {
        id: '1',
        enabledModels: ['claude-opus-4-6'],
      }),
    );
  });

  it('starring a discovered model is a local, visual-only toggle', async () => {
    mockedIpc.connectionList.mockResolvedValue([
      anthropicConnection({ availableModels: ['claude-opus-4-6'] }),
    ]);

    render(<Connections />);
    await screen.findByTestId('connection-row-anthropic');
    fireEvent.click(screen.getByTestId('connection-edit-anthropic'));

    const star = screen.getByTestId('conn-model-star-claude-opus-4-6');
    expect(star).toHaveAttribute('aria-pressed', 'false');

    fireEvent.click(star);

    expect(star).toHaveAttribute('aria-pressed', 'true');
  });

  it('cancelling the remove confirmation keeps the connection', async () => {
    mockedIpc.connectionList.mockResolvedValue([anthropicConnection()]);

    render(<Connections />);
    await screen.findByTestId('connection-row-anthropic');
    fireEvent.click(screen.getByTestId('connection-edit-anthropic'));
    fireEvent.click(screen.getByTestId('connection-remove'));
    fireEvent.click(screen.getByTestId('connection-remove-cancel'));

    expect(screen.queryByTestId('connection-remove-modal')).not.toBeInTheDocument();
    expect(mockedIpc.connectionRemove).not.toHaveBeenCalled();
    expect(await screen.findByTestId('connection-row-anthropic')).toBeInTheDocument();
  });

  it('changing the provider type on a fresh Add prefills that provider’s default base URL', async () => {
    render(<Connections />);
    await screen.findByTestId('connections-empty');
    fireEvent.click(screen.getByTestId('connections-empty-cta'));

    fireEvent.change(screen.getByTestId('conn-provider-type'), { target: { value: 'ollama' } });

    expect(screen.getByTestId('conn-provider-type')).toHaveValue('ollama');
    expect(screen.getByTestId('conn-base-url')).toHaveValue('http://localhost:11434');
  });

  it('cancelling the add sheet closes it without saving anything', async () => {
    render(<Connections />);
    await screen.findByTestId('connections-empty');
    fireEvent.click(screen.getByTestId('connections-empty-cta'));

    fireEvent.click(screen.getByTestId('connection-cancel'));

    expect(screen.queryByTestId('connection-modal')).not.toBeInTheDocument();
    expect(mockedIpc.connectionAdd).not.toHaveBeenCalled();
  });

  it('editing an existing connection saves base URL/key changes via connection_edit', async () => {
    mockedIpc.connectionList.mockResolvedValue([anthropicConnection()]);
    mockedIpc.connectionEdit.mockResolvedValue(anthropicConnection({ baseUrl: 'https://proxy.example.com' }));

    render(<Connections />);
    await screen.findByTestId('connection-row-anthropic');
    fireEvent.click(screen.getByTestId('connection-edit-anthropic'));

    fireEvent.change(screen.getByTestId('conn-base-url'), { target: { value: 'https://proxy.example.com' } });
    fireEvent.click(screen.getByTestId('connection-save'));

    await waitFor(() =>
      expect(mockedIpc.connectionEdit).toHaveBeenCalledWith({ id: '1', baseUrl: 'https://proxy.example.com' }),
    );
  });
});

// ── The Claude Code login shortcut ───────────────────────────────────────────
describe('Claude Code login', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mockedIpc.connectionList.mockResolvedValue([]);
    mockedIpc.connectionTest.mockResolvedValue(undefined);
    mockedIpc.secretsSet.mockResolvedValue(undefined);
    mockedIpc.openExternal.mockResolvedValue(undefined);
  });

  const claudeCodeConnection: Connection = {
    id: '9',
    providerKind: 'claude-code',
    baseUrl: 'https://api.anthropic.com',
    enabledModels: ['claude-opus-5'],
    availableModels: ['claude-opus-5'],
    keyRef: null,
  };

  it('offers the shortcut when a usable Claude Code login exists', async () => {
    mockedIpc.claudeCodeStatus.mockResolvedValue({ subscriptionType: 'max', canInfer: true });
    render(<Connections />);
    await screen.findByTestId('connections-empty');
    fireEvent.click(screen.getByTestId('connections-empty-cta'));

    expect(await screen.findByTestId('conn-use-claude-code')).toBeEnabled();
    expect(screen.getByTestId('claude-code-block')).toHaveTextContent(/max/);
  });

  it('adds the connection without ever handling a key', async () => {
    mockedIpc.claudeCodeStatus.mockResolvedValue({ subscriptionType: 'max', canInfer: true });
    mockedIpc.claudeCodeConnect.mockResolvedValue(claudeCodeConnection);
    render(<Connections />);
    await screen.findByTestId('connections-empty');
    fireEvent.click(screen.getByTestId('connections-empty-cta'));

    fireEvent.click(await screen.findByTestId('conn-use-claude-code'));

    await waitFor(() => expect(mockedIpc.claudeCodeConnect).toHaveBeenCalledTimes(1));
    // No key was collected or sent anywhere.
    expect(mockedIpc.connectionAdd).not.toHaveBeenCalled();
    expect(await screen.findByTestId('connection-save-ok')).toBeInTheDocument();
  });

  it('explains why instead of offering a button that would fail', async () => {
    mockedIpc.claudeCodeStatus.mockRejectedValue(
      new Error('your Claude Code login has expired — run any `claude` command to refresh it'),
    );
    render(<Connections />);
    await screen.findByTestId('connections-empty');
    fireEvent.click(screen.getByTestId('connections-empty-cta'));

    expect(await screen.findByTestId('claude-code-block')).toHaveTextContent(/expired/);
    expect(screen.getByTestId('conn-use-claude-code')).toBeDisabled();
  });

  it('surfaces a failure from the import itself', async () => {
    mockedIpc.claudeCodeStatus.mockResolvedValue({ subscriptionType: 'max', canInfer: true });
    mockedIpc.claudeCodeConnect.mockRejectedValue(new Error('no Claude Code login found'));
    render(<Connections />);
    await screen.findByTestId('connections-empty');
    fireEvent.click(screen.getByTestId('connections-empty-cta'));

    fireEvent.click(await screen.findByTestId('conn-use-claude-code'));

    expect(await screen.findByTestId('conn-sheet-error')).toHaveTextContent('no Claude Code login found');
  });

  it('labels a claude-code connection as the Claude Code login, not Anthropic', async () => {
    mockedIpc.claudeCodeStatus.mockResolvedValue({ subscriptionType: 'max', canInfer: true });
    mockedIpc.connectionList.mockResolvedValue([claudeCodeConnection]);
    render(<Connections />);

    const row = await screen.findByTestId('connection-row-claude-code');
    expect(row).toHaveTextContent('Claude Code login');
    expect(row).not.toHaveTextContent('Anthropic');
  });
});
