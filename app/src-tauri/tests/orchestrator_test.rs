//! Compiles the Phase A refine-pipeline modules (`prompt_builder`,
//! `orchestrator`) ahead of A14 wiring them into `lib.rs`'s module tree
//! (A14 owns the composition root, which this task does not touch), and
//! exercises the orchestrator's `refine`/`restore_original` pipeline with
//! fakes for the LLM provider and the text-inject capture/inject seam.
//!
//! Uses `include!` with an absolute path (via `CARGO_MANIFEST_DIR`) rather
//! than a relative `#[path = "../src/orchestrator.rs"]` — see
//! `tests/core_test.rs` for why: a relative `#[path]` from a file under
//! `tests/` embeds a literal, unnormalized `tests/../src/...` path in debug
//! info, which `cargo llvm-cov`'s default ignore rule for `tests/` silently
//! filters out — zeroing coverage for this task's real production code. The
//! absolute path here resolves to plain `.../src/*.rs`, so coverage
//! attributes correctly.

mod prompt_builder {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/prompt_builder.rs"));
}
