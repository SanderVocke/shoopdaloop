# egui Lua/master settings integration completion audit

## Audit contract and objective restatement

The objective is complete only when the branch contains current `origin/master`, the Lua milestone still works, and native script configuration/runtime management uses the fresh egui settings service through exactly one category-tabbed Settings dialog. The separate Scripts button/window and direct QML-format persistence must be absent. Native, browser, retained QML, package, dependency, documentation, and final PR CI evidence must cover the resulting artifacts.

This audit does not treat plan checkboxes or a green proxy suite as completion. Each requirement below maps to implementation and direct verification. **Current status: local artifact audit passed; authoritative post-push CI pending.**

## Prompt-to-artifact acceptance checklist

| # | Required result | Concrete implementation evidence | Direct verification evidence | Status |
|---|---|---|---|---|
| 1 | Current master integrated without dropped Lua/settings behavior | Merge commit `14e0a263`; both parents retained; conflict resolutions in settings, widget, runner, manifests, ledgers, workflow | `git merge-base --is-ancestor origin/master HEAD`; clean conflict-marker/unmerged-entry scans; warning-denying workspace build and full tests | Covered locally |
| 2 | One egui settings system; no egui legacy script persistence | Modular `shoop_settings`; `SettingsManager`; no `ScriptSettings`, `KnownScript`, old picker intents, or direct runtime settings path | source scans find no removed types/intents; legacy `default_settings_path` remains only in retained QML/Carla module and product | Covered |
| 3 | Typed native scripting registrations and defaults | `KEYBOARD_SCRIPT_ENABLED`, `APC_MINI_SCRIPT_ENABLED`, `USER_SCRIPTS`; `register_script_settings`; native-only composition registration | 19 settings tests; typed startup adapter product test proves keyboard on/APC off and ordered user entries | Covered |
| 4 | Exactly one category-tabbed Settings dialog | `SettingsDialog` category tabs; one global Settings action; removed `scripts_open`/Lua window | source scan finds exactly one `Window::new("Settings")` and no Scripts button/window/state; GUI tab tests at 360×200 and 900×600 | Covered |
| 5 | Complete native Scripts tab | Draft editors plus runtime cards in `settings_dialog.rs`: add/remove/enable, lifecycle, docs, errors, callback/timer counts, granular MIDI, logs, restart/stop/reload | 37 warning-denying GUI tests include complete diagnostic fixture, typed add/remove/deduplication, runtime restart/reload interaction, keyboard suppression | Covered |
| 6 | Safe generic format evolution | `StringToggle`, `StringToggleList`, value/type/editor/codec/validation support; documented v1 shape | valid deterministic round trip, malformed shape, empty/duplicate rejection, reset, and old-registry unknown-array retention tests | Covered |
| 7 | Transactional runtime application | settings loaded before `Runtime`; source preflight; manager publication; revision-driven add/remove/enable reconciliation | 12 product tests include committed add/toggle/remove and failed-save no-revision/no-runtime-change; manager stale/recovery/no-clobber tests | Covered |
| 8 | Lua lifecycle parity and exact path/ID ownership | existing actor intents/runtime; composition path map and pending ordered association; runtime-only actions separate from drafts | 30 app + 21 scripting tests; duplicate-name/rejected-slot tests; bundled keyboard/APC and reload/cleanup tests | Covered |
| 9 | Settings/session/filesystem architecture boundaries | filesystem and picker in product composition; generic settings core; machine paths excluded from session source documents | session machine/source separation test; `cargo tree`/source scans; warning-denying core/preview/native/Wasm checks | Covered |
| 10 | Browser and retained frontend safety | native-only registrations/dependencies; browser Settings omits Scripts; retained legacy settings module unchanged | release Wasm checks; trees exclude `mlua`, `midir`, `shoop_scripting`, frontend/Qt; hosted/direct-file settings workflows; retained Lua QML 45/45 | Covered locally |
| 11 | Truthful docs and ledgers | all surviving `plans/`; settings format; Lua contract; scripting/keyboard/MIDI docs; egui README | repository searches find no current separate-dialog or egui-QML-persistence claim; deleted-plan references removed | Covered |
| 12 | Cross-target final validation and delivery | commits `14e0a263`, `8a89e3bf`, `5aeea71b`, `78b22552`, `f15078b4` | local gates below pass except one unrelated CPAL QML host case; authoritative PR CI not yet run for final tip | Pending CI |

## Staged-plan and named-artifact checklist

| Plan stage / named artifact | Evidence |
|---|---|
| Stage 0 backup/baseline | `backup/lua-before-master-settings-merge-20260807` resolves to `dc16dc5d`; merge base/divergence/six conflicts recorded |
| Stage 1 merge/conflicts | `14e0a263`; master is ancestor; no unmerged entries; lockfile regenerated; master deletions retained |
| Stage 2 settings type/tabs | `StringToggleList`; category tabs/list editor; `docs/settings_format_v1.md`; 19 settings + 37 GUI tests |
| Stage 3 startup/reconciliation | native-only registration, preflight, `configured_startup_scripts`, `reconcile_script_settings`; 12 product tests |
| Stage 4 one complete dialog | only Settings entry/window; complete Scripts tab; interaction/paint/source/dependency tests |
| Stage 5 docs/ledgers | `EGUI_FEATURE_PARITY_MATRIX.md`, `EGUI_REPLACEMENT_PROJECT.md`, Lua plan/audit, four Lua docs, settings format, README updated |
| `Cargo.toml` / `Cargo.lock` | locked metadata succeeds; vendored Lua/native MIDI plus settings dependencies retained; target gates compile |
| `.github/workflows/build_and_test_egui.yml` | merged settings workflow and Lua/ALSA dependency fixes retained; final hosted run pending |
| Browser artifacts | release Trunk bundle, release zip, and self-contained HTML build/verify; worklet has zero imports |
| Retained frontend | full offscreen suite reaches 235/236 with only environment-sensitive `CpalPorts::test_virtual_playback_ports_are_app_connectable`; focused retained Lua is 45/45 |

## Recorded local commands and results

- `cargo fmt --all -- --check` — pass.
- `RUSTFLAGS="-D warnings" cargo build --workspace --all-targets --features shoop_engine/app_backend` — pass; only the repository's external gold-linker deprecation message appears.
- `SHOOP_ALLOW_MISSING_BACKENDS=1 RUSTFLAGS="-D warnings" cargo test --workspace --features shoop_engine/app_backend` — pass, including 24 no-allocation tests and all settings/Lua/application/product suites.
- `cargo test -p shoop_settings --features legacy` — 19/19 pass.
- `RUSTFLAGS="-D warnings" cargo test -p shoop_egui` — 37/37 pass.
- `cargo test -p shoopdaloop_egui -- --test-threads=1` — 12/12 pass.
- `cargo test -p shoop_app` — 30/30 pass.
- `cargo test -p shoop_scripting` — 21/21 pass; native MIDI explicitly reports unavailable ALSA sequencer on this host.
- Warning-denying native product/preview and debug/release `wasm32-unknown-unknown` checks — pass.
- Release `trunk build`, web/native packaging and verification — pass after adding the Nix `lld` path required by this shell.
- Browser dependency scans exclude Lua/scripting/native MIDI/frontend/Qt; worklet Wasm has zero imports.
- Chromium 147 hosted settings save/reload/rejection, unavailable storage, direct-file settings, hosted 360×200/900×600 Web Audio, output-only, self-contained offline, and direct-file output-only workflows — pass.
- `QT_QPA_PLATFORM=offscreen ... tst_LuaEngine_SessionControlHandler.qml` — 45/45 pass.
- Full offscreen retained suite — 235/236; only the CPAL virtual-playback host case fails, while all 235 other cases pass. Authoritative Linux CI remains the acceptance surface for that environment-sensitive case.

## Remaining authoritative evidence

- Commit this audit and updated final plan status.
- Push the final branch.
- Require PR #678 Linux/Windows/macOS/Wasm egui matrix, main Linux build/test/QML job, docs, and CodeQL to pass.
- Record final run IDs and commit in this audit/plan, then verify a clean upstream-tracking worktree.
