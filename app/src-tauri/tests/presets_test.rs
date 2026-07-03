//! Compiles `presets.rs` (the built-in + user preset store) ahead of C17
//! wiring it into `lib.rs`'s module tree, so
//! `cargo nextest run -p redrafter presets` can find and run its tests
//! before then.
//!
//! Uses `include!` with an absolute path (via `CARGO_MANIFEST_DIR`) rather
//! than a relative `#[path = "../src/presets.rs"]` -- see `tests/core_test.rs`
//! for why: a relative `#[path]` from a file under `tests/` embeds a
//! literal, unnormalized `tests/../src/...` path in debug info, which
//! `cargo llvm-cov`'s default ignore rule for `tests/` silently filters out
//! -- zeroing coverage for this task's real production code. The absolute
//! path here resolves to plain `.../src/*.rs`, so coverage attributes
//! correctly.

mod presets {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/presets.rs"));
}
