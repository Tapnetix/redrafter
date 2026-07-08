import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import App from './App';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

// App mounts the global `FeedbackCues` overlay and a `tray:navigate` listener,
// both of which subscribe to Tauri events on mount — stub the event API so the
// boot-flow tests don't reach into the (absent) Tauri event internals, and
// capture each subscription's callback by event name so a test can simulate an
// emit (used to drive the tray-navigation test below).
const { trayHandlers } = vi.hoisted(() => ({
  trayHandlers: {} as Record<string, (event: { payload: unknown }) => void>,
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((name: string, cb: (event: { payload: unknown }) => void) => {
    trayHandlers[name] = cb;
    return Promise.resolve(() => {
      delete trayHandlers[name];
    });
  }),
}));

const mockedInvoke = vi.mocked(invoke);

/**
 * Configures the mocked backend for the boot sequence: permission grant state,
 * whether any connection exists, and stored settings. App.tsx routes on these
 * (ungranted -> onboarding; granted but no provider -> first-run).
 */
function mockBackend({
  granted = true,
  connections = [{ id: '1', providerKind: 'ollama', baseUrl: 'http://localhost:11434', enabledModels: ['default'] }],
  settings = {} as Record<string, string>,
}: {
  granted?: boolean;
  connections?: unknown[];
  settings?: Record<string, string>;
} = {}) {
  mockedInvoke.mockImplementation((cmd, args) => {
    const a = args as Record<string, unknown> | undefined;
    switch (cmd) {
      case 'permission_status':
        return Promise.resolve({ granted });
      case 'connection_list':
        return Promise.resolve(connections);
      case 'settings_get': {
        const key = a?.key as string;
        return Promise.resolve(key in settings ? settings[key] : null);
      }
      case 'settings_set':
      case 'permission_open_settings':
        return Promise.resolve(null);
      case 'history_list':
        return Promise.resolve([]);
      default:
        return Promise.resolve(null);
    }
  });
}

describe('App', () => {
  beforeEach(() => {
    window.matchMedia = vi.fn().mockReturnValue({ matches: false }) as unknown as typeof window.matchMedia;
    document.documentElement.classList.remove('light');
    mockedInvoke.mockReset();
  });
  afterEach(() => {
    mockedInvoke.mockReset();
  });

  it('boots to onboarding when Accessibility is not granted', async () => {
    mockBackend({ granted: false });
    render(<App />);

    // The onboarding permission gate is shown, not the settings chrome.
    expect(await screen.findByTestId('perm-continue')).toBeInTheDocument();
    expect(screen.queryByTestId('icon-rail')).not.toBeInTheDocument();
  });

  it('boots to first-run when granted but no provider is connected', async () => {
    mockBackend({ granted: true, connections: [] });
    render(<App />);

    expect(await screen.findByTestId('firstrun-continue')).toBeInTheDocument();
    expect(screen.queryByTestId('icon-rail')).not.toBeInTheDocument();
  });

  it('boots to the settings shell (with nav rail and sidebar) when granted and a provider exists', async () => {
    mockBackend({ granted: true });
    render(<App />);

    expect(await screen.findByTestId('icon-rail')).toBeInTheDocument();
    expect(screen.getByTestId('sidebar')).toBeInTheDocument();
    // General is the default section.
    expect(screen.getByTestId('general-permission')).toBeInTheDocument();
  });

  it('navigates to the section the tray emits via tray:navigate', async () => {
    // Regression: the tray's Settings/Manage models/History items show this
    // window and emit `tray:navigate` — App must switch to that section.
    // Previously those tray items were no-ops and the UI was unreachable.
    mockBackend({ granted: true });
    render(<App />);
    await screen.findByTestId('icon-rail');
    expect(screen.getByTestId('general-permission')).toBeInTheDocument();

    expect(trayHandlers['tray:navigate']).toBeDefined();
    await act(async () => {
      trayHandlers['tray:navigate']({ payload: 'history' });
    });

    expect(await screen.findByTestId('history-screen')).toBeInTheDocument();
  });

  it('switches sections via the nav rail', async () => {
    mockBackend({ granted: true });
    render(<App />);

    await screen.findByTestId('icon-rail');
    fireEvent.click(screen.getByTestId('rail-behavior'));

    expect(await screen.findByTestId('behavior-default-direction')).toBeInTheDocument();
  });

  it('switches sections via the sidebar', async () => {
    mockBackend({ granted: true });
    render(<App />);

    await screen.findByTestId('icon-rail');
    fireEvent.click(screen.getByTestId('nav-behavior'));

    expect(await screen.findByTestId('behavior-default-direction')).toBeInTheDocument();
  });

  it("shows the sidebar's active-model summary from the connected provider", async () => {
    mockBackend({
      granted: true,
      connections: [
        { id: '1', providerKind: 'ollama', baseUrl: 'http://localhost:11434', enabledModels: ['llama3'] },
      ],
    });
    render(<App />);

    await screen.findByTestId('icon-rail');
    expect(await screen.findByTestId('sidebar-active-model')).toHaveTextContent('llama3');
  });

  it('applies the persisted theme on boot', async () => {
    mockBackend({ granted: true, settings: { theme: 'light' } });
    render(<App />);

    await screen.findByTestId('icon-rail');
    await waitFor(() => {
      expect(document.documentElement.classList.contains('light')).toBe(true);
    });
  });
});
