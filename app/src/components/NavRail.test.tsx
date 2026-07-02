import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import NavRail, { RAIL_ITEMS } from './NavRail';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);

describe('NavRail', () => {
  beforeEach(() => {
    window.matchMedia = vi.fn().mockReturnValue({ matches: false }) as unknown as typeof window.matchMedia;
    document.documentElement.classList.remove('light');
    mockedInvoke.mockReset();
    mockedInvoke.mockResolvedValue(undefined);
  });
  afterEach(() => {
    mockedInvoke.mockReset();
  });

  it('renders a rail button for every navigable section plus the logo', () => {
    render(<NavRail active="general" onNavigate={() => {}} />);

    expect(screen.getByTestId('rail-logo')).toBeInTheDocument();
    for (const item of RAIL_ITEMS) {
      expect(screen.getByTestId(`rail-${item.id}`)).toBeInTheDocument();
    }
  });

  it('renders the theme-toggle control', () => {
    render(<NavRail active="general" onNavigate={() => {}} />);
    expect(screen.getByTestId('theme-toggle')).toBeInTheDocument();
  });

  it('marks the active section button as active', () => {
    render(<NavRail active="behavior" onNavigate={() => {}} />);
    expect(screen.getByTestId('rail-behavior').className).toContain('active');
    expect(screen.getByTestId('rail-general').className).not.toContain('active');
  });

  it('calls onNavigate with the section id when a rail button is clicked', () => {
    const onNavigate = vi.fn();
    render(<NavRail active="general" onNavigate={onNavigate} />);

    fireEvent.click(screen.getByTestId('rail-connections'));
    expect(onNavigate).toHaveBeenCalledWith('connections');
  });

  it('routes the logo to the General screen', () => {
    const onNavigate = vi.fn();
    render(<NavRail active="behavior" onNavigate={onNavigate} />);

    fireEvent.click(screen.getByTestId('rail-logo'));
    expect(onNavigate).toHaveBeenCalledWith('general');
  });

  it('toggling the theme persists it via settings_set', async () => {
    render(<NavRail active="general" onNavigate={() => {}} />);

    fireEvent.click(screen.getByTestId('theme-toggle'));

    await waitFor(() => {
      expect(mockedInvoke).toHaveBeenCalledWith('settings_set', { key: 'theme', value: 'light' });
    });
    expect(document.documentElement.classList.contains('light')).toBe(true);
  });
});
