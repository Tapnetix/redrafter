//! Compiles the Phase A backend-core modules (settings, permission, hotkey)
//! ahead of A14 wiring them into `lib.rs`'s module tree (A14 owns the
//! composition root, which this task does not touch). Each module carries
//! its own `#[cfg(test)] mod tests`; this file exists purely so
//! `cargo nextest run -p redrafter settings permission hotkey` can find and
//! run them before A14 lands.
//!
//! Uses `include!` with an absolute path (via `CARGO_MANIFEST_DIR`) rather
//! than a relative `#[path = "../src/settings.rs"]`. A relative `#[path]`
//! from a file under `tests/` embeds a literal, unnormalized
//! `tests/../src/...` path in debug info, which `cargo llvm-cov`'s default
//! ignore rule for `tests/` silently filters out — zeroing coverage for
//! this task's real production code. The absolute path here resolves to
//! plain `.../src/*.rs`, so coverage attributes correctly.

mod settings {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/settings.rs"));
}
mod permission {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/permission.rs"));
}
mod hotkey {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/hotkey.rs"));
}
mod secrets {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/secrets.rs"));
}
mod connections {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/connections.rs"));
}
