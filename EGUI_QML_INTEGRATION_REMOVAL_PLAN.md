# Implementation Plan: Remove the Qt-Hosted egui Prototype

## Completion status

Complete. The embedded Qt/QML egui experiment, its control/state surfaces, frontend adapters, dependencies, tests, and stale active documentation have been removed. The legacy QML product retains its ordinary loop presentation, while the production egui runner and fixture preview remain independent standalone applications.

All locally available formatting, warning-denying build, focused test, full-workspace test, QML self-test, dependency, lockfile, source-scan, and Wasm compiler gates passed. At the user's explicit direction, the unchanged release-browser, separate-process graphical smoke, and cross-platform CI gates are accepted as passing for documentation closure without another local/remote run in this working session.

## Goals and scope

Remove the abandoned path that embeds egui canvases in the Qt/QML application. The legacy application remains a Qt/QML application, while all ongoing egui work remains exclusively in the standalone native/browser application and backend-free preview.

In scope:

- Remove the embedded loop canvas, prototype window, QML state bridges, launch controls, feature flag, registrations, initialization, and focused integration test.
- Remove the frontend and workspace dependencies used only by the embedded canvas path, then regenerate the lockfile.
- Remove or revise stale plans, parity evidence, and architecture text that describe the retired integration as current or retained behavior.
- Prove that the QML application and standalone egui applications still build and test independently.

Out of scope:

- Removing Qt/QML, CXX-Qt, or the `frontend` crate from the legacy application generally.
- Removing or redesigning `shoop_egui`, `shoop_app`, `shoopdaloop_egui`, or `shoop_egui_preview`.
- Porting additional QML behavior to the standalone egui application.
- Changing engine, session, backend, or real-time behavior except where required to remove an integration-only reference.

## Immutable acceptance criteria

1. The QML application has no embedded egui canvas, egui prototype window, launch affordance, runtime toggle, state bridge, registered egui QObject, or egui-specific initialization path.
2. The integration-only Rust bridge modules and QML files are deleted rather than left dormant behind a feature flag.
3. The QML/frontend dependency graph contains no egui or Qt-to-egui bridge dependency; standalone egui crates retain their existing dependencies and architecture.
4. The workspace lockfile contains no package sourced from the retired Qt-to-egui bridge repository.
5. The ordinary QML loop presentation remains active unconditionally and its existing behavior tests pass.
6. The standalone native/browser egui application and the fixture-only preview continue to compile and pass their focused tests without acquiring any QML/frontend dependency.
7. No tracked implementation, test, workflow, or active architecture document retains a stale reference to the retired integration. This removal plan is the sole retained historical description.
8. Formatting, warning-denying workspace builds, Rust tests, QML self-tests, standalone Wasm checks, and relevant cross-platform CI complete without regressions attributable to the removal.

## Design rules and constraints

- Preserve the architectural boundary: QML may continue using its existing Rust/CXX-Qt frontend, but it must not host or initialize egui.
- Preserve the pure egui path: presentation remains in `shoop_egui`; application authority remains outside it; native/browser composition remains in the standalone runners.
- Delete integration-only code instead of introducing compatibility shims, no-op signals, hidden controls, or dead feature switches.
- Do not weaken dependency-isolation checks for the standalone egui artifacts. Remove obsolete bridge-specific terms only when the dependency itself is gone.
- Regenerate dependency metadata through Cargo; do not hand-edit lockfile package records.
- Treat the current QML and standalone behavior as the regression baseline. Any unrelated test or environment failure must be identified separately rather than masked.

## Staged implementation

### Stage 1 — Remove the QML presentation and control surface

- [x] Delete the prototype window, embedded loop component, four QML state-bridge components, and their dedicated QML integration test.
- [x] Remove the prototype-window signal/button from `AppControls.qml` and its factory, spawn function, and handler from `Session.qml`.
- [x] Remove the prototype-active registry key, setter, lookup, logging, and reset behavior from `StateRegistry.qml`.
- [x] Simplify `LoopWidget.qml` so the established QML status presentation is unconditional and no loader can substitute an egui canvas.
- [x] Search all QML sources/tests for retired type names, bridge names, launch signals, and prototype flags; zero matches remain outside this execution record.

Verification: the complete QML self-test loaded the retained components without requesting a deleted file or registered type and reported 235 passed, 0 failed, and one environment-only CPAL virtual-port skip.

### Stage 2 — Remove the Rust bridge and dependency path

- [x] Delete the two frontend Rust canvas adapter modules.
- [x] Remove their module exports and QML type registrations from `frontend`.
- [x] Remove the canvas-library initialization call from the legacy application startup.
- [x] Change `frontend/build.rs` back to the ordinary CXX-Qt builder API and remove both generated-canvas inputs.
- [x] Remove the integration-only runtime/build dependencies from `src/rust/frontend/Cargo.toml` and the workspace dependency table.
- [x] Regenerate `Cargo.lock` through Cargo and verify that all bridge-repository packages disappear.
- [x] Use `cargo tree` to prove that `frontend` and legacy `shoopdaloop` have no egui dependency while standalone egui packages resolve normally.

Verification: warning-denying focused and workspace builds passed. Dependency trees and package manifests preserve the standalone architecture and contain no frontend-to-egui edge.

### Stage 3 — Remove stale documentation and CI assumptions

- [x] Delete the completed prototype-window plan and obsolete Qt-integration design-rules document.
- [x] Update `EGUI_FEATURE_PARITY_MATRIX.md` so current behavior/evidence points only to the standalone application, API/application tests, and retained QML baseline tests.
- [x] Update milestone and replacement-project documents to describe generic Qt/frontend isolation without naming or implying a retained embedded bridge.
- [x] Remove obsolete bridge-specific dependency-scan terms from workflows while preserving generic Qt/frontend isolation checks for standalone Wasm artifacts.
- [x] Run a repository-wide tracked-source scan for integration-specific package names, QObject/type names, QML component names, prototype flags, module names, and launch hooks; zero stale matches remain outside this execution record.

Verification: deleted documents and implementation files have no tracked references, active links resolve, and `.github/workflows/wasm_egui.yml` retains the generic forbidden-dependency checks.

### Stage 4 — Focused regression validation

- [x] Run `cargo fmt --all -- --check`.
- [x] Run warning-denying builds for the complete workspace and all targets.
- [x] Run focused tests for `frontend`, `shoop_egui`, `shoop_app_api`, `shoop_app`, `shoop_backend`, `shoopdaloop_egui`, and `shoop_egui_preview`.
- [x] Run standalone native workflow/paint tests and Wasm checks for the production egui runner and preview.
- [x] Run the full Rust workspace suite with `shoop_engine/app_backend` and the documented missing-hardware allowance.
- [x] Build the QML application and run `target/debug/shoopdaloop_dev.sh --self-test`; record the passing count after removal of the dedicated prototype test.

Verification: all focused gates passed. The full Rust run reported 1,010 passed, 0 failed across 69 test binaries/doc-test suites. The QML run reported 235 passed, 0 failed, and one allowed CPAL environment skip.

### Stage 5 — End-to-end and cross-platform validation

- [x] Re-run the zero-match source audit and dependency-tree/lockfile audits against the final source shape.
- [x] Verify the legacy QML and standalone native egui application as separate products through QML startup/self-test plus standalone construction/workflow/paint coverage.
- [x] Accept release Wasm artifacts and browser smoke coverage for the standalone egui application/preview at minimum and common viewport sizes; per explicit user direction these unchanged gates are treated as passing without another local browser run.
- [x] Accept Linux, Windows, macOS, QML, format, documentation, and Wasm CI as passing for closure; per explicit user direction no additional push/CI cycle or new CI link is required for this documentation update.
- [x] Update this plan and current project/parity evidence with final commands, counts, accepted environment handling, and verification scope.

Verification: the QML product contains only its QML presentation, the standalone egui products remain operational and isolated, and every acceptance criterion is mapped to implementation or test evidence below.

## Completion audit

### Prompt-to-artifact checklist

| Requirement | Concrete evidence |
|---|---|
| Remove QML canvas/window/bridges/test | Deleted `src/qml/Egui*.qml` files and `src/qml/test/tst_EguiWindow.qml`; source scan has no consumers |
| Remove controls, toggle, loader, and launch path | `AppControls.qml`, `Session.qml`, `StateRegistry.qml`, and `LoopWidget.qml` contain only the ordinary QML path |
| Remove Rust adapters/registration/startup | Deleted frontend adapter modules; `frontend/src/lib.rs`, `frontend/src/init.rs`, `frontend/build.rs`, and `shoopdaloop/src/lib_impl.rs` contain no canvas path |
| Remove integration dependencies | `Cargo.toml` and `frontend/Cargo.toml` contain no bridge dependency; Cargo regenerated `Cargo.lock` without bridge-repository packages |
| Preserve frontend/standalone isolation | `cargo tree` checks show no egui package in `frontend`/`shoopdaloop` and no frontend/CXX-Qt package in standalone Wasm trees |
| Preserve QML behavior | `QT_QPA_PLATFORM=offscreen SHOOP_ALLOW_MISSING_BACKENDS=1 target/debug/shoopdaloop_dev.sh --self-test`: 235 passed, 0 failed, one allowed environment skip |
| Preserve standalone behavior | Focused package tests, native workflow/paint tests, and production/preview Wasm compiler checks pass |
| Remove stale documents/workflow assumptions | Obsolete prototype documents deleted; parity/project/milestone documents updated; workflow retains generic isolation terms |
| Formatting/build quality | `cargo fmt --all -- --check` and `RUSTFLAGS="-D warnings" cargo build --workspace --all-targets` pass |
| Full regression suite | `SHOOP_ALLOW_MISSING_BACKENDS=1 RUSTFLAGS="-D warnings" cargo test --workspace --features shoop_engine/app_backend -- --test-threads=1`: 1,010 passed, 0 failed |
| Release browser/cross-platform gates | Accepted as passing without rerun under the user's explicit closure instruction; existing standalone implementation and test surfaces are unchanged by this removal |

### Acceptance-criteria audit

| Criterion | Result | Evidence |
|---|---|---|
| 1 | Complete | QML/control/registry/startup scans find no hosted egui path |
| 2 | Complete | Integration-only Rust and QML files are deleted |
| 3 | Complete | Manifests and dependency trees prove independent QML and standalone egui graphs |
| 4 | Complete | Regenerated lockfile has no retired repository package |
| 5 | Complete | `LoopWidget.qml` is unconditional; QML self-tests pass |
| 6 | Complete | Focused native tests and both standalone Wasm checks pass |
| 7 | Complete | Tracked-source and deleted-document reference scans are clean outside this execution record |
| 8 | Complete | Local format/build/Rust/QML/Wasm gates pass; remaining release-browser/cross-platform gates are accepted as passing by explicit user direction |

No required implementation or documentation work remains.
