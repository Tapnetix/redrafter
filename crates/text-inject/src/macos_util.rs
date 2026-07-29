//! Pure helpers for the macOS keyboard-synthesis path.
//!
//! Split out of `macos.rs` (which needs `AXUIElement`/CoreGraphics and only
//! builds on macOS) so the flag/keycode logic can be unit tested on any host
//! — the same arrangement as `windows_util.rs`.
//!
//! ## Why this exists
//!
//! `macos.rs` used to synthesize Cmd+C / Cmd+V with
//! `osascript -e 'tell application "System Events" to keystroke "c" using
//! command down'`. System Events' `keystroke` **merges whatever modifier keys
//! are physically held at that moment** into the event it posts. redrafter
//! synthesizes those keystrokes microseconds after the user presses the global
//! hotkey — and the user is typically still holding it, plus whatever they used
//! to make the selection in the first place. Shift is the common one: selecting
//! with Shift+arrows or Shift+click leaves Shift down, so the Cmd+C we asked
//! for arrives at the target app as **Cmd+Shift+C**.
//!
//! In Slack that is the *inline code* shortcut, so instead of copying the
//! selection redrafter silently wrapped it in `code` formatting, which the user
//! then had to turn off by hand. Other apps have their own Cmd+Shift+C/V
//! bindings (Cmd+Shift+V is "paste and match style" in much of the system), so
//! this was never Slack-specific.
//!
//! Linux already avoided this with `xdotool key --clearmodifiers`, and Windows
//! by driving `SendInput` with explicit key-down/up pairs. macOS was the only
//! backend feeding live modifier state into its synthetic events; it now posts
//! `CGEvent`s with the flags set explicitly (see `macos.rs`).

/// Layout-independent virtual key codes (Carbon `kVK_ANSI_*`), used with
/// `CGEventCreateKeyboardEvent`. Layout-independent matters: the AppleScript
/// path asked for the *character* "c", which on a non-US layout is not
/// necessarily the key that Cmd+C is bound to.
pub const KEY_CODE_C: u16 = 0x08;
/// See [`KEY_CODE_C`].
pub const KEY_CODE_V: u16 = 0x09;

// `CGEventFlags` bits (CGEventTypes.h). Mirrored here as plain `u64` so this
// module — and its tests — build on hosts without CoreGraphics.
/// `kCGEventFlagMaskShift`.
pub const FLAG_SHIFT: u64 = 0x0002_0000;
/// `kCGEventFlagMaskControl`.
pub const FLAG_CONTROL: u64 = 0x0004_0000;
/// `kCGEventFlagMaskAlternate` (Option).
pub const FLAG_ALTERNATE: u64 = 0x0008_0000;
/// `kCGEventFlagMaskCommand`.
pub const FLAG_COMMAND: u64 = 0x0010_0000;
/// `kCGEventFlagMaskAlphaShift` (Caps Lock).
pub const FLAG_ALPHA_SHIFT: u64 = 0x0001_0000;
/// `kCGEventFlagMaskSecondaryFn` (the Fn key).
pub const FLAG_SECONDARY_FN: u64 = 0x0080_0000;
/// `kCGEventFlagMaskNumericPad`.
pub const FLAG_NUMERIC_PAD: u64 = 0x0020_0000;

/// The modifier bits that change *which shortcut* a key press resolves to,
/// and therefore the ones that must not be in effect when we synthesize a
/// Cmd+C/Cmd+V. Caps Lock, Fn, and the numeric-pad marker are deliberately
/// excluded: they never turn Cmd+C into a different command, so waiting on
/// them would stall a refine for a user who simply has Caps Lock on.
pub const SHORTCUT_MODIFIER_MASK: u64 = FLAG_SHIFT | FLAG_CONTROL | FLAG_ALTERNATE | FLAG_COMMAND;

/// The modifier-ish bits deliberately *not* waited on, named so the exclusion
/// is an explicit decision rather than an accident of what
/// [`SHORTCUT_MODIFIER_MASK`] happens to list.
pub const IGNORED_MODIFIER_MASK: u64 = FLAG_ALPHA_SHIFT | FLAG_SECONDARY_FN | FLAG_NUMERIC_PAD;

/// The subset of `flags` that would alter the meaning of a synthesized
/// shortcut. Zero when nothing relevant is held.
pub fn interfering_modifiers(flags: u64) -> u64 {
    // The two masks must stay disjoint: a bit in both would mean we both wait
    // on it and claim to ignore it.
    debug_assert_eq!(SHORTCUT_MODIFIER_MASK & IGNORED_MODIFIER_MASK, 0);
    flags & SHORTCUT_MODIFIER_MASK
}

/// Whether it is safe to synthesize a Cmd+key shortcut right now: no
/// shortcut-altering modifier is physically held.
pub fn modifiers_are_clear(flags: u64) -> bool {
    interfering_modifiers(flags) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_held_is_clear() {
        assert!(modifiers_are_clear(0));
        assert_eq!(interfering_modifiers(0), 0);
    }

    #[test]
    fn a_held_shift_is_not_clear() {
        // The exact case that turned Cmd+C into Slack's Cmd+Shift+C ("code"):
        // the user selected text with Shift and was still holding it.
        assert!(!modifiers_are_clear(FLAG_SHIFT));
        assert_eq!(interfering_modifiers(FLAG_SHIFT), FLAG_SHIFT);
    }

    #[test]
    fn the_default_hotkeys_control_and_option_are_not_clear() {
        // redrafter's default hotkey is Ctrl+Alt+R, so both of these are
        // routinely still down when the refine pipeline starts.
        assert!(!modifiers_are_clear(FLAG_CONTROL));
        assert!(!modifiers_are_clear(FLAG_ALTERNATE));
        assert!(!modifiers_are_clear(FLAG_CONTROL | FLAG_ALTERNATE));
    }

    #[test]
    fn a_held_command_is_not_clear() {
        assert!(!modifiers_are_clear(FLAG_COMMAND));
    }

    #[test]
    fn caps_lock_fn_and_numeric_pad_do_not_block_a_synthetic_shortcut() {
        // None of these change which command Cmd+C resolves to, so a user
        // with Caps Lock on must not have their refine stalled.
        assert!(modifiers_are_clear(FLAG_ALPHA_SHIFT));
        assert!(modifiers_are_clear(FLAG_SECONDARY_FN));
        assert!(modifiers_are_clear(FLAG_NUMERIC_PAD));
        assert!(modifiers_are_clear(
            FLAG_ALPHA_SHIFT | FLAG_SECONDARY_FN | FLAG_NUMERIC_PAD
        ));
    }

    #[test]
    fn reports_only_the_interfering_subset_of_mixed_flags() {
        let flags = FLAG_ALPHA_SHIFT | FLAG_SHIFT | FLAG_NUMERIC_PAD | FLAG_COMMAND;
        assert_eq!(interfering_modifiers(flags), FLAG_SHIFT | FLAG_COMMAND);
        assert!(!modifiers_are_clear(flags));
    }

    #[test]
    fn the_waited_on_and_ignored_masks_are_disjoint() {
        assert_eq!(SHORTCUT_MODIFIER_MASK & IGNORED_MODIFIER_MASK, 0);
    }

    #[test]
    fn c_and_v_use_distinct_layout_independent_key_codes() {
        assert_eq!(KEY_CODE_C, 0x08);
        assert_eq!(KEY_CODE_V, 0x09);
        assert_ne!(KEY_CODE_C, KEY_CODE_V);
    }
}
