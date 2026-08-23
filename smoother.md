# Loop-edge discontinuity smoother implementation plan

## Goals and scope

Implement a causal, per-audio-channel discontinuity smoother for loop playback. It must suppress abrupt sample-value steps when playback starts, stops, wraps, or otherwise jumps to a non-contiguous source position, without adding a full crossfade, changing loop timing, or modifying recorded content.

Expose one persistent application setting, **Loop edge smoothing**, expressed as whole milliseconds. The default is **3 ms**; **0 ms disables smoothing**. The setting must be available in the settings dialog and work in both native and browser audio backends. Runtime propagation may be asynchronous and may take up to two loop cycles; it does not require a synchronous hard-real-time update.

## Immutable acceptance criteria

1. With smoothing set to `0 ms`, loop playback uses the existing additive copy behavior and is sample-identical to unsmoothed playback.
2. With smoothing enabled, each detected playback discontinuity starts from the channel contribution rendered immediately before the edge and converges to the new raw contribution over the configured duration. This covers:
   - silence to playback at loop start;
   - playback to silence at stop or at the end of available channel content;
   - loop-tail to loop-head wrapping, including a wrap at a callback boundary;
   - non-contiguous playback source jumps such as seeks or offset changes.
3. Ordinary waveform progression, contiguous chunk boundaries, contiguous callback boundaries, recording, replacing, MIDI, and unrelated sources already present in an additive output buffer are not smoothed or otherwise altered.
4. A stop can emit only a bounded correction tail; it must not continue reading loop content, delay the loop state transition, or change transport position.
5. The configured duration is converted from milliseconds using the active sample rate. Existing channels, future channels, session replacement, and native audio-driver replacement all retain the active global setting.
6. The setting is persisted as an application setting rather than session content, defaults to `3 ms`, accepts `0 ms` as off, and can be changed without restarting the application. A saved change reaches the audio backend within two loop cycles.
7. Native, local/dummy, Web Audio/worklet, and fake/test backend paths implement the same control contract. Remote worklet reconnection replays the most recent value.
8. The audio processing path remains allocation-free and lock-free for steady-state smoothing and does not add graph sub-blocks or points of interest.
9. The implementation is a discontinuity correction smoother only: no overlapping playback regions, lookahead crossfade, destructive edge editing, or loop-length adjustment is introduced.

## Design rules and constraints

- Implement smoothing on each `AudioChannel` contribution before it is added to the destination port. Never smooth the already-mixed destination, because it can contain unrelated sources.
- Use the existing deferred playback command timeline. Treat uncovered destination ranges as raw silence so that stop corrections can finish even when no playback command is queued.
- Track whether raw playback was active, the expected next source position, previous gain, the previous rendered channel contribution, and a bounded correction ramp. A contiguous source command with unchanged gain is not an edge even when it crosses a storage-chunk or callback boundary.
- At an edge, set the correction so `new_raw + correction` equals the previous rendered contribution. Decay the correction linearly to exactly zero over the configured frame count. A later edge restarts the ramp from the then-current rendered contribution so overlapping edge conditions remain continuous and bounded.
- A nonzero duration that rounds below one frame uses one frame. Use checked/saturating integer conversion from milliseconds and sample rate.
- Disabling smoothing stops creating new corrections but allows an already-running correction to finish, avoiding a new click caused by changing the setting. Duration changes apply to subsequent edges; this satisfies the permitted delayed propagation.
- Reconfiguration must establish or preserve continuity tracking without treating the configuration command itself as a playback edge.
- Keep the low-level duration representation in frames for deterministic DSP tests. Let `Session` own the millisecond setting and recompute frames when its sample rate changes.
- Store the setting outside serialized session data. Backend/session replacement must explicitly carry the current runtime configuration into the replacement engine.
- Use a bounded unsigned-integer settings editor (`0..=100 ms`) with clear help text that `0` disables smoothing. Render this generic Audio setting independently of native driver-specific controls so it is visible in browser builds too.
- Preserve realtime constraints: scalar state only in the smoother, no per-cycle vectors or lookup tables, and no callback-time logging.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Stage 1 — Implement and unit-test the channel smoother

- [x] Add a small private smoother state to `src/rust/shoop_engine/src/audio_channel.rs`, with deterministic frame-duration configuration and explicit disabled/priming behavior.
- [x] Refactor playback finalization so recording copy commands retain their ordering while playback commands and the silence gaps between them form one raw channel-contribution timeline.
- [x] Detect active/inactive transitions, non-contiguous source positions, and abrupt playback-gain changes; explicitly avoid classifying contiguous chunk and callback boundaries as edges.
- [x] Apply the bounded linear correction before additive mixing, continue corrections through raw-silence gaps, and include smoothed samples/tails in output-peak publication.
- [x] Preserve a fast disabled path whose output and peak behavior match the current implementation exactly.
- [x] Add focused `AudioChannel` tests for start, stop, within-callback wrap, callback-boundary wrap, source seek, gain change, short/repeated loops, content ending before loop length, additive destination content, chunk continuity, duration changes, and `0 ms` bypass.

**Stage verification**

- [x] Run the targeted `shoop_engine` audio-channel and audio-loop tests.
- [x] Run allocation-sensitive engine tests relevant to channel processing and confirm smoothing adds no steady-state allocation.
- [x] Confirm test fixtures explicitly distinguish unsmoothed exact-copy expectations from enabled smoothing expectations.

## Stage 2 — Add session-level duration and sample-rate propagation

Depends on Stage 1.

- [x] Add a global loop-smoothing duration to `shoop_engine::Session`, with methods to set/query milliseconds and derive the configured frame count from `Session::sample_rate()`.
- [x] Propagate configuration to all existing audio channels, initialize every newly added audio channel with it, and recompute channel frame durations after sample-rate changes.
- [x] Ensure direct, bounded-capacity, state-mirrored, and snapshot-enabled channel creation paths all inherit the same value.
- [x] Add session tests for existing/future channels, `0 ms`, sample-rate conversion, sample-rate changes, and preservation across graph processing.

**Stage verification**

- [x] Run targeted session, graph, audio-loop, and no-allocation tests.
- [x] Verify changing the duration neither rebuilds the graph nor adds processing sub-blocks.

## Stage 3 — Carry the global control through every in-process backend

Depends on Stage 2.

- [x] Add a global loop-smoothing setter to the `shoop_backend::Backend` contract and a corresponding `AppIntent`; route the intent through the application model to the backend.
- [x] Implement the setter for `EngineBackend` and `LocalDummyBackend`, retaining the value when a staged session replacement is built and committed.
- [x] Add a queued `BackendSession` control in `shoop_engine::app_backend` so native threaded engines update their `Session` at a callback command boundary without direct cross-thread mutation.
- [x] Make `NativeBackend` retain the global value outside `NativeRuntime`, apply it to a newly started runtime, and reapply it during driver switching, rollback, and session restoration.
- [x] Extend `FakeBackend` observability so application-routing tests can assert the requested duration.
- [x] Add backend and application tests for command routing, current/future loops, session replacement, driver replacement/rollback, and failure reporting.

**Stage verification**

- [x] Run targeted `shoop_engine`, `shoop_backend`, `shoop_app`, and `shoop_app_api` tests.
- [x] Confirm the setting is absent from captured/serialized session documents while replacement runtimes still inherit it.

## Stage 4 — Extend the browser/worklet control protocol

Depends on Stage 3.

- [x] Add a global smoothing-duration command to `shoop_audio_protocol` and bump the protocol version.
- [x] Make a newer smoothing command supersede an older one in the worklet client's durable journal so reconnect/restart replays only the latest value.
- [x] Implement the `Backend` setter in `RemoteWorkletBackend`, classify failures as a global audio-processing mutation, and handle the command in `shoop_audio_worklet` by configuring its `EngineBackend`.
- [x] Update exhaustive command matching, protocol fixtures, worker fixtures, and transport/journal tests.
- [x] Add a worklet integration test proving `0 ms` and a nonzero duration reach the engine and survive journal replay.

**Stage verification**

- [x] Run targeted audio-protocol, worklet-client, audio-worklet, and WASM runtime tests.
- [x] Build both `shoopdaloop` and `shoop_audio_worklet` for `wasm32-unknown-unknown`.

## Stage 5 — Add the persistent settings-dialog control and runtime reconciliation

Depends on Stages 3 and 4.

- [x] Register `audio.loop_edge_smoothing_ms` as a cross-platform `u32` setting with default `3`, range `0..=100`, and help text explaining milliseconds and `0 = off`.
- [x] Adjust the custom Audio settings page to render generic loop-audio definitions before platform-specific driver controls, including in browser builds and native builds where driver switching is unavailable.
- [x] Add a settings helper that reads the value with a safe `3 ms` fallback and reports malformed settings consistently with existing startup fallbacks.
- [x] Configure the backend from the active settings snapshot during native and browser runtime creation, before ordinary loop use.
- [x] Reconcile later active-settings revisions independently of script-settings revision tracking, dispatching the global `AppIntent` after a save or settings recovery and retrying on dispatch failure rather than marking the value applied.
- [x] Ensure changing this setting does not trigger an audio-driver switch and does not become part of session save/load data.
- [x] Add registry, settings-dialog, persistence/recovery, native runtime, and browser runtime tests for the `3 ms` default, `0 ms`, another nonzero value, and asynchronous application.

**Stage verification**

- [x] Run targeted `shoop_settings`, `shoop_egui`, and `shoopdaloop` tests on native and WASM targets.
- [x] Verify in UI tests that the control remains visible and editable both with native driver controls and on the browser Audio page.

## Stage 6 — End-to-end validation and listening comparison

Depends on all previous stages.

- [x] Add an end-to-end deterministic loop fixture with deliberately mismatched nonzero edge samples and verify start, stop, and wrap continuity at several callback/chunk alignments.
- [x] Compare the same fixture at `0 ms`, default `3 ms`, and a clearly audible longer value; assert `0 ms` remains exact and each enabled correction reaches zero in its configured frame count.
- [ ] Manually run the native app, repeatedly start/stop and wrap a discontinuous waveform, and A/B `0 ms`, `3 ms`, and a longer duration from the settings dialog without restarting.
- [ ] Repeat the A/B smoke test in a browser build when browser tooling is available.
- [ ] Check short loops, stereo loops, dry/wet channel modes, pre-play/start offsets, gain changes, session load, and audio-driver switching for bounded, artifact-free behavior.
- [x] Run `cargo fmt --all -- --check`.
- [x] Run `RUSTFLAGS="-D warnings" cargo build --workspace`.
- [ ] Run `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`.
- [x] Run `python3 scripts/check_shoop_test_usage.py` because Rust tests changed.
- [ ] Run `python3 scripts/check_tracing_coverage.py --require-closed`.
- [ ] Run the documented browser builds and smoke checks where browsers are available.
- [x] Record any unavailable host/browser validation explicitly, including the missing facility and the remaining command or manual check. Interactive browser and physical listening validation remain unavailable in this non-interactive agent run; the remaining work is the manual native/browser A/B and artifact smoke checks listed above.
