import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import General, { formatHotkey } from './General';
import * as ipc from '@/lib/ipc';

vi.mock('@/lib/ipc', () => ({
  getPermissionStatus: vi.fn(),
  settingsGet: vi.fn(),
  settingsSet: vi.fn(),
  hotkeySet: vi.fn(),
  setLaunchAtLogin: vi.fn(),
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
    mockedIpc.hotkeySet.mockResolvedValue({ ok: true, conflict: false });
    mockedIpc.setLaunchAtLogin.mockResolvedValue(undefined);
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

describe('Hotkey rebind dialog (C6/S34)', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mockedIpc.getPermissionStatus.mockResolvedValue({ granted: true });
    mockedIpc.settingsGet.mockImplementation((key: string) =>
      Promise.resolve(key === 'hotkey_combo' ? 'Ctrl+Alt+R' : null),
    );
    mockedIpc.settingsSet.mockResolvedValue(undefined);
    mockedIpc.hotkeySet.mockResolvedValue({ ok: true, conflict: false });
    mockedIpc.setLaunchAtLogin.mockResolvedValue(undefined);
  });

  async function openDialog() {
    render(<General />);
    await waitFor(() => expect(screen.getByTestId('hotkey-value')).toHaveTextContent('⌃⌥R'));
    fireEvent.click(screen.getByTestId('hotkey-change'));
  }

  it('opens the capture dialog when hotkey-change is clicked', async () => {
    await openDialog();

    expect(screen.getByTestId('hotkey-modal')).toBeInTheDocument();
    expect(screen.getByTestId('hotkey-capture')).toBeInTheDocument();
    expect(screen.getByTestId('hotkey-cancel')).toBeInTheDocument();
    expect(screen.getByTestId('hotkey-save')).toBeInTheDocument();
  });

  it('captures a key combo, saves it via hotkeySet, and updates the shown hotkey on success', async () => {
    await openDialog();

    fireEvent.keyDown(screen.getByTestId('hotkey-capture'), { key: 'S', ctrlKey: true, altKey: true });
    expect(screen.getByTestId('hotkey-capture')).toHaveTextContent('⌃⌥S');

    fireEvent.click(screen.getByTestId('hotkey-save'));

    await waitFor(() => expect(mockedIpc.hotkeySet).toHaveBeenCalledWith('Ctrl+Alt+S'));
    await waitFor(() => expect(screen.getByTestId('hotkey-value')).toHaveTextContent('⌃⌥S'));
    expect(screen.queryByTestId('hotkey-modal')).not.toBeInTheDocument();
  });

  it('shows a conflict warning and keeps the previous hotkey when hotkeySet reports a conflict', async () => {
    mockedIpc.hotkeySet.mockResolvedValue({ ok: false, conflict: true });
    await openDialog();

    fireEvent.keyDown(screen.getByTestId('hotkey-capture'), { key: 'T', ctrlKey: true, altKey: true });
    fireEvent.click(screen.getByTestId('hotkey-save'));

    await waitFor(() => expect(screen.getByTestId('hotkey-conflict')).toBeInTheDocument());
    expect(screen.getByTestId('hotkey-modal')).toBeInTheDocument();
    expect(screen.getByTestId('hotkey-value')).toHaveTextContent('⌃⌥R');
  });

  it('shows a conflict warning when hotkeySet rejects outright', async () => {
    mockedIpc.hotkeySet.mockRejectedValue(new Error('backend unavailable'));
    await openDialog();

    fireEvent.keyDown(screen.getByTestId('hotkey-capture'), { key: 'T', ctrlKey: true, altKey: true });
    fireEvent.click(screen.getByTestId('hotkey-save'));

    await waitFor(() => expect(screen.getByTestId('hotkey-conflict')).toBeInTheDocument());
    expect(screen.getByTestId('hotkey-value')).toHaveTextContent('⌃⌥R');
  });

  it('closes the dialog without changing the hotkey when hotkey-cancel is clicked', async () => {
    await openDialog();

    fireEvent.keyDown(screen.getByTestId('hotkey-capture'), { key: 'S', ctrlKey: true, altKey: true });
    fireEvent.click(screen.getByTestId('hotkey-cancel'));

    expect(mockedIpc.hotkeySet).not.toHaveBeenCalled();
    expect(screen.queryByTestId('hotkey-modal')).not.toBeInTheDocument();
    expect(screen.getByTestId('hotkey-value')).toHaveTextContent('⌃⌥R');
  });
});

describe('Launch-at-login toggle (C6)', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mockedIpc.getPermissionStatus.mockResolvedValue({ granted: true });
    mockedIpc.settingsSet.mockResolvedValue(undefined);
    mockedIpc.hotkeySet.mockResolvedValue({ ok: true, conflict: false });
    mockedIpc.setLaunchAtLogin.mockResolvedValue(undefined);
  });

  it('reflects the persisted state and toggles via setLaunchAtLogin', async () => {
    mockedIpc.settingsGet.mockImplementation((key: string) =>
      Promise.resolve(key === 'launch_at_login' ? 'false' : null),
    );

    render(<General />);

    const toggle = await screen.findByTestId('general-launch-login');
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'false'));

    fireEvent.click(toggle);

    await waitFor(() => expect(mockedIpc.setLaunchAtLogin).toHaveBeenCalledWith(true));
    expect(toggle).toHaveAttribute('aria-checked', 'true');
  });

  it('reverts the toggle when setLaunchAtLogin rejects', async () => {
    mockedIpc.settingsGet.mockResolvedValue(null);
    mockedIpc.setLaunchAtLogin.mockRejectedValue(new Error('nope'));

    render(<General />);

    const toggle = await screen.findByTestId('general-launch-login');
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'true'));

    fireEvent.click(toggle);

    await waitFor(() => expect(mockedIpc.setLaunchAtLogin).toHaveBeenCalledWith(false));
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'true'));
  });
});
