# Implementation Plan: egui Cross-Target Build and Test Workflow

## Status

In progress. Stage 1 is complete; implementation and validation remain.

This plan restructures CI and artifact production for the standalone `shoopdaloop_egui` application. It does not change the retained Qt/QML product or turn the fixture-only `shoop_egui_preview` into a product build.

## Goals and scope

Create a GitHub Actions workflow equivalent in trigger purpose to `build_and_test.yml`, but dedicated to the egui application. The workflow will use one matrix job whose cells build, package, upload, and then test one target/profile combination in sequence.

In scope:

- Linux x86_64, Windows x86_64, macOS Apple silicon, and browser Wasm targets.
- Cargo debug and release profiles for every target, yielding eight matrix cells.
- Native application archives with explicit target, architecture, and profile names.
- A hosted web bundle archive and a separately downloadable self-contained HTML file for both web profiles.
- The authoritative `shoopdaloop_egui` web application with its Web Audio/AudioWorklet driver, microphone permission flow, and connections dialog.
- Profile-correct worklet and UI builds, production-artifact packaging, Rust build caching, target-appropriate tests, and current documentation.
- Fast initial workflow iteration with the `nektos/act` local GitHub Actions runner before consuming hosted-runner cycles.
- Retirement of the superseded Wasm-only product workflow after its production checks are represented in the new matrix.

Out of scope:

- Changing the Qt/QML workflow, production entry point, or Qt packaging.
- Adding native physical-audio drivers to the egui application; native artifacts retain the current threaded dummy backend.
- Removing the backend-free `shoop_egui_preview` crate or its focused tests. It remains a fixture/development surface, but it is not built or uploaded as a product artifact by this workflow.
- Installers, code signing, notarization, AppImage, DMG, or release publication. The new workflow uploads CI artifacts only.
- Coverage instrumentation, coverage-specific build flavors, and coverage report upload. This workflow initially has debug and release flavors only.
- Claiming microphone support when the self-contained HTML is opened directly from `file:`; hosted HTTPS/localhost remains required for Web Audio microphone operation.

## Immutable acceptance criteria

These criteria may not change without explicit user approval.

1. A new `.github/workflows/build_and_test_egui.yml` provides push, pull-request, tag, scheduled, and manual coverage appropriate to the egui branch and the main development branches.
2. The workflow has one build/test matrix job definition and no separate downstream build or test jobs. Its eight cells are Linux, Windows, macOS, and web crossed with Cargo debug and release profiles; `fail-fast` is disabled.
3. Every matrix cell performs the phases in this order in the same checked-out workspace and runner: environment/cache setup, build, package, upload, then test. A test failure therefore does not prevent an already-built artifact from being uploaded.
4. Debug cells use Cargo's development profile and release cells use Cargo's release profile throughout compilation, packaging, and tests. The browser UI and dedicated AudioWorklet Wasm use the same selected profile.
5. Native cells build `shoopdaloop_egui`, package the resulting executable/application bundle into a uniquely named archive containing target, architecture, and profile, and upload that archive as a single unwrapped file artifact.
6. Each web cell emits exactly two production deliverables with profile-specific names: a hosted bundle archive and `shoopdaloop_egui` self-contained HTML. The hosted archive contains the UI Wasm/glue, `audio_worklet.js`, and the dedicated worklet Wasm required for complete microphone Web Audio operation.
7. No web deliverable, workflow title, job, step, or artifact is called a preview or contains the connection-fixture preview application. The production application remains the normal ShoopDaLoop egui app and includes the connections dialog through `shoop_egui`.
8. The self-contained HTML remains directly loadable and preserves its precise secure-context behavior: explicit offline dummy operation is supported, while direct-file microphone support is not claimed. The hosted bundle passes the real Web Audio fake-media workflow.
9. Every matrix cell uses `Swatinem/rust-cache@v2` with keys separated sufficiently by runner/target/profile to avoid incompatible outputs while reusing Cargo registry, Git, and compilation data on later runs. Trunk and the Rust/Wasm target versions remain pinned or toolchain-controlled.
10. Native tests cover the egui API/application/backend/presentation/composition path in the selected profile and validate the packaged archive shape. Web tests cover Wasm compilation, worklet/module isolation, hosted production startup and audio I/O, and the self-contained mode after both artifacts are uploaded. The release web cell retains the extended Chrome lifecycle/stress and Firefox coverage from the existing workflow.
11. The old `.github/workflows/wasm_egui.yml` is removed after equivalent production checks move to the new workflow, preventing duplicate uploads and eliminating the connection-preview product artifact. Preview compatibility may still be checked as a non-uploaded focused test where needed.
12. Workflow and packaging changes do not add Qt, QML, CXX-Qt, frontend, native audio-driver, or plugin dependencies to the standalone egui native/browser product graphs. The retained Qt workflow is unchanged.
13. Artifact names, root/package documentation, and current egui project/parity documents accurately distinguish native dummy builds, hosted Web Audio builds, and direct-file offline behavior; no active document points to a removed workflow or calls the production web app a preview.
14. Initial workflow development is exercised with `nektos/act` using checked-in or documented event/input commands for locally runnable Linux/web matrix cells before the first hosted GitHub run. `act` limitations for non-Linux hosted runners are documented rather than treated as macOS/Windows evidence.
15. The matrix and all artifact/test naming expose only `debug` and `release`; there is no coverage matrix entry, coverage flag, instrumentation, or coverage upload step.

## Design rules and important constraints

- Keep the workflow declarative: matrix metadata owns runner label, platform/architecture label, Cargo target, profile flags, archive format, and target-specific commands. Do not duplicate four large jobs.
- Use explicit hosted-runner labels rather than floating architecture assumptions: `ubuntu-24.04` for Linux/web, `windows-2022` for Windows x86_64, and `macos-15` for Apple silicon.
- Build only the standalone egui dependency graph. Do not invoke the Qt/vcpkg-oriented top-level composite build actions or `cargo build --workspace` from this workflow.
- Use `--locked` for Cargo builds/tests and keep the repository's required nightly toolchain. Add `wasm32-unknown-unknown` only to web cells.
- Use standard `debug` and `release`, not the Qt workflow's `release-with-debug` flavor.
- Make `build_worklet.py` derive its Cargo profile from Trunk's `TRUNK_PROFILE`, with explicit validation and profile-correct target paths. A release UI must never silently package a debug worklet or vice versa.
- Package already-built outputs without rebuilding. Keep native packaging small and product-specific rather than adapting Qt launchers or Qt portable-folder assets.
- A macOS artifact may use a minimal `.app` layout and existing ShoopDaLoop metadata/icon, but must launch the egui executable directly and must not contain Qt runtime assets.
- Call native outputs application archives rather than portable/installable packages unless dependency-closure evidence supports a stronger claim.
- Construct the hosted web archive from a clean Trunk `dist` and keep the separately generated single-file HTML out of that archive so the two deliverables have unambiguous purposes.
- Upload prebuilt archives and the HTML with `actions/upload-artifact@v7` and `archive: false`; artifact filenames must be unique across all eight cells.
- Keep all validation after upload as requested. Build-time failures naturally prevent packaging, but test-only failures must leave artifacts available for diagnosis.
- Use `nektos/act` early to validate workflow parsing, expressions, matrix selection, step conditions, Linux/web commands, cache compatibility, artifact staging, and phase ordering. Pin/document the runner image and use target/profile matrix filtering; do not pretend Docker-based `act` execution validates native macOS or Windows runners.
- Keep coverage out of the first workflow revision. Do not inherit the Qt workflow's coverage input, matrix row, Qoverage actions, Rust coverage flags, or Codecov handling.
- Preserve the strongest useful browser evidence without multiplying every expensive scenario unnecessarily: both profiles run core hosted and self-contained smoke; release additionally runs denial/retry, lifecycle, saturation, sustained recording, and Firefox.
- Keep generated `dist`, worklet, staging, and archive outputs ignored by Git.
- Do not silently weaken the dependency-isolation scans currently protecting the browser UI and worklet.

## Frozen implementation contract

| Target | Runner | Architecture | Profiles | Native artifact / web deliverables |
|---|---|---|---|---|
| Linux | `ubuntu-24.04` | `x86_64` | debug, release | `shoopdaloop-egui-linux-x86_64-{profile}.tar.gz` |
| Windows | `windows-2022` | `x86_64` | debug, release | `shoopdaloop-egui-windows-x86_64-{profile}.zip` |
| macOS | `macos-15` | `arm64` | debug, release | `shoopdaloop-egui-macos-arm64-{profile}.tar.gz` |
| Web | `ubuntu-24.04` | `wasm32` | debug, release | `shoopdaloop-egui-web-wasm32-{profile}.zip` and `shoopdaloop-egui-web-wasm32-{profile}.html` |

Native archives have one `shoopdaloop-egui/` root. Linux and Windows contain the profile-built executable, `README.md`, and `LICENSE`. The macOS archive contains `ShoopDaLoop egui.app/Contents/MacOS/shoopdaloop_egui`, a minimal `Info.plist`, the existing icon under `Contents/Resources`, and the same documentation at the archive root. These are unsigned CI application archives, not portable dependency-closure or installer claims.

The web archive has one `shoopdaloop-egui/` root containing only Trunk's hosted `index.html`, the generated `shoopdaloop_egui-*.js` and `*_bg.wasm`, `audio_worklet.js`, and `generated/shoop_audio_worklet.wasm`. The profile-specific self-contained HTML is generated separately and is excluded from the hosted archive.

Native tests are profile-matched tests for `shoop_app_api`, `shoop_engine` core, `shoop_backend`, `shoop_app`, `shoop_egui`, and `shoopdaloop_egui`. Web tests add host protocol/worklet tests, warning-denying Wasm checks, dependency/module isolation, core Chrome hosted and direct-file checks in both profiles, and the existing extended Chrome/Firefox scenarios in release.

Initial local workflow commands are:

```sh
act pull_request -W .github/workflows/build_and_test_egui.yml \
  -j build_and_test --matrix target:linux --matrix profile:debug \
  -P ubuntu-24.04=-self-hosted \
  --artifact-server-path .act/artifacts

act pull_request -W .github/workflows/build_and_test_egui.yml \
  -j build_and_test --matrix target:web --matrix profile:debug \
  -P ubuntu-24.04=-self-hosted \
  --artifact-server-path .act/artifacts
```

The self-hosted mapping is useful on development systems where nested containers are unavailable; invoke the web command from an environment that supplies Trunk 0.21.14. Under `act`, toolchain/cache setup and browser/device automation are skipped, and the already-created files are validated in local staging because `act`'s artifact server does not yet implement the `upload-artifact@v7` unwrapped-file protocol. The same hosted workflow performs the real cache, upload, Chrome, and Firefox steps. Native Windows/macOS evidence must come from GitHub-hosted runners. There is no coverage matrix value or coverage command.

## Staged implementation

### Stage 1 — Freeze matrix, artifact, and command contracts

- [x] Record the explicit eight matrix entries, runner labels, architecture labels, Cargo profile flags, target directories, and artifact filenames.
- [x] Define native archive layouts: Linux executable archive, Windows executable archive, and minimal macOS `.app` archive, each with license/readme metadata and no Qt launch assets.
- [x] Define the web bundle allowlist and separate HTML output so stale files in `dist` cannot enter artifacts.
- [x] Map current Wasm checks to the new web test phase: production/package compiler checks, worklet import isolation, dependency scans, Chrome hosted/self-contained flows, extended release scenarios, and Firefox.
- [x] Define the focused native Rust package list so the egui application path is covered without compiling the Qt workspace.
- [x] Define documented `nektos/act` commands and event payloads for at least Linux debug and web debug, including matrix filtering and any safe local artifact/cache substitutions needed by `act`.
- [x] Confirm the matrix contract contains only debug/release and deliberately omits coverage.
- [x] Update this plan if implementation evidence requires command-level revisions; do not alter goals or acceptance criteria.

Verification:

- Matrix review enumerates exactly eight unique target/profile/artifact combinations.
- Every old production check in `.github/workflows/wasm_egui.yml` has a destination in the new workflow; the preview artifact steps are explicitly excluded.
- Packaging layouts contain an unambiguous runnable entry point and no Qt-oriented wrapper/configuration.

Stage 1 evidence: the frozen table enumerates eight unique cells and ten unique output files; the production checks in the old workflow are mapped above, while its three preview build/smoke/upload blocks are deliberately excluded. The archive layouts launch `shoopdaloop_egui` directly and contain no Qt wrapper or configuration.

Commit the frozen CI/artifact contract before changing build tooling.

### Stage 2 — Make build and packaging tooling profile-correct

Depends on Stage 1.

- [x] Update `src/rust/shoopdaloop_egui/build_worklet.py` to validate and honor `TRUNK_PROFILE`, select the matching Cargo flags/output directory, and copy the matching worklet Wasm.
- [x] Keep `Trunk.toml` and direct local commands compatible with debug `trunk build` and release `trunk build --release`.
- [x] Add a small cross-platform egui packaging tool/script that consumes an already-built binary or Trunk directory and produces the frozen native/web artifact layouts and names.
- [x] Generate the self-contained HTML with its existing builder and a profile-specific output name; do not label it as a preview.
- [x] Add deterministic package validation that lists/extracts each archive, checks required files and executable placement, rejects stale/forbidden files, and verifies the hosted bundle includes both Wasm modules and the worklet shim.
- [x] Update package-local build documentation with debug/release commands and resulting files.

Verification:

- Locally build debug and release Trunk outputs and confirm the UI and worklet come from their matching Cargo profile directories.
- Run packaging/manifest checks on representative native and web outputs.
- `cargo check --locked -p shoopdaloop_egui --target wasm32-unknown-unknown` and the dedicated worklet build pass with `RUSTFLAGS="-D warnings"`.
- Generated output remains ignored by Git.

Stage 2 evidence: warning-denying Trunk 0.21.14 debug and release builds pass and byte comparison confirms each copied worklet came from the matching Cargo target directory. `package_artifacts.py` produced and verified Linux, Windows-layout, macOS-layout, debug web, and release web artifacts; the hosted manifests contain exactly the five required production files, and profile-specific standalone HTML outputs are non-empty. Python compilation, Node syntax checking, and Git ignore checks pass.

Commit profile-aware browser tooling and product packaging before adding CI.

### Stage 3 — Add the single-job cross-target workflow

Depends on Stages 1–2.

- [x] Add `.github/workflows/build_and_test_egui.yml` with the agreed push/PR/tag/schedule/manual triggers, read-only permissions, concurrency cancellation for superseded branch/PR runs, and one `fail-fast: false` matrix job.
- [x] Check out the repository and install the required nightly toolchain in every cell; install the Wasm target and pinned Trunk only for web cells.
- [x] Add `Swatinem/rust-cache@v2` to every cell with target/profile-aware keys and settings that preserve reusable Cargo data without sharing incompatible binaries.
- [x] Install only target-required system packages; do not invoke vcpkg, Qt setup, QML tooling, or the old top-level build composites.
- [x] Implement profile-selected native `cargo build --locked -p shoopdaloop_egui` and web Trunk build steps with warning denial.
- [x] Package from the selected profile's built outputs.
- [x] Upload one native archive per native cell and both the hosted bundle archive and standalone HTML per web cell, all as pre-compressed/unwrapped single-file artifacts.
- [x] Place all Cargo, package-manifest, dependency, and browser test steps after the upload step(s).
- [x] Use shell-specific commands only behind matrix metadata or portable scripts so Windows does not depend on Bash path semantics.
- [x] Run the locally supported Linux/web cells with `nektos/act`, fix workflow expression/step failures there first, and record commands plus known differences from GitHub-hosted execution.

Verification:

- Parse/lint the workflow and inspect the expanded matrix for eight cells.
- `act` completes the selected Linux debug and web debug paths, or any unavailable external browser/service portion is isolated and documented while all preceding workflow phases pass locally.
- A dry command audit proves each cell follows setup → build → package → upload → test.
- Artifact and cache names are collision-free across runner OS, architecture, profile, and web/native target.
- Workflow scans contain no preview artifact or Qt build action.

Stage 3 evidence: `actionlint` passes; `act` 0.2.89 expands the selected matrix cells and completes Linux debug build/package/staging/archive verification, the full focused native suite, and formatting. The web debug `act` path completes profile-correct Trunk/worklet build, both artifact builds, staging/verification, protocol/worklet/presentation/composition tests, production and fixture Wasm compiler checks, and dependency/module isolation. Browser/device steps remain correctly reserved for GitHub-hosted runners, and native Windows/macOS are not claimed from local evidence. The workflow contains one eight-entry job, profile-unique filenames, target/profile cache keys, no coverage path, no Qt action, and no preview artifact.

Commit the new workflow before retiring the old one.

### Stage 4 — Complete post-upload target tests and retire duplicate CI

Depends on Stage 3.

- [ ] Run profile-matched focused Rust tests in each native cell for `shoop_app_api`, `shoop_engine` core, `shoop_backend`, `shoop_app`, `shoop_egui`, and `shoopdaloop_egui`.
- [ ] Validate each uploaded native archive's staged source file by extracting it and checking the expected executable or `.app` layout; run any available headless composition smoke without requiring physical audio.
- [ ] In both web cells, run warning-denying Wasm checks for the production app/worklet, protocol/worklet host tests, forbidden dependency scans, and no-import worklet module inspection.
- [ ] Run core hosted Chrome fake-media audio and self-contained offline/secure-context workflows for both web profiles after upload.
- [ ] In the release web cell, retain the denial/retry, lifecycle, saturation, sustained recording, and Firefox null-output/fake-media workflows.
- [ ] Ensure connection-dialog coverage comes from production presentation/composition tests; preview fixtures may be compiler/test inputs but are never packaged or uploaded.
- [ ] Remove `.github/workflows/wasm_egui.yml` only after all authoritative application checks have migrated.
- [ ] Confirm the new workflow is the sole egui product-artifact workflow.

Verification:

- Focused debug and release native suites pass on Linux, Windows, and macOS.
- Both web profiles load the authoritative app; hosted fake media proves non-zero microphone recording/waveform/output and the self-contained checks prove documented direct-file behavior.
- Release extended browser scenarios and Firefox pass.
- Repository workflow scans find no uploaded `shoop_egui_preview`/connection-preview artifact and no duplicate egui Wasm product build.

Commit migrated tests and old-workflow retirement as one meaningful CI milestone.

### Stage 5 — Documentation and end-to-end validation

Depends on all prior stages.

- [ ] Update `README.md`, `src/rust/shoopdaloop_egui/README.md`, `EGUI_REPLACEMENT_PROJECT.md`, and current entries in `EGUI_FEATURE_PARITY_MATRIX.md` for the new cross-target workflow, two profiles, artifact names, target-specific drivers, and preview exclusion.
- [ ] Update active references in completed egui records where they would otherwise point to the removed workflow, while preserving historical test evidence accurately.
- [ ] Confirm no product-facing name calls the web app a preview and no documentation claims direct-file microphone support.
- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build --locked -p shoopdaloop_egui` and the focused native package tests locally.
- [ ] Run both debug and release Wasm/Trunk/package commands locally, including package manifest and dependency-isolation checks.
- [ ] Validate the workflow syntax, re-run the documented `nektos/act` development commands, and inspect the final diff for generated artifacts or unrelated Qt changes.
- [ ] Push a branch only after the locally supported `act` paths pass, then verify all eight GitHub Actions cells, cache save/restore reporting, upload-before-test ordering, downloadable artifact contents, and browser test results.
- [ ] Record exact workflow links/results and any genuine hosted-runner limitation without weakening acceptance criteria.

Final validation evidence must include:

- Eight completed matrix cells with unique artifacts.
- Six native archives plus two hosted bundle archives and two self-contained HTML files.
- Cache activity in every cell, with a subsequent run demonstrating restore reuse where GitHub permits it.
- Recorded passing `nektos/act` commands for locally supported Linux/web development paths, with macOS/Windows limitations clearly separated from hosted evidence.
- Passing profile-matched native tests, package checks, Wasm isolation checks, core browser checks in both profiles, and extended release browser checks.
- A workflow scan proving there is no coverage flavor, instrumentation flag, Qoverage/Codecov step, or coverage artifact.
- A source/workflow/documentation scan showing the authoritative app is never mislabeled as a preview and the removed workflow has no stale active reference.
- No regression or modification to the retained Qt/QML build workflow.

Commit final documentation and validation evidence.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
