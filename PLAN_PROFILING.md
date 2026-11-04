# Runtime profiling implementation plan

Status: completed. Implementation and all local acceptance gates pass. The user
waived remaining CI verification on 2026-11-05 and requested final delivery.

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
- [x] Verify runtime toggle outcomes, reconnect/resync and host replacement,
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

- [x] Compare the same release workloads on the recorded pre-feature baseline,
  profiling off, profiling on, and on with a stalled UI/collector or xrun trigger.
  Cover graph sizes, Q=1/16/32/64/128, host partitions, routing-heavy audio/MIDI,
  recording, transitions and graph rebuilds. Measure native clock costs and
  worklet clock-import costs separately.
- [x] Report whole-callback median/tails/maxima/misses, RT budget percentage-point
  cost, relative DSP slowdown, collector/UI CPU, bytes per second, storage high-water
  and losses. Keep profiling's own records out of benchmark timing methodology.
  Fix unexplained disabled overhead, unbounded work and deadline regressions;
  publish observed limits without presenting VM results as target-device acceptance.
- [x] Exercise enable, record/play, live statistics, graph recompile/reset, injected
  and real host xruns where available, latest-capture statistics, disable/re-enable,
  disconnect/reconnect, host replacement and shutdown in native and browser UI flows.
  Verify unchanged audio/MIDI and no old-generation rows returning after resets.
- [x] Run affected package suites, no-heap/lock and dependency-boundary gates,
  formatting, warning-denying workspace build, test-attribute and tracing inventory
  checks, the complete native suite and applicable Node/Chromium/Wasm/app smokes
  from current repository guidance. Record genuinely unavailable device facilities.
- [x] Update user/developer documentation with duration/interval semantics, reset
  behavior, on/off costs, xrun attribution limits and measured platform results.

Verification: all immutable acceptance criteria have recorded evidence. Use
`cargo fmt --all`, `RUSTFLAGS="-D warnings" cargo build --workspace`,
`python3 scripts/check_shoop_test_usage.py` when Rust tests change, and the current
`.agents/info/test.md` suite commands in the environment prescribed by build guidance.
Commit the validation/documentation milestone.

### Stage 7 — Deliver and review

Depends on Stage 6.

- [x] Push the implementation branch and open a PR targeting
  `feat/fixed-quantum-runtime`, describing behavior, reset semantics and measured
  overhead with links to validation evidence.
- [x] Complete CI/review follow-up to the user-approved stopping point. Remaining
  CI verification was explicitly waived on 2026-11-05. No automated reviewer or
  inline review feedback was available. Record observed results below.
- [x] Update this plan with completed stages, final evidence, PR link and remaining
  target-device limitations. Do not merge as part of this plan.

Verification: the PR reflects the final implementation and recorded local evidence.
Remaining CI gating is waived by the user; this is not a claim that every remote
check passed. Automated review was unavailable. The PR remains unmerged.

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

The implementation branch began at `cb79706e95350539638368bcca9f50b1601795ae`.
The target subsequently advanced; merge `3ca192e10552afe9ca9dfe6f4482c8b5081602bc`
and preserve its graph API, wide host ports, previews, host manager and wire tags.
A further clean merge includes the final MIDI recovery fixes at
`df4683527dbde0ec7483c3e522fc3873e39a81c8`. The final matching uninstrumented
measurement baseline is `2e19210bdbb5e32510322f8b8344715ce7cbefe5`, including
shared-media and buffered-processor support. Both release binaries use that base.
Production profiling instruments `GraphRuntime::{process,run_tick,run_graph}`.
Engine/app protocol versions are 8/21; SDM2 adds independent profiling admission.

Implemented contracts: prepared no-std recorder, elapsed-time host clocks, raw
callback/tick/phase/node-ready records, bounded live streaming and independent
pre/post-xrun history, non-RT Welford statistics, coherent generation-fenced
transport and application views. Failed/identical recompiles, activation, re-enable
and engine replacement invalidate all prior data. Activation while off also clears
the retained report, with no clock reads or heap activity. Partial faults exclude
unfinished steps. Host deadlines cover complete driver callbacks, including split
engine calls; external AudioContext and engine stream frames remain distinct.

The dedicated window opens below Host Manager in the backend LED hover/click panel.
It has an immediate switch, the overall DSP bar, responsive aligned table, Live/
Latest xrun and microsecond/quantum-budget presentation. It has no Settings-window
UI. Runtime requests precede background preference writes; stale in-flight saves
and unrelated settings drafts preserve the latest runtime choice. Actual pointer
input, persistence/retry/rapid-order tests and native visual inspection pass.
[Screenshots and usage](docs/runtime_profiling.md).

Release evidence: [measurement report](docs/runtime_profiling/README.md), 288
randomized runs, all raw callback durations/periods and summaries, nine balanced
baseline/off pairs for the largest initial outlier, separate native/JS/Wasm-import
clock and final-window CPU timings. The baseline/current workload source matches
exactly after removing profiling setup and collection. Ordinary collection has no
live-batch drops; stalled readers demonstrate bounded loss without blocking audio.
Small-quantum/large-graph workloads already exceed deadlines on the baseline and
have explicit throughput/collection limits. The refreshed paired disabled check is +0.21%
relative DSP time (+0.041 callback-budget percentage points) on this VM. The full
matrix also has negative deltas, demonstrating variation rather than acceleration.
These observations provide no
universal zero-cost or target-device guarantee.

Validation completed:

- Warning-denying workspace build and full native suite: **2,757 passed**, seven
  skipped tests. Current-revision coverage passes, including a legacy meter test
  that failed in an earlier run. The 16 focused recorder/runtime/window tests include
  no-heap saturation/reset/fault paths, independent prescribed node work and all
  five quanta with arbitrary host partitions and unchanged audio/MIDI.
- Packaged socket and shared-memory probes: default off, enable, current names and
  statistics, recording/playback, media reads, reconnect, disable/re-enable and
  graceful shutdown pass.
- Node/Wasm: **35 packages, 2,280 tests passed** in the complete suite on the
  reconciled base. [Results](docs/runtime_profiling/node-validation.json).
- Optimized Trunk browser bundle builds. The hosted production browser workflow
  passes audio/MIDI recording/playback, waveforms/details, controls, cancellation,
  save/reload/resave, restored playback, bus removal/recreation, generation resets
  and profiling off/re-enable/off. Browser reports zero xruns in this run.
- Fresh production worklet/codec artifact contract passes real stereo/MIDI graph
  execution, successful control completion, node timestamps, off clock-call checks,
  automatic capture and stable Wasm memory.
- All three bare-metal target builds pass; targets without 64-bit atomics report
  timing unavailable with no lock fallback or user-facing build option. Both
  standalone host-fixture lockfiles include the new dependency, preserve existing
  versions, and pass locked resolution/build checks.
- Formatting, test-attribute policy, 423-module tracing inventory, four-target
  runtime/engine dependency boundaries and the new crate's Wasm test dependency
  audit pass. Chromium verification passes **898 tests across fourteen packages**, including
  the MessagePort credit tests and the new buffered-processor/engine contracts.
  One Wasm harness-load timeout passes on the isolated buffered-package retry.
  [Results](docs/runtime_profiling/chrome-validation.json).

Platform limits: this environment has no physical audio/MIDI devices. Native
integration uses the dummy host, prescribed/injected xruns and observed dummy-host
lateness; remote JACK acceptance supplies an independent native device-server gate.
Chrome for Testing 147.0.7727.117 has no `performance` object in its actual
AudioWorklet realm. The requested preference is acknowledged but elapsed timing
is explicitly unavailable; logical audio time and `Date.now()` are never used as
substitutes. The raw artifact contract separately exercises an injected monotonic
clock. These limits are anticipated by acceptance criterion 8.

Delivery: https://github.com/SanderVocke/shoopdaloop/pull/938 targets
`feat/fixed-quantum-runtime`. Implementation and release evidence are pushed;
all local validation passes. Remaining CI verification was waived by the user on
2026-11-05. CI monitoring has stopped, and no automated reviewer was available.
The release CI large-media workflow exposed a startup race; the self-test now
waits for queued and accepted operation acknowledgements before requesting a new
session, including after profiling acknowledgement and bus recreation.
The browser MessagePort has a separate 256-record consumption-credit window,
including status, names and data. Credits return after client decoding and are
coalesced into one main-thread microtask. Saturation blocks only profiling
publication, preserves bounded RT history/stream ownership and permits reliable
controls. Native flow/saturation tests pass. The final artifact verifies 512 callbacks
without returned credits stay within the 256-record window and that reliable
disable still succeeds. No Wasm memory growth occurs. Regular and large-media
workflows both pass with zero reported xruns.
The PR remains unmerged. No automated review has appeared at the latest check.

Late target reconciliation: PRs #927 and #937 advanced the target to `2e19210b`
and caused conflicts before CI could start on `30bc167f`. Preserve both changes:
shared-media stores and input epochs, buffered-processor support, both crate
lockfile entries, both tracing inventories, and both Wasm dependency audits.
The warning-denying merged workspace build and 423-module inventory pass.
The full native and Node suites, fourteen affected Chromium packages, all bare-metal
builds and the matching benchmark comparison pass on this base. Consolidating the
engine test allocator resolves the two upstream test modules each declaring a global
allocator. Browser startup now waits for the initial persisted profiling preference
and pending operations before the self-test enables profiling; this prevents the
initial default from reversing its first runtime toggle. The final optimized
regular and large-media workflows pass in isolation with zero reported xruns. An
earlier large-load acknowledgement timed out during concurrent testing; it passes
in isolation and both hosted/single-file workflows pass remotely.

At the user-directed CI stopping point, revision `5036ad06` had passed CodeQL,
docs, coverage, bare-metal, JACK, all three process-inspection jobs and the
shared-media foundation. Windows debug reported a native application smoke-test
timeout waiting for MIDI detail; its later missing-evidence upload failure followed
from that test failure. The workflow passes twenty local repetitions, including ten
on one CPU with tracing enabled. Its Windows trace shows ordered note-on, note-off
and play submissions and continued application polling, without establishing the
cause of the incomplete detail. Remaining jobs were running or queued when
monitoring stopped; their results are not asserted here. The user explicitly
waived further CI verification and requested delivery. No automated review or
inline thread was available at the last review check.
