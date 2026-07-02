import { describe, expect, it, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import {
  getPermissionStatus,
  settingsGet,
  settingsSet,
  hotkeySet,
  connectionAdd,
  connectionList,
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
      { id: '1', providerKind: 'ollama', baseUrl: 'http://localhost:11434', enabledModels: ['default'] },
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
