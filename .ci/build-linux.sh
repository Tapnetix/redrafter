#!/usr/bin/env bash
# redrafter Linux build/test/package — runs INSIDE the Ubuntu 24.04 CI
# container (see .ci/Dockerfile.linux). Invoked by the Jenkins "Linux" stage as:
#   docker run ... redrafter-ci-linux:<tag> bash .ci/build-linux.sh
#
# Each step is a subshell so a failure in a `cd && cmd` chain still aborts the
# script under `set -e` (a bare `cd a && cmd` in an && chain does NOT trip -e).
set -euo pipefail

# pnpm refuses to auto-remove a stale node_modules without a TTY unless CI is
# set (ERR_PNPM_ABORTED_REMOVE_MODULES_DIR_NO_TTY). The Jenkins workspace can
# carry a node_modules from a prior host build, so mark this a CI run.
export CI=true

echo "=== Toolchain ==="
rustc --version && cargo --version && node --version && pnpm --version

# Jenkins checks out as a different uid; mark the mounted tree safe for git.
git config --global --add safe.directory '*'

echo "=== Frontend (Next.js static export -> app/out) ==="
# generate_context! (app/src-tauri/build.rs) needs app/out to exist BEFORE
# any cargo build/test of the workspace, so this runs first.
( cd app && pnpm install --frozen-lockfile && pnpm build )

echo "=== Rust workspace tests (nextest, JUnit for CI) ==="
( cargo nextest run --workspace --profile ci )

echo "=== Clippy (redrafter, llm-provider, text-inject) ==="
( cargo clippy -p redrafter -p llm-provider -p text-inject -- -D warnings )

echo "=== Coverage (llvm-cov, workspace) ==="
# `cargo llvm-cov nextest` (not the plain `cargo llvm-cov`, which drives
# `cargo test`) — the GTK tray init (redrafter's `wireup_test`) SIGSEGVs when
# multiple tests share a process, same reason the test suite above runs
# under nextest's per-test process isolation rather than the default harness.
( cargo llvm-cov nextest --workspace --json > coverage.json )

echo "=== cargo check (workspace) ==="
( cargo check --workspace )

echo "=== Real-app E2E (debug binary + WebdriverIO under xvfb) — non-blocking ==="
# e2e-real is a quality signal but must NOT block release artifacts: it drives
# a real windowed binary under xvfb, which is inherently flakier than the unit
# and IPC-mocked suites. Failures are logged and the JUnit report is still
# collected by the Jenkins post block; the build proceeds to bundling.
( cargo build -p redrafter --bin redrafter ) \
    || echo "WARN: redrafter debug binary build failed (non-blocking)"
( cd e2e-real && pnpm install --frozen-lockfile && xvfb-run -a pnpm run wdio:x11 ) \
    || echo "WARN: real-app E2E failed (non-blocking for release)"

echo "=== Release bundles (deb, rpm, AppImage) ==="
( cd app && pnpm tauri build 2>&1 | tail -20 )

echo "=== Linux build script complete ==="
