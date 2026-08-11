# Remove the legacy QML application

## Goal and scope

Make `shoopdaloop_egui` the only ShoopDaLoop application and remove the QML application's source, Rust integration layer, build/package infrastructure, CI, dependencies, and active documentation. The egui application is accepted as complete in its current form; this work does not add missing QML features or preserve QML feature parity.

The current Cargo graph shows that the egui/native/WebAssembly closure does not use these workspace packages: `config`, `crashhandling`, `cxx_qt_lib_shoop`, `frontend`, `macros`, `midi_processing`, `packaging`, `qt_header_bindings`, and `shoopdaloop`. The legacy packaging path also owns `distribution/`, the repository-local `vcpkg/` overlays/bootstrap path, and the old `.github/actions/` graph. The retained egui packager currently reads `distribution/macos/icon.icns`; move that asset to an egui-owned resource before deleting `distribution/`.

### In scope

- QML source, tests, schemas, launchers, integration crates, and QML-only shared code paths.
- Workspace manifests, features, dependencies, lockfile, resources, and tracing inventory.
- Legacy portable-folder/installer/container packaging, dependency lists, vcpkg overlays/bootstrap scripts, and other scripts with no retained egui caller.
- Legacy CI workflows/actions, obsolete coverage configuration, and stale repository automation.
- A complete rewrite/audit of root, user, developer, format-contract, CI, and agent-facing documentation.
- Removal of the existing `plans/` directory.

### Out of scope

- Implementing deferred or missing QML features in egui.
- Importing or migrating old QML settings/session/media formats.
- Renaming the `shoopdaloop_egui` Cargo package or executable.
- Reintroducing AppImage, installer, DMG, portable dependency-closure, or release-publication machinery; current egui native archives and web artifacts remain the packaging contract.
- Replacing historical screenshots in this change.

## Immutable acceptance criteria

1. `shoopdaloop_egui` is the only application entry point. `src/qml/`, its `src/qml/third_party/QtMaterialDesignIcons` submodule gitlink, `.gitmodules`, `src/rust/shoopdaloop/`, and all QML launch/self-test/package paths are absent.
2. The nine legacy-only workspace packages listed above are removed. Retained native and WebAssembly Cargo graphs contain no Qt, cxx-qt, QML, or removed local package dependency.
3. QML-only shared features and modules are removed, including the `shoop_settings` `legacy` feature/settings implementation, QML capture rotation/manifest vocabulary, and old JSON schema tree. Shared engine/backend APIs that the egui native path still uses remain supported.
4. `Cargo.toml` contains only dependencies referenced by retained manifests/source, and `Cargo.lock` is regenerated from the reduced workspace.
5. `distribution/`, the repository-local `vcpkg/` tree, its bootstrap/prebuild path, legacy Dockerfiles/dependency lists, and scripts used only by the removed build/package/test system are gone. The macOS icon used by egui artifacts remains available from an egui-owned resource path.
6. The only application build/test workflow is the egui cross-target workflow. It still builds/tests/packages Linux x86_64, Windows x86_64, macOS arm64, and WebAssembly in debug and release, and all retained workspace tests have a CI owner. No QML test, qoverage, old package, container-image, or legacy release job remains.
7. Repository-local vcpkg overlays, preparation actions, and scripts are removed. The runner-provided vcpkg invocation currently required to install Lilv/pkgconf in the Windows egui CI cell may remain, but no other project vcpkg path/reference remains unless an egui CI dependency is demonstrated.
8. `README.md` and `INSTALL.md` describe the current egui native/browser application, its actually implemented features, prerequisites, commands, and actual artifacts. They do not advertise removed installers, QML functionality, parity work, or stale roadmap/status claims.
9. All maintained documentation presents egui as ShoopDaLoop's sole UI. Historical screenshots may remain, and concise notes that predecessor file formats are unsupported may remain, but no instructions or feature descriptions treat the QML app as available.
10. `plans/` is absent. No new feature-parity plan or obligation replaces it.
11. Current egui behavior, native driver/FX support, browser AudioWorklet/Web MIDI paths, Lua/script key compatibility, fresh settings/session formats, artifact manifests, and Tracy capture continue to pass their retained tests. Loss of QML-only behavior is accepted.
12. Qt use needed to build the standalone Tracy profiler workflow is not mistaken for an application dependency and remains allowed. Transitive crates named `vcpkg` in `Cargo.lock` are also allowed when required by a retained Rust dependency.

## Design rules and constraints

- Use native egui, browser egui, hidden Carla-worker, artifact, test, and documentation reachability—not names such as `legacy`, `frontend`, or `qt` alone—to decide what survives.
- Preserve current egui dependencies that originated as compatibility surfaces: notably `shoop_engine/app_backend`, deterministic JACK/CPAL test adapters, the numeric script key/modifier ABI, and explicit rejection of unsupported predecessor documents. Rename misleading QML-era vocabulary where useful instead of deleting live behavior.
- Delete an entire legacy subsystem rather than leave disabled features, no-op launchers, dead Cargo features, compatibility wrappers, or orphaned CI actions.
- Keep native and WebAssembly dependency boundaries explicit; do not make browser builds acquire native audio, MIDI, LV2, or GUI dependencies.
- Keep documentation claims evidence-based against current egui UI, tests, and artifact manifests. Do not use the removed parity matrices as a product roadmap.
- Do not require removal of old screenshot/image files in this change, but do remove or rewrite text that uses them to document unavailable UI.

## Staged implementation

### Stage 1 — Remove the QML application and reduce the Cargo workspace

- [x] Remove the `src/qml/third_party/QtMaterialDesignIcons` git submodule cleanly (gitlink and submodule metadata), delete the now-empty `.gitmodules`, then delete the rest of `src/qml/` and the legacy-only crates `src/rust/{config,crashhandling,cxx_qt_lib_shoop,frontend,macros,midi_processing,packaging,qt_header_bindings,shoopdaloop}`.
- [x] Delete `src/session_schemas/`; the egui `.shoop` and settings formats remain owned by `shoop_session` and `shoop_settings`.
- [x] Remove the `shoop_settings` `legacy` feature, `legacy_settings.rs`, and legacy-only conditional branches/tests while retaining the egui native store.
- [x] Audit retained crates module-by-module for QML-only branches, helpers, test fixtures, comments, trace names, and public APIs. Remove dead paths; generalize surviving `common::tracing_capture`, engine/backend, scripting-key, and test-adapter terminology around the egui application.
- [x] Prune root workspace dependencies made unreachable by the deleted crates (including the Qt/cxx-qt/bindgen/codegen, legacy crash, schema, and old packager dependency groups), remove obsolete commented Qt overrides, and regenerate `Cargo.lock`.
- [x] Verify with `cargo metadata --no-deps`, native and Wasm `cargo tree` inspection, `RUSTFLAGS="-D warnings" cargo build --workspace`, and `cargo test --workspace --features shoop_engine/app_backend -- --test-threads=1`. Confirm the removed package names and Qt/cxx-qt crates are absent from the retained application trees.

### Stage 2 — Remove legacy packaging, dependencies, scripts, and assets

- [x] Repoint `src/rust/shoopdaloop_egui/package_artifacts.py` from `distribution/macos/icon.icns` to the identical retained icon under `resources/iconset/` (or another egui-owned resource location), then delete all of `distribution/`.
- [x] Delete the repository-local `vcpkg/` overlays/manifest/triplets and `scripts/vcpkg_prebuild.py`; retain only the runner-provided Windows egui CI install step while it remains necessary for native FX.
- [x] Remove scripts whose only callers were the deleted workflow/actions/packager. Retain `scripts/check_tracing_coverage.py` and any other script only when a surviving egui workflow, source path, or maintained document has a concrete caller.
- [x] Remove unreferenced QML-only resources and generated/build support after a tracked-file reference audit. Preserve the old documentation screenshots as permitted, and preserve Lua, click, font, logo, and icon assets consumed by egui.
- [x] Verify a native egui build and `package_artifacts.py native`/`verify` round trip on the host platform; inspect the archive manifest and ensure no deleted distribution/config/QML path is embedded or required.

### Stage 3 — Make egui CI and repository automation canonical

- [ ] Delete `.github/workflows/build_and_test.yml`, `.github/workflows/build_ci_containers.yml`, and the transitive legacy `.github/actions/` graph for vcpkg setup, qoverage, QML tests, screenshots, crash tests, old packages/installers, checkpoints, and package installation.
- [ ] Remove obsolete manual CI-debug workflows that have no egui-specific purpose, and audit the remaining workflows for references to deleted paths. Preserve `build_and_test_egui.yml`, docs, CodeQL, and standalone Tracy tooling.
- [ ] Tighten `build_and_test_egui.yml` into the sole application workflow: remove stale branch filters and old frontend isolation wording, add a warning-denying full retained-workspace test gate, keep formatting and tracing-inventory checks, and preserve all eight native/web build/package/test cells and browser smoke coverage.
- [ ] Remove orphaned `codecov.yml` and the README badge unless egui Rust coverage is deliberately added. Replace stale Dependabot submodule/Python entries with automation for ecosystems that actually remain, or remove them.
- [ ] Verify every local action reference and deleted workflow/path reference resolves cleanly; run the Linux debug and web debug workflow-equivalent commands locally, and use GitHub Actions validation for cross-OS YAML/expression behavior.

### Stage 4 — Rewrite README, installation, and maintained documentation

- [ ] Rewrite `README.md` around the sole egui product: concise purpose/status, current native/browser capabilities and limitations, current screenshot allowance, build/docs links, actual CI badge, artifact types, and credits. Remove the QML roadmap, parity framing, stale backend/platform notes, unsupported MIDI-learn claims, and unaudited comparison-table claims.
- [ ] Rewrite `INSTALL.md` from the egui workflow/package contract: Cargo/Trunk commands, per-platform native prerequisites, browser secure-context guidance, and unsigned native/web archive behavior. Remove submodule, Qt/QMake, vcpkg-prebuild, editable-QML, AppImage/Inno Setup/DMG, and old dev-launcher instructions.
- [ ] Audit every page under `docs/` against current egui UI and tests. Rewrite architecture, track/loop/MIDI/Lua, tracing, ports, sessions/settings, browser, Carla, build/test, and packaging text; remove the generic QML MIDI-rule editor instructions and other unavailable UI while retaining implemented egui behavior.
- [ ] Update `docs/tracing_coverage.csv` to exactly match retained production Rust modules and rewrite the tracing baseline/capture documentation around native egui rather than QML rotation/self-tests.
- [ ] Update `src/rust/shoopdaloop_egui/README.md`, `.agents/info/*.md`, `.agents/skills/tracy/`, and `.github/pi_coding_agent/build_instructions.md` so developer and agent commands use the retained egui tests/run path. Remove obsolete QML-only troubleshooting; retain generic CI contention guidance where still applicable.
- [ ] Delete `plans/` and remove links to its parity/future/replacement documents. Historical screenshots may remain in `docs/source/resources/`, but references must not imply that their old controls are current.
- [ ] Verify `sphinx-build -W --keep-going docs/source _build`, `python3 scripts/check_tracing_coverage.py --require-closed`, and a tracked-text link/path scan. Review all remaining `QML`, `Qt`, `cxx-qt`, `vcpkg`, removed-crate, old-workflow, and old-artifact matches against the explicit acceptance exceptions.

### Stage 5 — Final repository and end-to-end validation

- [ ] Prove the removed trees, packages, features, workflows, actions, scripts, schemas, Dockerfiles, dependency lists, vcpkg overlays, Qt Material Design Icons submodule/gitlink metadata, and `plans/` directory are absent with `git ls-files --stage`, `git submodule status`, `cargo metadata`, `cargo tree`, and targeted `rg` audits.
- [ ] Run `cargo fmt --all -- --check`, `RUSTFLAGS="-D warnings" cargo build --workspace`, and the complete retained workspace test suite with the application backend feature and serialized tests.
- [ ] Run warning-denying native `shoopdaloop_egui` default/no-default-feature builds and tests, artifact packaging/verification, and an offline/dummy application startup smoke test.
- [ ] Run the WebAssembly checks for `shoopdaloop_egui` and `shoop_audio_worklet`, Trunk debug/release builds, hosted/self-contained artifact verification, dependency-isolation scans, and available Chrome/Firefox smoke workflows.
- [ ] Rebuild Sphinx with warnings as errors and rerun the closed tracing inventory check after all source/document moves.
- [ ] Require the authoritative GitHub egui workflow to pass all eight Linux/Windows/macOS/WebAssembly debug/release cells before merge; record any hardware-only checks as environment limitations rather than restoring QML tests.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
