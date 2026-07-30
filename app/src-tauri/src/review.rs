// The review panel: the window Behavior's "Review & confirm" inject mode
// always implied but never had.
//
// With that mode selected, `refine` deliberately injects nothing and parks the
// result in `RefineState::pending_review` for the user to accept, edit or
// discard. Every piece of that existed — the orchestrator's `PendingReview`
// flow, `inject_text` (accept, which clears the pending result) and
// `cancel_refine` (discard) — except a surface to show it on. The app declared
// exactly one window, nothing created another, and nothing emitted or listened
// for a pending review. So choosing the mode called the model, spent the
// tokens, and silently dropped the answer.
//
// ## Why accepting hides the window first
//
// Injection targets whatever application is frontmost. This panel, unlike the
// HUD, *must* take focus — the user has to be able to type into it — which
// makes redrafter itself frontmost. Injecting from that state would paste the
// refined text into our own window.
//
// So `accept` hides the panel, waits for the window server to hand focus back
// to the application the user came from, and only then injects. That ordering
// lives here rather than in the frontend so it cannot race.

use tauri::{Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

use crate::orchestrator::RefineOutcome;
use crate::RefineState;

/// Window label for the review panel.
pub const REVIEW_LABEL: &str = "review";

/// Logical size. Roomy enough for a paragraph of before/after without
/// becoming a second settings window.
const REVIEW_WIDTH: f64 = 620.0;
const REVIEW_HEIGHT: f64 = 460.0;

/// How long to wait after hiding the panel before injecting, so the window
/// server has handed focus back to the app the user was in. Long enough to be
/// reliable, short enough not to feel like a stall.
const FOCUS_RESTORE_DELAY_MS: u64 = 160;

/// Event emitted when a refine parks a result for review, so an already-open
/// panel refreshes instead of showing the previous draft.
pub const REVIEW_PENDING_EVENT: &str = "review:pending";

/// Builds the review window, hidden. Called once from the app's `setup`.
///
/// Failure is logged, not fatal: without the panel the app still works in the
/// default blind-inject mode.
pub fn create<R: Runtime>(app: &tauri::AppHandle<R>) {
    if app.get_webview_window(REVIEW_LABEL).is_some() || crate::auxiliary_windows_disabled() {
        return;
    }
    let built = WebviewWindowBuilder::new(app, REVIEW_LABEL, WebviewUrl::App("review".into()))
        .title("redrafter — review")
        .inner_size(REVIEW_WIDTH, REVIEW_HEIGHT)
        .min_inner_size(420.0, 320.0)
        .resizable(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        // NOT `.center()`: centering at build time asks tao for monitor
        // geometry before the event loop is running, and it aborts the process
        // with an unwrap-on-None inside tao's GTK event loop — at startup, for
        // every user, not just headless. Centering happens in `show` instead,
        // by which point the monitor is known. Reproduced and fixed here.
        .visible(false)
        .build();

    if let Err(e) = built {
        eprintln!("[review] could not create the review window: {e}");
    }
}

/// Shows the panel and gives it focus, so the user can act on the draft
/// immediately. No-op when the window is missing.
pub fn show<R: Runtime>(app: &tauri::AppHandle<R>) {
    let Some(window) = app.get_webview_window(REVIEW_LABEL) else {
        return;
    };
    let _ = window.center();
    let _ = window.show();
    let _ = window.set_always_on_top(true);
    // Focus is wanted here (unlike the HUD): the panel is interactive, and
    // `accept` restores focus to the previous app before injecting.
    let _ = window.set_focus();
}

/// Hides the panel. No-op when the window is missing.
pub fn hide<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window(REVIEW_LABEL) {
        let _ = window.hide();
    }
}

/// Tauri command: the draft awaiting review, if any.
///
/// Returning `None` rather than erroring lets the panel open on an empty state
/// (e.g. reopened after the draft was already resolved) instead of throwing.
#[tauri::command]
pub fn review_pending(refine_state: tauri::State<'_, RefineState>) -> Option<RefineOutcome> {
    refine_state.pending_review.lock().unwrap().clone()
}

/// Tauri command: accepts `text` (the refined draft, possibly edited) and
/// injects it into the app the user came from.
///
/// Hides the panel and waits before injecting — see this module's header.
#[tauri::command]
pub async fn review_accept<R: Runtime>(
    app: tauri::AppHandle<R>,
    text: String,
) -> Result<(), String> {
    hide(&app);
    tokio::time::sleep(std::time::Duration::from_millis(FOCUS_RESTORE_DELAY_MS)).await;
    crate::inject_text_impl(&app.state::<RefineState>(), &text)
}

/// Tauri command: discards the pending draft and closes the panel. The
/// original text is left untouched in the source app.
#[tauri::command]
pub fn review_discard<R: Runtime>(app: tauri::AppHandle<R>) {
    hide(&app);
    crate::clear_pending_review(&app.state::<RefineState>());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_panel_is_big_enough_to_read_a_paragraph_of_before_and_after() {
        assert!(REVIEW_WIDTH >= 480.0 && REVIEW_HEIGHT >= 360.0);
    }

    #[test]
    fn focus_restore_is_perceptible_but_not_a_stall() {
        // Long enough for the window server to hand focus back, short enough
        // that accepting still feels immediate.
        assert!((80..=400).contains(&FOCUS_RESTORE_DELAY_MS));
    }

    #[test]
    fn the_review_window_has_its_own_label_distinct_from_the_hud() {
        assert_ne!(REVIEW_LABEL, crate::hud::HUD_LABEL);
        assert_ne!(REVIEW_LABEL, "main");
    }
}
