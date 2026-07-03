import { describe, expect, it, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import {
  getPermissionStatus,
  settingsGet,
  settingsSet,
  hotkeySet,
  connectionAdd,
  connectionList,
  connectionEdit,
  connectionRemove,
  connectionTest,
  connectionRefreshModels,
  modelAddManual,
  secretsSet,
  permissionOpenSettings,
  refine,
  restoreOriginal,
  injectText,
  cancelRefine,
  trayRefine,
  trayQuit,
} from './ipc';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);

describe('ipc', () => {
  beforeEach(() => {
    mockedInvoke.mockReset();
  });

  it('getPermissionStatus invokes permission_status with no args', async () => {
    mockedInvoke.mockResolvedValue({ granted: true });

    await expect(getPermissionStatus()).resolves.toEqual({ granted: true });
    expect(mockedInvoke).toHaveBeenCalledWith('permission_status');
  });

  it('settingsGet invokes settings_get with the key', async () => {
    mockedInvoke.mockResolvedValue('dark');

    await expect(settingsGet('theme')).resolves.toBe('dark');
    expect(mockedInvoke).toHaveBeenCalledWith('settings_get', { key: 'theme' });
  });

  it('settingsSet invokes settings_set with the key and value', async () => {
    mockedInvoke.mockResolvedValue(undefined);

    await settingsSet('theme', 'dark');
    expect(mockedInvoke).toHaveBeenCalledWith('settings_set', { key: 'theme', value: 'dark' });
  });

  it('hotkeySet invokes hotkey_set with the combo and returns its result', async () => {
    mockedInvoke.mockResolvedValue({ ok: true, conflict: false });

    await expect(hotkeySet('Ctrl+Alt+R')).resolves.toEqual({ ok: true, conflict: false });
    expect(mockedInvoke).toHaveBeenCalledWith('hotkey_set', { combo: 'Ctrl+Alt+R' });
  });

  it('connectionList invokes connection_list and returns the stored connections', async () => {
    const connections = [
      {
        id: '1',
        providerKind: 'ollama',
        baseUrl: 'http://localhost:11434',
        enabledModels: ['default'],
        availableModels: [],
      },
    ];
    mockedInvoke.mockResolvedValue(connections);

    await expect(connectionList()).resolves.toEqual(connections);
    expect(mockedInvoke).toHaveBeenCalledWith('connection_list');
  });

  it('connectionAdd invokes connection_add with the given args and returns its result', async () => {
    mockedInvoke.mockResolvedValue({
      id: '1',
      providerKind: 'openai',
      baseUrl: 'https://api.openai.com',
      enabledModels: ['gpt-4o-mini'],
      availableModels: [],
    });

    const result = await connectionAdd({
      providerKind: 'openai',
      baseUrl: 'https://api.openai.com',
      apiKey: 'sk-test',
    });

    expect(mockedInvoke).toHaveBeenCalledWith('connection_add', {
      providerKind: 'openai',
      baseUrl: 'https://api.openai.com',
      apiKey: 'sk-test',
    });
    expect(result.enabledModels).toEqual(['gpt-4o-mini']);
  });

  it('connectionEdit invokes connection_edit with the given args and returns the updated connection', async () => {
    mockedInvoke.mockResolvedValue({
      id: '1',
      providerKind: 'openai',
      baseUrl: 'https://api.example.com',
      enabledModels: ['gpt-4o-mini'],
      availableModels: ['gpt-4o-mini', 'gpt-4o'],
    });

    const result = await connectionEdit({ id: '1', baseUrl: 'https://api.example.com' });

    expect(mockedInvoke).toHaveBeenCalledWith('connection_edit', {
      id: '1',
      baseUrl: 'https://api.example.com',
    });
    expect(result.baseUrl).toBe('https://api.example.com');
  });

  it('connectionRemove invokes connection_remove with the id', async () => {
    mockedInvoke.mockResolvedValue(undefined);

    await connectionRemove('1');
    expect(mockedInvoke).toHaveBeenCalledWith('connection_remove', { id: '1' });
  });

  it('connectionTest invokes connection_test with the given args', async () => {
    mockedInvoke.mockResolvedValue(undefined);

    await connectionTest({ providerKind: 'anthropic', baseUrl: 'https://api.anthropic.com', apiKey: 'sk-test' });

    expect(mockedInvoke).toHaveBeenCalledWith('connection_test', {
      providerKind: 'anthropic',
      baseUrl: 'https://api.anthropic.com',
      apiKey: 'sk-test',
    });
  });

  it('connectionRefreshModels resolves a "discovered" result on success', async () => {
    mockedInvoke.mockResolvedValue({
      id: '1',
      providerKind: 'anthropic',
      baseUrl: 'https://api.anthropic.com',
      enabledModels: ['claude-opus-4-6'],
      availableModels: ['claude-opus-4-6', 'claude-sonnet-4-6'],
    });

    const result = await connectionRefreshModels('1');

    expect(mockedInvoke).toHaveBeenCalledWith('connection_refresh_models', { id: '1' });
    expect(result).toEqual({
      status: 'discovered',
      connection: {
        id: '1',
        providerKind: 'anthropic',
        baseUrl: 'https://api.anthropic.com',
        enabledModels: ['claude-opus-4-6'],
        availableModels: ['claude-opus-4-6', 'claude-sonnet-4-6'],
      },
    });
  });

  it('connectionRefreshModels normalizes a rejection into a "manual_required" result', async () => {
    mockedInvoke.mockRejectedValue('provider returned no models');

    const result = await connectionRefreshModels('1');

    expect(result).toEqual({ status: 'manual_required', reason: 'provider returned no models' });
  });

  it('modelAddManual invokes model_add_manual with the id and model id', async () => {
    mockedInvoke.mockResolvedValue({
      id: '1',
      providerKind: 'anthropic',
      baseUrl: 'https://api.anthropic.com',
      enabledModels: ['my-custom-model'],
      availableModels: ['my-custom-model'],
    });

    const result = await modelAddManual('1', 'my-custom-model');

    expect(mockedInvoke).toHaveBeenCalledWith('model_add_manual', { id: '1', modelId: 'my-custom-model' });
    expect(result.availableModels).toEqual(['my-custom-model']);
  });

  it('secretsSet invokes secrets_set with the chosen storage location', async () => {
    mockedInvoke.mockResolvedValue(undefined);

    await secretsSet('keychain');
    expect(mockedInvoke).toHaveBeenCalledWith('secrets_set', { location: 'keychain' });
  });

  it('permissionOpenSettings invokes permission_open_settings with no args', async () => {
    mockedInvoke.mockResolvedValue(undefined);

    await permissionOpenSettings();
    expect(mockedInvoke).toHaveBeenCalledWith('permission_open_settings');
  });

  it('refine invokes refine with no args and resolves the outcome', async () => {
    const outcome = { original: 'rough', refined: 'polished', model: 'claude-opus-4-6' };
    mockedInvoke.mockResolvedValue(outcome);

    await expect(refine()).resolves.toEqual(outcome);
    expect(mockedInvoke).toHaveBeenCalledWith('refine');
  });

  it('restoreOriginal invokes restore_original and resolves the saved original', async () => {
    mockedInvoke.mockResolvedValue('rough draft');

    await expect(restoreOriginal()).resolves.toBe('rough draft');
    expect(mockedInvoke).toHaveBeenCalledWith('restore_original');
  });

  it('injectText invokes inject_text with the text', async () => {
    mockedInvoke.mockResolvedValue(undefined);

    await injectText('rough draft');
    expect(mockedInvoke).toHaveBeenCalledWith('inject_text', { text: 'rough draft' });
  });

  it('cancelRefine invokes cancel_refine with no args', async () => {
    mockedInvoke.mockResolvedValue(undefined);

    await cancelRefine();
    expect(mockedInvoke).toHaveBeenCalledWith('cancel_refine');
  });

  it('trayRefine invokes tray_refine and resolves the outcome', async () => {
    const outcome = { original: 'rough', refined: 'polished', model: 'claude-opus-4-6' };
    mockedInvoke.mockResolvedValue(outcome);

    await expect(trayRefine()).resolves.toEqual(outcome);
    expect(mockedInvoke).toHaveBeenCalledWith('tray_refine');
  });

  it('trayQuit invokes tray_quit with no args', async () => {
    mockedInvoke.mockResolvedValue(undefined);

    await trayQuit();
    expect(mockedInvoke).toHaveBeenCalledWith('tray_quit');
  });
});
