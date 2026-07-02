import { describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { connectionAdd } from './ipc';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const invokeMock = vi.mocked(invoke);

describe('connectionAdd', () => {
  it('invokes connection_add with the given args and returns its result', async () => {
    invokeMock.mockResolvedValue({
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

    expect(invokeMock).toHaveBeenCalledWith('connection_add', {
      providerKind: 'openai',
      baseUrl: 'https://api.openai.com',
      apiKey: 'sk-test',
    });
    expect(result.enabledModels).toEqual(['gpt-4o-mini']);
  });
});
