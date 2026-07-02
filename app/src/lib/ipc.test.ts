import { describe, expect, it, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import {
  getPermissionStatus,
  settingsGet,
  settingsSet,
  permissionOpenSettings,
  refine,
  restoreOriginal,
  injectText,
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

  it('trayQuit invokes tray_quit with no args', async () => {
    mockedInvoke.mockResolvedValue(undefined);

    await trayQuit();
    expect(mockedInvoke).toHaveBeenCalledWith('tray_quit');
  });
});
