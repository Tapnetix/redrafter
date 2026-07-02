// Binary entry point. All real setup (command registry, managed state,
// plugins, tray, hotkey) lives in the library crate's `run()` so it can be
// exercised from `tests/wireup_test.rs` without going through a `main`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    redrafter_lib::run();
}
