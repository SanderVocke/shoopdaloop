# Script Composite Track Auto-Arm Plan

## Goal

Add a default-enabled **Auto-arm track inputs** control to the top bar. While a script composite is running, the application enables input monitoring on tracks whose descendant loops are capturing external input, beginning during the preceding sync cycle and continuing through the end of capture. Monitoring that auto-arm enabled is restored afterward; monitoring that was already enabled remains enabled.

This is application-level, cycle-ahead orchestration. It does not require sample-exact switching or realtime engine automation.

## Scope

This work covers shared application state and reconciliation, script-composite schedule lookahead, nested composite descendants, top-bar UI, session persistence, tests, and user documentation. It applies to ordinary `Recording` and `Replacing`, which capture track input. `RecordingDryIntoWet` is excluded because it records existing dry media and has different routing semantics.

It does not add a Lua control API, alter composite transition/conflict semantics, change the existing exclusive-input policy, or add backend/worklet protocol fields solely for sample-exact timing.

## Immutable acceptance criteria

1. A top-bar auto-arm toggle exists, is enabled by default, and accurately reflects the application state.
2. The setting is saved and loaded with sessions. A session without the new field loads with auto-arm enabled.
3. For each running script composite, every input-capable track containing a primitive descendant scheduled for `Recording` or `Replacing` is monitored during the immediately preceding composite iteration and throughout the capture interval.
4. A synchronized script composite pending at its start boundary arms iteration-zero capture as soon as the pending transition is observed. An immediate start, seek, or other case with no observable preceding cycle arms at the first application update that observes the demand.
5. Cycle-level application polling is sufficient: monitoring need not switch at an exact sample or callback boundary.
6. Current and next-iteration demand is derived only from active or imminently starting script-composite execution, not from stopped composite definitions or ordinary direct/regular-composite recording.
7. Nested composite descendants are handled, including regular composites put into an external-input recording mode by a script composite and nested script composites with explicit recording entries.
8. Demand is aggregated by track across loops and composites. A track remains monitored until all auto-arm demand for it has ended, including adjacent or overlapping capture windows.
9. Auto-arm records ownership only when it changes a track from muted to monitored. When demand ends, auto-arm mutes only tracks it owns; tracks already monitored before acquisition remain monitored.
10. Disabling auto-arm immediately releases all owned tracks, while leaving pre-existing monitored tracks unchanged. Stopping or completing composites, removing tracks, replacing/loading a session, and changing schedules cannot leave stale ownership behind.
11. Auto-arm monitoring changes do not invoke or enforce the existing `auto_mute_other_track_inputs` policy. That policy remains independently controlled and retains its current behavior for explicit respecting requests.
12. The implementation behaves consistently through the shared application layer on native and browser backends and does not introduce realtime allocation or engine callback work.
13. Focused application, session, and UI tests pass, followed by the complete project validation gates.

## Design rules and constraints

- Treat auto-arm as a demand/ownership reconciler, not as a persisted per-track arm flag.
- Compute one `BTreeSet<TrackId>` of demanded tracks each update, then reconcile it against one `BTreeSet<TrackId>` of tracks currently owned by auto-arm.
- Acquire ownership only after finding an eligible demanded track currently has input monitoring disabled. Tracks already enabled are demand participants but are never added to the ownership set.
- Release ownership only after demand disappears or auto-arm is disabled. Continue to ensure owned demanded tracks are enabled; an explicit mute during an active demand may therefore be re-enabled on the next reconciliation.
- Use non-respecting monitoring mutations so auto-arm never mutes unrelated tracks through the exclusive-input policy.
- Base lookahead on composite iterations and plans, not elapsed wall-clock time or guessed callback timing.
- Match existing composite duration rules: explicit positive `n_cycles`, otherwise the child length rounded up to sync cycles with a minimum duration of one cycle. Preserve deterministic occurrence/overlap behavior.
- Use observed active-child/runtime state for current execution and schedule evaluation for the next boundary. Recursion must be cycle-safe even though persisted composite graphs are validated as acyclic.
- Do not arm channel-free tracks or modes that do not capture external input.
- Keep schedule prediction and ownership reconciliation as separate, directly testable helpers. Do not duplicate backend command submission or optimistic-control bookkeeping outside the existing track-control path.
- Backend rejection and stale-track handling must use existing error reporting and leave the next update able to converge; no failure may retain an invalid track ID indefinitely.
- New-session defaults, missing-field deserialization defaults, and the UI default must agree.
- Avoid unrelated changes to composite compilation, Lua API compatibility, track routing, or session format versioning.

## Stage 1 — State, persistence, and UI contract

- [x] Add a clearly named auto-arm boolean to `GlobalControlState`, defaulting to `true`, and add the corresponding typed `GlobalControlAction`.
- [x] Handle the action in the application model without changing current monitoring merely because the setting is enabled; disabling will be connected to ownership release in Stage 3.
- [x] Add the persisted global session field with an explicit enabled default for missing documents, and wire it through save and load.
- [x] Update session fixtures and exhaustive state literals affected by the new field.
- [x] Add a top-bar toggle with enabled/disabled styling, concise tooltip text, typed action dispatch, and test hitbox coverage consistent with adjacent controls.
- [x] Add tests proving the runtime default, missing-session-field default, save/load round trip, and UI toggle action/state.

Verification:

- [x] `cargo nextest run -p shoop_session`
- [x] `cargo nextest run -p shoop_egui`
- [x] Targeted `shoop_app` global-state and session round-trip tests pass.

## Stage 2 — Script-composite capture lookahead

- [x] Add a pure application-side planner that identifies externally capturing primitive descendants for a composite at the current iteration and at the next boundary.
- [x] Reuse the application composite documents, loop-to-track mapping, loop lengths, sync length, observed composite mode/iteration, pending transition delay, and active-child observations; do not add realtime backend automation.
- [x] Implement effective-mode propagation through nested regular and script composites, including regular first-recording behavior where applicable.
- [x] Distinguish `Recording` and `Replacing` from `RecordingDryIntoWet`, playback, stopped, and unknown modes.
- [x] Detect imminent iteration-zero capture for synchronized pending starts, and define first-observed demand for immediate starts/seeks where a preceding cycle is unavailable.
- [x] Aggregate all active script-composite roots into a deduplicated demanded-track set, while excluding stopped definitions and tracks without inputs.
- [x] Add focused planner tests for explicit/default durations, capture starting at iteration zero and later iterations, the preceding-cycle window, end boundaries, adjacent windows, overlapping entries, multiple loops on one track, multiple script roots, pending starts, early stop, seek, nested regular/script descendants, and excluded modes.
- [x] Add regression tests comparing planner expectations with observed backend composite advancement at representative boundaries so application lookahead cannot silently drift from composite semantics.

Verification:

- [x] Targeted `shoop_app` composite planner and positioned-composite tests pass.
- [x] Existing `shoop_engine` composite state-machine/timeline tests remain unchanged; no engine semantic helper was changed.

## Stage 3 — Monitoring ownership and reconciliation

- [x] Add private application-model ownership state for tracks auto-arm changed from muted to monitored; initialize and clear it at lifecycle boundaries.
- [x] Reconcile auto-arm after each successful backend snapshot, before publishing application state, using the demanded-track set from Stage 2.
- [x] Route acquire/release operations through the existing backend track-control and optimistic desired-control machinery, with `respect_auto_mute = false` behavior.
- [x] Acquire all demanded muted tracks without taking ownership of demanded tracks already monitored.
- [x] Keep owned tracks enabled while any demand remains, and release/mute them only when their aggregate demand reaches zero.
- [x] On toggle-off, restore owned tracks immediately and prevent new acquisition; on re-enable, acquire current demand on the next reconciliation.
- [x] Prune removed/stale tracks and clear ownership safely during new-session/session-replacement paths and backend structure changes.
- [x] Preserve convergence when an enable/disable command is rejected or a track disappears, using existing mutation-failure reporting rather than introducing a separate retry loop.
- [x] Add application tests covering sequential tracks, overlapping roots, adjacent windows, pre-monitored tracks, mixed pre-monitored/owned tracks, toggle-off/on, stop-all/early stop, schedule replacement, track removal, session replacement, backend rejection, and independence from exclusive-input auto-mute.
- [x] Verify published track controls show the optimistic auto-arm state without waiting for an extra backend poll.

Verification:

- [x] `cargo nextest run -p shoop_app`
- [x] Targeted engine-backend and Wasm-compatible application tests exercise the shared reconciliation behavior.

## Stage 4 — Documentation and integration coverage

- [x] Document auto-arm beside input monitoring and the exclusive-input control in `docs/source/usage.trackcontrols.rst`.
- [x] State that it is default-on, operates one composite cycle ahead, restores only monitoring it enabled, supports simultaneous demanded tracks, and is not sample-exact.
- [x] Update session-format documentation for the persisted global setting and missing-field default.
- [x] Update concise application README text where top-bar/input-monitoring behavior is summarized.
- [x] Review terminology so **Auto-arm track inputs** is not confused with exclusive input, loop recording mode, or backend recording preparation.
- [x] No browser smoke assertion was added: the smoke harness has no script-composite editor flow, while the shared application and UI behavior is covered directly on native and Node Wasm.

Verification:

- [x] Documentation references and UI tooltip agree with the acceptance criteria.
- [x] Relevant `shoop_app` auto-arm and `shoop_egui` global-control tests pass in the Node Wasm harness.

## Stage 5 — End-to-end validation

Run commands in the environment selected by `.agents/info/build.md`; on Nix/NixOS, enter the repository development shell first.

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build --workspace`.
- [ ] Run `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`.
- [ ] Run `python3 scripts/check_tracing_coverage.py --require-closed`.
- [ ] Build `shoopdaloop` and `shoop_audio_worklet` for `wasm32-unknown-unknown` and run the documented browser smoke checks when a supported browser is available; record an explicit skip reason otherwise.
- [ ] Manually verify a script composite that records sequentially across at least two tracks: each muted track enables during its preceding cycle, remains enabled through recording, and returns muted afterward.
- [ ] In the same scenario, verify a track monitored before composite start remains monitored after completion, and overlapping recordings keep all demanded tracks monitored.
- [ ] Verify toggling auto-arm off during execution restores only owned tracks and that re-enabling reacquires ongoing/upcoming demand.
- [ ] Confirm no engine/worklet protocol or realtime callback behavior changed unless implementation evidence required a documented design-rule revision.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
