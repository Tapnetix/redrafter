import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import ThemeToggle from './ThemeToggle';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const mockedInvoke = vi.mocked(invoke);

// Moved here from NavRail: the rail is hidden whenever the sidebar is shown
// (they listed the same six sections), so the toggle now lives in the topbar,
// which both layouts render.
describe('ThemeToggle', () => {
  beforeEach(() => {
    window.matchMedia = vi.fn().mockReturnValue({ matches: false }) as unknown as typeof window.matchMedia;
    document.documentElement.classList.remove('light');
    mockedInvoke.mockReset();
    mockedInvoke.mockResolvedValue(undefined);
  });
  afterEach(() => mockedInvoke.mockReset());

  it('renders the control', () => {
    render(<ThemeToggle />);
    expect(screen.getByTestId('theme-toggle')).toBeInTheDocument();
  });

  it('applies and persists the toggled theme', async () => {
    render(<ThemeToggle />);

    fireEvent.click(screen.getByTestId('theme-toggle'));

    await waitFor(() => {
      expect(mockedInvoke).toHaveBeenCalledWith('settings_set', { key: 'theme', value: 'light' });
    });
    expect(document.documentElement.classList.contains('light')).toBe(true);
  });
});
