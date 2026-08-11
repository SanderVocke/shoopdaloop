# First-Class Audio Processor Graph Plan

## Goal

Move every dry/wet processor category used by the egui application onto one explicit, first-class audio-graph processor-node path:

1. external send/return processing;
2. the deterministic `Test2x2x1` processor;
3. Tiny Synth/FX;
4. the Carla family (Rack, Patchbay, and Patchbay 16x, regardless of in-process or subprocess hosting).

Processor output must be produced at the graph position consumed by wet loop recording and normal output routing. The signal recorded into a wet channel must therefore be the processor signal available to monitoring in the same callback, rather than silence, a test-only MIDI impulse, or another bypassed signal.

## Scope

- Extend the engine graph description, schedule, and realtime node dispatch with explicit processor nodes and typed processor routes.
- Give all processor categories the same graph-facing lifecycle and port-ordering contract while retaining processor-specific execution behind that contract.
- Replace name-based processor discovery and special test-FX hooks with explicit topology metadata.
- Route processor output through internal processor output ports; remove direct post-graph writes to device-facing wet outputs.
- Preserve the external processor's real callback-boundary semantics: its staged wet return is an input to the node, not a zero-latency result inferred from the current callback's dry send.
- Cover the Rust engine and both native and cooperative/WebAudio backend paths that use it.
- Do not redesign session documents, UI controls, processor editors, plugin state, or external connection UX except where adaptation to the processor-node API is required.

## Immutable acceptance criteria

1. Every configured external, test, Tiny Synth/FX, or Carla processor is represented by explicit processor topology and a scheduled processor node; no processor kind is identified from port-name patterns during realtime processing.
2. The graph enforces, for audio and MIDI, `input prepare/processing and dry-loop playback -> processor -> processor-output processing/propagation -> wet-channel recording and device output`.
3. Processor output is written to internal processor output ports exactly once per callback. Tiny, Carla, and test processors are not run in a post-schedule pass and do not use `add_late_output()` to bypass graph consumers.
4. During monitored recording, the wet loop records the same pre-track-output-control processor signal used by monitoring, sample-aligned for in-process processors. Unmonitored recording captures the same wet data while emitting no live monitored output.
5. `Playing`, `PlayingDryThroughWet`, and `RecordingDryIntoWet` retain their intended dry/wet semantics, including synchronized transitions and the first callback at each transition boundary.
6. The deterministic test processor has one documented implementation of its predictable 2-audio-input/2-audio-output/1-MIDI-input mapping, and comprehensive tests use that implementation rather than engine-side synthetic port hooks.
7. Tiny Synth/FX has smoke coverage proving sustained synthesized output reaches wet recording and dry-through-wet playback; a note-on may not degrade into an isolated velocity click in wet data.
8. Carla graph integration is tested with a deterministic fake `CarlaProcessor` without requiring an installed Carla LV2 plugin. Optional real-Carla smoke tests skip cleanly when Carla is unavailable. Baseline builds and tests without the `lv2`/`native-fx` features continue to pass.
9. External processing remains covered with staged-return tests that verify recording, monitoring, playback, and callback-boundary timing without requiring external hardware or software.
10. Processor-node processing performs no allocation, deallocation, blocking lock, topology search, or port-name construction on the realtime thread after control-path preparation.
11. Existing session persistence, processor controls/state restoration, graph generation/stale-schedule handling, track gain/mute/balance, and non-processor loop behavior remain compatible.
12. The workspace builds with warnings denied and all applicable Rust and frontend integration test gates pass.

## Design rules and constraints

- Use one graph-facing processor route contract with explicit audio inputs, audio outputs, MIDI inputs, and any external return/send boundary ports. Processor-specific code may vary behind this contract, but scheduling and output publication may not.
- A processor node depends on every source required for the current callback and precedes every destination that consumes its output. Assert these edges directly; do not rely on node names or deterministic tie-breaking.
- Keep graph topology snapshots detached and `Send`, and keep expensive schedule construction off the realtime thread.
- Resolve and validate processor port indices on the control/topology path. Missing, duplicate, directionally invalid, or out-of-range routes must fail explicitly instead of silently producing partial audio.
- Preserve arena/tombstone and graph-generation safety so an older installed schedule never dereferences replaced processor or port storage.
- Prepare all audio planes, MIDI staging, and processor scratch to the maximum callback size before the node becomes active.
- Apply processor output-port gain/mute/passthrough and track output controls through ordinary graph port processing. Wet recording must tap the processor result before controls intended only for audible track output.
- Keep external processing latency explicit: current dry sends cannot causally produce a return already staged for that same callback.
- Use the test processor for exact sample assertions. Use invariant-based smoke assertions for Tiny and real Carla, whose synthesis/plugin output is not a stable golden waveform.
- Tests must not make plugin discovery, a Carla installation, JACK, MIDI hardware, or an audio device a CI prerequisite.

## Staged implementation

### Stage 1 — Pin the regression and processor contract

- [ ] Add a concise processor-route contract in engine types covering explicit audio/MIDI inputs and outputs plus external send/return boundary data.
- [ ] Add failing graph-order tests demonstrating that a processor must run after all required inputs and before processor-output port processing and wet-channel finalization.
- [ ] Add focused failing backend reproductions for monitored and unmonitored test-processor wet recording, including the original “audible sustained output but isolated click/silence in wet data” class of mismatch.
- [ ] Document the deterministic test processor mapping used by assertions: corresponding audio input at half gain plus note-on velocity contribution at the event frame on both outputs, with explicit inactive behavior.

Verification:

- The new behavior tests fail for the known scheduling/bypass reason, not because of unavailable devices or plugins.
- Existing graph tests still describe unchanged port/loop/channel ordering.

### Stage 2 — Add first-class processor nodes to graph construction

- [ ] Extend `GraphDesc`, `NodeMap`, topology snapshots, prepared schedules, and realtime `NodeAction` dispatch with processor descriptors and processor-node indices.
- [ ] Lower explicit processor dependencies into graph edges for audio input, MIDI input, processor output preparation, output processing/propagation, and loop-channel consumers.
- [ ] Resolve scheduled processor actions to stable processor arena entries and preserve safe behavior across asynchronous graph rebuild/install and processor replacement.
- [ ] Add graph unit tests for mono, stereo/multi-port, MIDI-only, missing-port rejection, parallel processor isolation, processor-to-loop paths, and cycle detection.
- [ ] Add schedule inspection/tracing coverage so processor nodes are visible as bounded `engine.rt.fx.*` work nested at their scheduled position.

Verification:

- `cargo test -p shoop_engine graph`
- Targeted topology/schedule tests prove ordering from edges even when names and insertion order are varied.
- Topology and prepared-schedule `Send` assertions remain valid.

### Stage 3 — Move the deterministic test processor onto the common node path

- [ ] Implement `Test2x2x1` as an explicit processor implementation invoked only by its registered processor node.
- [ ] Delete the port-name-driven `fill_test2x2x1_fx_output`, per-port synthetic processing, end-of-cycle synthetic output pass, and permissive default activation behavior.
- [ ] Publish test-processor results into its internal processor output buffers and let ordinary port propagation feed wet channels and track outputs.
- [ ] Add exact sample tests for:
  - [ ] live audio and timestamped MIDI processing;
  - [ ] monitored and unmonitored initial recording;
  - [ ] normal stored-wet playback without dry reprocessing;
  - [ ] monitored playback mixed with live processed input;
  - [ ] `PlayingDryThroughWet`;
  - [ ] `RecordingDryIntoWet` wet replacement;
  - [ ] synchronized record/start/stop and dry-to-wet boundaries, including the first and last callback;
  - [ ] one loop recording while another plays stored wet audio;
  - [ ] inactive/reactivated routing, output mute/gain, and no contamination between two processors;
  - [ ] mono/subset channel mappings and the full 2x2x1 mapping.
- [ ] Exercise the scenarios at engine level and through the native dummy backend; retain or consolidate equivalent higher-level tests rather than duplicating assertions unnecessarily.

Verification:

- Targeted engine and `shoop_backend` test-processor suites pass with exact expected buffers.
- No recorded output is produced by a processor that was not explicitly registered as `Test2x2x1`.

### Stage 4 — Migrate Tiny Synth/FX and Carla to the common node path

- [ ] Adapt Tiny Synth/FX to consume graph-routed audio/MIDI and write all logical channels to internal processor output ports during its scheduled action.
- [ ] Preserve preset/control/state replacement and MIDI playback behavior without scanning loops or ports by name in the callback.
- [ ] Adapt the Carla realtime endpoint—both in-process and subprocess hosting—to the same processor-node input/output contract for Rack, Patchbay, and Patchbay 16x layouts.
- [ ] Remove the global post-schedule Tiny and Carla passes and all direct late writes to wet device outputs.
- [ ] Ensure inactive, unavailable, crashed, recovered, and replaced processors have explicit safe output behavior and cannot leave stale buffer contents.
- [ ] Add Tiny smoke tests for live monitoring, wet recording, MIDI note duration, normal wet playback, dry-through-wet playback, dry-into-wet replacement, and state restoration across buffer-size/backend replacement.
- [ ] Add deterministic fake-Carla node tests that run without plugin discovery and cover audio/MIDI routing, inactive state, wet recording, and bridge realtime constraints.
- [ ] Keep optional real-Carla tests for all available layouts finite-output/lifecycle smoke only; return a reported skip when the host/plugin is unavailable.

Verification:

- `cargo test -p shoop_engine`
- `cargo test -p shoop_backend`
- Feature-enabled fake-Carla tests pass without an installed Carla plugin.
- Feature-disabled builds prove Carla remains optional.

### Stage 5 — Represent external processing through the same graph contract

- [ ] Register external dry/wet tracks as explicit external processor nodes rather than unrelated send/return passthrough conventions.
- [ ] Model current-callback dry sends and driver-staged wet returns as separate sides of the external boundary, preserving real round-trip latency and avoiding a false same-callback dependency.
- [ ] Route staged wet return through the processor output consumed by wet recording and monitoring, with the same downstream ordering as hosted processors.
- [ ] Add deterministic dummy-driver tests for audio-only, MIDI-only, and mixed external processing; monitored/unmonitored recording; normal wet playback; dry-through-wet; dry-into-wet; synchronized transitions; and disconnected/missing returns.
- [ ] Verify existing external connection descriptors and saved connections remain unchanged at the application/session-document boundary.

Verification:

- External processor tests use staged buffers only and require no host process or hardware.
- Expected callback latency is asserted explicitly and recorded wet samples match the staged return consumed by monitoring.

### Stage 6 — Realtime, integration, and cleanup validation

- [ ] Extend no-allocation/no-lock tests to first activation and steady-state processing for test, Tiny, fake Carla, and external processor nodes, including maximum configured channels and callback size.
- [ ] Verify graph rebuilds, processor insertion/replacement/removal, driver switching, session save/load, and stale-schedule cycles do not leak old processor routes or produce unsafe output.
- [ ] Update Tracy stage assertions/instrumentation expectations to show processor work at graph-scheduled positions, without user-controlled zone names.
- [ ] Remove obsolete route rebuilds, naming conventions used only for runtime discovery, late-output helpers if no remaining caller needs them, and redundant tests of the superseded paths.
- [ ] Run formatting, warnings-as-errors builds, targeted suites, workspace suites, and frontend dry/wet integration tests.

Verification:

- `cargo fmt --all -- --check`
- `RUSTFLAGS="-D warnings" cargo build --workspace --features shoop_engine/app_backend`
- `cargo test --workspace --features shoop_engine/app_backend`
- Native-FX build/tests where supported, with fake Carla mandatory and real Carla optional/skippable.
- Build the frontend and run `target/debug/shoopdaloop_dev.sh --self-test`.

## Final end-to-end validation

- [ ] Reproduce the original workflow with a synchronized click loop and a Tiny Synth/FX dry+wet track: record a sustained MIDI phrase, verify normal wet playback matches the monitored performance, and verify dry-through-wet remains correct.
- [ ] Repeat the mode matrix with `Test2x2x1` and inspect exact wet sample data at synchronization boundaries.
- [ ] Exercise an external staged-return track and, when locally available, one real Carla track; absence of Carla is recorded as a skip rather than a failure.
- [ ] Capture a detailed Tracy trace for one test/Tiny run and confirm processor execution is graph-ordered between input preparation and wet recording/output propagation, with no post-graph Tiny/Carla FX pass.
- [ ] Confirm no new warnings, realtime allocation/lock diagnostics, xruns attributable to processor scheduling, or regressions in direct tracks, session persistence, driver switching, and multi-loop playback.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
