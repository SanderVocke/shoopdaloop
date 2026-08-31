# Unsynchronized Primitive Loop Repeat Plan

## Goal

Make a primitive loop that enters a playback mode through a `Loop_DontWaitForSync` transition repeat at its own loop boundary instead of waiting for its configured sync source after the first iteration.

## Scope

### In scope

- Primitive loop playback in all modes for which `LoopMode::is_playing_mode()` is true:
  - `Playing`
  - `Replacing`
  - `PlayingDryThroughWet`
  - `RecordingDryIntoWet`
- Native and browser/worklet behavior shared through the Rust engine.
- Explicit `loop_set_repeat_sync()` overrides.
- Application-side repeat-policy bookkeeping for primitive scripting transitions.
- Focused engine and application regression coverage.

### Out of scope

- Composite-loop repeat semantics.
- Changes to backend traits, browser/worklet protocols, session formats, or public Lua function signatures.
- Changing the timing semantics of stopped or plain `Recording` transitions.

## Immutable acceptance criteria

1. With an active sync source whose cycle is longer than a primitive loop, starting that primitive in a playing mode with `cycles_delay == None` makes it wrap at its own boundary before the sync source wraps.
2. The primitive remains in its requested playing mode across that independent wrap.
3. A playing transition with a normal cycle delay retains synchronized-repeat behavior: at its own end, the primitive waits for the active sync source.
4. A later synchronized playing transition can restore synchronized repeating after an immediate playing transition.
5. `loop_set_repeat_sync(true)` restores synchronized repeating, and `loop_set_repeat_sync(false)` restores independent repeating; the latest applied playing-transition policy or explicit setter wins.
6. Stop, `Recording`, and other non-playing transitions do not silently alter the latched repeat policy.
7. Composite-loop behavior remains unchanged.
8. Existing Rust tests, warning-denying builds, formatting checks, and project test-usage checks pass.

## Design rules and constraints

- Represent the behavior as primitive-loop state in `BasicLoop`; do not implement it by automatically detaching the primitive from its sync source in the application layer.
- Keep the sync-source relationship available for normal trigger propagation and later explicit synchronization changes.
- Use a default-false latch such as `repeat_unsynced`, preserving current synchronized behavior whenever a sync source exists and no immediate playing transition has selected independent repeating.
- Change the latch only when a playing transition is applied or an explicit repeat-sync policy is configured. Non-playing transitions must leave it unchanged.
- An applied playing transition with `n_cycles_delay == None` selects independent repeating. An applied playing transition with a cycle delay selects synchronized repeating.
- Update the policy when a queued transition executes, not prematurely when it is merely queued.
- Sync-source snapshot refreshes during processing must not overwrite the latch. Only explicit sync/repeat configuration may do so.
- Preserve the existing behavior that a loop with no active playing sync source wraps independently regardless of the latch.
- Keep `LoopModel.repeat_sync` consistent with accepted primitive scripting transitions without applying the new policy to backend composites.
- Avoid protocol or persistence changes unless implementation evidence proves one is unavoidable; if that occurs, stop and revise the plan before broadening scope.

## Stage 1: Engine repeat-policy latch

- [x] Add focused failing tests in `src/rust/shoop_engine/src/basic_loop.rs` that establish:
  - an immediate playing transition wraps at the primitive's own end while its sync source is still playing and has not triggered;
  - a synchronized playing transition still waits at the primitive's end;
  - a later synchronized playing transition restores waiting after an immediate playing transition;
  - non-playing transitions do not change the selected repeat policy.
- [x] Add the default-false independent-repeat latch to `BasicLoop`.
- [x] Centralize application of planned transitions so entering any `is_playing_mode()` updates the latch from that transition's wait policy before changing mode.
- [x] Set independent repeating for immediately applied transitions whose cycle delay is `None`.
- [x] Set synchronized repeating when a queued playing transition actually executes.
- [x] Update loop-end handling to self-trigger when independent repeating is latched, while retaining the existing no-source/inactive-source behavior.
- [x] Add a narrow engine setter for explicit repeat-sync policy changes; do not expose the latch in snapshots unless tests demonstrate a consumer requirement.
- [x] Run the targeted engine tests in the development environment selected by `.agents/info/build.md`.
- [x] Commit the completed engine behavior and unit tests as one meaningful milestone.

### Stage 1 verification

- The immediate-start regression test fails before the implementation and passes afterward.
- The synchronized control case proves the change does not make every primitive repeat independently.
- Existing `BasicLoop` tests pass.

Completed evidence: `cargo test -p shoop_engine basic_loop::tests` passed 20 focused tests; `cargo fmt --all -- --check`, the project test-usage checker, and a workspace warning-denying build also passed before the milestone commit.

## Stage 2: Explicit policy and application integration

Depends on Stage 1.

- [x] Update `Session::set_loop_sync_source()` so an explicit source configuration applies the corresponding repeat policy exactly once at configuration time.
- [x] Ensure per-processing-cycle sync-source snapshot refreshes only refresh source state and cannot reset an immediate transition's independent-repeat latch.
- [x] In primitive handling for `ControlOperation::Transition`, update `LoopModel.repeat_sync` after an accepted playing transition according to whether `cycles_delay` is present.
- [x] Leave `LoopModel.repeat_sync` unchanged for stop, plain recording, and composite transitions.
- [x] Preserve `ControlOperation::SetRepeatSync` as the explicit override path and verify that application metadata and engine behavior agree after both `true` and `false` operations.
- [x] Add an application integration test using the real in-process engine backend:
  - configure a playing sync loop longer than the primitive;
  - keep the primitive configured with that sync source;
  - apply a primitive scripting transition with `cycles_delay: None`;
  - advance exactly one primitive length;
  - assert the primitive is still playing at position zero before the sync loop wraps.
- [x] Extend the integration coverage to verify a subsequent synchronized start and explicit repeat-sync overrides.
- [x] Add a negative assertion or focused test showing a composite transition does not receive primitive repeat-policy bookkeeping.
- [x] Run targeted `shoop_engine` and `shoop_app` tests.
- [x] Run `python3 scripts/check_shoop_test_usage.py` because Rust tests changed.
- [x] Commit session/application integration and regression coverage as one meaningful milestone.

### Stage 2 verification

- The end-to-end application test reproduces the reported timing relationship rather than only inspecting internal flags.
- Repeat-policy metadata agrees with observed primitive behavior.
- Explicit setters override the transition-derived policy when applied later.
- Composite tests retain their previous behavior.

Completed evidence: `cargo nextest run -p shoop_engine -p shoop_app` passed all 1,015 package tests, including the real-backend timing and composite-negative regressions. Formatting, the test-usage checker, and a workspace warning-denying build passed before the milestone commit.

## Stage 3: Contract documentation and focused review

Depends on Stages 1 and 2.

- [x] Clarify the Lua compatibility documentation: `Loop_DontWaitForSync` on a primitive playing transition also selects independent repeating until superseded by a later playing transition or explicit repeat-sync setting.
- [x] Review the implementation for all playing modes, same-mode playing transitions, aligned immediate transitions, inactive sync sources, and zero-length handling.
- [x] Confirm no backend trait, worklet protocol, or session-format changes were introduced.
- [x] Run formatting and the targeted tests again.
- [x] Commit documentation and any review-driven cleanup as a meaningful milestone.

### Stage 3 verification

- Documentation matches tested precedence and lifetime semantics.
- The diff remains limited to primitive engine mechanics, application bookkeeping/tests, and relevant documentation.

Completed evidence: focused tests cover every playing mode plus same-mode and aligned immediate transitions; existing zero-length and inactive/no-source behavior remains passing. The refined application test applies both explicit repeat-policy overrides while playback is active. No backend trait, protocol, or session-format file changed. Formatting, focused tests, the test-usage checker, and the warning-denying workspace build passed before the milestone commit.

## Final end-to-end validation

Run all commands in the environment selected by `.agents/info/build.md`; on Nix/NixOS, enter the repository development shell first.

- [ ] `cargo fmt --all -- --check`
- [ ] `RUSTFLAGS="-D warnings" cargo build --workspace`
- [ ] `python3 scripts/check_shoop_test_usage.py`
- [ ] `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`
- [ ] `python3 scripts/check_tracing_coverage.py --require-closed`
- [ ] Review the final diff and test output against every immutable acceptance criterion.
- [ ] Confirm `git status` contains only intended source, test, documentation, and plan updates.
- [ ] Commit the final validated state if validation required any follow-up changes.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
