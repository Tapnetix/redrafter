//! Real Linux backend for [`crate::PlatformOps`], covering both X11 and
//! Wayland sessions.
//!
//! Linux has no cross-desktop equivalent of macOS's `AXSelectedText`
//! attribute, so this backend leans on the design's clipboard-primary
//! approach:
//!
//! - [`LinuxOps::ax_read_selection`] reads the X11/Wayland **PRIMARY**
//!   selection (the buffer toolkits populate as soon as text is
//!   mouse-selected, with no keystroke required) via `xclip`/`xsel` on
//!   X11 or `wl-paste --primary` on Wayland. This is the closest Linux
//!   analog to an AX read: when it works, it never touches the
//!   `CLIPBOARD` selection the user actually copies/pastes with.
//! - There is no reliable direct write to the selection/focused element
//!   on Linux, so [`LinuxOps::ax_write_selection`] always errs, and
//!   `inject_with` (see `inject.rs`) always falls through to the
//!   clipboard save/write/paste/restore path for writes.
//! - `clipboard_get`/`clipboard_set` read/write the `CLIPBOARD`
//!   selection; `simulate_copy`/`simulate_paste` drive Ctrl+C/Ctrl+V.
//!
//! The session type (X11 vs Wayland) is detected once at construction
//! time (`WAYLAND_DISPLAY`/`XDG_SESSION_TYPE`) and picks the tool set:
//!
//! | | X11 | Wayland |
//! |---|---|---|
//! | clipboard | `xclip`/`xsel` | `wl-copy`/`wl-paste` |
//! | key simulation | `xdotool key ctrl+c`/`ctrl+v` | `ydotool key <keycodes>` (falls back to `wtype`) |
//!
//! Wayland's security model deliberately restricts synthetic input, so
//! `ydotool` requires its `ydotoold` daemon and `uinput` access (the user
//! must be in the `input` group or run the daemon with appropriate
//! permissions) to work at all; `wtype` is tried as a fallback but is
//! itself compositor-dependent (relies on the `virtual-keyboard`
//! protocol). Neither this module nor its unit tests execute those
//! tools for real: command *construction* (which program, which args,
//! X11 vs Wayland selection) is unit-tested against a fake
//! [`CommandRunner`] below. Whether a live X11/Wayland session with
//! these tools installed actually round-trips a selection is deferred to
//! D4's real-surface pass.

use crate::PlatformOps;
use anyhow::{anyhow, Context, Result};
use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

/// Which windowing session we're running under — picks the clipboard and
/// key-simulation tools to shell out to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionType {
    X11,
    Wayland,
}

impl SessionType {
    /// Detect the live session from the environment.
    fn detect() -> Self {
        Self::detect_from(
            std::env::var("WAYLAND_DISPLAY").ok(),
            std::env::var("XDG_SESSION_TYPE").ok(),
        )
    }

    /// Pure detection logic, factored out of [`Self::detect`] so it can be
    /// unit-tested without mutating real process-wide environment
    /// variables (which is inherently racy across parallel tests).
    ///
    /// `WAYLAND_DISPLAY` being set (and non-empty) is the standard
    /// signal a Wayland compositor is running; `XDG_SESSION_TYPE=wayland`
    /// is a secondary signal for setups that don't export
    /// `WAYLAND_DISPLAY` to every process. Anything else is treated as
    /// X11 (including plain Xorg sessions and XWayland-only contexts).
    fn detect_from(wayland_display: Option<String>, xdg_session_type: Option<String>) -> Self {
        let wayland_display_set = wayland_display.map(|v| !v.is_empty()).unwrap_or(false);
        let xdg_says_wayland = xdg_session_type.as_deref() == Some("wayland");
        if wayland_display_set || xdg_says_wayland {
            SessionType::Wayland
        } else {
            SessionType::X11
        }
    }
}

/// The result of running an external command: whether it exited
/// successfully, and its captured (trailing-newline-trimmed) stdout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunOutput {
    pub success: bool,
    pub stdout: String,
}

/// Abstracts "run this external command" — the seam that lets command
/// *construction* (which program, which args, whether stdin is piped) be
/// unit-tested without a real X11/Wayland session or these tools
/// installed. [`RealCommandRunner`] is the only implementation used
/// outside tests.
pub(crate) trait CommandRunner {
    /// Run `program args...`, piping `stdin` to it if given, and capture
    /// stdout. Returns `Err` only when the command couldn't be spawned at
    /// all (e.g. `program` isn't installed) — a successfully spawned but
    /// nonzero-exit command is `Ok(RunOutput { success: false, .. })`, so
    /// callers can distinguish "tool missing, try the next one" from
    /// "tool ran and reported failure".
    fn run(&self, program: &str, args: &[String], stdin: Option<&str>) -> Result<RunOutput>;
}

/// Shells out via `std::process::Command`, mirroring `macos.rs`'s
/// `pbcopy`/`pbpaste`/`osascript` invocations — dependency-light, no
/// Linux-only crate required.
pub(crate) struct RealCommandRunner;

impl CommandRunner for RealCommandRunner {
    fn run(&self, program: &str, args: &[String], stdin: Option<&str>) -> Result<RunOutput> {
        let mut command = Command::new(program);
        command.args(args);
        command
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        let mut child = command
            .spawn()
            .with_context(|| format!("failed to spawn {program}"))?;

        if let Some(input) = stdin {
            if let Some(mut child_stdin) = child.stdin.take() {
                child_stdin
                    .write_all(input.as_bytes())
                    .with_context(|| format!("failed to write to {program} stdin"))?;
            }
        }

        let output = child
            .wait_with_output()
            .with_context(|| format!("failed to wait on {program}"))?;

        Ok(RunOutput {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout)
                .trim_end_matches('\n')
                .to_string(),
        })
    }
}

/// Ordered candidate `(program, args)` pairs to try for reading the
/// PRIMARY selection (the mouse-selection buffer, populated without any
/// keystroke) — the closest Linux analog to an AX read.
fn primary_get_candidates(session: SessionType) -> Vec<(&'static str, Vec<String>)> {
    match session {
        SessionType::X11 => vec![
            (
                "xclip",
                vec!["-selection".into(), "primary".into(), "-o".into()],
            ),
            ("xsel", vec!["--primary".into(), "--output".into()]),
        ],
        SessionType::Wayland => vec![("wl-paste", vec!["--primary".into(), "--no-newline".into()])],
    }
}

/// Ordered candidates to read the `CLIPBOARD` selection.
fn clipboard_get_candidates(session: SessionType) -> Vec<(&'static str, Vec<String>)> {
    match session {
        SessionType::X11 => vec![
            (
                "xclip",
                vec!["-selection".into(), "clipboard".into(), "-o".into()],
            ),
            ("xsel", vec!["--clipboard".into(), "--output".into()]),
        ],
        SessionType::Wayland => vec![("wl-paste", vec!["--no-newline".into()])],
    }
}

/// Ordered candidates to write `CLIPBOARD` (text is piped via stdin).
fn clipboard_set_candidates(session: SessionType) -> Vec<(&'static str, Vec<String>)> {
    match session {
        SessionType::X11 => vec![
            ("xclip", vec!["-selection".into(), "clipboard".into()]),
            ("xsel", vec!["--clipboard".into(), "--input".into()]),
        ],
        SessionType::Wayland => vec![("wl-copy", vec![])],
    }
}

/// Ordered candidates to simulate a Ctrl+C ("Copy") keystroke.
///
/// `xdotool` takes a symbolic key combo on X11. Wayland has no such
/// convenience: `ydotool key` takes raw Linux input event codes with a
/// press(`:1`)/release(`:0`) suffix, so Ctrl+C is
/// `KEY_LEFTCTRL(29) down, KEY_C(46) down, KEY_C up, KEY_LEFTCTRL up`.
/// `wtype` (which uses the compositor's virtual-keyboard protocol rather
/// than `uinput`) is tried as a fallback for compositors where `ydotool`
/// isn't set up.
fn key_copy_candidates(session: SessionType) -> Vec<(&'static str, Vec<String>)> {
    match session {
        SessionType::X11 => vec![(
            "xdotool",
            vec!["key".into(), "--clearmodifiers".into(), "ctrl+c".into()],
        )],
        SessionType::Wayland => vec![
            (
                "ydotool",
                vec![
                    "key".into(),
                    "29:1".into(),
                    "46:1".into(),
                    "46:0".into(),
                    "29:0".into(),
                ],
            ),
            (
                "wtype",
                vec![
                    "-M".into(),
                    "ctrl".into(),
                    "-k".into(),
                    "c".into(),
                    "-m".into(),
                    "ctrl".into(),
                ],
            ),
        ],
    }
}

/// Ordered candidates to simulate a Ctrl+V ("Paste") keystroke. See
/// [`key_copy_candidates`] for the Wayland keycode/fallback rationale
/// (`KEY_V` is event code 47).
fn key_paste_candidates(session: SessionType) -> Vec<(&'static str, Vec<String>)> {
    match session {
        SessionType::X11 => vec![(
            "xdotool",
            vec!["key".into(), "--clearmodifiers".into(), "ctrl+v".into()],
        )],
        SessionType::Wayland => vec![
            (
                "ydotool",
                vec![
                    "key".into(),
                    "29:1".into(),
                    "47:1".into(),
                    "47:0".into(),
                    "29:0".into(),
                ],
            ),
            (
                "wtype",
                vec![
                    "-M".into(),
                    "ctrl".into(),
                    "-k".into(),
                    "v".into(),
                    "-m".into(),
                    "ctrl".into(),
                ],
            ),
        ],
    }
}

#[cfg(test)]
mod command_candidate_tests {
    use super::*;

    #[test]
    fn x11_primary_get_prefers_xclip_then_xsel() {
        let candidates = primary_get_candidates(SessionType::X11);
        assert_eq!(
            candidates,
            vec![
                (
                    "xclip",
                    vec![
                        "-selection".to_string(),
                        "primary".to_string(),
                        "-o".to_string()
                    ]
                ),
                (
                    "xsel",
                    vec!["--primary".to_string(), "--output".to_string()]
                ),
            ]
        );
    }

    #[test]
    fn wayland_primary_get_uses_wl_paste() {
        let candidates = primary_get_candidates(SessionType::Wayland);
        assert_eq!(
            candidates,
            vec![(
                "wl-paste",
                vec!["--primary".to_string(), "--no-newline".to_string()]
            )]
        );
    }

    #[test]
    fn x11_clipboard_get_targets_clipboard_selection() {
        let candidates = clipboard_get_candidates(SessionType::X11);
        assert_eq!(
            candidates[0],
            (
                "xclip",
                vec![
                    "-selection".to_string(),
                    "clipboard".to_string(),
                    "-o".to_string()
                ]
            )
        );
    }

    #[test]
    fn wayland_clipboard_get_uses_wl_paste_without_primary_flag() {
        let candidates = clipboard_get_candidates(SessionType::Wayland);
        assert_eq!(
            candidates,
            vec![("wl-paste", vec!["--no-newline".to_string()])]
        );
    }

    #[test]
    fn x11_clipboard_set_targets_clipboard_selection() {
        let candidates = clipboard_set_candidates(SessionType::X11);
        assert_eq!(
            candidates[0],
            (
                "xclip",
                vec!["-selection".to_string(), "clipboard".to_string()]
            )
        );
    }

    #[test]
    fn wayland_clipboard_set_uses_wl_copy_with_no_args() {
        let candidates = clipboard_set_candidates(SessionType::Wayland);
        assert_eq!(candidates, vec![("wl-copy", vec![])]);
    }

    #[test]
    fn x11_key_copy_uses_xdotool_ctrl_c() {
        let candidates = key_copy_candidates(SessionType::X11);
        assert_eq!(
            candidates,
            vec![(
                "xdotool",
                vec![
                    "key".to_string(),
                    "--clearmodifiers".to_string(),
                    "ctrl+c".to_string()
                ]
            )]
        );
    }

    #[test]
    fn x11_key_paste_uses_xdotool_ctrl_v() {
        let candidates = key_paste_candidates(SessionType::X11);
        assert_eq!(
            candidates,
            vec![(
                "xdotool",
                vec![
                    "key".to_string(),
                    "--clearmodifiers".to_string(),
                    "ctrl+v".to_string()
                ]
            )]
        );
    }

    #[test]
    fn wayland_key_copy_prefers_ydotool_then_wtype() {
        let candidates = key_copy_candidates(SessionType::Wayland);
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].0, "ydotool");
        assert_eq!(candidates[1].0, "wtype");
        // Ctrl(29) down, C(46) down, C up, Ctrl up.
        assert_eq!(
            candidates[0].1,
            vec![
                "key".to_string(),
                "29:1".to_string(),
                "46:1".to_string(),
                "46:0".to_string(),
                "29:0".to_string()
            ]
        );
    }

    #[test]
    fn wayland_key_paste_uses_v_keycode_47() {
        let candidates = key_paste_candidates(SessionType::Wayland);
        assert_eq!(candidates[0].0, "ydotool");
        assert_eq!(
            candidates[0].1,
            vec![
                "key".to_string(),
                "29:1".to_string(),
                "47:1".to_string(),
                "47:0".to_string(),
                "29:0".to_string()
            ]
        );
    }
}

/// Real Linux [`PlatformOps`] backend. Generic over [`CommandRunner`] so
/// tests can substitute a fake that never spawns a real process; the
/// public [`LinuxOps::new`] constructor always uses [`RealCommandRunner`].
pub struct LinuxOps<R: CommandRunner = RealCommandRunner> {
    session: SessionType,
    runner: R,
}

impl LinuxOps<RealCommandRunner> {
    pub fn new() -> Self {
        Self {
            session: SessionType::detect(),
            runner: RealCommandRunner,
        }
    }
}

impl Default for LinuxOps<RealCommandRunner> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: CommandRunner> LinuxOps<R> {
    #[cfg(test)]
    fn with_runner(session: SessionType, runner: R) -> Self {
        Self { session, runner }
    }

    /// Try each `(program, args)` candidate in order, returning the first
    /// one that spawns *and* exits successfully. A candidate whose
    /// program can't be spawned (not installed) or that runs but exits
    /// nonzero is skipped in favor of the next candidate.
    fn run_first_success(
        &self,
        candidates: &[(&'static str, Vec<String>)],
        stdin: Option<&str>,
    ) -> Option<RunOutput> {
        for (program, args) in candidates {
            if let Ok(output) = self.runner.run(program, args, stdin) {
                if output.success {
                    return Some(output);
                }
            }
        }
        None
    }
}

impl<R: CommandRunner> PlatformOps for LinuxOps<R> {
    fn ax_read_selection(&self) -> Result<String> {
        self.run_first_success(&primary_get_candidates(self.session), None)
            .map(|output| output.stdout)
            .ok_or_else(|| {
                anyhow!(
                    "no PRIMARY-selection tool available (need xclip/xsel on X11 or \
                     wl-clipboard on Wayland)"
                )
            })
    }

    fn ax_write_selection(&self, _text: &str) -> Result<()> {
        // There is no reliable direct write to the selection/focused
        // element on Linux (no AT-SPI "set selected text" that works
        // broadly across toolkits); always defer to the clipboard
        // save/write/paste/restore fallback in `inject.rs`.
        Err(anyhow!(
            "Linux has no direct selection write; falls back to the clipboard"
        ))
    }

    fn clipboard_get(&self) -> Result<Option<String>> {
        Ok(self
            .run_first_success(&clipboard_get_candidates(self.session), None)
            .map(|output| output.stdout))
    }

    fn clipboard_set(&self, text: &str) -> Result<()> {
        self.run_first_success(&clipboard_set_candidates(self.session), Some(text))
            .map(|_| ())
            .ok_or_else(|| {
                anyhow!(
                    "no clipboard-set tool available (need xclip/xsel on X11 or wl-clipboard \
                     on Wayland)"
                )
            })
    }

    fn simulate_copy(&self) -> Result<()> {
        self.run_first_success(&key_copy_candidates(self.session), None)
            .map(|_| ())
            .ok_or_else(|| {
                anyhow!(
                    "no key-simulation tool available for Copy (need xdotool on X11 or \
                     ydotool/wtype on Wayland)"
                )
            })?;
        // Give the target app a moment to react to the keystroke before
        // the clipboard is read, mirroring macos.rs's post-keystroke
        // settle delay.
        thread::sleep(Duration::from_millis(100));
        Ok(())
    }

    fn simulate_paste(&self) -> Result<()> {
        self.run_first_success(&key_paste_candidates(self.session), None)
            .map(|_| ())
            .ok_or_else(|| {
                anyhow!(
                    "no key-simulation tool available for Paste (need xdotool on X11 or \
                     ydotool/wtype on Wayland)"
                )
            })?;
        thread::sleep(Duration::from_millis(100));
        Ok(())
    }
}

#[cfg(test)]
mod linux_ops_tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// A scripted response for one program name, and every call recorded
    /// against it, so tests can both control behavior and assert exactly
    /// what was invoked (which program, which args, what stdin).
    #[derive(Clone)]
    enum FakeOutcome {
        /// Simulates the program not being installed: `run()` errors.
        NotFound,
        /// Simulates the program running but exiting nonzero.
        Fails,
        /// Simulates the program running successfully with this stdout.
        Succeeds(&'static str),
    }

    /// One recorded call: `(program, args, stdin)`.
    type RecordedCall = (String, Vec<String>, Option<String>);

    #[derive(Default)]
    struct FakeCommandRunner {
        outcomes: RefCell<HashMap<&'static str, FakeOutcome>>,
        calls: RefCell<Vec<RecordedCall>>,
    }

    impl FakeCommandRunner {
        fn new() -> Self {
            Self::default()
        }

        fn set(&self, program: &'static str, outcome: FakeOutcome) {
            self.outcomes.borrow_mut().insert(program, outcome);
        }
    }

    impl CommandRunner for FakeCommandRunner {
        fn run(&self, program: &str, args: &[String], stdin: Option<&str>) -> Result<RunOutput> {
            self.calls.borrow_mut().push((
                program.to_string(),
                args.to_vec(),
                stdin.map(str::to_string),
            ));
            match self.outcomes.borrow().get(program) {
                Some(FakeOutcome::NotFound) | None => Err(anyhow!("{program}: not found")),
                Some(FakeOutcome::Fails) => Ok(RunOutput {
                    success: false,
                    stdout: String::new(),
                }),
                Some(FakeOutcome::Succeeds(stdout)) => Ok(RunOutput {
                    success: true,
                    stdout: stdout.to_string(),
                }),
            }
        }
    }

    #[test]
    fn ax_read_selection_reads_x11_primary_via_xclip() {
        let runner = FakeCommandRunner::new();
        runner.set("xclip", FakeOutcome::Succeeds("mouse-selected text"));
        let ops = LinuxOps::with_runner(SessionType::X11, runner);

        let text = ops.ax_read_selection().unwrap();

        assert_eq!(text, "mouse-selected text");
        assert_eq!(
            ops.runner.calls.borrow()[0],
            (
                "xclip".to_string(),
                vec![
                    "-selection".to_string(),
                    "primary".to_string(),
                    "-o".to_string()
                ],
                None
            )
        );
    }

    #[test]
    fn ax_read_selection_falls_back_to_xsel_when_xclip_missing() {
        let runner = FakeCommandRunner::new();
        runner.set("xclip", FakeOutcome::NotFound);
        runner.set("xsel", FakeOutcome::Succeeds("via xsel"));
        let ops = LinuxOps::with_runner(SessionType::X11, runner);

        let text = ops.ax_read_selection().unwrap();

        assert_eq!(text, "via xsel");
        let calls = ops.runner.calls.borrow();
        assert_eq!(calls.len(), 2, "should have tried xclip then xsel");
        assert_eq!(calls[0].0, "xclip");
        assert_eq!(calls[1].0, "xsel");
    }

    #[test]
    fn ax_read_selection_uses_wl_paste_primary_on_wayland() {
        let runner = FakeCommandRunner::new();
        runner.set("wl-paste", FakeOutcome::Succeeds("wayland primary"));
        let ops = LinuxOps::with_runner(SessionType::Wayland, runner);

        let text = ops.ax_read_selection().unwrap();

        assert_eq!(text, "wayland primary");
        assert_eq!(
            ops.runner.calls.borrow()[0].1,
            vec!["--primary".to_string(), "--no-newline".to_string()]
        );
    }

    #[test]
    fn ax_read_selection_errors_when_no_tool_available() {
        let runner = FakeCommandRunner::new();
        let ops = LinuxOps::with_runner(SessionType::X11, runner);

        assert!(ops.ax_read_selection().is_err());
    }

    #[test]
    fn ax_write_selection_always_errors_regardless_of_session() {
        let ops = LinuxOps::with_runner(SessionType::X11, FakeCommandRunner::new());
        assert!(ops.ax_write_selection("anything").is_err());

        let ops = LinuxOps::with_runner(SessionType::Wayland, FakeCommandRunner::new());
        assert!(ops.ax_write_selection("anything").is_err());
    }

    #[test]
    fn clipboard_get_returns_none_when_no_tool_available() {
        let runner = FakeCommandRunner::new();
        let ops = LinuxOps::with_runner(SessionType::X11, runner);

        assert_eq!(ops.clipboard_get().unwrap(), None);
    }

    #[test]
    fn clipboard_get_reads_clipboard_selection_not_primary() {
        let runner = FakeCommandRunner::new();
        runner.set("xclip", FakeOutcome::Succeeds("clipboard contents"));
        let ops = LinuxOps::with_runner(SessionType::X11, runner);

        assert_eq!(
            ops.clipboard_get().unwrap(),
            Some("clipboard contents".to_string())
        );
        assert_eq!(
            ops.runner.calls.borrow()[0].1,
            vec![
                "-selection".to_string(),
                "clipboard".to_string(),
                "-o".to_string()
            ]
        );
    }

    #[test]
    fn clipboard_set_pipes_text_as_stdin_to_xclip_on_x11() {
        let runner = FakeCommandRunner::new();
        runner.set("xclip", FakeOutcome::Succeeds(""));
        let ops = LinuxOps::with_runner(SessionType::X11, runner);

        ops.clipboard_set("hello clipboard").unwrap();

        let calls = ops.runner.calls.borrow();
        assert_eq!(calls[0].0, "xclip");
        assert_eq!(calls[0].2, Some("hello clipboard".to_string()));
    }

    #[test]
    fn clipboard_set_pipes_text_as_stdin_to_wl_copy_on_wayland() {
        let runner = FakeCommandRunner::new();
        runner.set("wl-copy", FakeOutcome::Succeeds(""));
        let ops = LinuxOps::with_runner(SessionType::Wayland, runner);

        ops.clipboard_set("hello wayland").unwrap();

        let calls = ops.runner.calls.borrow();
        assert_eq!(
            calls[0],
            (
                "wl-copy".to_string(),
                vec![],
                Some("hello wayland".to_string())
            )
        );
    }

    #[test]
    fn clipboard_set_errors_when_no_tool_available() {
        let runner = FakeCommandRunner::new();
        let ops = LinuxOps::with_runner(SessionType::X11, runner);

        assert!(ops.clipboard_set("text").is_err());
    }

    #[test]
    fn simulate_copy_runs_xdotool_ctrl_c_on_x11() {
        let runner = FakeCommandRunner::new();
        runner.set("xdotool", FakeOutcome::Succeeds(""));
        let ops = LinuxOps::with_runner(SessionType::X11, runner);

        ops.simulate_copy().unwrap();

        let calls = ops.runner.calls.borrow();
        assert_eq!(calls[0].0, "xdotool");
        assert!(calls[0].1.contains(&"ctrl+c".to_string()));
    }

    #[test]
    fn simulate_paste_runs_xdotool_ctrl_v_on_x11() {
        let runner = FakeCommandRunner::new();
        runner.set("xdotool", FakeOutcome::Succeeds(""));
        let ops = LinuxOps::with_runner(SessionType::X11, runner);

        ops.simulate_paste().unwrap();

        let calls = ops.runner.calls.borrow();
        assert_eq!(calls[0].0, "xdotool");
        assert!(calls[0].1.contains(&"ctrl+v".to_string()));
    }

    #[test]
    fn simulate_copy_runs_ydotool_key_codes_on_wayland() {
        let runner = FakeCommandRunner::new();
        runner.set("ydotool", FakeOutcome::Succeeds(""));
        let ops = LinuxOps::with_runner(SessionType::Wayland, runner);

        ops.simulate_copy().unwrap();

        let calls = ops.runner.calls.borrow();
        assert_eq!(calls[0].0, "ydotool");
    }

    #[test]
    fn simulate_copy_falls_back_to_wtype_when_ydotool_missing_on_wayland() {
        let runner = FakeCommandRunner::new();
        runner.set("ydotool", FakeOutcome::NotFound);
        runner.set("wtype", FakeOutcome::Succeeds(""));
        let ops = LinuxOps::with_runner(SessionType::Wayland, runner);

        ops.simulate_copy().unwrap();

        let calls = ops.runner.calls.borrow();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].0, "ydotool");
        assert_eq!(calls[1].0, "wtype");
    }

    #[test]
    fn simulate_copy_errors_when_no_key_tool_available() {
        let runner = FakeCommandRunner::new();
        let ops = LinuxOps::with_runner(SessionType::Wayland, runner);

        assert!(ops.simulate_copy().is_err());
    }

    #[test]
    fn simulate_paste_errors_when_tool_runs_but_exits_nonzero() {
        let runner = FakeCommandRunner::new();
        runner.set("xdotool", FakeOutcome::Fails);
        let ops = LinuxOps::with_runner(SessionType::X11, runner);

        assert!(ops.simulate_paste().is_err());
    }
}

#[cfg(test)]
mod session_detection_tests {
    use super::SessionType;

    #[test]
    fn wayland_display_set_means_wayland() {
        let session = SessionType::detect_from(Some("wayland-0".to_string()), None);
        assert_eq!(session, SessionType::Wayland);
    }

    #[test]
    fn xdg_session_type_wayland_means_wayland_even_without_wayland_display() {
        let session = SessionType::detect_from(None, Some("wayland".to_string()));
        assert_eq!(session, SessionType::Wayland);
    }

    #[test]
    fn neither_set_means_x11() {
        let session = SessionType::detect_from(None, None);
        assert_eq!(session, SessionType::X11);
    }

    #[test]
    fn empty_wayland_display_falls_back_to_xdg_session_type() {
        // Some setups export WAYLAND_DISPLAY="" rather than leaving it
        // unset; an empty value shouldn't be treated as "set".
        let session = SessionType::detect_from(Some(String::new()), Some("x11".to_string()));
        assert_eq!(session, SessionType::X11);
    }

    #[test]
    fn xdg_session_type_x11_is_x11() {
        let session = SessionType::detect_from(None, Some("x11".to_string()));
        assert_eq!(session, SessionType::X11);
    }
}
