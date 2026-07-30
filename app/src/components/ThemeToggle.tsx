'use client';

// The light/dark toggle, extracted from `NavRail` so it survives the rail
// being hidden.
//
// The shell used to show two complete navigations side by side above the
// 860px breakpoint: an icon rail and a sidebar listing the very same six
// sections, one as icons and one as icons with labels. Widening the window
// made the duplication obvious. The rail is now hidden whenever the sidebar
// is shown — but the toggle lived in the rail, so it moved to the topbar,
// which is present in both layouts. One instance, always reachable.

import { loadTheme, setTheme, toggledTheme, type Theme } from '@/lib/theme';

export default function ThemeToggle({ className = 'rail-btn' }: { className?: string }) {
  const onToggleTheme = async () => {
    const current: Theme = await loadTheme();
    await setTheme(toggledTheme(current));
  };

  return (
    <button
      className={className}
      id="theme-toggle"
      aria-label="Toggle theme"
      title="Toggle light / dark"
      data-testid="theme-toggle"
      onClick={onToggleTheme}
    >
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8} aria-hidden="true">
        <path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8Z" />
      </svg>
    </button>
  );
}
