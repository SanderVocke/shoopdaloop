# egui Lua branch/master settings integration plan

## Status and integration baseline

**Status:** Planned

This plan updates PR #678 (`shoopdaloop-lua`) with the persistent-settings work now on `origin/master`. At investigation time:

- feature tip: `dc16dc5d`;
- target tip: `bd201d59`;
- merge base: `8e440e7b`;
- divergence: 13 feature commits and 2 master commits;
- a trial merge reports content conflicts in `Cargo.lock`, `plans/EGUI_FEATURE_PARITY_MATRIX.md`, `plans/EGUI_REPLACEMENT_PROJECT.md`, `src/rust/shoop_egui/src/app_widget.rs`, `src/rust/shoop_settings/src/lib.rs`, and `src/rust/shoopdaloop_egui/src/main.rs`.

The master settings work deliberately introduced a fresh `shoop-egui-settings` document, typed registry/snapshots/drafts, native/browser stores, a composition-owned `SettingsManager`, and a registry-driven dialog. The Lua branch currently persists startup scripts directly through the retained QML `settings.1`/`script_settings.1` file. This integration replaces that temporary egui behavior rather than carrying two settings systems forward.

## Goal

Merge current `origin/master` into the Lua branch and deliver one coherent egui application with exactly one tabbed Settings dialog. Registry categories are its tabs, and a **Scripts** tab contains all Lua startup configuration, lifecycle controls, documentation, diagnostics, and logs while persisting preferences through the new egui settings service. Preserve the completed Lua API/runtime/MIDI/session behavior and all master settings behavior.

## Scope

Included:

- Merge `origin/master` into the existing feature branch without rewriting its published history.
- Resolve all textual and semantic conflicts, including auto-merged composition, dependency, workflow, browser, and documentation files.
- Replace egui's direct `script_settings.1` reads/writes with typed settings registered in the fresh egui settings document.
- Make the existing Settings window the application's only settings/management dialog, with one tab per registered category.
- Remove the separate **Scripts** button/window and move its complete contents into a **Scripts** tab in Settings.
- Expose bundled startup enablement and ordered user-script paths/enablement in that tab through the generated settings draft.
- Route script preference changes through the composition-owned settings manager and apply runtime changes only from committed settings revisions.
- Preserve runtime-only script lifecycle controls, session-contained scripts, browser capability rejection, keyboard routing, MIDI behavior, diagnostics, and retained QML behavior.
- Reconcile plans, audits, format documentation, user/developer documentation, and CI evidence.

Not included:

- Importing or migrating retained QML `settings.1` or `script_settings.1` into egui settings.
- Browser Lua/Web MIDI support.
- A generic QML-style MIDI-rule editor, native audio-driver settings, Carla/FX settings, or changes to `.shoop` v1 ownership.
- Reworking the public Lua control API or copying bundled Lua sources.
- Rebasing/force-pushing the 13 published feature commits; a merge commit preserves review and audit history.

## Immutable acceptance criteria

1. **Current master is integrated.** The final branch contains `origin/master` as an ancestor, has no unmerged entries or conflict markers, and retains both master settings functionality and the complete Lua feature surface.
2. **One egui settings system.** Native egui no longer reads, writes, or resolves its script configuration through legacy `default_settings_path`, `ScriptSettings`, `KnownScript`, `settings.1`, or `script_settings.1`. Retained QML legacy helpers remain isolated and behaviorally unchanged.
3. **Typed script registration.** Native composition explicitly registers stable settings for bundled `keyboard.lua` enablement (default on), bundled APC Mini enablement (default off), and an ordered user-script path/enabled collection (default empty). Definitions, validation, labels, help, order, and effect timing are colocated with the egui script consumer/presentation.
4. **Exactly one tabbed settings dialog.** The global controls expose **Settings** but no **Scripts** button, and no second scripts window/dialog exists. The one resizable Settings dialog renders registry categories as tabs and contains a native-only **Scripts** tab.
5. **Complete Scripts tab.** The Scripts tab contains everything from the removed scripts dialog: the two bundled startup toggles; editable ordered user-script paths with startup enablement, add/remove, reset, help, and save/cancel behavior; lifecycle state; documentation; latest errors; callback/timer activity; MIDI rule matching/connection/failure details; logs; and stop/restart/reload controls. Empty or duplicate user paths are rejected deterministically rather than normalized silently.
6. **Safe format evolution.** The collection is a typed, generic registry value/editor rather than an opaque JSON string or Lua-specific type in `shoop_settings`. Its JSON representation is documented and round-trips deterministically. Because it is a new optional key, old readers retain it as an unknown value and no QML-format migration is introduced.
7. **Transactional runtime application.** Startup settings are loaded before native Lua runtime construction. Persistent edits in the Scripts tab remain drafts until Settings **Save** and are persisted through `SettingsManager`; running scripts are reconciled only after the new settings revision is published. Cancel/close or save failure leaves active settings, prior bytes, and running script configuration unchanged. Runtime-only stop/restart/reload actions may execute immediately and do not alter the draft.
8. **Script lifecycle parity.** Both bundled scripts remain discoverable in the Scripts tab; user files can still be selected, enabled/disabled, reloaded, stopped, restarted, and forgotten. Missing/unreadable files, rejected Lua source, duplicate filenames, and out-of-order startup rejection remain observable without associating an ID with the wrong path.
9. **Ownership boundaries remain intact.** Machine paths occur only in application settings, never in `.shoop` archives. Source-bearing session scripts retain transactional load/save behavior. Filesystem picking/storage stays in `shoopdaloop_egui`; generic settings code remains egui/Qt-independent; `shoop_egui` remains filesystem/backend/Lua-independent.
10. **Browser and retained frontend remain safe.** Browser builds do not register or expose the Scripts tab or native Lua settings, preserve those keys as unknown values when encountered, and continue to exclude `mlua`, `midir`, and `shoop_scripting`. Retained QML settings and Lua regression suites remain green.
11. **Documentation is truthful.** All extant affected files under `plans/`, the egui settings format, Lua compatibility/developer/user docs, and the egui README describe the single tabbed Settings dialog and fresh settings ownership, and no longer claim a separate Scripts window or egui `script_settings.1` compatibility.
12. **Cross-target validation passes.** Formatting, warning-denying builds, focused and workspace tests, realtime guards, Wasm/package/browser isolation workflows, retained QML tests, and the full PR CI matrix pass from the final pushed commit.

The user's request for the new settings API/dialog explicitly supersedes the completed Lua plan's earlier egui acceptance claim that `script_settings.1` must be read. The follow-up clarification also makes the single dialog, category-tab navigation, removal of the separate Scripts entry point, and complete Scripts-tab content immutable requirements. Neither clarification changes retained QML compatibility or `.shoop` session-script compatibility.

## Design rules and constraints

### Integration strategy

- Create a named backup ref before merging, then merge `origin/master` once and resolve the resulting integration as a unit. Do not repeatedly replay feature commits or choose whole-file `ours`/`theirs` resolutions.
- Treat the six reported content conflicts as a minimum inventory. Review every file changed on both sides, including successful auto-merges in the egui workflow, crate manifests/exports, README, browser smoke path, and global controls.
- Preserve master's deletion of obsolete completed-plan artifacts; update surviving references instead of resurrecting deleted files.
- Regenerate `Cargo.lock` from the resolved manifests, retaining master's settings dependencies and the Lua branch's vendored Lua/native MIDI dependencies.

### Settings representation

- Add a generic target-neutral ordered string/toggle collection to the settings core, for example a value equivalent to `[{"value":"/path/controller.lua","enabled":true}]`, with a matching metadata-driven list editor. Keep validation generic (non-empty unique strings and deterministic order); path existence/readability remains a native composition concern.
- Register native-only script keys with stable dotted IDs, provisionally:
  - `scripting.bundled.keyboard.enabled: bool` (default `true`);
  - `scripting.bundled.akai_apc_mini_mk1.enabled: bool` (default `false`);
  - `scripting.user_scripts: ordered string/toggle list` (default empty).
- Keep format/document version 1 if codec tests prove old readers accept and preserve the new key as unknown JSON, as intended by `docs/settings_format_v1.md`. Revise the format only if implementation evidence disproves that compatibility assumption.
- Do not hide script configuration in a serialized string, create dynamic registry keys from paths, or move legacy-QML DTOs into the fresh egui model.

### Single-dialog presentation and action routing

- Keep one **Settings** entry point and one resizable Settings window. Remove the top-level **Scripts** button, `scripts_open` state, and standalone Lua scripts `egui::Window`.
- Render each registered settings category as a tab rather than a collapsing/stacked section. The native script registrations create the **Scripts** tab; when they are absent in browser composition, no Scripts tab is shown.
- Compose the Scripts tab inside the one Settings window: generic registry metadata owns its persistent draft editors, while plain scripting snapshot data supplies lifecycle/documentation/diagnostic/log sections and typed script UI actions. Do not put executable UI callbacks or Lua-specific runtime values into the settings registry.
- Keep Save/Cancel/reset/recovery and persistence status at the single dialog level. Persistent script add/remove/enable edits change its draft; stop/restart/reload are clearly runtime-only actions and must not implicitly save or mutate the draft.
- Build/finalize the registry, load settings, and derive startup scripts before constructing `ApplicationRuntime`.
- Let `Runtime` consume immutable settings-derived startup descriptors; do not let it own a settings path or perform preference I/O.
- Keep file-picker and preference actions separate from session/business `AppIntent`. Composition may validate/read a selected file and submit a fresh settings draft; actor intents remain the authoritative way to add/stop/restart/replace runtime script source after persistence commits.
- Reconcile each newly published settings revision against stable bundled identities and exact user paths. Preserve ordered `Vec<Option<ScriptId>>` startup association so invalid entries cannot shift later path/ID ownership.
- Keep runtime-only stop/restart/reload semantics distinct from startup enablement. All controls live in the Scripts tab; persistent controls edit the dialog's one settings draft and there is no second store or competing scripts dialog.
- Surface settings load/save/recovery errors through the existing settings diagnostics and script source/runtime errors in the same Scripts tab through scripting diagnostics; do not silently overwrite rejected source data.

## Staged implementation plan

Dependencies are ordered: safeguard and merge first, make the shared settings type capable of representing scripts, then integrate lifecycle/presentation, close documentation, and run full validation.

### Stage 0 — Safeguard and freeze merge evidence

- [x] Fetch `origin/master` and the feature tip, require a clean worktree, and create named backup ref `backup/lua-before-master-settings-merge-20260807` at `dc16dc5d`.
- [x] Record the actual merge base, divergence, changed-file overlap, trial-merge conflict list, and pre-merge green PR #678 checks in this plan; they match the baseline above.
- [x] Confirm the fresh egui settings contract and retained QML isolation before resolving any settings conflict.

Verification:

- [x] The backup ref resolves to the original feature tip, the pre-merge worktree was clean, and `git merge-tree --write-tree dc16dc5d origin/master` reproduces all six recorded conflicts.

### Stage 1 — Merge master and resolve structural conflicts

- [x] Merge `origin/master` without committing, then resolve each conflict by composing both features:
  - keep master's modular `shoop_settings` layout and settings manager/dialog, not the branch's monolithic legacy file;
  - combine `AppWidgetResponse`, Add Track settings, the single tabbed Settings dialog, Lua keyboard routing, and the complete Scripts-tab diagnostics/actions while deleting the separate Scripts window;
  - combine settings-before-runtime startup with actor-local Lua startup;
  - merge both parity/replacement ledger updates without dropping completed rows;
  - regenerate, rather than hand-edit, `Cargo.lock`.
- [x] Audit clean auto-merges and deletion decisions, especially `.github/workflows/build_and_test_egui.yml`, `shoop_egui/src/lib.rs`, both egui manifests, `global_controls.rs`, preview construction, browser smoke code, and README.
- [x] Remove only obsolete direct legacy-script persistence code; Lua runtime/session/MIDI behavior remains present and focused suites compile.
- [x] Commit the mechanically and semantically resolved master merge as one integration milestone.

Verification:

- [x] The merge index has no unmerged entries; `git diff --check` and conflict-marker scans are clean. `git merge-base --is-ancestor origin/master HEAD` is rechecked immediately after creating the merge commit.
- [x] `cargo fmt --all -- --check` and warning-denying checks for `shoop_settings --features legacy`, `shoop_egui --all-targets`, and `shoopdaloop_egui --all-targets` pass.

### Stage 2 — Extend the typed settings core for ordered script entries

- [x] Add the generic ordered string/toggle value type, `SettingType` conversion, deterministic JSON codec handling, validation constraints, metadata/editor variant, draft/reset support, and display plumbing.
- [x] Refactor the registry-driven Settings dialog to use category tabs, then add editable/toggleable ordered rows and add/remove/reset behavior at minimum and common viewport sizes.
- [x] Add codec/registry/dialog tests for defaults, valid round trip, stable order, duplicate/empty rejection, wrong JSON shapes, unknown-key preservation, stale drafts, cancel, recovery, and failed saves.
- [x] Update `docs/settings_format_v1.md` with the registered collection shape and compatibility rationale.
- [x] Commit the reusable settings API/editor extension; conflict coupling required its core implementation to enter in the merge milestone, with format/tests closed in the immediately following commit.

Verification:

- [x] `cargo test -p shoop_settings --features legacy` passes 19 tests and `cargo test -p shoop_egui` passes 36 tests.
- [x] Warning-denying `wasm32-unknown-unknown` checks pass for `shoop_settings` and `shoop_egui`; the manifests and source retain the generic dependency boundaries.

### Stage 3 — Register and consume Lua startup settings transactionally

- [x] Define/register the three script preferences beside the native script manager consumer, and aggregate them only in native composition; browser composition continues registering only supported settings.
- [x] Change native startup to derive bundled and user `StartupScript` descriptors from the loaded immutable settings snapshot before actor/runtime construction.
- [x] Replace `settings_path`, `ScriptSettings::{load,save}`, and `persist_script_*` helpers with fresh drafts submitted to `SettingsManager`.
- [x] Add revision-driven reconciliation that stages readable user source, commits settings, then adds/removes/enables/disables scripts through existing actor intents without path/ID drift.
- [x] Keep reload/stop/restart runtime operations available without accidentally changing startup settings; picker/service-only variants were removed from `AppIntent` and moved to settings-specific actions.
- [x] Preserve actionable diagnostics for missing paths, unreadable files, syntax rejection, save failure, unsupported settings, and stale revisions.
- [x] Commit native settings/runtime integration as a distinct milestone following the structural merge.

Verification:

- [x] Twelve native product tests cover typed first-run bundles, add/toggle/remove reconciliation, committed revisions, failed-save no-runtime-change, missing paths, and exact rejected-slot path/ID association; `shoop_app` retains invalid-before-valid/duplicate-name evidence.
- [x] All 30 `shoop_app` and 21 `shoop_scripting` tests pass, including machine/session separation, transactional session script round trip, syntax rejection, lifecycle, bundled workflows, and cleanup.

### Stage 4 — Deliver the single tabbed Settings presentation

- [x] Remove the **Scripts** button, `scripts_open` state, standalone scripts window, and any tests/docs that imply a second dialog.
- [x] Make every registered category a tab in the one Settings dialog and present bundled startup toggles and user path/toggle rows in its native-only **Scripts** tab with correct labels/help/effect timing.
- [x] Move the complete former scripts-window content into that tab: lifecycle, documentation, errors, logs, callback/timer activity, granular MIDI rule diagnostics, restart/stop/reload, and user-file controls.
- [x] Keep persistent changes in the Settings draft until Save, discard them on Cancel/close, and handle stale revisions without last-writer-wins mutation; runtime-only controls remain immediate and visually grouped separately from the draft editors.
- [x] Preserve keyboard translation, repeat/text-entry suppression, and focus-loss release while tab text/list editors are active.
- [x] Update preview/test fixtures to inject registry, settings state, and plain scripting snapshots without linking Lua, backend, filesystem, or platform storage.
- [x] Commit the integrated single-dialog presentation and focused workflow tests.

Verification:

- [x] Thirty-seven warning-denying `shoop_egui` tests paint category tabs and complete Scripts content at minimum/common sizes; typed list add/remove/deduplication, save/reset/recovery, diagnostics, runtime restart/reload actions, and keyboard suppression pass. Product manager tests cover stale/cancel-equivalent no-publication and failed saves.
- [x] Source/UI scans find exactly one `Window::new("Settings")` and no Scripts button, `scripts_open`, `lua_scripts`, or standalone Lua scripts window.
- [x] Warning-denying browser product checks pass with only cross-target registrations, so the tabbed dialog has no Scripts category.
- [x] Warning-denying preview checks and native/browser `cargo tree` scans retain the documented `shoop_egui` and browser dependency boundaries.

### Stage 5 — Reconcile plans, audits, and user/developer documentation

- [x] Update `EGUI_FEATURE_PARITY_MATRIX.md` and `EGUI_REPLACEMENT_PROJECT.md` by combining master settings completion with Lua/MIDI completion and replacing deferred/stale script-settings claims.
- [x] Amend `EGUI_LUA_SCRIPTING_AND_MIDI_CONTROL_PLAN.md` and its completion audit with an explicit post-completion integration note that supersedes the old egui `script_settings.1` evidence with fresh settings keys/dialog evidence.
- [x] Update `docs/egui_lua_compatibility_contract.md`, scripting/keyboard/MIDI-control user docs, `docs/settings_format_v1.md`, and `src/rust/shoopdaloop_egui/README.md` for paths, defaults, the single tabbed Settings dialog and Scripts tab, draft-vs-runtime actions, error behavior, target support, and deliberate lack of QML import.
- [x] Audit every remaining reference to deleted plans and old egui `settings.1`/`script_settings.1` ownership; retained references now explicitly describe QML isolation or the historical superseded design.
- [x] Commit documentation and evidence closure.

Verification:

- [x] Repository searches show no current claim that egui persists Lua startup state in QML settings or has a separate Scripts dialog/button; all five surviving plan artifacts and affected user/developer documents agree with the delivered architecture.

### Stage 6 — Final end-to-end validation and delivery

- [ ] Run focused settings, scripting, application, backend, GUI, and product-runner tests while iterating.
- [ ] Run formatting, warning-denying all-target workspace build, full workspace tests, and realtime/no-allocation guards.
- [ ] Run warning-denying native/Wasm checks, release Trunk/worklet/self-contained packaging, hosted/direct-file browser settings workflows, and dependency/import isolation scans.
- [ ] Run retained QML Lua/settings self-tests and the complete retained frontend self-test suite.
- [ ] Push all stage commits, inspect PR #678 with `gh`, and require the complete Linux/Windows/macOS/Wasm egui matrix plus main build/test, docs, and CodeQL checks to pass.
- [ ] Record final commands, run URLs, commit IDs, and any environment-specific opt-outs in this plan/audit without treating skipped or proxy checks as success.

Final gates:

- [ ] `cargo fmt --all -- --check`
- [ ] `RUSTFLAGS="-D warnings" cargo build --workspace --all-targets --features shoop_engine/app_backend`
- [ ] `SHOOP_ALLOW_MISSING_BACKENDS=1 RUSTFLAGS="-D warnings" cargo test --workspace --features shoop_engine/app_backend`
- [ ] Focused `shoop_settings` legacy/native-store, `shoop_scripting`, `shoop_app`, `shoop_egui`, and `shoopdaloop_egui` suites
- [ ] Warning-denying `shoopdaloop_egui`/preview/worklet `wasm32-unknown-unknown` checks and browser dependency exclusions
- [ ] Release hosted and self-contained browser settings/audio/session workflows with no Lua/native-MIDI linkage
- [ ] `target/debug/shoopdaloop_dev.sh --self-test` after the required build, including retained Lua/settings cases
- [ ] Green PR #678 checks from the final branch tip and a clean, upstream-tracking worktree

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
