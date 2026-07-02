import { describe, expect, it, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { getPermissionStatus, settingsGet, settingsSet, connectionAdd } from './ipc';

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
});
