# e2e-real: cross-platform real-surface acceptance (S38)

This directory is the **real-surface acceptance gate** for redrafter's
cross-platform work (D1 Linux text-inject, D2 Windows text-inject, D3
platform-conditional permission/hotkey/tray). Per
`design-redrafter.md`'s "Acceptance Tooling":

> Playwright IPC-mock specs provide breadth but are never the acceptance
> gate for a surface.

`app/e2e/specs/d4.spec.ts` (Playwright, IPC-mocked) is breadth-only and
runs on every CI/dev machine including headless ones. **This directory is
the actual S38 gate** and only runs against the real packaged binary on a
real desktop session:

| Target | Tool | Spec |
|---|---|---|
| Linux (X11) | tauri-driver + WebdriverIO | `specs/refine-loop.e2e.ts` |
| Linux (Wayland) | tauri-driver + WebdriverIO | `specs/refine-loop.e2e.ts` |
| Windows | tauri-driver + WebdriverIO | `specs/refine-loop.e2e.ts` |
| macOS | launch + AppleScript smoke (no WKWebView WebDriver) | `specs/launch-smoke.macos.ts` |

None of these can run on a headless CI host (no live display, no packaged
binary, no macOS/Windows) -- that's why they live outside `app/e2e` and are
run by hand. This README is the hand-off: run the section for whichever
machine you're on, then record the result in the phase's completion notes.

## One-time setup (any OS)

```sh
cd e2e-real
pnpm install
pnpm exec tsc --noEmit   # sanity-check the harness typechecks
```

`tauri-driver` (Linux/Windows only) is a separate Rust binary, not an npm
package:

```sh
cargo install tauri-driver
```

Linux also needs WebKitGTK's WebDriver and the OS-level automation tools
`text-inject`'s own backend uses (D1's module doc):

```sh
# Debian/Ubuntu
sudo apt install webkit2gtk-driver xdotool xclip xsel wl-clipboard wtype ydotool sqlite3
```

## Linux: X11

```sh
cd e2e-real
export DISPLAY=:0                 # a live X11 session, or an Xvfb virtual one
pnpm run wdio:x11
```

This:
1. `cargo build`s the debug `redrafter` binary (or set `REDRAFTER_BINARY`
   to an already-built path, e.g. a release/bundled one).
2. Starts `tauri-driver`, which launches the built binary and drives it
   through WebdriverIO.
3. Runs `specs/refine-loop.e2e.ts`: adds a connection pointed at a local
   stub model endpoint (no cloud keys needed), sets it active, then drives
   the real global hotkey (`Ctrl+Alt+R`) against a real scratch text editor
   window and asserts the refined text actually landed there via the real
   X11 `xdotool`/`xclip` backend.

## Linux: Wayland

```sh
cd e2e-real
# WAYLAND_DISPLAY should already be set by your compositor session (do not
# also export DISPLAY, or the harness's session detection -- mirroring
# linux.rs's -- will pick X11/XWayland instead).
pnpm run wdio:wayland
```

Same flow as X11, but the refine loop's OS-level key simulation goes
through `wtype`/`ydotool` and the clipboard round-trip through
`wl-copy`/`wl-paste`. `ydotool` needs its `ydotoold` daemon running and
`uinput` access (see `crates/text-inject/src/linux.rs`'s module doc); if
it's not set up, the harness falls back to `wtype`, which is itself
compositor-dependent (relies on the `virtual-keyboard` protocol) -- if
your compositor doesn't support it, set `REDRAFTER_E2E_EDITOR` to a
different scratch editor or adjust `e2e-real/helpers/desktop.ts` to a tool
your compositor does support.

## Windows

```powershell
cd e2e-real
pnpm run wdio:windows
```

Same flow, but the scratch target is Notepad and the OS-level key
simulation/clipboard round-trip goes through PowerShell's
`System.Windows.Forms.SendKeys`/`Get-Clipboard`. `tauri-driver` on Windows
drives the WebView2 WebDriver (`msedgedriver`) directly; no separate
WebKitWebDriver-equivalent install is needed beyond `tauri-driver` itself.

## macOS (launch-smoke)

macOS has no WKWebView WebDriver, so there is no tauri-driver/WebdriverIO
run here -- `specs/launch-smoke.macos.ts` is a plain script combining a
real app launch with AppleScript/System Events UI scripting.

```sh
# 1. Build the real .app bundle.
cd app && pnpm build && pnpm tauri build
# (or: cd app/src-tauri && cargo tauri build)

# 2. Grant Accessibility once (standard macOS one-time step): open
#    System Settings > Privacy & Security > Accessibility and enable the
#    built redrafter.app (and/or your terminal, if driving it that way).

# 3. Run the smoke.
cd e2e-real
pnpm install   # if not already done
pnpm run launch-smoke:macos
```

This:
1. Seeds `~/Library/Application Support/com.redrafter.app`'s on-disk
   settings/connections SQLite stores with a connection pointed at a local
   stub model endpoint and makes it the active model (there's no WebDriver
   on macOS to click through Connections/Models with, and this needs no
   real cloud API key -- mirrors exactly what `connection_add` +
   `connection_refresh_models` + `model_set_active` would persist).
2. Launches the built `.app` and asserts the process reaches a running
   state with no crash/error dialog frontmost (the tray/onboarding
   reached-without-error part of S38).
3. Opens a real TextEdit document, selects its text, sends the real
   `Ctrl+Alt+R` global hotkey via System Events, and asserts TextEdit's
   content actually changed to the refined text -- the real macOS
   Accessibility-API `text-inject` capture/inject path, the same one
   Phase 1 built and D1-D3 didn't touch.

`REDRAFTER_APP_BUNDLE` overrides the default build-output path if your
bundle lands somewhere else.

## Recording the result

S38 is done once all four rows in the table above have been run
successfully at least once (a fresh run isn't required for every future
change -- see the phase gate in `phase-d/completion.md`). Record, per OS:
who ran it, when, and pass/fail (with a note on any workaround needed, e.g.
which scratch editor or Wayland key-sim tool was actually available) in
the phase completion notes.
