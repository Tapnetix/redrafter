// The in-flight HUD: a small always-on-top chip that appears next to the
// pointer while a refine runs.
//
// The menu-bar spinner (see `tray.rs::set_busy_icon`) is always on screen but
// peripheral — while you are typing in Slack you are not looking at the menu
// bar. This puts the same signal where your eyes already are.
//
// Deliberate constraints, each of which would break the refine if got wrong:
//
//   * **Never takes focus.** The refine pastes into whatever app is frontmost;
//     if showing this window activated redrafter, the paste target would
//     change underneath us. Built with `focused(false)` and only ever shown,
//     never focused.
//   * **Click-through.** `set_ignore_cursor_events(true)` so it can sit over
//     the text you are about to edit without swallowing a click.
//   * **Created once, at startup.** Building a window per refine would both
//     risk activation and show an unpainted webview on first frame.
//   * **Opaque, not transparent.** Transparent windows need macOS private
//     APIs; a small solid chip needs none.
//
// Anchoring is to the *pointer*, not the text caret. Caret bounds mean
// `AXUIElementCopyParameterizedAttributeValue` (the `accessibility` crate has
// no binding for it), which is untestable without Accessibility permission --
// see this module's tests and the README note.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use tauri::{Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

/// Window label for the HUD, used for every later lookup.
pub const HUD_LABEL: &str = "hud";

/// Logical size of the chip. Wide enough for "Refining…" beside a spinner.
const HUD_WIDTH: f64 = 132.0;
const HUD_HEIGHT: f64 = 34.0;

/// Gap between the pointer and the chip, so the chip never sits under the
/// cursor itself (where it would hide the very text being refined).
const CURSOR_GAP: f64 = 18.0;

/// A rectangle in physical pixels — enough of a monitor/window for
/// [`position_near_cursor`] to work without a Tauri handle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Places the chip below-right of the cursor, flipping to the other side
/// whenever that would push it off `monitor`.
///
/// Pure so the placement is unit-testable on any host: the interesting cases
/// are all edges of the screen, and none of them need a window system.
pub fn position_near_cursor(cursor: (f64, f64), hud: (f64, f64), monitor: Rect) -> (f64, f64) {
    let (cx, cy) = cursor;
    let (w, h) = hud;

    // Preferred: below and to the right, the direction a tooltip grows.
    let mut x = cx + CURSOR_GAP;
    let mut y = cy + CURSOR_GAP;

    // Flip rather than merely clamp when the preferred side doesn't fit, so
    // the chip stays beside the pointer instead of sliding under it.
    if x + w > monitor.x + monitor.width {
        x = cx - CURSOR_GAP - w;
    }
    if y + h > monitor.y + monitor.height {
        y = cy - CURSOR_GAP - h;
    }

    // A monitor narrower than the chip (or a cursor right at the origin) can
    // still leave it out of bounds; clamp as the last resort.
    x = x.clamp(monitor.x, (monitor.x + monitor.width - w).max(monitor.x));
    y = y.clamp(monitor.y, (monitor.y + monitor.height - h).max(monitor.y));
    (x, y)
}

/// Environment variable that suppresses the HUD window entirely.
///
/// `tauri-driver` (and the WebKitWebDriver behind it) cannot create a session
/// against an app that exposes a second webview — session creation just times
/// out — which takes down `e2e-real`, the only harness that drives the real
/// packaged binary. Rather than lose that acceptance gate, the HUD can be
/// switched off for a WebDriver run; `e2e-real/wdio.conf.ts` sets this.
pub const DISABLE_HUD_ENV: &str = "REDRAFTER_DISABLE_HUD";

/// Whether [`DISABLE_HUD_ENV`] asks for the HUD to be suppressed. Any value
/// other than an empty string or "0" counts as "yes".
pub fn hud_disabled_from(value: Option<&str>) -> bool {
    matches!(value, Some(v) if !v.is_empty() && v != "0")
}

fn hud_disabled() -> bool {
    crate::auxiliary_windows_disabled()
}

/// What the chip is currently saying.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HudState {
    /// `"refining"` (spinner) or `"error"` (what went wrong).
    pub kind: String,
    /// Shown for an error; empty while refining.
    pub text: String,
}

impl HudState {
    pub fn refining() -> Self {
        Self { kind: "refining".to_string(), text: String::new() }
    }
    pub fn error(text: &str) -> Self {
        Self { kind: "error".to_string(), text: text.to_string() }
    }
}

/// Event the chip listens on.
pub const HUD_STATE_EVENT: &str = "hud:state";

/// How long a failure stays on screen before the chip dismisses itself.
const ERROR_VISIBLE_MS: u64 = 6_000;

static HUD_MESSAGE: LazyLock<Mutex<Option<HudState>>> = LazyLock::new(|| Mutex::new(None));

/// Bumped on every state change, so an auto-dismiss timer only hides the chip
/// if it is still showing the message that scheduled it.
static HUD_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Tauri command: what the chip should show right now. Read on mount, since
/// the window may have been created before the first event was emitted.
#[tauri::command]
pub fn hud_state() -> Option<HudState> {
    HUD_MESSAGE.lock().unwrap().clone()
}

fn publish<R: Runtime>(app: &tauri::AppHandle<R>, state: HudState) -> u64 {
    *HUD_MESSAGE.lock().unwrap() = Some(state.clone());
    let generation = HUD_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    if let Err(e) = app.emit(HUD_STATE_EVENT, state) {
        eprintln!("[hud] failed to publish the chip state: {e}");
    }
    generation
}

/// Shows the in-flight chip.
pub fn show_refining<R: Runtime>(app: &tauri::AppHandle<R>) {
    publish(app, HudState::refining());
    show(app);
}

/// Shows a failure on the chip, then dismisses it after a few seconds.
///
/// Without this a failed refine was *completely* silent: the global-shortcut
/// handler drops the JoinHandle its dispatch returns, so the `Err` was never
/// read, logged or surfaced anywhere. The user saw the spinner appear, vanish,
/// and nothing else — with no way to find out why.
pub fn show_error<R: Runtime>(app: &tauri::AppHandle<R>, message: &str) {
    let generation = publish(app, HudState::error(message));
    show(app);

    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(ERROR_VISIBLE_MS));
        // Only dismiss if nothing has happened since; a refine started in the
        // meantime owns the chip now.
        if HUD_GENERATION.load(Ordering::SeqCst) == generation {
            hide(&handle);
        }
    });
}

/// Builds the HUD window, hidden. Called once from the app's `setup`.
///
/// Failure is logged and swallowed: a missing HUD must never stop the app from
/// starting, and the menu-bar spinner still reports progress without it.
pub fn create<R: Runtime>(app: &tauri::AppHandle<R>) {
    if app.get_webview_window(HUD_LABEL).is_some() {
        return;
    }
    if hud_disabled() {
        return;
    }
    let built = WebviewWindowBuilder::new(app, HUD_LABEL, WebviewUrl::App("hud".into()))
        .title("redrafter status")
        .inner_size(HUD_WIDTH, HUD_HEIGHT)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .focused(false)
        .visible(false)
        .build();

    match built {
        Ok(window) => {
            // Clicks must reach whatever is underneath — this chip sits over
            // the text the user is about to keep editing.
            if let Err(e) = window.set_ignore_cursor_events(true) {
                eprintln!("[hud] could not make the HUD click-through: {e}");
            }
        }
        Err(e) => eprintln!("[hud] could not create the HUD window: {e}"),
    }
}

/// Shows the chip beside the pointer. No-op when the window is missing.
pub fn show<R: Runtime>(app: &tauri::AppHandle<R>) {
    let Some(window) = app.get_webview_window(HUD_LABEL) else {
        // Either creation failed or the aux windows are gated off. Say which,
        // because "the chip never appeared" is otherwise indistinguishable
        // from "the chip appeared somewhere I wasn't looking".
        eprintln!(
            "[hud] no HUD window to show (creation failed, or disabled via the env gate: {})",
            crate::auxiliary_windows_disabled()
        );
        return;
    };

    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());

    match (app.cursor_position(), monitor.as_ref()) {
        (Ok(cursor), Some(monitor)) => {
            let pos = monitor.position();
            let size = monitor.size();
            let scale = window.scale_factor().unwrap_or_else(|_| monitor.scale_factor());
            let (x, y) = position_near_cursor(
                (cursor.x, cursor.y),
                (HUD_WIDTH * scale, HUD_HEIGHT * scale),
                Rect {
                    x: pos.x as f64,
                    y: pos.y as f64,
                    width: size.width as f64,
                    height: size.height as f64,
                },
            );
            eprintln!(
                "[hud] cursor=({:.0},{:.0}) monitor={}x{}+{}+{} scale={scale} -> chip at ({x:.0},{y:.0})",
                cursor.x, cursor.y, size.width, size.height, pos.x, pos.y
            );
            if let Err(e) = window.set_position(tauri::PhysicalPosition::new(x, y)) {
                eprintln!("[hud] could not position the chip: {e}");
            }
        }
        (cursor, monitor) => {
            // Without a cursor or a monitor there is nothing to anchor to.
            // Centre rather than leaving the chip wherever it was built, which
            // may be off-screen entirely.
            eprintln!(
                "[hud] cannot anchor to the pointer (cursor: {}, monitor: {}) — centring instead",
                cursor.map(|_| "ok").unwrap_or("unavailable"),
                if monitor.is_some() { "ok" } else { "unavailable" }
            );
            let _ = window.center();
        }
    }

    // `show` only — never `set_focus`, which would pull the frontmost app out
    // from under the paste that follows.
    if let Err(e) = window.show() {
        eprintln!("[hud] show failed: {e}");
    }
    let _ = window.set_always_on_top(true);
    // macOS in particular: a window belonging to an app that isn't frontmost
    // can be ordered in without becoming visible. Report what the window
    // system actually thinks, so "it never appeared" is a fact rather than a
    // guess.
    match window.is_visible() {
        Ok(visible) => eprintln!("[hud] window reports visible={visible}"),
        Err(e) => eprintln!("[hud] could not read window visibility: {e}"),
    }
}

/// Hides the chip. No-op when the window is missing.
pub fn hide<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window(HUD_LABEL) {
        let _ = window.hide();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HUD: (f64, f64) = (132.0, 34.0);

    fn screen() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        }
    }

    #[test]
    fn the_disable_switch_reads_the_usual_truthy_spellings() {
        assert!(!hud_disabled_from(None));
        assert!(!hud_disabled_from(Some("")));
        assert!(!hud_disabled_from(Some("0")));
        assert!(hud_disabled_from(Some("1")));
        assert!(hud_disabled_from(Some("true")));
    }

    #[test]
    fn sits_below_and_right_of_the_cursor_with_room_to_spare() {
        let (x, y) = position_near_cursor((400.0, 300.0), HUD, screen());
        assert_eq!((x, y), (400.0 + CURSOR_GAP, 300.0 + CURSOR_GAP));
    }

    #[test]
    fn never_sits_directly_under_the_pointer() {
        // Whatever the corner, the chip must be offset — covering the caret
        // would hide the text being refined.
        for cursor in [(0.0, 0.0), (1919.0, 1079.0), (960.0, 540.0), (1919.0, 0.0)] {
            let (x, y) = position_near_cursor(cursor, HUD, screen());
            let overlaps_x = cursor.0 >= x && cursor.0 <= x + HUD.0;
            let overlaps_y = cursor.1 >= y && cursor.1 <= y + HUD.1;
            assert!(!(overlaps_x && overlaps_y), "chip covers the pointer at {cursor:?}");
        }
    }

    #[test]
    fn flips_left_at_the_right_edge() {
        let (x, _) = position_near_cursor((1900.0, 300.0), HUD, screen());
        assert_eq!(x, 1900.0 - CURSOR_GAP - HUD.0);
    }

    #[test]
    fn flips_above_at_the_bottom_edge() {
        let (_, y) = position_near_cursor((400.0, 1070.0), HUD, screen());
        assert_eq!(y, 1070.0 - CURSOR_GAP - HUD.1);
    }

    #[test]
    fn flips_both_ways_in_the_bottom_right_corner() {
        let (x, y) = position_near_cursor((1915.0, 1075.0), HUD, screen());
        assert_eq!(x, 1915.0 - CURSOR_GAP - HUD.0);
        assert_eq!(y, 1075.0 - CURSOR_GAP - HUD.1);
    }

    #[test]
    fn stays_inside_the_monitor_on_every_edge() {
        for cursor in [
            (0.0, 0.0),
            (1919.0, 0.0),
            (0.0, 1079.0),
            (1919.0, 1079.0),
            (5.0, 5.0),
        ] {
            let (x, y) = position_near_cursor(cursor, HUD, screen());
            assert!(x >= 0.0 && x + HUD.0 <= 1920.0, "x out of bounds at {cursor:?}: {x}");
            assert!(y >= 0.0 && y + HUD.1 <= 1080.0, "y out of bounds at {cursor:?}: {y}");
        }
    }

    #[test]
    fn honours_a_secondary_monitor_offset() {
        // A monitor to the right of the primary: coordinates are global, so
        // the chip must land inside that monitor, not back on the first.
        let right = Rect {
            x: 1920.0,
            y: 0.0,
            width: 1280.0,
            height: 1024.0,
        };
        let (x, y) = position_near_cursor((1930.0, 40.0), HUD, right);
        assert!(x >= 1920.0, "chip escaped onto the primary monitor: {x}");
        assert!(x + HUD.0 <= 3200.0);
        assert!(y >= 0.0 && y + HUD.1 <= 1024.0);
    }

    #[test]
    fn degrades_gracefully_on_a_monitor_narrower_than_the_chip() {
        let tiny = Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 20.0,
        };
        let (x, y) = position_near_cursor((50.0, 10.0), HUD, tiny);
        // Can't fit; must still be finite and pinned to the origin rather than
        // flung off-screen.
        assert_eq!((x, y), (0.0, 0.0));
    }
}
