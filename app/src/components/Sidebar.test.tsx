import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import Sidebar from './Sidebar';
import { RAIL_ITEMS } from './NavRail';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);

/** Configures the mocked `connection_list` response the sidebar's active-model
 * summary reads on mount. */
function mockConnections(connections: unknown[] = []) {
  mockedInvoke.mockImplementation((cmd) => {
    if (cmd === 'connection_list') return Promise.resolve(connections);
    return Promise.resolve(null);
  });
}

describe('Sidebar', () => {
  beforeEach(() => {
    mockedInvoke.mockReset();
    mockConnections();
  });
  afterEach(() => {
    mockedInvoke.mockReset();
  });

  it('renders a nav item for every navigable section', async () => {
    render(<Sidebar active="general" onNavigate={() => {}} />);

    for (const item of RAIL_ITEMS) {
      expect(await screen.findByTestId(`nav-${item.id}`)).toBeInTheDocument();
    }
  });

  it('marks the active section as active', async () => {
    render(<Sidebar active="behavior" onNavigate={() => {}} />);

    expect(await screen.findByTestId('nav-behavior')).toHaveClass('active');
    expect(screen.getByTestId('nav-general').className).not.toContain('active');
  });

  it('calls onNavigate with the section id when a nav item is clicked', async () => {
    const onNavigate = vi.fn();
    render(<Sidebar active="general" onNavigate={onNavigate} />);

    fireEvent.click(await screen.findByTestId('nav-connections'));
    expect(onNavigate).toHaveBeenCalledWith('connections');
  });

  it('shows "No model selected" when no connection has an enabled model', async () => {
    mockConnections([]);
    render(<Sidebar active="general" onNavigate={() => {}} />);

    expect(await screen.findByTestId('sidebar-active-model')).toHaveTextContent('No model selected');
  });

  it('shows the active model summary when a connection has an enabled model', async () => {
    mockConnections([
      { id: '1', providerKind: 'ollama', baseUrl: 'http://localhost:11434', enabledModels: ['llama3'] },
    ]);
    render(<Sidebar active="general" onNavigate={() => {}} />);

    await waitFor(() => {
      expect(screen.getByTestId('sidebar-active-model')).toHaveTextContent('llama3');
    });
  });

  it('navigates to the Models screen when the active-model summary is clicked', async () => {
    const onNavigate = vi.fn();
    render(<Sidebar active="general" onNavigate={onNavigate} />);

    fireEvent.click(await screen.findByTestId('sidebar-active-model'));
    expect(onNavigate).toHaveBeenCalledWith('models');
  });
});
