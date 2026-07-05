pipeline {
    agent none

    options {
        timestamps()
        // redrafter has no whisper-rs/OpenBLAS-class slow native compiles, but
        // the Windows Rust build + packaging is the long pole and this timeout
        // wraps every parallel branch, so it must clear a *cold* Windows agent
        // (empty cargo cache) plus the sequential GitHub Release stage. 60 min
        // was too tight — build #11 aborted at ~58 min mid-Windows-build — so
        // bumped to 90 to leave headroom on a cold cache.
        timeout(time: 90, unit: 'MINUTES')
        buildDiscarder(logRotator(numToKeepStr: '20', artifactNumToKeepStr: '5'))
    }

    environment {
        CARGO_TERM_COLOR = 'always'
        RUST_BACKTRACE = '1'
    }

    stages {
        stage('Build & Test') {
            failFast false
            parallel {
                stage('Linux') {
                    // The Docker Pipeline plugin isn't installed on this
                    // controller, so we drive Docker explicitly via `sh`
                    // rather than an `agent { dockerfile }`.
                    agent { label 'linux' }
                    stages {
                        stage('Linux: CI image') {
                            steps {
                                // Layer-cached after the first build on each node.
                                sh 'docker build -t redrafter-ci-linux:latest -f .ci/Dockerfile.linux .ci'
                            }
                        }
                        stage('Linux: Build, Test & Package') {
                            steps {
                                // Signing creds are only exposed for the duration of the
                                // docker run (never echoed/interpolated — passed through as
                                // `-e VAR` so docker reads the value from this step's env).
                                // `tauri build` (inside build-linux.sh) requires them because
                                // createUpdaterArtifacts:true fails the build without them.
                                withCredentials([
                                    string(credentialsId: 'tauri-signing-private-key', variable: 'TAURI_SIGNING_PRIVATE_KEY'),
                                    string(credentialsId: 'tauri-signing-key-password', variable: 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD')
                                ]) {
                                    // Run as the host uid so workspace files stay jenkins-owned
                                    // (cleanable). The named volume persists the cargo registry
                                    // cache; target/ persists via the mounted workspace.
                                    sh 'docker run --rm -u $(id -u):$(id -g) -e HOME=$WORKSPACE -e CARGO_TERM_COLOR=always -e RUST_BACKTRACE=1 -e TAURI_SIGNING_PRIVATE_KEY -e TAURI_SIGNING_PRIVATE_KEY_PASSWORD -v $WORKSPACE:$WORKSPACE -w $WORKSPACE -v redrafter-cargo:/opt/cargo --shm-size=2g redrafter-ci-linux:latest bash .ci/build-linux.sh'
                                }
                                // Verify bundles (host-side; files are in the mounted workspace).
                                sh '''
                                    echo "=== Linux Artifact Verification ==="
                                    for deb in target/release/bundle/deb/*.deb; do
                                        [ -f "$deb" ] && echo "DEB: $(basename "$deb") $(du -h "$deb" | cut -f1)" && dpkg-deb --info "$deb" | head -5
                                    done
                                    for rpm in target/release/bundle/rpm/*.rpm; do
                                        [ -f "$rpm" ] && echo "RPM: $(basename "$rpm") $(du -h "$rpm" | cut -f1)"
                                    done
                                    for appimage in target/release/bundle/appimage/*.AppImage; do
                                        [ -f "$appimage" ] && echo "AppImage: $(basename "$appimage") $(du -h "$appimage" | cut -f1)"
                                    done
                                    for sig in target/release/bundle/appimage/*.AppImage.sig; do
                                        [ -f "$sig" ] && echo "AppImage sig: $(basename "$sig")"
                                    done
                                '''
                            }
                            post {
                                always {
                                    junit allowEmptyResults: true, testResults: 'target/nextest/ci/junit.xml'
                                    junit allowEmptyResults: true, testResults: 'e2e-real/test-results/**/*.xml'
                                    archiveArtifacts artifacts: 'coverage.json', allowEmptyArchive: true, fingerprint: true
                                }
                                success {
                                    archiveArtifacts artifacts: 'target/release/bundle/**/*.deb, target/release/bundle/**/*.rpm, target/release/bundle/appimage/*.AppImage, target/release/bundle/appimage/*.AppImage.sig', allowEmptyArchive: true, fingerprint: true
                                    // Stash for the GitHub Release stage (cross-agent, same run).
                                    stash name: 'bundles-linux',
                                          includes: 'target/release/bundle/deb/*.deb, target/release/bundle/rpm/*.rpm, target/release/bundle/appimage/*.AppImage, target/release/bundle/appimage/*.AppImage.sig',
                                          allowEmpty: true
                                }
                            }
                        }
                    }
                }

                stage('macOS') {
                    agent { label 'macos' }
                    // The macOS agents share the chronically-flaky remoting link
                    // that drops mid-build (AgentOfflineException / "Connection was
                    // broken"). Retry on agent loss so a transient disconnect
                    // re-runs the stage on a reconnected agent instead of aborting
                    // the whole run — mirrors the Windows stage's resilience.
                    options {
                        retry(count: 2, conditions: [agent(), nonresumable()])
                    }
                    environment {
                        PATH = "/Users/jenkins/Library/Python/3.9/bin:/Users/jenkins/.local/bin:/Users/jenkins/.cargo/bin:/Users/jenkins/.nvm/versions/node/v24.14.0/bin:/Users/jenkins/.nvm/versions/node/v24.14.1/bin:/opt/homebrew/bin:/usr/local/bin:${env.PATH}"
                    }
                    stages {
                        stage('macOS: Setup') {
                            steps {
                                sh '''
                                    echo "=== Rust ===" && rustc --version && cargo --version

                                    echo "=== Node ==="
                                    if command -v node &>/dev/null; then
                                        node --version
                                    elif [ -d "$HOME/.nvm" ]; then
                                        export NVM_DIR="$HOME/.nvm"
                                        . "$NVM_DIR/nvm.sh"
                                        node --version
                                    else
                                        echo "Node.js not found — installing via brew"
                                        brew install node 2>&1 | tail -3
                                    fi

                                    echo "=== pnpm ==="
                                    if ! command -v pnpm &>/dev/null; then
                                        corepack enable pnpm 2>/dev/null || npm install -g pnpm 2>/dev/null || true
                                    fi
                                    pnpm --version

                                    echo "=== cargo-nextest ==="
                                    cargo nextest --version 2>/dev/null || cargo install cargo-nextest --locked 2>&1 | tail -3
                                '''
                            }
                        }
                        stage('macOS: Frontend') {
                            steps {
                                dir('app') {
                                    sh '''
                                        # Source nvm if needed
                                        if ! command -v node &>/dev/null && [ -d "$HOME/.nvm" ]; then
                                            export NVM_DIR="$HOME/.nvm"
                                            . "$NVM_DIR/nvm.sh"
                                        fi
                                        pnpm install --frozen-lockfile && pnpm build
                                    '''
                                }
                            }
                        }
                        stage('macOS: Rust Tests') {
                            steps {
                                // generate_context! needs app/out (built above); the
                                // workspace lives at the repo root, so nextest runs
                                // there rather than inside app/src-tauri.
                                sh 'cargo nextest run --workspace --profile ci'
                            }
                            post {
                                always {
                                    junit allowEmptyResults: true,
                                         testResults: 'target/nextest/ci/junit.xml'
                                }
                            }
                        }
                        stage('macOS: Clippy & Cargo Check') {
                            steps {
                                sh 'cargo clippy -p redrafter -p llm-provider -p text-inject -- -D warnings'
                                sh 'cargo check --workspace'
                            }
                        }
                        stage('macOS: Package') {
                            steps {
                                withCredentials([
                                    string(credentialsId: 'tauri-signing-private-key', variable: 'TAURI_SIGNING_PRIVATE_KEY'),
                                    string(credentialsId: 'tauri-signing-key-password', variable: 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD')
                                ]) {
                                    dir('app') {
                                        sh '''
                                            echo "=== Building release bundles (dmg, app) ==="
                                            pnpm tauri build 2>&1 | tail -20
                                        '''
                                    }
                                }
                                // ── Code Signing + Notarization ───────────────────────────
                                // Hardened-runtime Developer ID signing when
                                // APPLE_SIGNING_IDENTITY is configured on the agent, ad-hoc
                                // fallback otherwise; notarization only when
                                // the App Store Connect credentials are present. No creds are
                                // configured today, so this currently ad-hoc signs and skips
                                // notarization — set the env vars to flip it on.
                                //
                                // NOTE: updater-artifact signing (the .app.tar.gz.sig) is
                                // produced above by `tauri build` via the
                                // tauri-signing-private-key / tauri-signing-key-password
                                // Jenkins credentials — this codesign/notarization block is
                                // orthogonal (Gatekeeper signing of the .app/.dmg, not the
                                // updater's minisign signature).
                                sh '''
                                    set -e
                                    APP_PATH=$(find target/release/bundle/macos -name "*.app" -type d | head -1)
                                    if [ -n "$APP_PATH" ]; then
                                        if [ -n "$APPLE_SIGNING_IDENTITY" ]; then
                                            echo "=== Signing with Developer ID: $APPLE_SIGNING_IDENTITY ==="
                                            codesign --force --deep --options runtime \
                                                --sign "$APPLE_SIGNING_IDENTITY" \
                                                --entitlements app/src-tauri/entitlements.plist \
                                                "$APP_PATH"
                                        else
                                            echo "=== Ad-hoc signing (no APPLE_SIGNING_IDENTITY set) ==="
                                            codesign --force --deep --sign - "$APP_PATH"
                                        fi
                                        echo "=== Verifying signature ==="
                                        codesign --verify --deep --strict "$APP_PATH" && echo "Signature: OK" || echo "Signature: INVALID"
                                        codesign -dvv "$APP_PATH" 2>&1 | grep -E "Authority|TeamIdentifier|Signature" || true
                                    fi

                                    DMG_PATH=$(find target/release/bundle/dmg -name "*.dmg" | head -1)
                                    if [ -n "$APPLE_ID" ] && [ -n "$APPLE_PASSWORD" ] && [ -n "$APPLE_TEAM_ID" ] && [ -n "$DMG_PATH" ]; then
                                        echo "=== Re-creating DMG with signed .app ==="
                                        DMG_NAME=$(basename "$DMG_PATH")
                                        DMG_DIR=$(dirname "$DMG_PATH")
                                        MOUNT_DIR=$(mktemp -d)
                                        hdiutil attach "$DMG_PATH" -mountpoint "$MOUNT_DIR" -quiet
                                        NEW_DMG="${DMG_DIR}/${DMG_NAME%.dmg}-signed.dmg"
                                        hdiutil create -volname "redrafter" -srcfolder "$MOUNT_DIR" -ov -format UDZO "$NEW_DMG"
                                        hdiutil detach "$MOUNT_DIR" -quiet
                                        mv "$NEW_DMG" "$DMG_PATH"

                                        echo "=== Submitting for notarization ==="
                                        xcrun notarytool submit "$DMG_PATH" \
                                            --apple-id "$APPLE_ID" \
                                            --password "$APPLE_PASSWORD" \
                                            --team-id "$APPLE_TEAM_ID" \
                                            --wait --timeout 10m

                                        echo "=== Stapling notarization ticket ==="
                                        xcrun stapler staple "$DMG_PATH"

                                        echo "=== Verifying notarization ==="
                                        spctl --assess --type open --context context:primary-signature "$DMG_PATH" 2>&1 && echo "Notarization: OK" || echo "Notarization: check failed"
                                    else
                                        echo "=== Skipping notarization (set APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID to enable) ==="
                                    fi

                                    echo "=== macOS Artifact Verification ==="
                                    for dmg in target/release/bundle/dmg/*.dmg; do
                                        [ -f "$dmg" ] && echo "DMG: $(basename "$dmg") $(du -h "$dmg" | cut -f1)" && hdiutil verify "$dmg" 2>&1 | tail -1
                                    done

                                    # Regression guard: the .app INSIDE the dmg must carry a
                                    # valid signature. Tauri ad-hoc signs during bundling
                                    # (bundle.macOS.signingIdentity = "-") so the dmg copy is
                                    # signed; without it macOS refuses to open the app
                                    # ("damaged") on Apple Silicon. Fail loudly if it regresses.
                                    if [ -n "$DMG_PATH" ]; then
                                        MNT=$(mktemp -d)
                                        hdiutil attach "$DMG_PATH" -mountpoint "$MNT" -nobrowse -quiet
                                        APP_IN_DMG=$(find "$MNT" -maxdepth 1 -name "*.app" -type d | head -1)
                                        if codesign --verify --deep --strict "$APP_IN_DMG"; then
                                            echo "DMG-embedded app signature: OK"
                                            codesign -dvv "$APP_IN_DMG" 2>&1 | grep -E "Identifier|Signature|TeamIdentifier" || true
                                            hdiutil detach "$MNT" -quiet
                                        else
                                            echo "ERROR: app inside dmg is NOT validly signed — macOS would refuse to open it."
                                            hdiutil detach "$MNT" -quiet
                                            exit 1
                                        fi
                                    fi
                                '''
                            }
                            post {
                                success {
                                    archiveArtifacts artifacts: 'target/release/bundle/**/*.dmg, target/release/bundle/macos/*.app.tar.gz, target/release/bundle/macos/*.app.tar.gz.sig', allowEmptyArchive: true, fingerprint: true
                                    stash name: 'bundles-macos',
                                          includes: 'target/release/bundle/dmg/*.dmg, target/release/bundle/macos/*.app.tar.gz, target/release/bundle/macos/*.app.tar.gz.sig',
                                          allowEmpty: true
                                }
                            }
                        }
                    }
                }

                // ── Windows Build (NSIS) ─────────────────────────────────────
                // redrafter has no whisper-rs/cmake/perl build-time dependency
                // (rusqlite's "bundled" feature and reqwest's default-tls on
                // Windows both use plain `cc`/SChannel, no cmake or vendored
                // C-library toolchain needed) — so this stage needs no extra
                // native provisioning, just a plain Rust + Node build.
                // catchError keeps a Windows failure
                // from sinking the Linux/macOS artifacts or the release.
                stage('Windows') {
                    agent { label 'windows' }
                    options {
                        retry(count: 2, conditions: [agent(), nonresumable()])
                    }
                    steps {
                        catchError(buildResult: 'UNSTABLE', stageResult: 'FAILURE') {
                            bat '''
                                if exist target\\debug\\incremental rmdir /S /Q target\\debug\\incremental
                                if exist target\\release\\incremental rmdir /S /Q target\\release\\incremental
                                echo === Disk space ===
                                fsutil volume diskfree C:
                            '''
                            withCredentials([
                                string(credentialsId: 'tauri-signing-private-key', variable: 'TAURI_SIGNING_PRIVATE_KEY'),
                                string(credentialsId: 'tauri-signing-key-password', variable: 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD')
                            ]) {
                                bat '''
                                    REM Halve the target footprint — debug info/incremental aren't needed for a release bundle.
                                    set CARGO_PROFILE_RELEASE_INCREMENTAL=false

                                    cd app
                                    call corepack enable pnpm
                                    call pnpm install --frozen-lockfile
                                    call pnpm tauri build --bundles nsis

                                    echo === Windows Artifact Verification ===
                                    cd ..
                                    for %%f in (target\\release\\bundle\\nsis\\*.exe) do echo NSIS: %%~nxf
                                    for %%f in (target\\release\\bundle\\nsis\\*.exe.sig) do echo NSIS sig: %%~nxf
                                '''
                            }
                        }
                    }
                    post {
                        success {
                            archiveArtifacts artifacts: 'target/release/bundle/nsis/*.exe, target/release/bundle/nsis/*.exe.sig', allowEmptyArchive: true, fingerprint: true
                            stash name: 'bundles-windows',
                                  includes: 'target/release/bundle/nsis/*.exe, target/release/bundle/nsis/*.exe.sig',
                                  allowEmpty: true
                        }
                        cleanup {
                            // The Windows VMs have small C: drives and Rust target
                            // dirs accumulate tens of GB per build — wipe aggressively.
                            bat '''
                                if exist target rmdir /S /Q target
                                if exist app\\node_modules rmdir /S /Q app\\node_modules
                                if exist app\\out rmdir /S /Q app\\out
                                echo === Disk after cleanup ===
                                fsutil volume diskfree C:
                            '''
                        }
                    }
                }

                stage('E2E Tests (macOS)') {
                    agent { label 'macos' }
                    // The macOS agent is the flakiest link in the fleet — its
                    // remoting channel drops mid-build repeatedly. Retry on agent
                    // loss so a disconnect re-runs on a reconnected agent rather
                    // than aborting the whole run. E2E here is a real-surface
                    // quality signal (D4); the release artifacts come from the
                    // Linux/macOS/Windows package stages, so a persistently-down
                    // agent should not sink them.
                    options {
                        retry(count: 2, conditions: [agent(), nonresumable()])
                    }
                    environment {
                        NVM_DIR = '/Users/jenkins/.nvm'
                        PATH = "/Users/jenkins/.cargo/bin:/Users/jenkins/.nvm/versions/node/v24.14.0/bin:/Users/jenkins/.nvm/versions/node/v24.14.1/bin:/opt/homebrew/bin:/usr/local/bin:${env.PATH}"
                    }
                    stages {
                        stage('E2E: Setup') {
                            steps {
                                sh '''
                                    echo "=== Node ==="
                                    node --version || echo "Node not on PATH, trying nvm..."

                                    echo "=== pnpm ==="
                                    if ! command -v pnpm &>/dev/null; then
                                        corepack enable pnpm 2>/dev/null || npm install -g pnpm 2>/dev/null || true
                                    fi
                                    pnpm --version
                                '''
                            }
                        }
                        stage('E2E: Install') {
                            steps {
                                dir('app') {
                                    sh '''
                                        pnpm install --frozen-lockfile
                                        npx playwright install chromium webkit
                                    '''
                                }
                            }
                        }
                        stage('E2E: Vitest') {
                            steps {
                                dir('app') {
                                    sh 'pnpm exec vitest run'
                                }
                            }
                        }
                        stage('E2E: Run Tests') {
                            steps {
                                dir('app') {
                                    // WebKit is frozen on this macOS version — test Chromium only
                                    sh 'pnpm exec playwright test --project=chromium'
                                }
                            }
                            post {
                                always {
                                    junit allowEmptyResults: true,
                                         testResults: 'app/test-results/e2e-junit.xml'
                                    publishHTML(target: [
                                        allowMissing: true,
                                        alwaysLinkToLastBuild: true,
                                        keepAll: true,
                                        reportDir: 'app/playwright-report',
                                        reportFiles: 'index.html',
                                        reportName: 'Playwright E2E Report'
                                    ])
                                    archiveArtifacts artifacts: 'app/test-results/**', allowEmptyArchive: true
                                }
                            }
                        }
                    }
                }
            }
        }

        // ── GitHub Release ───────────────────────────────────────────────────
        // When the commit built on `main` is exactly a `v*` tag, collect every
        // platform's bundles (stashed above, same run) and attach them to the
        // matching GitHub Release — creating it if absent, otherwise uploading
        // with --clobber. Decoupled from Jenkins tag-discovery (the
        // basic-branch-build-strategies plugin isn't installed) so it works on
        // the existing main-branch job. Non-fatal if the token is missing.
        stage('GitHub Release') {
            when { branch 'main' }
            agent { label 'linux' }
            steps {
                script {
                  // Probe for the release credential first: if this controller
                  // has no `github-release-token` (publishing not set up here),
                  // skip the stage cleanly rather than failing the build UNSTABLE.
                  // A normal branch build produces artifacts, not a GitHub Release;
                  // only a configured-but-failing release should show as UNSTABLE.
                  boolean hasToken = true
                  try {
                    withCredentials([string(credentialsId: 'github-release-token', variable: 'GH_TOKEN')]) { }
                  } catch (ignored) { hasToken = false }
                  if (!hasToken) {
                    echo 'github-release-token not configured — skipping GitHub Release (publishing not set up on this controller).'
                    return
                  }
                  catchError(buildResult: 'UNSTABLE', stageResult: 'UNSTABLE') {
                    withCredentials([string(credentialsId: 'github-release-token', variable: 'GH_TOKEN')]) {
                        def repo = 'Tapnetix/redrafter'
                        // The version in tauri.conf.json is the release version; the
                        // matching tag is v<version>. We publish only when that tag
                        // resolves (via the GitHub API — no SSH/local tags needed) to
                        // the exact commit being built. This is robust against the
                        // Jenkins checkout fetching --no-tags and ad-hoc `git fetch`
                        // having no SSH credential.
                        def ver = sh(script: "python3 -c \"import json;print(json.load(open('app/src-tauri/tauri.conf.json'))['version'])\"", returnStdout: true).trim()
                        // Validate before it ever reaches a shell — the version is
                        // repo-controlled, so reject anything outside a strict semver-ish
                        // charset to prevent command injection via interpolation.
                        if (!(ver ==~ /^[0-9A-Za-z.+_-]{1,64}$/)) {
                            error("Refusing to release: invalid version in tauri.conf.json: '${ver}'")
                        }
                        def tag = "v${ver}"
                        def head = sh(script: 'git rev-parse HEAD', returnStdout: true).trim()
                        // Pass values through the environment, not Groovy interpolation.
                        def tagSha = ''
                        withEnv(["REPO=${repo}", "TAG=${tag}"]) {
                            tagSha = sh(script: 'gh api "repos/$REPO/commits/$TAG" --jq .sha 2>/dev/null || true', returnStdout: true).trim()
                        }
                        if (tagSha != head) {
                            echo "HEAD (${head}) is not tagged ${tag} (tag -> ${tagSha ?: 'absent'}) — skipping GitHub Release."
                            return
                        }
                        echo "Publishing GitHub Release ${tag} (tag matches HEAD ${head})."
                        ['bundles-linux', 'bundles-macos', 'bundles-windows'].each { n ->
                            try { unstash n } catch (ignored) { echo "no stash: ${n} (platform build may have failed)" }
                        }
                            withEnv(["RELEASE_TAG=${tag}", "RELEASE_VERSION=${ver}"]) {
                                sh '''
                                    set -eux
                                    REPO="Tapnetix/redrafter"
                                    mkdir -p release-artifacts
                                    find target/release/bundle -type f \\
                                        \\( -name '*.deb' -o -name '*.AppImage' -o -name '*.rpm' \\
                                           -o -name '*.dmg' -o -name '*.exe' -o -name '*.app.tar.gz' \\
                                           -o -name '*.sig' \\) \\
                                        -exec cp -v {} release-artifacts/ \\; || true
                                    echo "=== release artifacts (bundles) ==="; ls -lh release-artifacts/ || true

                                    if [ -z "$(ls -A release-artifacts 2>/dev/null)" ]; then
                                        echo "No artifacts to upload — failing the release stage."
                                        exit 1
                                    fi

                                    # ── Tauri updater manifest (latest.json) ───────────────────
                                    # The desktop app's updater endpoint is
                                    # .../releases/latest/download/latest.json, so latest.json
                                    # MUST be published as a release asset alongside the signed
                                    # bundles. Only emit a platform entry when that platform's
                                    # .sig actually landed in release-artifacts/ — a platform
                                    # build may have failed or been skipped (e.g. Windows runs
                                    # under catchError -> UNSTABLE), and a broken/missing entry
                                    # would break the updater for every platform, not just the
                                    # missing one.
                                    export PUB_DATE="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
                                    python3 - "$RELEASE_TAG" "$RELEASE_VERSION" <<'PY'
import glob
import json
import os
import sys

tag, version = sys.argv[1], sys.argv[2]
repo = "Tapnetix/redrafter"
base_url = f"https://github.com/{repo}/releases/download/{tag}"


def entry(bundle_suffix):
    sig_paths = glob.glob(os.path.join("release-artifacts", "*" + bundle_suffix + ".sig"))
    if not sig_paths:
        return None
    sig_path = sig_paths[0]
    bundle_name = os.path.basename(sig_path)[: -len(".sig")]
    bundle_path = os.path.join("release-artifacts", bundle_name)
    if not os.path.isfile(bundle_path):
        return None
    with open(sig_path) as f:
        signature = f.read().strip()
    return {"signature": signature, "url": f"{base_url}/{bundle_name}"}


platforms = {}
for key, bundle_suffix in (
    ("darwin-aarch64", ".app.tar.gz"),
    ("linux-x86_64", ".AppImage"),
    ("windows-x86_64", "-setup.exe"),
):
    e = entry(bundle_suffix)
    if e:
        platforms[key] = e

manifest = {
    "version": version,
    "notes": "See the release notes.",
    "pub_date": os.environ["PUB_DATE"],
    "platforms": platforms,
}

with open("release-artifacts/latest.json", "w") as f:
    json.dump(manifest, f, indent=2)
    f.write("\\n")

print("latest.json platforms:", sorted(platforms.keys()))
PY

                                    echo "=== release artifacts (final, incl. latest.json) ==="; ls -lh release-artifacts/ || true

                                    if gh release view "$RELEASE_TAG" --repo "$REPO" >/dev/null 2>&1; then
                                        echo "Release $RELEASE_TAG exists — uploading assets (clobber)."
                                        gh release upload "$RELEASE_TAG" release-artifacts/* --repo "$REPO" --clobber
                                    else
                                        echo "Creating release $RELEASE_TAG."
                                        gh release create "$RELEASE_TAG" release-artifacts/* \
                                            --repo "$REPO" \
                                            --title "redrafter ${RELEASE_TAG}" \
                                            --generate-notes
                                    fi
                                    echo "GitHub Release ${RELEASE_TAG} published with $(ls release-artifacts | wc -l) asset(s)."
                                '''
                            }
                        }
                    }
                }
            }
        }
    }

    post {
        failure {
            echo 'Build failed! Check test results above.'
        }
        unstable {
            echo 'Build UNSTABLE — a platform build (likely Windows) or the release upload failed. Check stage logs.'
        }
        success {
            echo 'All builds and tests passed!'
        }
    }
}
