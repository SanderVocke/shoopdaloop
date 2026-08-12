# Loop synchronization and composite playback fix plan

## Goal and scope

Make the three reproduced loop-control regressions pass through real application/backend behavior:

1. Turning off the global sync control makes primitive loops start immediately and repeat at their own boundaries instead of waiting for the global sync loop.
2. A composite remains stopped at position zero when one of its children is started independently.
3. Playing a regular composite created by the basic editor starts the composite runtime and its scheduled children.

Use the engine's tested composite runtime for regular composites rather than inferring parent state from child state or relying on application elapsed-time emulation. Add backend-neutral composite lifecycle, configuration, control, and snapshot contracts so native, in-process engine, fake, and Web Audio adapters can expose the same behavior. Preserve the canonical `CompositeDocument` as the persistence/editing model and leave a clear extension point for script composites, nested composites, richer modes, and delayed events.

This work includes focused unit/contract tests and straightforward full-stack application tests. It does not add new composite editing gestures, redesign the editor, or broaden the basic editor beyond serial/parallel regular composition.

## Immutable acceptance criteria

1. The three regression tests currently in `shoop_app` pass without weakening their assertions:
   - `disabling_sync_makes_a_primitive_loop_repeat_at_its_own_boundary`;
   - `independently_playing_a_child_does_not_advance_its_composite`;
   - `gui_play_on_a_regular_composite_starts_the_composite_and_first_child`.
2. `SetSync(false)` removes repeat synchronization from all current non-sync primitive loops; loops created or restored while sync is off are also unsynchronized. `SetSync(true)` restores the global sync source. Explicit per-loop repeat-sync scripting remains available, with the latest explicit operation determining that loop's link.
3. Immediate versus delayed transition semantics remain separate from repeat synchronization. Synchronized actions still wait when global sync is enabled, and immediate actions do not wait.
4. A regular composite created or loaded by the application has a backend composite identity, an installed engine plan, and independently published mode, pending mode, length, position, cycle count, and active-child state.
5. GUI and script play/stop operations on backend-owned regular composites control the composite identity, not the cleared primitive placeholder. Starting a child directly never changes the parent composite state.
6. A three-section regular composition starts its first child immediately and advances through the remaining children at the configured boundaries, then repeats according to regular-composite engine semantics.
7. Empty composites remain safe stopped no-ops. Reconfiguration rejects stale targets and cycles without partially mutating the authoritative application composition.
8. Session save/load and audio-driver replacement preserve canonical composition data and recreate backend composite identities/configurations against the replacement primitive-loop IDs.
9. Native, in-process `EngineBackend`, fake, and browser/Web Audio implementations compile against one backend-neutral composite contract. Browser protocol commands and snapshots preserve composite configuration/control/state without GUI-thread timing emulation.
10. Existing rich script-composite data, delays, modes, playlists, and persistence are not discarded. Any schedule not yet representable by the first regular-composite lowering path remains explicitly on the existing script path rather than being silently miscompiled.
11. New full-stack tests drive typed `AppIntent`s through `CooperativeApplicationRuntime` with a real `EngineBackend` and prove unsynchronized repeat, independent-child isolation, and three-child composite playback. Backend and Web Audio protocol contract tests cover the same backend surface below the GUI.
12. All required formatting, warning-denying builds, tracing checks, workspace tests, and WebAssembly builds pass.

## Design rules and constraints

- Keep `CompositeDocument` authoritative for editing and persistence; do not store engine identities in session files or public GUI snapshots.
- Add backend-owned composite IDs and state alongside primitive loop IDs. A track slot may retain its primitive placeholder for media/channel ownership, but composite actions and state must not be routed through it.
- Define composite configuration in `shoop_backend` using backend IDs and semantic sections/events. Keep `shoop_engine` identities and handles inside backend adapters.
- Reuse or extract the existing engine composite-registry compilation logic instead of creating different compilers for `NativeBackend`, `EngineBackend`, and the audio worklet.
- Compile the basic editor's regular schedules exactly. Reject or retain fallback ownership for richer schedules that cannot yet be represented; never round frame delays or modes silently.
- Backend configuration must be transactional: validate and prepare a candidate plan before replacing the installed plan or committing application model changes.
- Publish parent mode and position only from backend composite state while that composite is backend-owned. Child-derived projection is not an acceptable fallback for such composites.
- Keep realtime work bounded and allocation-free. Plan compilation, protocol serialization, registry mutation, and reclamation stay off the audio callback.
- Keep browser protocol payloads bounded and versioned. Composite configuration commands must obey existing command-size and journaling rules.
- Preserve existing selection, solo, recording, primitive media, Lua/controller, and composite-editor behavior outside these fixes.
- Tests must be deterministic and headless; use exact frame counts or bounded backend fences rather than wall-clock sleeps where possible.

## Staged implementation plan

### Stage 0 — Lock semantics and regression baseline

- [x] Keep the three existing failing regression tests unchanged and record their current failure values in this plan while implementing.
- [x] Add small model tests for sync toggling before/after track creation and for the latest `SetRepeatSync` operation overriding a prior global toggle for its target loop.
- [x] Identify the regular `CompositeDocument` subset produced by the basic editor and add fixtures for empty, three serial children, one parallel section, stale target, and cycle rejection.
- [x] Document which richer script schedules remain on the existing fallback path during this milestone.

**Verification:** the three reported regressions fail for the expected reasons, fixture-only tests pass, and no production behavior has changed.

### Stage 1 — Separate global action sync from primitive repeat sync

- [x] Track each non-sync loop's desired repeat-sync link in `LoopModel` rather than assuming every primitive always follows the sync loop.
- [x] Change `SetSync(false)` to clear backend sync sources for existing non-sync primitive loops and `SetSync(true)` to restore them, committing the global value only after all backend operations succeed.
- [x] Apply the current global policy when creating aligned rows, adding tracks, loading sessions, and rebuilding after an audio-driver switch.
- [x] Route `ControlOperation::SetRepeatSync` through the same per-loop state so script overrides remain observable and survive subsequent model/backend polling.
- [x] Add backend contract coverage proving an immediate transition with no sync source restarts at the primitive's own boundary while a linked loop waits for its source.

**Verification:** `disabling_sync_makes_a_primitive_loop_repeat_at_its_own_boundary` passes, synchronized transition tests remain green, and focused fake plus `EngineBackend` tests prove link/unlink operations.

### Stage 2 — Add a backend-neutral composite contract

- [x] Add stable `BackendCompositeId`, backend target/configuration types, and `BackendCompositeState` to `shoop_backend`.
- [x] Extend `Backend` with create, configure/replace, transition, option, remove, and state/snapshot operations for composites, with explicit errors for unsupported or stale configurations.
- [x] Extend `BackendSnapshot` with composite states and add fake operations/state sufficient for application unit tests and failure injection.
- [x] Extract the reusable registry preparation/validation needed to map backend primitive/composite IDs to engine identities and compile a candidate timeline transactionally.
- [x] Add shared backend contract tests for empty installation, three serial children, immediate start, boundary advancement, independent child triggering, reconfiguration, stale targets, cycles, and removal cleanup.

**Verification:** the contract tests pass for `FakeBackend` where deterministic simulation is sufficient and for the in-process engine implementation where realtime schedule behavior is required.

### Stage 3 — Implement engine and native composite adapters

- [x] Implement the contract in `EngineBackend` using the engine composite timeline, state mirrors, and a backend-owned composite registry.
- [x] Implement the same contract in `NativeBackend` using `BackendSession`/`CompositeLoop` handles, queued plan installation, command fences, and mirror polling.
- [x] Ensure topology replacement and plan reclamation stay off the realtime callback and that backend snapshots report independent parent state.
- [x] Cover native dummy-driver creation, configuration, immediate play, serial advancement, state polling, and teardown with focused integration tests.

**Verification:** shared composite backend tests pass against `EngineBackend` and native dummy backend; existing `shoop_engine` composite suites remain unchanged and green.

### Stage 4 — Bind application composites to backend composites

- [x] Add optional backend composite identity/ownership to `LoopModel` while retaining the primitive slot identity needed by tracks and session media capture.
- [x] On conversion, prepare/install an empty backend composite before committing the model conversion; clean up the prepared composite if clearing the primitive placeholder fails.
- [x] On serial/parallel append, build a candidate canonical document and backend configuration, install it first, then atomically commit `composite`, `script_composition`, length, and kind.
- [x] Recreate and configure backend composites after session load and driver replacement, using the replacement primitive IDs and preserving canonical documents.
- [x] Mark configurations dirty and safely recompile when source lengths or the sync length change; retain the last valid installed plan until replacement succeeds.
- [x] Route play, play-dry, record where supported, stop, clear, solo interaction, and play-after-record to backend composite controls for backend-owned regular composites.
- [x] Populate parent mode, pending transition, length, position, and progress from `BackendCompositeState`; remove child-derived state projection for backend-owned composites.
- [x] Keep the application timer path explicitly limited to non-backend-owned script schedules, and ensure it records playback ownership before projecting any parent state.

**Verification:** the two composite regression tests pass; existing conversion, persistence, script composition, selection, and snapshot tests remain green.

### Stage 5 — Carry composites through Web Audio

- [x] Add bounded wire types and commands for composite creation/configuration/control/removal, bump the protocol version, and add serialization/journal tests.
- [x] Execute those commands in `shoop_audio_worklet` through its `EngineBackend` and include composite states in `WireSnapshot`.
- [x] Add ID reservation, command submission, snapshot application, and error handling to `WebAudioBackend`.
- [x] Add worklet-host tests that install a three-child composite, start it, process exact quanta, observe child changes and parent position, and prove an independent child start leaves the parent stopped.

**Verification:** protocol round trips, audio-worklet tests, browser backend tests, and warning-denying Wasm builds pass.

### Stage 6 — Add straightforward full-stack regression coverage

- [x] Add a `CooperativeApplicationRuntime<EngineBackend>` test that starts a running sync loop, disables global sync through `AppIntent::Global`, plays a shorter primitive through `AppIntent::Loop`, advances exact frames, and observes an independent restart in the immutable `AppSnapshot`.
- [x] Add a runtime test that converts a slot, appends three sources through `ComposeLoopSerial` intents, plays it through the ordinary GUI `PlayClicked` intent, and observes parent `Playing`, first-child start, A-to-B-to-C boundary changes, parent progress, and regular wrap.
- [x] Add a runtime test that builds the same composition but plays only a child through `AppIntent::Loop`; assert the child advances while the parent stays `Stopped` at zero with no active composite playback.
- [x] Add a save/load continuation to the three-child test, proving the loaded composite is recreated in the backend and can play without an editor mutation after load.
- [x] Keep or add one headless egui routing test proving the basic editor's play control emits the ordinary stable-ID `LoopAction::PlayClicked`; leave playback semantics to the application full-stack tests.

**Verification:** all focused unit, backend-contract, worklet, egui-routing, and cooperative-runtime tests pass together with no ignored expected failures.

### Stage 7 — End-to-end validation

- [x] Run `cargo fmt --all -- --check`.
- [x] Run `RUSTFLAGS="-D warnings" cargo build --workspace`.
- [x] Run `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test --workspace --features shoop_engine/app_backend -- --test-threads=1`.
- [x] Run `python3 scripts/check_tracing_coverage.py --require-closed` and update the inventory only for intentional new control-path zones.
- [x] Run `RUSTFLAGS="-D warnings" cargo test --locked --no-default-features -p shoop_audio_protocol -p shoop_audio_worklet -p shoop_egui -p shoopdaloop`.
- [x] Run `RUSTFLAGS="-D warnings" cargo build --locked --no-default-features -p shoopdaloop --target wasm32-unknown-unknown` and `RUSTFLAGS="-D warnings" cargo build --locked -p shoop_audio_worklet --target wasm32-unknown-unknown`.
- [x] Review the final diff for duplicate composite compilers, child-derived parent state, GUI-thread timers, unbounded protocol payloads, stale-handle leaks, and unrelated changes.

**Verification:** every listed command is green, including all three original regressions and the new full-stack tests, with a clean intended worktree.

## Completion evidence

### Recorded baseline

Before production changes, the unchanged regressions failed with these observed values:

- `disabling_sync_makes_a_primitive_loop_repeat_at_its_own_boundary`: position `4`, expected `0`;
- `independently_playing_a_child_does_not_advance_its_composite`: parent `(Playing, 2)`, expected `(Stopped, 0)`;
- `gui_play_on_a_regular_composite_starts_the_composite_and_first_child`: `(Stopped, Stopped)`, expected `(Playing, Playing)`.

### Supported lowering and explicit fallback

The basic editor's regular subset is lowered exactly: each playlist is a timeline, serial sections remain serial, parallel events remain parallel, source lengths derive implicit regular durations, and zero frame delays remain zero. Script schedules are lowered only when all referenced loops resolve, every nonzero frame delay is an exact multiple of the sync-loop length, and modes are recognized without mixing implicit and explicit semantics. Unsupported richer schedules retain their canonical `CompositeDocument` and `script_composition` data and continue on the pre-existing application-owned script path; they are never rounded or silently rewritten.

### Verification record

The completed implementation is covered by the three unchanged regressions, fake sync-policy tests, a shared fake/in-process composite lifecycle contract, injected fake-backend reconfiguration failure, transactional in-process backend tests (empty, serial, parallel, aligned, delayed, stale, cycle, reconfigure, remove, and child isolation), native dummy-driver progression and teardown, protocol round-trip, worklet command/state/position, public-intent progression/isolation/wrap, session round-trip, driver replacement, and stable-ID egui routing tests. Final local validation ran every Stage 7 command successfully. On this Nix host, the two Wasm builds required making the installed Rust `wasm-ld` available under the `lld` name expected by the Nix Rust wrapper; no source or project configuration workaround was needed.

### Prompt-to-artifact completion audit

| Requirement | Concrete evidence |
| --- | --- |
| Goal regressions and acceptance 1 | The unchanged named tests in `shoop_app::tests` assert position `0`, isolated parent `(Stopped, 0)`, and composite/first-child `Playing`; the workspace run executes all three successfully. |
| Acceptance 2–3 and Stage 1 | `LoopModel::repeat_sync`, `GlobalControlAction::SetSync`, and `ControlOperation::SetRepeatSync` own repeat links independently of action delay. `latest_global_or_script_repeat_sync_policy_applies_to_existing_and_new_loops`, the original sync regression, the public-intent repeat test, and retained synchronized-transition suites cover creation, restoration, unlink/relink, latest-operation precedence, immediate behavior, and delayed behavior. |
| Acceptance 4–7 and Stages 2–4 | `BackendCompositeId`, `BackendCompositeConfig`, `BackendCompositeState`, and the `Backend` methods form the neutral contract. `EngineBackend`, `NativeBackend`, and opt-in `FakeBackend` implementations publish independent state. Shared fake/engine lifecycle tests plus `engine_backend_composite_contract_is_independent_and_transactional` cover empty, serial, parallel, immediate, delayed, aligned, stale, cycle, reconfigure, remove, and child isolation. `failed_backend_composite_reconfiguration_does_not_commit_application_schedule` proves application transactionality under injected backend failure. |
| Acceptance 8 | `restore_backend_composites` rebuilds identities from replacement primitive IDs. `public_intents_drive_three_section_engine_composite_and_isolate_child_control` continues through save/load and audio-driver replacement, then starts the restored composite without an editor mutation. |
| Acceptance 9 and Stage 5 | Native, engine, fake, and browser adapters implement the same trait surface. Protocol version 11 adds bounded composite commands/config/state; `shoop_audio_worklet` executes them through `EngineBackend`; `WebAudioBackend` reserves IDs, submits commands, and applies snapshots. Protocol and worklet contract tests pass, as do both CI WebAssembly jobs. |
| Acceptance 10 | `CompositeDocument` remains the session/editor authority. `backend_composite_config` returns fallback ownership for schedules that cannot be represented exactly; `rich_composite_survives_session_load_and_save_without_projection_loss` verifies delays, modes, playlists, and persistence are retained. |
| Acceptance 11 and Stage 6 | The two `public_intents_*` tests use `CooperativeApplicationRuntime` with a real `EngineBackend`, typed intents, exact frame advancement, and immutable snapshots. They prove running-sync unlink/repeat, advancing-child parent isolation with no active parent children, A→B→C progression, progress, wrap, save/load, and driver replacement. The egui response test proves stable-ID `PlayClicked` routing. |
| Acceptance 12 and Stage 7 | The exact formatting, warning-denying workspace build, serialized workspace test, tracing verifier, no-default-feature tests, and two warning-denying Wasm builds listed above all pass locally. PR CI passes Linux/macOS/Windows debug and release, WebAssembly debug and release, docs, and CodeQL. |
| Scope and design constraints | The diff adds no composite editing gesture or editor control; the only `shoop_egui` change is routing-test coverage. Public snapshots expose application IDs/state rather than engine handles. Backend-owned parents project only `BackendCompositeState`; application elapsed-time projection is filtered to non-backend-owned script schedules. Engine compilation and timeline installation remain off callback paths, with retained engine allocation/reclamation suites green. |
| Delivery gates | `LOOP_SYNC_AND_COMPOSITE_PLAYBACK_FIX_PLAN.md` has no unchecked item. The implementation and this audit are committed on `fix/composite-backend-integration`, pushed to its matching upstream, and represented by PR #728; completion requires that PR's head SHA to match the pushed branch and every reported check to be successful. |

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
