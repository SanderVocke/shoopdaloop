# Targeted loop content and length updates plan

## Pre-implementation evidence and swap audit

The application currently has no backend operation for replacing one loop's audio/MIDI content or changing only its logical length. It therefore edits a captured `BackendSessionData` and calls `Backend::replace_session`, even though the engine already has prepared audio/MIDI storage and callback-boundary commit primitives.

Current application uses of whole-session replacement classify as follows:

- **Unnecessary and in scope:** staged exact/WAV audio import, staged exact/standard MIDI import, generated audio click tracks, generated MIDI click tracks, the synchronous audio-import helper, the synchronous MIDI-import helper, and every optional loop-length update bundled with those content operations.
- **Necessary and retained:** loading a different session document and switching/recovering an audio driver. Those operations intentionally replace topology/runtime and continue using session replacement and backend-ID remapping.
- **Capture-only continuity cases:** session save, loop-audio export, and loop-MIDI export capture data but do not replace the backend. They need regression coverage proving capture/export does not disturb playback, synchronization, callbacks, session identity, or graph state. Replacing their whole-session reads with narrower read APIs is an optimization outside this plan unless tests expose a continuity or correctness defect.
- **No current standalone application path:** a loop-length-only edit is not presently exposed through `Backend`; length changes occur during content import or session restoration. This plan adds and verifies the backend primitive so future/details editing cannot fall back to session replacement.

The current engine boundary already supports preparing `PreparedAudioChannelData`, `PreparedMidiChannelData`, and content snapshots off the realtime thread, swapping them in a control command, and returning displaced storage for destruction off the realtime thread. `BasicLoop::set_length` preserves mode and content, but currently clamps an out-of-range position to `length - 1`; the required behavior is modulo the new non-zero length.

## Goals and scope

Replace loop-local media and length mutations without replacing the backend session, audio driver, objects, or graph. Commit bounded work at the next realtime iteration, preserve continuity for all unrelated loops, and keep preparation, allocation, locking, and reclamation off the realtime thread.

In scope:

- targeted atomic audio and MIDI content replacement for an existing primitive loop;
- optional length changes committed atomically with content replacement;
- standalone length-only updates;
- native, direct engine/dummy, fake, browser proxy/protocol/worklet backend implementations;
- migration of every unnecessary replacement listed above;
- test-first continuity, atomicity, graph, callback, save/export, and realtime-safety coverage.

Out of scope:

- seamless hot replacement within an actively playing target loop; a content replacement may stop that target at the next realtime iteration;
- changing session-load or audio-driver-switch replacement behavior;
- adding new GUI length-editing controls;
- changing session file formats;
- optimizing valid whole-session capture used by save/export when continuity and correctness already pass.

## Immutable acceptance criteria

1. Audio import, MIDI import, generated audio clicks, generated MIDI clicks, and their synchronous/helper variants never call `capture_session`, `replace_session`, `switch_audio_driver`, or backend-entity remapping to mutate one loop.
2. A content replacement validates and prepares the complete update before submission, then makes every affected audio/MIDI channel and optional loop length visible together at one realtime iteration. Observers can see either all old content or all new content, never a partial multi-channel update.
3. Content replacement of a stopped or playing target may commit the target to `Stopped` and clear its pending transitions. It must not reset/recreate any backend object. Loading into a recording/replacing target is rejected without changing content or transport state.
4. Content replacement preserves the target's sync-source relationship, gain/balance and unaffected channel settings, and preserves every unrelated loop's mode, position, content, sync source, and pending transitions.
5. A standalone length-only update commits at the next realtime iteration without changing channel content, loop mode, sync source, or pending transitions. A playing loop keeps playing.
6. Lengthening preserves position and makes the existing engine behavior for content followed by silence remain valid. Shortening to a non-zero length changes an out-of-range position to `old_position % new_length`; shortening to zero sets position to zero. An already in-range position is unchanged.
7. Targeted content and length operations preserve the audio driver and engine/backend session identity. No track, loop, port, channel, processor, or external-connection identity is remapped.
8. Audio callbacks continue monotonically through content load, length update, session save, loop-audio export, and loop-MIDI export. The tested operation must not restart/reset callback accounting or introduce a driver/session reconstruction gap.
9. Content and length updates are non-topological: graph request/applied generations and graph-apply counts do not change as a result of the update.
10. Multi-channel loads preserve channel ordering and commit all channels on the same iteration; MIDI start state, event ordering/timestamps, channel length, offsets, and preplay follow the requested update without changing unspecified channel properties.
11. Session save and loop audio/MIDI export return the expected content while preserving session identity, callback progress, graph state, playback mode/position progression, and synchronization.
12. All payload allocation, conversion, validation, snapshot preparation, and old-storage destruction occur off the realtime thread. The realtime commit is bounded by affected channel count rather than sample/event count, takes no project-owned lock, and allocates or frees no memory.
13. Failure before or during submission is atomic: old content and length remain usable, transport/session/graph state is unchanged, and no partial prepared update is leaked or destroyed on the realtime thread.
14. Shared backend/application contracts pass for fake, direct engine/dummy, native threaded, and browser worklet paths, while actual session loading and driver switching retain their existing replacement behavior.

## Design rules and constraints

- Treat media and logical length as loop-local state, never as topology or audio-driver configuration.
- Add explicit backend operations for targeted content replacement and length-only updates. Defaults must report unsupported behavior rather than silently falling back to whole-session replacement.
- Represent an update as a patch over identified existing channels. Unspecified content/configuration remains unchanged; application code must not capture a session merely to preserve fields it is not changing.
- For native threaded operation, prepare all channel storage and content snapshots on the control side and submit one engine control command. Swap prepared storage in that command and return displaced storage through the existing bounded command/reclamation path.
- Do not queue one independently observable command per channel. The loop mode change, all channel swaps, and optional length change form one commit.
- Content replacement uses a deterministic stopped-target result rather than boundary-aware hot replacement. Preserve the existing MIDI stop/all-notes-off behavior.
- Length-only updates do not stop playback or modify channel storage. Implement modulo normalization in the core loop so every backend receives identical semantics.
- Keep content operations off the topology command path; do not arm or apply a graph rebuild.
- Keep bulk decoding, resampling, click generation, serialization, and browser transfer assembly outside the realtime commit. Browser messages may be chunked, but only one complete generation-checked update may commit.
- Validate target identity, channel shape/modes, lengths, MIDI messages, queue capacity, and recording/replacing state before mutation. Once accepted by the realtime command, commit should be infallible or return a bounded acknowledgement without partial state.
- Preserve `BackendSessionData` and `BackendSessionReplacement` for persistence, actual session load, and driver switching; do not expand persistence data with live mode/sync state as a workaround.
- Use test-only/private diagnostics for session identity and graph generations where possible rather than exposing engine implementation IDs in product snapshots.
- A test-first red state may be run and recorded locally, but must not be committed as a default-test-breaking branch state. Commit the new tests with the first implementation milestone that makes their relevant subset green.

## Staged implementation

Dependencies are sequential unless stated otherwise. Complete and verify each stage before beginning its dependent stage.

### Stage 0 — Freeze the inventory and operation contracts

- [ ] Confirm every application and protocol call to `capture_session`, `replace_session`, and `switch_audio_driver`; update the audit above if another loop-local replacement is found.
- [ ] Define backend-neutral content patch, channel selector/index, optional length, commit generation/result, and failure semantics without exposing engine prepared-storage types.
- [ ] Define deterministic transport semantics: content replacement stops only the target and clears its plans; recording/replacing rejects; length-only mutation preserves mode/plans and normalizes position only when required.
- [ ] Define test diagnostics for stable native `BackendSession::session_id`, callback count, and graph request/applied/apply counters without adding product-facing identity coupling.

Verification:

- [ ] Contract/unit tests compile for unsupported/default, fake, direct-engine, native, and browser proxy implementations before behavioral assertions are enabled.
- [ ] The audit explicitly distinguishes retained session/driver replacement and capture-only save/export paths.
- [ ] Commit the contract and fixture milestone.

### Stage 1 — Write the failing continuity and atomicity tests first

- [ ] Add native/backend tests which load audio and MIDI into an existing synchronized session and initially expose session-ID replacement, callback disruption, lost sync/mode state, and unnecessary graph reconstruction.
- [ ] Add an application regression reproducing the traced workflow: play the sync loop, add a synchronized multi-channel/MIDI track, load/generated-click content, and assert that only the target may stop while the sync loop, identities, and synchronization remain continuous.
- [ ] Add old-before-commit/new-after-one-iteration atomicity tests for stereo/multi-channel audio and MIDI plus optional length, including failure/queue-full cases with no partial mutation.
- [ ] Add length-only tests for playing/stopped loops, unchanged content and mode, lengthening into silence, in-range position preservation, modulo shortening, and zero length.
- [ ] Add session-save, loop-audio-export, and loop-MIDI-export continuity tests covering exact returned data, stable session ID, monotonically advancing callbacks, unchanged graph generations, and preserved sync/playback.
- [ ] Add assertions that actual session load and audio-driver switch still replace/remap as designed so later broad call-site removal cannot break them.
- [ ] Run the new targeted tests against the baseline and record the expected failures before implementation. Keep this red state local and combine the tests with the first green implementation commit.

Verification:

- [ ] Each required invariant has a focused assertion that fails for the intended baseline reason, not because of timing, unavailable hardware, or unrelated setup.
- [ ] Deterministic dummy/test drivers are used for callback and graph assertions; no physical audio service is required.

### Stage 2 — Implement core atomic content commit and length semantics

- [ ] Add a loop-level prepared-content bundle which owns every prepared audio/MIDI channel update, prepared snapshots, optional new length, and acknowledgement/reclamation state.
- [ ] Prepare/copy payloads and reserve snapshot publication entirely on the control side.
- [ ] Commit the bundle in one non-topological engine control command: validate the ready loop, stop/clear plans for content replacement, swap every channel, apply optional length, and return displaced storage off realtime.
- [ ] Ensure rejected, stale, queue-full, and disconnected commands cancel prepared snapshots and reclaim payloads off realtime without changing the target.
- [ ] Add a length-only control operation and change core shortening behavior from `length - 1` clamping to modulo normalization, preserving mode/content/plans.
- [ ] Preserve MIDI stop-state handling and all unspecified audio/MIDI channel properties.

Verification:

- [ ] Engine tests prove one-iteration all-or-nothing commits and length behavior for audio-only, MIDI-only, and mixed/multi-channel loops.
- [ ] `assert_no_alloc` and realtime lock-guard tests execute the command application and following process iteration for small and large payloads.
- [ ] Callback cost/operation count is independent of payload length, and graph request/applied/apply counters remain unchanged.
- [ ] Commit the green core-engine/test milestone, including the Stage 1 tests now satisfied at this layer.

### Stage 3 — Implement every backend and browser transport

- [ ] Add `Backend` targeted content and length methods with explicit unsupported defaults; implement faithful state and operation recording in `FakeBackend`.
- [ ] Implement direct `EngineBackend` mutation between processing iterations using the same atomic semantics and without rebuilding `Session`.
- [ ] Implement `NativeBackend` by resolving the existing `NativeLoop` handles and using the prepared loop-level application-backend command; wait/poll only for its bounded commit acknowledgement, never for driver reconstruction.
- [ ] Add generation-checked, size-bounded browser protocol commands/events for content updates and length-only changes. Reuse chunked transfer where needed, assemble/validate fully before commit, and never route through session-replace messages.
- [ ] Implement browser proxy and AudioWorklet handling, preserving command/event queue bounds and waveform/content revision invalidation.
- [ ] Ensure all implementations reject recording/replacing targets atomically and preserve sync-source/object mappings.

Verification:

- [ ] Run one shared backend contract against fake, direct engine/dummy, and native threaded implementations for audio, MIDI, mixed channels, stopping, sync preservation, length semantics, failures, callbacks, and no graph update.
- [ ] Protocol/worklet tests prove chunk ordering, stale generation rejection, queue saturation recovery, one complete commit, no `BeginSessionReplace` traffic, and continued callback progress.
- [ ] Production Wasm checks compile the updated protocol, worklet, and browser proxy.
- [ ] Commit the cross-backend/protocol milestone.

### Stage 4 — Migrate all loop media application paths

- [ ] Replace session-capture mutation in staged audio import with a direct multi-channel content patch and optional length.
- [ ] Replace session-capture mutation in staged MIDI import with a direct MIDI patch and optional length.
- [ ] Migrate generated audio and MIDI click-track commits to the same targeted operation.
- [ ] Migrate or remove the synchronous/dead-code audio and MIDI import helpers so no alternative path retains whole-session replacement.
- [ ] Remove `BackendSessionData` and backend-ID remapping from `PendingIo::CommitLoopImport`; keep remapping only for actual session load and driver switching.
- [ ] Update application model length/empty/waveform revisions only after successful targeted acknowledgement, and retain the previous model state after rejection.
- [ ] Reject content load/generation against a recording/replacing target before mutation; loading a playing target may complete stopped without affecting other loops.
- [ ] Re-run the swap audit and add a source-level guard test or narrowly scoped scan preventing loop import/click code from calling session replacement.

Verification:

- [ ] Application tests cover exact/WAV audio, exact/standard MIDI, generated audio/MIDI clicks, with and without length updates, on direct and dry/wet multi-channel tracks.
- [ ] The traced workflow regression proves stable session ID, uninterrupted sync-loop callback/playback progress, retained follower sync links, target-only stopping, and no graph update.
- [ ] Session document load and audio-driver switching tests still exercise the retained full replacement/remapping path.
- [ ] Commit the application migration milestone.

### Stage 5 — Complete continuity, persistence, and realtime verification

- [ ] Turn all Stage 1 continuity tests green across applicable backends and remove any temporary ignore/red-test scaffolding.
- [ ] Verify session save and loop audio/MIDI export while loops are playing; assert exact content and no transport/session/graph mutation before and after completion.
- [ ] Verify load-then-save round trips for every supported audio/MIDI format and multi-channel topology.
- [ ] Stress repeated large content swaps and length changes while unrelated loops run; verify monotonic callbacks, no graph generations, no command overflow/leak, and off-realtime reclamation.
- [ ] Add Tracy zones/fields only if existing bounded engine-control instrumentation cannot distinguish prepare, queue, commit, and acknowledgement without dynamic names or payload recording.
- [ ] Update developer/design documentation to state that loop content/length changes are non-topological and session replacement is reserved for session/driver operations.

Verification:

- [ ] Targeted package tests pass with realtime allocation and lock guards enabled.
- [ ] A captured deterministic workflow shows one bounded content command at the next callback, no driver/session creation, no graph rebuild, and continuous unrelated-loop callbacks.
- [ ] Commit the continuity and documentation milestone.

### Stage 6 — Final end-to-end validation

- [ ] Run `cargo fmt --all -- --check` and `git diff --check`.
- [ ] Run focused engine/backend/application/protocol/worklet/runner tests while iterating.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build --workspace --features shoop_engine/app_backend`.
- [ ] Run `cargo test --workspace --features shoop_engine/app_backend`.
- [ ] Build first, then run `target/debug/shoopdaloop_dev.sh --self-test`.
- [ ] Run the production native and Wasm/browser checks used by the egui workflow, including AudioWorklet build and browser protocol tests.
- [ ] Exercise end to end: keep a sync loop playing; load stereo audio, MIDI, and generated clicks into synchronized followers; perform standalone lengthen/shorten operations; save/export content; verify target-only stopping, modulo position, exact media, stable session/driver identity, continuous callbacks, unchanged graph generations, and retained sync behavior.
- [ ] Record exact test commands, counts, trace evidence, environment skips, and residual limitations in this plan.
- [ ] Commit the final validation/documentation milestone.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
