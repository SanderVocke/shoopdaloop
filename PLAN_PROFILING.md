# Runtime profiling implementation plan

Status: dedicated profiling window and RT/transport integration implemented;
release measurements and final cross-platform validation in progress.

Base branch: `feat/fixed-quantum-runtime`.
Investigated baseline: `cb79706e95350539638368bcca9f50b1601795ae`.
Implementation branch: `feat/runtime-profiling`, with its PR targeting the base branch.

## Goals and scope

Add optional, topology-aware real-time profiling to ShoopDaLoop. A single on/off
control in a dedicated profiling window controls continuous timestamp recording, statistics and automatic xrun
capture. Show results in a statistics table with graph node names and their average,
worst and standard deviation over the measured interval.

Retain node output-ready timestamps and out-of-graph phase boundaries for complete
callback accounting and bounded xrun snapshots. The initial UI uses the same table
for live results and the latest xrun capture. Timeline views, graph visualizations,
Perfetto export, configurable detail levels and historical topology comparison are
outside this implementation.

## Immutable acceptance criteria

1. Profiling is available through a single immediate runtime on/off switch in its
   own window, opened below Host Manager in the backend status LED panel. It is not
   shown in the Settings window. It defaults to off and changes without restarting audio. There are no profiling
   build flags, recording levels or separate xrun-arming controls. Persist the
   preference through the existing settings system; display acknowledged runtime
   state rather than assuming a request succeeded.
2. Off performs no profiling clock reads, sample writes, buffer clearing, history
   maintenance or profiling publication on RT. It performs only bounded enable/reset
   checks and predictable control flow. Enabling starts a fresh measured interval;
   disabling stops at a callback boundary and leaves the final report readable.
3. The UI table has exactly `Node`, `Avg`, `Worst`, `Stddev` columns, one row per
   active compiled graph node, in execution order, with useful names and a presentation selector for microseconds or percentage of
   one fixed quantum's time budget. Include the same overall DSP load reading as
   the bottom status bar. Results cover audio and MIDI nodes. Interval bounds and
   incomplete/lost-data status are shown outside the table.
4. Each live report covers all accepted samples since enable or the latest reset,
   through its stated end boundary. A node sample is one complete fixed-tick
   executor step, including local buffer preparation, incoming routing, MIDI
   handling, processing and output recovery. `Avg` is its arithmetic mean,
   `Worst` its maximum, and `Stddev` its population standard deviation. Completion
   offsets remain available in recorded data and are distinct from these durations.
5. Every application graph recompilation/rebuild resets all profiling tracking,
   including identical topologies and unchanged node IDs. Discard accumulators,
   queued samples/reports and retained xrun data; never remap or carry measurements
   to a node's position in a rebuilt graph. Reset also on graph activation and
   engine replacement so no interval spans different executable layouts.
6. While on, retain bounded recent history and automatically preserve a snapshot
   around measured callback overruns and host-reported xruns. Capture requires no
   extra user action. Retain raw timing boundaries and provide the same statistics
   table for the actual captured interval. Identify trigger cause, delayed or
   uncertain callback attribution, truncation and record loss truthfully.
7. Profiling never allocates, frees owning storage, locks, waits, performs I/O or
   serializes messages on the render path. Storage and publication work are bounded;
   collector/UI stalls cannot block audio. Profiling exhaustion cannot fault audio
   or consume command, recovery or media-transfer capacity.
8. Deliver production native and browser integration through existing host,
   protocol, application and settings boundaries. Supply actual elapsed-time clocks;
   logical sample frames are not execution-duration measurements. Report unavailable
   timing capabilities explicitly. Profiled and unprofiled execution preserve the
   same audio, MIDI, command ordering and fault/recovery behavior.
9. Measure enabled and disabled overhead against the same uninstrumented baseline,
   including complete callback tails and misses. Earlier percentage estimates are
   planning estimates, not performance evidence or universal guarantees.

## Design rules

- Instrument the production fixed-tick kernel first. At this baseline it is
  `GraphRuntime::{process,run_tick,run_graph}` in `rt_runtime`, reached
  through `shoop_rt_engine::Render`. The prior standalone runtime prototypes have been removed by the refreshed base.
- Keep the core `no_std` and preserve dependency boundaries and unsafe-code policy.
  Hosts supply a monotonic clock with documented resolution and bounded call cost.
  Native and worklet clocks may have different origins; compare only calibrated
  domains. Browser clock resolution and Wasm-to-JavaScript call overhead need direct
  measurement, not assumptions based on native clocks.
- Cache the enable decision once per outer callback. Prepare capacity during host
  construction so runtime enable does not resize buffers. Disabled checks must
  precede any argument construction or helper that reads a clock or touches storage.
- Compile a dense numeric profiling layout alongside each executable graph. RT
  stores timestamps by prepared position; name resolution and formatting occur
  outside rendering. Names distinguish track/loop/bus roles and duplicate labels.
  Metadata describes only the current generation; no cross-topology reconciliation.
- Prefer one output-ready timestamp per node plus explicit graph/phase boundaries.
  Consecutive step boundaries define inclusive node durations, including bounded
  executor bookkeeping. Mark readiness after output recovery/clearing has settled.
  Outgoing host routing and other non-node phases have separate boundaries. Document
  that elapsed duration includes preemption and is not exclusive CPU time or device
  playback latency.
- Store callback sequence, host/sample frames, tick frame, graph identity, profiling
  generation, clock domain and completion/error state. An outer driver callback may
  split into engine callbacks; an engine callback may contain zero or several ticks.
  Do not compare every tick against a fictitious independent driver deadline.
- Use preallocated recording pages and explicit producer/consumer ownership.
  Synchronize publication per batch, not per node. Streaming can drop bounded
  batches with sequence/loss counters; reserved render-owned history/capture capacity
  preserves the retained xrun window independently of UI progress. Never overwrite
  storage leased to a reader. Reset/reuse storage logically without RT destruction
  or a full-history clear/copy.
- Make recompilation invalidation explicit at each application graph-build entry,
  not only when a successful plan is activated. A build later rejected or failed
  still clears prior tracking; the unchanged active graph can begin a fresh interval.
  Successful activation resets again before its first new-layout sample. Use a
  generation token so delayed samples, descriptors and UI chunks are rejected.
- Accumulate count, mean, maximum and Welford M2 outside RT; calculate
  `sqrt(M2 / count)` when reporting. Empty rows show an em dash; a singleton has zero
  standard deviation. Exclude unfinished node steps from statistics, preserve their
  partial timing/fault markers, and describe incomplete intervals explicitly.
- Refresh live reports at a bounded rate, initially at most five times per second.
  Keep one latest xrun capture with bounded pre/post-trigger history. Define memory
  caps before rendering and report actual retained duration rather than guaranteeing
  a history length under every quantum/topology. A leased capture is not overwritten;
  count incidents that cannot be retained. A graph reset invalidates all captures.
- Keep collectors outside audio callbacks. The worklet endpoint currently pumps on
  the audio thread, so it may only perform bounded transport forwarding; aggregation
  belongs in a non-audio worker/client. Use independently budgeted profiling traffic,
  coherent generation-tagged report chunks and fair scheduling. Existing outbox
  ordering prioritizes bulk over telemetry and must not let captures starve meters.
- Extend existing settings and application protocols; do not make egui access an
  engine or carrier directly. Keep profiling distinct from existing developer
  tracing. Do not persist measurements in saved sessions or expand feature scope.

## Implementation anchors

Reconcile these against the current base before implementation.

| Area | Current code |
| --- | --- |
| Production kernel and graph descriptors | `rt_runtime/src/executor.rs`, `graph.rs`, `graph/activation.rs`, `host_buffer.rs` |
| Engine graph builds and render ownership | `shoop_rt_engine/src/{graph,jobs,replacement,render,cells,node,pool}.rs` |
| Host timing, bridges and xrun causes | `shoop_rt_host_native/src/{cycle,direct,status,cpal_driver,jack_driver,dummy}.rs`; `shoop_rt_host_worklet/src/{host,abi}.rs`; `shoopdaloop/rt_audio_worklet.js` |
| Engine messages and bounded carriers | `shoop_rt_protocol/src/{schema,message,codec}.rs`; `shoop_rt_transport/src/{client,carrier}.rs`; record carrier implementations |
| Backend and application projection | `shoop_rt_backend/src/{backend,incoming,mirror}.rs`; `shoop_backend/src/lib.rs`; `shoop_app/src/service/{projection,commands,management}.rs`; `shoop_app_protocol`; `shoop_app_client/src/runtime.rs`; `shoop_app_api/src/lib.rs` |
| UI and settings composition | `shoop_egui/src/{app_widget,settings_dialog}.rs`; `shoop_settings`; `shoopdaloop/src/main.rs` |
| Existing performance evidence | `scripts/benchmark_rt_runtime.py`; `shoop_loops/examples/fixed_quantum_benchmark.rs`; `shoop_engine_service/examples/measure_service`; package heap/host tests |

Paths above are relative to `src/rust/` unless prefixed with `scripts/`.

## Staged implementation

### Stage 0 — Prepare and reconcile

- [x] Read current repository mandates, style, build/test guidance and dependency
  boundary checks. Preserve unrelated work.
- [x] Fetch the base branch, create `feat/runtime-profiling` from its current tip,
  carry this plan onto it and record the actual baseline.
- [x] Confirm every production graph-build/activation path, native/process/worklet
  launch path, elapsed-clock provider and application projection hop. Inventory
  existing tests and select representative benchmark workloads.

Verification: the branch/diff is correct and the inventory identifies the actual
production kernel and all reset entry points. Commit the planning milestone.

### Stage 1 — Define timing, interval and lifecycle contracts

Depends on Stage 0.

- [x] Specify dense timestamp layouts, step/phase boundaries, stable current-graph
  names, generation/reset acknowledgement, incomplete-step handling and clock units.
- [x] Specify toggle acknowledgement, final report on disable, cumulative live
  interval, captured interval, fixed memory/publication budgets, capture replacement
  and loss reporting. Preserve audio when timing is unavailable.
- [x] Define coherent descriptor/report/capture chunks and capability negotiation
  across engine and app protocols; include generation, interval and sequence IDs.
  Append wire tags and update version/compatibility handling where required.
- [x] Add independent synthetic-data tests for mean/max/population standard
  deviation, empty/singleton samples and large-baseline numerical stability, plus
  codec bounds, stale generations, resets and partial report assembly.

Verification: contract and codec tests pass; a report can never combine generations,
intervals or partial row sets. Commit the contract milestone.

### Stage 2 — Record production RT timing with bounded storage

Depends on Stage 1.

- [x] Add host-injected clocks and a prepared recorder to the production kernel.
  Record callback/tick, node-ready and non-node phase boundaries, including host
  adaptation, commands, planning/state settlement, final routing and reclamation.
- [x] Cache enable/reset controls per callback; invalidate at graph-build requests
  and at tick-aligned activation. Support a graph change inside a multi-tick callback
  without mixing layouts or dropping owning objects on RT.
- [x] Implement batch ownership/publication and reserved circular history with
  explicit saturation, sequence, fault and truncation counters.
- [x] Test with an injected deterministic clock and independently prescribed node
  work. Cover direct/buffered partitions, zero-tick and multi-tick callbacks,
  audio/MIDI recovery, all supported quanta, faults, toggling and stalled consumers.
  Assert zero profiling clock calls/storage activity while off and no heap/locks
  on enabled, disabled, full, reset and fault paths.

Verification: boundary records account for prescribed work, partial faults never
create fabricated zero-duration samples, output behavior is unchanged and RT guards pass.
Commit the recorder milestone.

### Stage 3 — Collect statistics and preserve xrun captures

Depends on Stage 2.

- [x] Implement bounded non-RT draining and Welford accumulation over the current
  interval; retain only current-generation metadata and reject delayed batches.
- [x] Wire native host and worklet overrun/xrun causes to bounded trigger signals.
  Retain preceding history and bounded following work, then freeze/rotate ownership
  without RT copying. Attribute delayed notifications conservatively.
- [x] Produce live and latest-capture reports with interval bounds, sample/loss
  counts and timing capability. Reset all data on every rebuild, activation,
  re-enable and engine replacement; retain the final report on disable unless a
  later graph reset invalidates it.
- [x] Test fake-clock outliers, simultaneous/delayed triggers, collector stalls,
  capture leases, insufficient history, repeated xruns and generation invalidation.

Verification: exact known statistics and retained raw boundaries match the test
workload; a blocked collector neither blocks audio nor fabricates a complete capture.
Commit the collector/capture milestone.

### Stage 4 — Carry profiling through production transports and app state

Depends on Stages 1–3.

- [x] Connect controls, descriptors, reports and capture reads through native,
  process/socket, shared-memory and worklet carriers, as supported by current hosts.
  Give profiling independent bounded admission and forwarding work.
- [x] Complete backend, service, app protocol/client and `AppSnapshot` projection.
  Reassemble reports atomically and reject stale/reset-generation data at each hop.
- [x] Implement the worklet elapsed-time import and bounded forwarding; drain and
  aggregate outside the audio realm. Report missing or coarse clocks honestly.
- [ ] Verify runtime toggle outcomes, reconnect/resync and host replacement,
  incompatible peers, malformed/oversized chunks and transport saturation alongside
  commands, recovery, meters and media transfers.

Verification: production round trips deliver coherent reports across process and
browser boundaries, preserve independent host lifetime and do not starve existing
traffic. Commit the integration milestone.

### Stage 5 — Add the profiling window and statistics presentation

Depends on Stage 4.

- [x] Provide a dedicated profiling window reached from below Host Manager in the
  backend status LED panel, including its hover panel. Remove profiling UI from
  Settings. Connect its immediate switch to the default-off persisted preference. Expose acknowledged state and unavailable/error feedback through the
  existing settings/application flow.
- [x] Style the profiling window with a clear immediate switch, a copy of the
  overall DSP load bar, four aligned statistics columns, interval/sample summaries,
  and Live/Latest xrun. Add µs / % budget presentation using the fixed quantum
  duration at the current sample rate.
  Do not add profiling levels or an arm action.
- [x] Clear the view on graph-reset generation changes, show pending/empty rows
  truthfully and preserve the final table while off. Display incomplete/lost-data
  and trigger attribution outside the table.
- [x] Test immediate switching independently of asynchronous preference saving,
  rapid toggle ordering, popup/window opening, both unit presentations, persistence/defaults, runtime acknowledgement, duplicate node
  labels, known table values, rebuild/reordering/slot reuse, capture selection,
  unavailable timing and late reports after reset.

Verification: UI tests and a native/browser smoke exercise the actual production
control and report paths with the required table. Commit the UI milestone.

### Stage 6 — Measure overhead and complete end-to-end validation

Depends on Stages 2–5.

- [ ] Compare the same release workloads on the recorded pre-feature baseline,
  profiling off, profiling on, and on with a stalled UI/collector or xrun trigger.
  Cover graph sizes, Q=1/16/32/64/128, host partitions, routing-heavy audio/MIDI,
  recording, transitions and graph rebuilds. Measure native clock costs and
  worklet clock-import costs separately.
- [ ] Report whole-callback median/tails/maxima/misses, RT budget percentage-point
  cost, relative DSP slowdown, collector/UI CPU, bytes per second, storage high-water
  and losses. Keep profiling's own records out of benchmark timing methodology.
  Fix unexplained disabled overhead, unbounded work and deadline regressions;
  publish observed limits without presenting VM results as target-device acceptance.
- [ ] Exercise enable, record/play, live statistics, graph recompile/reset, injected
  and real host xruns where available, latest-capture statistics, disable/re-enable,
  disconnect/reconnect, host replacement and shutdown in native and browser UI flows.
  Verify unchanged audio/MIDI and no old-generation rows returning after resets.
- [ ] Run affected package suites, no-heap/lock and dependency-boundary gates,
  formatting, warning-denying workspace build, test-attribute and tracing inventory
  checks, the complete native suite and applicable Node/Chromium/Wasm/app smokes
  from current repository guidance. Record genuinely unavailable device facilities.
- [ ] Update user/developer documentation with duration/interval semantics, reset
  behavior, on/off costs, xrun attribution limits and measured platform results.

Verification: all immutable acceptance criteria have recorded evidence. Use
`cargo fmt --all`, `RUSTFLAGS="-D warnings" cargo build --workspace`,
`python3 scripts/check_shoop_test_usage.py` when Rust tests change, and the current
`.agents/info/test.md` suite commands in the environment prescribed by build guidance.
Commit the validation/documentation milestone.

### Stage 7 — Deliver and review

Depends on Stage 6.

- [ ] Push the implementation branch and open a PR targeting
  `feat/fixed-quantum-runtime`, describing behavior, reset semantics and measured
  overhead with links to validation evidence.
- [ ] Work through CI failures until required checks are green. Check for automated
  review and repeatedly address applicable feedback, rerunning affected checks,
  until reviewer approval. Record any unavailable reviewer explicitly.
- [ ] Update this plan with completed stages, final evidence, PR link and remaining
  target-device limitations. Do not merge as part of this plan.

Verification: the PR reflects the final implementation, required CI is green and
automated-review feedback is resolved through approval where a reviewer exists.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
- Do not reformat code or apply unnecessary whitespace changes.
- Use comments sparingly; code should usually explain itself. Keep comments concise,
  describe current behavior and generic usage patterns, and avoid references to
  other files/callsites, prior implementations or the instructions behind the change.

## Progress and evidence

Stage 0: implementation branch created from the investigated baseline. The prepared
environment is activated with `source /workspace/.setup/activate.sh`; it supplies
Rust 1.99.1, cargo-nextest 0.9.116 and native dependencies. Production profiling
will use the fixed execution kernel, with collection in the application backend
so worklet endpoint pumping performs only bounded forwarding.

Core/native milestone: added the portable recorder/collector, production fixed-kernel
node and phase boundaries, current-generation node metadata, bounded raw transport,
settings control and plain statistics table. Engine protocol is version 7, app
protocol is version 15 and shared-memory descriptors are SDM2 with an independent
profiling queue. Six focused native tests pass, including production round trips
at Q=1/16/32/64/128, exact average/worst/stddev, no disabled clock reads and automatic
xrun capture/reset. Runtime and engine dependency audits pass on all four targets.
The first warning-denying workspace build passed. Full native/browser verification
is ongoing; full-suite debug linking exhausted the environment disk, so subsequent
validation disables debug symbols and incremental artifacts through the task environment.

Production integration evidence: both packaged socket and shared-memory probes pass
runtime enable/disable/re-enable, live names/statistics, recording/playback,
UI reconnect, media reads and graceful shutdown. Settings persistence/default-off
and retry-on-full dispatch pass. The worklet artifact contract passes with a real
stereo/MIDI graph, exact clock-call checks, successful request completion,
completion timestamps, automatic xrun capture and no Wasm memory growth.

Browser capability finding: Chrome 147's actual AudioWorklet realm has no
`performance` object. The implementation acknowledges the requested setting and
reports timing unavailable there. It does not substitute logical audio time or
wall-clock time. Production imports are exercised separately with a monotonic
clock in the artifact contract; this is a platform capability limit anticipated
by acceptance criterion 8. No production profiling build flags were introduced.

Validation exposed a native shutdown reporting race: reset confirmation was read
before joining the driver, missing a reset completed by the final callback during
that join. The result is now read after ownership returns. Slow debug Q=1 callbacks
under competing builds made this visible; isolated shutdown tests pass. Native,
Node and Chromium suites and browser persistence smoke are being completed with
resource contention controlled. CI PR filters now include the requested base branch.

Native milestone: the complete CI-profile suite passes **2,645/2,645** tests
(4 facility-dependent skips) in 143 seconds. The latest focused recorder/runtime/UI/
backend run passes 13/13, including independent prescribed node work, rejection of
inconsistent chunk headers and oversized keys on 32-bit Wasm, capture selection,
clock-unavailable presentation and zero-heap saturation/reset/fault paths.
Warning-denying workspace builds, formatting, test attributes, tracing inventory
and four-target runtime/engine dependency audits pass. The profiling request now
returns its explicit successful completion; actual active state remains separately
acknowledged at the callback boundary.

Delivery ordering refinement: the implementation milestone can be committed and a
draft PR opened while the larger cross-platform CI matrix and automated review run;
benchmark evidence and final browser validation follow in a separate validation
milestone before marking delivery complete. This overlaps independent verification
without changing any goals or acceptance criteria.

Base reconciliation: while this task ran, the target advanced to
`3ca192e10552afe9ca9dfe6f4482c8b5081602bc`, including quantum API cleanup, wide host
I/O, host graph control and previews. Merge both sets of functionality, preserve
its new wire tags, append profiling request tag 35, and fence engine/app protocols
as versions 8/21. The matching uninstrumented benchmark baseline is now that
refreshed base; the earlier 2,645-test result describes the initial baseline and
will be superseded by fresh validation. The target also independently fixed the
same final-callback shutdown acknowledgement race.

Refreshed-base acceptance: warning-denying workspace build and all **2,682/2,682**
native tests pass (6 facility-dependent skips), in 155 seconds. All 15 profiling
integration tests and the production worklet artifact contract pass after the
protocol/API reconciliation. An additional recorder regression verifies that split
engine callbacks cannot create fictitious device deadlines: native/worklet hosts
use the complete outer callback budget; standalone offline callbacks retain their
own complete budget. Four-target dependency audits and the 395-module tracing
inventory remain clean.

Draft delivery: https://github.com/SanderVocke/shoopdaloop/pull/938 . No automated
review has appeared yet. The refreshed-base merge is ready for remote CI; final
release measurements and the complete Wasm/browser checks remain in progress.

Capacity refinement: a fixed 64-page stream cannot hold an ordinary 256-frame
Q=1 callback before collection is possible. Select 64–1,024 pages during preparation
from an 8 MiB byte budget (the existing minimum preserves large-node trace support),
keep history independent, and test all 256 node completions without loss before the
reader runs. Reset draining skips a bounded 64 stale pages per call. This improves
coverage without adding RT allocation or publication per node; actual sample-array
storage and queue high-water remain reported.

User-requested UI revision (2026-11-04): profiling belongs in its own window,
opened below Host Manager in the backend status LED hover panel. The window has
an immediate switch, overall DSP bar and µs / % quantum-budget presentation.
Acceptance criteria 1 and 3 and Stage 5 above reflect this explicit user change.
The hidden preference remains in the existing persistence system. Runtime requests
precede background preference saves; a pending local choice cannot be undone by
an earlier save, and ordinary Settings drafts preserve the current profiling choice.
The first five focused window/popup/persistence/formatting tests pass.

Dedicated-window verification: the complete native suite passes **2,683/2,683**
tests (6 facility-dependent skips). A running native app verifies opening from
the LED hover panel, immediate switching, live statistics, the overall DSP bar,
budget presentation and background preference persistence. Further review adds
an off-state activation regression: a newly activated layout invalidates retained
data even with recording stopped, without clock reads or heap activity. All 16
focused recorder/runtime/window tests pass, including real pointer input for the
switch. Final native/Wasm gates and release measurements use this correction.
