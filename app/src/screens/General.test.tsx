import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import General, { formatHotkey } from './General';
import * as ipc from '@/lib/ipc';

vi.mock('@/lib/ipc', () => ({
  getPermissionStatus: vi.fn(),
  settingsGet: vi.fn(),
  settingsSet: vi.fn(),
}));

const mockedIpc = vi.mocked(ipc);

describe('formatHotkey', () => {
  it('converts modifier names to their glyphs with no separators', () => {
    expect(formatHotkey('Ctrl+Alt+R')).toBe('⌃⌥R');
  });

  it('handles a single modifier plus key', () => {
    expect(formatHotkey('Cmd+K')).toBe('⌘K');
  });

  it('passes through a combo with no recognized modifiers unchanged', () => {
    expect(formatHotkey('F5')).toBe('F5');
  });
});

describe('General', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mockedIpc.getPermissionStatus.mockResolvedValue({ granted: true });
    mockedIpc.settingsGet.mockResolvedValue(null);
    mockedIpc.settingsSet.mockResolvedValue(undefined);
  });

  it('shows granted permission status, the hotkey, active-model summary, and the menu-bar link', async () => {
    mockedIpc.settingsGet.mockImplementation((key: string) =>
      Promise.resolve(key === 'hotkey_combo' ? 'Ctrl+Alt+R' : null),
    );

    render(<General />);

    await waitFor(() => expect(mockedIpc.getPermissionStatus).toHaveBeenCalledTimes(1));

    const permStatus = screen.getByTestId('perm-status');
    await waitFor(() => expect(permStatus).toHaveAttribute('data-granted', 'true'));
    expect(permStatus).toHaveTextContent('Granted');

    await waitFor(() => expect(screen.getByTestId('hotkey-value')).toHaveTextContent('⌃⌥R'));
    expect(screen.getByTestId('hotkey-change')).toBeInTheDocument();
    expect(screen.getByTestId('active-model-link')).toHaveTextContent('No model selected');
    expect(screen.getByTestId('general-tray-link')).toBeInTheDocument();
    expect(screen.getByTestId('setting-theme')).toBeInTheDocument();
  });

  it('shows a not-granted status when permission_status reports ungranted', async () => {
    mockedIpc.getPermissionStatus.mockResolvedValue({ granted: false });

    render(<General />);

    const permStatus = screen.getByTestId('perm-status');
    await waitFor(() => expect(permStatus).toHaveAttribute('data-granted', 'false'));
    expect(permStatus).toHaveTextContent('Not granted');
  });

  it('re-checks permission status when perm-recheck is clicked', async () => {
    render(<General />);

    await waitFor(() => expect(mockedIpc.getPermissionStatus).toHaveBeenCalledTimes(1));

    fireEvent.click(screen.getByTestId('perm-recheck'));

    await waitFor(() => expect(mockedIpc.getPermissionStatus).toHaveBeenCalledTimes(2));
  });

  it('persists the chosen theme via settingsSet and marks it active', async () => {
    render(<General />);

    await waitFor(() => expect(mockedIpc.getPermissionStatus).toHaveBeenCalledTimes(1));

    fireEvent.click(screen.getByTestId('theme-dark'));

    expect(mockedIpc.settingsSet).toHaveBeenCalledWith('theme', 'dark');
    expect(screen.getByTestId('theme-dark')).toHaveClass('active');
  });
});
