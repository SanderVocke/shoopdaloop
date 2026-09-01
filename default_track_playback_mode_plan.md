# Track Default Playback Mode Implementation Plan

## Goals

- Give every track one persisted default playback mode: regular playback or dry-through-wet playback, with dry-through-wet valid only for dry+wet tracks.
- Let users choose the mode in new-track defaults, override it while creating a track, and change it later from the track context menu.
- Make every default loop action resolve playback from the loop's owning track at trigger time while keeping all explicit playback commands explicit.
- Make regular-composite schedule entries mean only `DefaultPlayback`; remove inherited or concrete playback modes from regular-composite plans. Keep every script-composite event mode explicit.
- Deliver the feature on native and browser backends, publish it in a pull request for issue #294, and complete only after CI is green and automated Codex review feedback is resolved.

## Scope

This work covers application/API track state, new-track settings and creation, track controls, session persistence, GUI and bundled-script default actions, regular/script composite compilation and runtime resolution, backend and AudioWorklet command paths, compatibility documentation, tests, commits, push, pull-request creation, CI follow-up, and review follow-up.

It does not change explicit Play, Play Dry Through Wet, or explicit Lua/script-composite event semantics for primitive loops. It does not store a concrete track default in each regular-composite event, rebuild a composite plan when a track default changes, add a default playback mode to individual loop/session records, or make dry-through-wet valid for direct, trigger, sync, or composite targets themselves.

## Immutable Acceptance Criteria

1. Every track has exactly one default playback mode, `Regular` or `DryThroughWet`; new and migrated tracks default to `Regular`.
2. `DryThroughWet` is accepted only for dry+wet tracks. Direct, trigger, and sync tracks are always `Regular`, and invalid session/API input is rejected or normalized before backend mutation according to the existing transaction contract.
3. The Track Defaults settings page contains the default playback mode, the Add Track draft initializes from it, the creation dialog allows an applicable override, and the existing **make default** flow includes it.
4. A dry+wet track's context menu can change its default playback mode. A successful change is reflected in application state and the realtime backend; a failed change leaves both unchanged.
5. When a GUI or bundled-script default loop action would currently enter playback, each primitive target enters its owning track's current default mode. This includes stopped non-empty loops and the recording-to-playback branch. Empty-loop recording/grab, cancellation, and stop branches retain their current behavior.
6. Explicit GUI Play and Play Dry actions, explicit Lua `loop_transition`/`loop_trigger` modes, and explicit script-composite events never consult the track default.
7. A regular-composite persisted event has no explicit mode and compiles to the sole regular child playback semantic, `DefaultPlayback`. The former regular `Inherit` semantic is replaced, not complemented.
8. Starting a regular composite performs ordinary composite playback. Each primitive child resolves `DefaultPlayback` from its owning track when that child is triggered; nested regular composites recursively do the same at the same sample. The regular composite itself has no dry-through-wet playback variant and does not propagate a concrete parent playback mode to its children.
9. Every script-composite event retains an explicit concrete mode. A script event targeting a primitive loop uses that exact mode; a script event that explicitly plays a nested regular composite starts the regular composite, whose own default-playback child semantics then apply.
10. Changing a track default does not alter, replace, reconfigure, or duplicate data in any regular-composite plan. It does not change an already active child. The next activation/retrigger of a default-playback child resolves the then-current track default.
11. Track defaults round-trip through `.shoop` sessions. Supported older session document versions load with `Regular`, and malformed or inapplicable values fail before partial application.
12. Track-default updates and default-playback resolution are bounded and allocation-free on the realtime path. Native and AudioWorklet backends produce the same behavior.
13. Tests directly cover regular/dry defaults, mixed-track selections, stopped and recording default actions, explicit-mode bypass, nested regular composites, explicit script composites, live default changes without plan replacement, session migration/round-trip, settings/creation/context-menu behavior, native/browser protocol parity, and realtime no-allocation behavior.
14. The branch is rebased onto the current `origin/master`, all required local validation passes, the branch is pushed, a PR linked to #294 is opened, all required CI checks are green on the final head commit, and every actionable automated Codex review comment is addressed and acknowledged before completion.

## Design Rules and Constraints

- Use a dedicated domain enum such as `DefaultPlaybackMode`; do not overload `LoopMode` with a persistent preference or use booleans.
- Keep the semantic source of truth on the track. A bounded engine-side lookup or per-target runtime mirror may cache the owning track's value, but regular-composite schedules must contain only the symbolic `DefaultPlayback` action and must never snapshot the concrete setting.
- Resolve `DefaultPlayback` only when an inactive child is activated or an actual retrigger occurs. Latch the resulting concrete mode for that active occurrence so editing a track does not mutate playback already in progress.
- Preserve the existing regular-composite document representation (`mode: null`/absent) and translate it to `DefaultPlayback`. Continue requiring concrete non-`Unknown` modes for script-composite events.
- Regular composites accept ordinary playback/stop control at their outer boundary. Do not expose or silently reinterpret outer dry-through-wet playback as a second regular-composite playback mode.
- Preserve composite boundary precedence, authoritative start/seek behavior, nested same-sample propagation, stale-identity checks, fixed capacities, deterministic traces, and no-allocation guarantees.
- Preserve explicit script behavior. Add only the scripting operation needed for the helper's default-playback branch; ordinary mode-taking APIs remain concrete and unchanged.
- Apply synchronized trigger, solo, target, fixed-cycle, and play-after-record policies consistently with their current applicable paths. Do not broaden track defaults to unrelated automatic post-record transitions unless they are initiated by the default action itself.
- Update track/backend state transactionally: validate capability first, apply backend state, then publish application state.
- Bump the session document version if required by the repository's exact schema contract, add an explicit migration/default for all currently supported prior versions, and update format documentation and fixtures together.
- Add the application-settings key as an optional version-1 registered setting unless implementation evidence requires a settings document-version change; preserve unknown settings and recovery behavior.
- Rebase before implementation because this checkout is four commits behind `origin/master` and the intervening changes include the unified Track Defaults work from #827.
- Avoid unrelated refactors, formatting churn, generated artifacts, and changes to audio routing or channel processing.

## Execution Contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Staged Implementation

### Stage 0: Establish the Current Baseline

- [x] Commit this plan as the planning milestone if it is not already committed.
- [x] Fetch `origin`, rebase `shoopdaloop-playdrydefault` onto current `origin/master`, and resolve conflicts by preserving the current unified Track Defaults, latency, auto-arm, and browser behavior.
- [x] Re-read affected APIs after the rebase and update only implementation details in this plan if paths or names changed.
- [x] Confirm the worktree is clean and record the baseline commit.

**Verification**

- [x] Run formatting and a warning-denying workspace build on the rebased baseline.
- [x] Run focused existing application, settings, session, scripting, composite-plan, composite-runtime, timeline, backend, protocol, and worklet tests before behavior changes.

### Stage 1: Define the Track Preference and Persistence Contract

- [x] Add the shared `DefaultPlaybackMode` enum and conversion helpers between application, backend, engine, wire, settings, and session representations without coupling it to transient `LoopMode` state.
- [x] Add the preference to `TrackSpec`, `TrackModel`, `TrackState`, backend track state/request data, and every track creation/replacement path. Force `Regular` for non-dry+wet and sync tracks.
- [x] Add a track action/backend operation for changing the preference with capability validation and transactional application-model publication.
- [x] Add the field to `TrackDocument`, update session validation, migration/defaulting, fixtures, round-trip capture/load, and the session format version/documentation as required.

**Verification**

- [x] Add API validation tests for valid dry+wet and invalid non-dry+wet combinations.
- [x] Add session encode/decode, old-version migration, invalid-document transaction, native replacement, and browser replacement tests.
- [x] Run targeted `shoop_app_api`, `shoop_session`, `shoop_app`, and remote application tests.

### Stage 2: Carry the Preference Through Native and Browser Runtimes

- [x] Store the current preference in backend track ownership state and make newly added track loops inherit access to that track-owned value.
- [x] Add the bounded engine/runtime metadata needed to resolve a primitive target's owning-track default without consulting or modifying a composite plan.
- [x] Carry track creation and live preference changes through native, local engine, fake backend, audio protocol, worklet client, and AudioWorklet command paths.
- [x] Ensure track deletion, session replacement, driver switching, rollback, and stale command handling remove or restore the associated runtime metadata safely.

**Verification**

- [x] Add backend tests proving every loop in a track observes the current track value, newly added loops observe it, and another track remains independent.
- [x] Add wire encode/decode and worklet command tests for creation, mutation, stale IDs, and replacement.
- [x] Confirm a failed backend mutation leaves the application and runtime preference unchanged.

### Stage 3: Replace Regular Composite Inheritance with Dynamic Default Playback

- [x] Replace `CompiledChildMode::Inherit` with `CompiledChildMode::DefaultPlayback`; do not retain both semantics.
- [x] Compile every mode-less regular-composite entry to `DefaultPlayback` and continue compiling every script-composite entry to `Explicit(LoopMode)`.
- [x] Carry the symbolic regular action through reconciliation until an actual child activation/retrigger, then resolve primitive targets from current runtime track metadata and composite targets as ordinary playback.
- [x] Latch the concrete primitive mode for the active occurrence so changing a track default does not alter an already active child.
- [x] Preserve regular looping, explicit start/seek snapshots, nested same-sample propagation, conflict priority, natural advancement, offsets, empty-child behavior, and plan replacement rules.
- [x] Reject unsupported outer regular-composite playback variants rather than treating dry-through-wet as another inherited/default mode.
- [x] Keep composite configuration/signatures independent of track default values so a live preference change queues no plan compile, configure, replacement, or restart operation.

**Verification**

- [x] Add compiler tests proving regular plans contain only `DefaultPlayback`, script plans contain only explicit modes, and mixed/unknown script data remains invalid.
- [x] Add state-machine and timeline tests for regular and dry defaults, different defaults in one composite, nested regular composites, explicit script events, start/seek, wraparound, and conflicts.
- [x] Add a regression test that changes a default while a child is active, observes no immediate mode change or plan replacement, stops/retriggers it, and observes the new mode.
- [x] Extend no-allocation tests across compilation-independent live preference changes and default-playback boundaries.
- [x] Run targeted composite plan, semantics, state-machine, control, timing, timeline, app-backend, and no-allocation tests on native and Wasm-capable test paths.

### Stage 4: Apply Defaults to GUI and Scripted Default Actions

- [x] Update the application default-action playback branches so stopped non-empty primitive loops and recording primitive loops request track-default playback.
- [x] Make default actions on regular and script composites request ordinary composite playback; do not apply the carrier track's dry preference to the composite itself.
- [x] Add a mode-less/default-playback scripting control operation for `shoop_helpers.default_loop_action` and keep existing mode-taking scripting APIs explicit.
- [x] Update the helper and bundled keyboard/controller scripts so only their default-action playback branch uses the new operation; explicit dry modifiers and explicit mode shortcuts retain their current concrete modes.
- [x] Handle mixed selected primitive/composite targets deterministically without crashing or disabling a bundled script when an operation is inapplicable.
- [x] Update the Lua API minor version, function inventory, helper documentation, and bundled script announcements if the public scripting surface changes.

**Verification**

- [x] Add application tests for stopped-to-play, recording-to-play, empty recording/grab, cancellation, stop, mixed track defaults, selected groups, and composite targets.
- [x] Add scripting bridge/helper tests proving default playback is dynamic while explicit `Playing` and `PlayingDryThroughWet` bypass it.
- [x] Run production keyboard and APC script tests on native and Wasm paths.

### Stage 5: Implement Settings, Creation, and Track Context UI

- [x] Register `tracks.new.default_playback_mode` as a stable string-choice setting with `regular` as its default and document its effect timing.
- [x] Integrate the field into the unified `NewTrackConfiguration`, Track Defaults custom editor, Add Track draft, validation, and **make default** persistence/retry flow.
- [x] Show an applicable playback-mode selector in the Add Track form and force/display regular playback when the selected topology is not dry+wet.
- [x] Add a track context submenu/radio choice for dry+wet tracks and route changes through the new track action. Do not offer an inapplicable dry choice on other tracks.
- [x] Publish the current mode in `TrackState` so widgets remain snapshot-driven and deterministic.

**Verification**

- [x] Add settings registry/default/invalid-value/unknown-key tests and Add Track draft/make-default retry tests.
- [x] Add egui interaction tests for creation overrides, topology changes, context-menu visibility, selected value, emitted action, and backend failure reconciliation.
- [x] Add browser settings persistence coverage for the new optional key.

### Stage 6: Audit Semantics and Documentation

- [x] Update user documentation for track defaults, creation, context-menu changes, default action behavior, and regular versus script composite playback.
- [x] Update composite semantic documentation to state that regular entries are symbolic `DefaultPlayback`, resolved at child trigger time without plan mutation, while script entries are explicit.
- [x] Update session/settings format documentation and Lua compatibility documentation, including compatibility/version boundaries.
- [x] Audit all `Playing` and `PlayingDryThroughWet` call sites that represent default actions, regular-composite child actions, explicit actions, and post-record policy; classify each intentionally and add missing tests rather than broad substitutions.
- [x] Audit native and browser backend parity and confirm no track default is serialized into a composite entry/configuration/signature.

**Verification**

- [x] Search for the removed regular `Inherit` variant and confirm no production or test reference remains.
- [x] Inspect encoded regular-composite/session fixtures and backend configurations to confirm their child mode remains symbolic/absent rather than a concrete track preference.
- [x] Review terminology consistently for `default playback`, `regular playback`, `dry through wet`, `explicit`, `trigger time`, and `active occurrence`.

### Stage 7: Full Local Validation

- [x] Run `cargo fmt --all` and verify with `cargo fmt --all -- --check`.
- [x] Run `RUSTFLAGS="-D warnings" cargo build --workspace`.
- [ ] Run `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`.
- [x] Run `python3 scripts/check_shoop_test_usage.py` because Rust tests changed.
- [x] Run `python3 scripts/check_tracing_coverage.py --require-closed`.
- [ ] Build/check `shoopdaloop` and build `shoop_audio_worklet` for `wasm32-unknown-unknown`.
- [ ] Run `python3 scripts/run_wasm_tests.py --profile dev --runtime node` and the policy-relevant Chrome suite when a browser is available.
- [ ] Run the documented raw Wasm host/worklet dependency and smoke checks affected by protocol changes.
- [ ] Review the complete diff for unrelated changes, accidental generated files, concrete defaults embedded in composite plans, and direct evidence for every acceptance criterion.
- [ ] Commit each remaining coherent milestone and leave a clean worktree.

**Verification**

- [ ] Record every command and result in the plan, including host-facility or browser limitations.
- [ ] Re-run any isolated failure to distinguish a reproducible defect from resource contention; fix reproducible failures before pushing.

### Stage 8: Push and Open the Pull Request

- [ ] Fetch and rebase onto the latest `origin/master` again, resolve any new conflicts, and rerun affected targeted tests plus formatting and warning-denying build.
- [ ] Push `shoopdaloop-playdrydefault` to `origin` with upstream tracking; use `--force-with-lease` only if the reviewed rebase requires it.
- [ ] Open a non-draft GitHub pull request against `master` with a concise behavior summary, design explanation, migration notes, test evidence, and `Closes #294`.
- [ ] Explicitly call out that regular plans store only symbolic default playback, track edits do not reconfigure plans, and script-composite modes remain explicit.

**Verification**

- [ ] Confirm the PR head SHA matches the pushed local head, the base is `master`, issue #294 is linked, and no unrelated commits/files appear in the PR.

### Stage 9: CI and Automated Codex Review Closure

- [ ] Monitor all PR checks with `gh pr checks`, `gh run watch`, and workflow/job logs until every required check reports success on the latest head SHA.
- [ ] For failures, inspect the exact attempt, matrix job, logs, and artifacts. Reproduce and fix product defects locally; if a Perfetto trace is needed, read the Perfetto skill first. Do not dismiss a failure as flaky without evidence.
- [ ] Inspect PR reviews, inline review threads, and issue comments through `gh pr view` and `gh api`, including every automated Codex review comment.
- [ ] Address each actionable Codex comment with code/tests/docs, commit, and push; reply with the resolution and evidence. For a non-actionable or conflicting suggestion, document the acceptance-criteria/design reason and obtain a clean follow-up state rather than silently ignoring it.
- [ ] After every review-driven push, rerun affected local tests and wait for all required CI checks on the new head SHA.
- [ ] Repeat review and CI inspection until no unresolved actionable Codex feedback remains and all required checks are green.

**Verification**

- [ ] Record the final PR URL and head SHA.
- [ ] Confirm the required-check rollup is green for that SHA, automated Codex review has no outstanding actionable request, conversations are resolved or explicitly answered, and the worktree matches the pushed branch.
- [ ] Report completion with the acceptance-criterion evidence, local/CI command outcomes, review resolutions, and any environment-only skipped checks.
