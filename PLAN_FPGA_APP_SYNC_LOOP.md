# Default application Sync loop through the FPGA simulator

Status: implementation complete; final static checks and PR delivery in progress.
Working branch: `feat/fpga-app-sync-loop`, based on `proto/fpga-rust-reference`
through `e8c43c6d`, merged during implementation (initial base: `821ba859`).
PR base: `proto/fpga-rust-reference`. Push only the working branch.

## Goal and scope

Connect the actual application service/client to the FPGA adapter through the
existing engine endpoint and runtime queues. Support the fresh application's
one mono Sync strip and one primitive sync loop, including their real startup
transaction, strip/master signal path, controls, observations and recording reads.
Use Q=32, 32-sample chunks, Q8.24 audio execution, and Rust + Marlin/Verilator.

This is an offline application integration milestone. Supply silence or
deterministic synthetic samples in memory. The Marlin-backed engine runs
continuously as fast as possible: immediately start the next iteration when the
previous one finishes, independently of wall-clock/audio-rate deadlines. Provide an optional native application/demo entry point for interacting
with the Sync loop through the existing UI. It must work without a host process,
JACK, CPAL, audio-device enumeration, physical port publication or new live audio
I/O. Reuse existing offline facilities and existing sample/read APIs. Simulated
ports are logical endpoints for test samples; they are not physical audio ports.
No wall-clock playback, recording-device access or hard real-time claim is required.
Use no sleep, timer pacing or rate limiter between engine iterations. Deterministic
replays may bound/step the number of iterations to place inputs/actions at known
frame boundaries; each admitted batch still executes at maximum speed.

Support record, stop, play/wrap, clear and re-record; the sync loop's own pulse,
repeat and scheduled-transition behavior; applicable input/output/loop/master gain,
balance, mute and manual/automatic monitoring; the normal initial routes and
virtual connections needed to exercise this fixed session. Preserve default
control ramps and loop smoothing. Include UI state, meters and waveform reads.
Provide at least five seconds of mono recording at 48 kHz, with an earlier
five-second take retained while another is recorded.

Additional tracks/loops, inter-loop sync, composites, MIDI processing, FX/plugins,
dry/wet processing, retrospective capture, imported sessions, file persistence,
preview/metronome audio, arbitrary graph editing, board access and DRAMsim3
integration are outside this milestone. Unsupported features must be reported
honestly without preventing the supported fresh-session startup.

## Current implementation and concrete gaps

- `shoop_app::Service::new` creates `sync_loop` with one audio channel, MIDI off
  and one loop. It registers existing buses, sets loop smoothing and optionally
  connects Master. Application defaults include 3 ms of smoothing; ordinary
  engine preparation includes a 240-frame control ramp at 48 kHz. Read the actual
  composition's settings rather than substituting the simplified test profile.
- `shoop_rt_backend::RtBackend` already speaks the engine protocol over any
  carrier with an injected `EngineClock`. Existing `OfflineRender` and
  application-service fixtures establish a device-free composition pattern.
- `shoop_rt_engine::ControlEndpoint` uses `Control<EngineNode, LoopPlanner,
  EngineOp>` with float samples/parameters. The existing `LoopAdapter` instead
  uses a primitive loop/gain wrapper and Q8.24. Reusing queue construction does
  not make those concrete Rust types interchangeable.
- Engine preparation and `Render` currently construct/own a CPU `GraphRuntime`.
  The endpoint's startup jobs use node-pair and topology staging, `StageGraph`,
  configuration and later activation. The current adapter accepts one initial
  `Commit` at frame zero; its loader resets the simulator. That is insufficient
  for a clock already advancing while the app builds its session.
- The real graph contains strip stages, loop faders, buses and a bulk reader.
  Its prepared channel widths can exceed the Sync strip's one active channel.
  It also carries unused auxiliary/MIDI facilities. Establish which paths are
  genuinely inactive before simplifying them.
- Native render/planner observers publish shared cells used for telemetry,
  scheduling and app readiness. App bulk reads use `EngineOp::ReadStart`/Stop
  and a byte ring, rather than only the low-level resource reads already tested.
- The small Marlin preset has 128 chunks, about 85 ms of mono audio at 48 kHz,
  and fixed capacity checks. Existing RTL supplies chunk allocation, immutable
  views, metadata caches, AXI memory, TP settlement and SP/JP execution; reuse it.

## Immutable acceptance criteria

1. Actual `Service` startup and an ordinary app client reach ready through
   `RtBackend`, the existing protocol/carrier, engine endpoint and runtime queues.
   The confirmed model has the expected Sync strip/loop, buses and connections.
   There is an interactive offline app/demo entry point, not only a new low-level
   hardware test. It never starts or connects to JACK, CPAL or a host process.
2. Initial configuration is derived from real production preparation and ordered
   graph/topology descriptions. Hardware inputs do not contain oracle outputs,
   expected state/jobs or a hand-written replacement graph. Each accepted initial
   structural candidate has a bounded compilation/activation path. Loop modes,
   gains and monitoring changes do not recompile the signal program.
3. The supported operations listed above work through public app commands,
   including default nonzero ramps/smoothing, repeated wraps and clear/re-record.
   Hardware performs transitions, sample processing, capture and playback.
   Host-side metadata/response shortcuts preserve the command's real effect,
   ownership, observation cut, completion timing and graph/topology epochs.
4. Public replies, readiness, state, revisions, pending transitions, pulse/wrap
   observations and handle lifetimes match native behavior for the same admitted
   inputs and offline clock. Normalize opaque handles through checked logical
   correspondence. Exact fixed-point comparisons use shared production Q8.24
   arithmetic; compare the app's float audio/parameters against that result with
   explicit, justified error bounds fixed before RTL acceptance runs.
5. UI telemetry advances, synthetic input/output meters reflect actual samples,
   and waveform/bulk reads return the FPGA recording in the app's expected float
   encoding. Retained takes stay immutable; partial reads, cancellation, EOF,
   release and stale generations behave correctly.
6. The configured store admits two five-second mono takes at 48 kHz with required
   retained views/metadata. Repeated clear/re-record/release permits reuse.
   Capacity refusals and injected device faults yield terminal results and a
   coherent app failure state; no successful activation or sample result is invented.
7. A reproducible offline scenario covers startup, recording, playback, controls,
   waveform retrieval, clear, re-record, reconnect/resync and shutdown. Existing
   adapter/Marlin regressions and normal CPU/Wasm app paths remain working and
   independent of optional simulator dependencies. The new run path needs no Python.

## Design rules and constraints

- Keep the app and engine wire protocols, owning SPSC queues, scheduler and receipt
  machinery. Factor only necessary preparation/render interfaces and descriptions;
  keep the normal CPU execution path intact. `rt_fpga_adapter` already depends on
  `shoop_rt_engine`: introduce no reverse dependency cycle or Marlin dependency in
  the production engine, runtime, application or Wasm path.
- Consume the app's real `EngineNode`/`EngineOp` command family. Share compiler,
  device, lease and numeric plumbing with the existing adapter instead of adding
  another transport or software FPGA executor. Keep float representation at the
  app boundary and Q8.24 in hardware, including explicit bulk-read conversion.
- Treat staging as host-side preparation when possible: retain owners, validate
  complete candidates, allocate checked IDs and preserve cancellation/retirement.
  Existing endpoint discovery/resync stays host-side. A successful staging reply
  is distinct from activation; activation completes only once the device matches.
- Support the actual sequence of initial configurations and route establishment,
  including activation after logical frame zero. Quiescent single-bank loading
  while the offline clock pauses is acceptable. Preserve the externally visible
  timeline, state and media; do not use a reset/reload shortcut that loses them.
  General live replacement and a new double-buffered hardware architecture are
  unnecessary unless a demonstrated startup requirement makes them unavoidable.
- TP remains authoritative for musical state; SP/JP/memory execute audio. The
  adapter may format/cache observed state and implement host media streaming,
  but must not run a native planner/resolver or DSP to stand in for hardware.
- Describe supported node kinds and reuse production signal order, rounding and
  ramp conventions. Simplify only proven inactive/identity paths. Do not merge
  gain stages if doing so changes Q8.24 rounding. Do not disable smoothing, set
  ramps to zero or force all controls immediate to make comparisons pass.
- Use a declared audio-only device profile. Empty MIDI reset/bookkeeping and
  inactive prepared ports may be represented on the host; reject MIDI traffic.
  Distinguish prepared capacity from the single live strip/loop. Advertise only
  capabilities and limits that the app can actually use.
- Run the engine without wall-clock pacing or fixed iteration deadlines. After
  finishing an iteration, immediately start the next; keep UI polling separate.
  Bounded/stepped execution is for deterministic tests and replay, not a speed
  limit on the wrapped backend. Reuse offline buffering patterns. Keep one owner
  for each Marlin model,
  runtime and VCD; bound waits/work per pump and preserve backpressure. Build/load
  the model outside normal UI interaction. No audio callback/thread integration
  or simulator real-time performance work is part of this task.
- Scale the existing store and functional backing memory with checked capacities.
  Keep long chunk tables/sample storage in the existing external-memory model;
  retain small-test presets. Change synthesizable RTL only for demonstrated
  missing signal, transition, smoothing or interface behavior.

## Staged implementation

Stages are sequential. Establish the actual app contract before changing the
consumer; settle activation before signal behavior; verify state/audio before
adding UI streaming. Use focused tests until the final integration gates.

### 1. Capture the real startup and supported app contract

- [x] Continue on a new working branch from latest `proto/fpga-rust-reference`;
  fetch/merge upstream at the start and between milestones. Push only that branch.
- [x] Run the actual fresh `Service`/`RtBackend`/app-client path against the native
  offline engine at Q=32. Record startup requests, runtime operations, staged
  payloads, graph order, node bindings, settings, capabilities and observation cuts.
  Include logical Master routing and the synthetic input connection needed by tests.
- [x] Define a versioned common input/action replay using public app actions,
  synthetic samples and explicit logical-frame/iteration advances. Pin defaults, admitted controls,
  native pulse/scheduled behavior and resource/telemetry expectations. Include
  nonzero default ramps/smoothing; establish fixed-point/float comparison bounds.
- [x] Verify native startup, immediate and scheduled sync-loop actions, recording,
  playback/wrap, clear and reads. List unsupported requests explicitly. These
  contract tests must use the real startup, not a reduced hand-built graph.

### 2. Expose an offline engine/adapter composition seam

- [x] Factor production preparation so its endpoint, prepared node descriptions,
  queues and observation publications can pair with a CPU or device consumer.
  Provide an optional native offline simulator composition with `EngineClock`
  and an in-process carrier; preserve the ordinary CPU preparation APIs.
- [x] Reuse the app's actual typed command family. Add narrowly scoped immutable
  node/configuration descriptions and observation hooks as needed. Define which
  helpers are reusable without introducing crate cycles or executing CPU DSP.
- [x] Supply logical port inventory and synthetic/zero sample buffers without
  any host adapter/device discovery. Make unsupported features/capacities visible
  through existing capability/limit/refusal paths.
- [x] Verify endpoint connection/resync, ordinary queue ownership and unchanged
  CPU construction with a recording/stalling device. Check native/Wasm dependency
  trees for simulator leakage before undertaking the larger consumer extension.

### 3. Implement staged startup and boundary activation

- [x] Consume the required node-pair, topology, graph and configuration staging
  operations, including owning payloads, candidate IDs, validation, cancellation
  and release. Reuse native staging codecs/validators and checked descriptions.
  Keep preparation and eligible bookkeeping responses in Rust.
- [x] Compile the app's submitted ordered graph and TP topology. Support every
  required startup activation/virtual-route transaction at its effective boundary,
  with silence while no app graph is active and a continuously correct clock.
  Preserve loop/media/control state during any later permitted configuration.
- [x] Separate transport admission, staging validation, scheduled delivery and
  device activation acknowledgements. Keep bound identities/generations stable
  and publish graph/topology epochs at the native cuts.
- [x] Verify through recording/stalling/failing devices: no early activation,
  activation after frame zero, invalid/stale candidates, incomplete payloads,
  queue pressure, cancellation, owner return and partial-load failure. Then run
  the captured startup through Marlin and reach an idle confirmed app model.

### 4. Execute the default signal graph and Sync loop controls

- [x] Lower supported strip input/output, loop fader, Master/bus and loop nodes
  to shared hardware operations, preserving the active/prepared channel layout,
  routing and arithmetic order. Handle inactive auxiliary/MIDI ports explicitly.
- [x] Implement the applicable gain/balance/mute/monitoring operations, including
  automatic monitoring, configuration/presence changes and default control ramps.
  Reuse existing gain/mix machinery; add RTL behavior only where necessary.
- [x] Complete record, stop, play, clear/re-record, self-pulse/repeat and scheduled
  sync-loop transition translation. Implement default loop boundary smoothing,
  including durations that are not multiples of 32, and correct wrap behavior.
- [x] Reuse/factor only supported production arithmetic for exact Q8.24 expectations.
  Verify small deterministic sample vectors, gain/ramp changes at boundaries,
  smoothing around short loops, monitoring/mute, cancellation/supersession and
  requested/effective/terminal frames before extending the full app replay.
  Assert state-only controls leave the compiled program installed.

### 5. Publish app observations and service recording reads

- [x] Populate existing observation cells/hooks from actual TP state, publications
  and device results. Match native frame/epoch cuts, loop modes/length/position,
  revisions, pending actions and pulse counts. Publish sample-derived meters and
  truthful fault/status information so readiness and `wait_idle` can progress.
- [x] Bridge the app's reader start/stop and bulk ring in Rust using existing FPGA
  exports, checked leases and read commands. Convert raw Q8.24 words to the float
  byte/sample encoding expected by the app. Retain read budgets/backpressure.
- [x] Parameterize hard-coded adapter/driver/top limits from a checked preset large
  enough for two five-second mono takes plus metadata/views. Preserve a small
  regression preset and report allocator/view usage without claiming elastic RTL.
- [x] Verify observations/dirty telemetry and streaming under unread-result pressure,
  partial reads/EOF, cancellation, retained takes, release/reuse and stale handles.
  Include coherent disconnect/fault/shutdown and no leaking candidate/read owners.
  Exercise the long-capacity workload once after short correctness cases pass.

### 6. Connect the actual offline app and UI

- [x] Add an optional native entry point composing the existing application
  service/client/UI with the simulator-backed engine endpoint. Reuse application
  controls; keep simulator details out of ordinary product interactions.
  Provide in-memory silence/test-signal selection and continuous execution at
  maximum speed, with app/UI polling independent of engine iteration timing. It must launch without a host process or JACK/CPAL initialization.
- [x] Adapt the existing callback-to-quantum buffering for logical offline blocks
  larger than or not divisible by 32. Preserve time remainders, processing latency,
  scheduling and partial output; perform no new live audio input/output.
- [x] Run actual app-client intents to create the default session, record, stop,
  play through wraps, change supported controls, fetch a waveform, clear and
  re-record. Verify snapshots, structural confirmation and public refusal messages.
- [x] Verify disconnect/reconnect/resync and orderly close with no physical host
  facilities. Perform an interactive or automated GUI smoke of the same workflow;
  retain headless service/client coverage for deterministic regression testing.

### 7. End-to-end comparison, documentation and delivery

- [x] Replay the same public app startup/actions, samples and offline clock against
  native and FPGA compositions. Compare logical results, state/revisions/events,
  epochs, observation cuts, waveform/bulk samples and lifetimes; use exact Q8.24
  and the pre-established float accuracy bounds. Include default ramp/smoothing,
  future actions, clear/reuse, reconnect, pressure and injected faults. Report the
  first divergence with action/frame context and a reproducible VCD command.
- [x] Retain a replayable short app trace plus one storage-capacity case. Document
  one-command headless comparison and offline GUI launch, prerequisites, controls,
  limits and unsupported features. Measure model build/load, graph compilation,
  configuration, execution cycles and app readiness separately; no live audio claim.
- [ ] Run affected runtime/engine/backend/app, adapter and Marlin tests; preserve
  existing gain/mono/direct-loop regressions. If RTL changed, run relevant subsystem
  and integrated lint/synthesis/protocol gates. Run formatting, warning-denying
  builds, the Rust test-attribute policy and CPU/Wasm isolation/build checks.
- [x] Broaden native testing once at final delivery. Keep host binaries stable
  during execution, isolate failures before diagnosing regressions and reuse valid
  expensive evidence. Do not repeat the previously waived 64-loop benchmark or
  add JACK/CPAL/physical-device tests for this offline feature.
- [ ] Commit/push milestones and open/update the PR onto
  `proto/fpga-rust-reference`. Check incoming automated review and address
  applicable findings until approval or no actionable findings remain. Under the
  standing user instruction, do not inspect PR CI; rely on recorded local gates.
  Do not push directly to the upstream branch or merge autonomously.

## Testing cost

Start with one native app contract and protocol-only device doubles. Reuse the
cached Marlin model for short signal/transition and media cases; rebuild only
when RTL or its preset changes. Add full service/client/UI comparisons after
activation and numeric semantics pass. Keep long recordings, wider native gates
and GUI smoke for final integration. Separate simulator build time from execution,
reuse unchanged passing evidence and repeat checks only when edits invalidate it
or failures leave an unresolved concern.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Implementation evidence

Stage 1 uses the actual native service/client and a shared public-action trace.
Startup and waveform-selection findings, settings and predeclared numeric bounds
are recorded in `src/rust/rt_fpga_sim/APP_CONTRACT.md`. The production preparation
seam reuses the native render owner and observation publisher; native library
regressions remain passing. Default simulator execution is explicitly unpaced
under the user's clarification; deterministic replay uses bounded logical runs.

Stages 2–6 now run the actual service/client against the hardware consumer.
The app compiler preserves production signal order and maps controls to installed
state; TP/SP/JP perform transitions, smoothing and samples. The optional window
uses a continuously advancing worker without timing deadlines; stepped execution
is limited to deterministic replay. Short arithmetic, protocol, app and media
fixtures pass, including exact production-Q8.24 ramps/smoothing and the declared
float bounds. The full retained-take app case passed in 1000.08 seconds: two
five-second takes, waveform retrieval, clear/re-record and reuse, with a peak
of 30032 physical slots out of 32768. An isolated metadata-capacity fixture also
holds both stereo-prepared takes and their snapshots concurrently.

Final delivery validation is in progress. The broad native suite reported 3219
passes and four failures; the first isolated retry still failed. On the untouched
`821ba859` baseline, three of those tests pass and driver-switch recovery also
fails, at a later deadline. The final quiet targeted run after the upstream merge passes the other three
tests; driver recovery still times out at startup. No test deadlines or assertions
were relaxed. Detailed results and limitations are recorded in the simulator
validation document.

Upstream's acyclic topology format and memory-reporting changes are integrated.
The native consumer retains the new memory publisher and sample-rate argument;
the shared endpoint and external outbox reserve the additional telemetry slot.
The FPGA composition does not publish a fictitious native media-memory report.
