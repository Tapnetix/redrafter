import { fireEvent, render, screen } from '@testing-library/react';
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

  it('no longer carries the theme toggle', () => {
    // It moved to the topbar (see ThemeToggle.test.tsx): the rail is hidden
    // whenever the sidebar is shown, and the toggle has to survive that.
    render(<NavRail active="general" onNavigate={() => {}} />);
    expect(screen.queryByTestId('theme-toggle')).not.toBeInTheDocument();
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

});
