# Interactive runtime profiling graph implementation plan

Status: implementation and local validation complete; final CI delivery gate pending.

Target branch: `feat/runtime-profiling-graph`, targeting `feat/fixed-quantum-runtime` after basic profiling PR #938 was merged.
Investigated baseline: `ac4748a4` (2026-11-04).

## Goals and scope

Extend the dedicated profiling window with an automatically laid out executable
audio/MIDI graph. Make processor cost, upstream/downstream influence and measured
cumulative output readiness visible in both Live and Latest xrun views. Retain the
existing table and add selection of individual captured callbacks/ticks with a
compact execution timeline for investigating xruns.

Intended graph widget: `nannou-org/egui_graph`, as requested by the user. Work it
into the existing egui UI using its rendering, interaction and automatic layout;
confirm compatibility and the required behaviors in Stage 0 before integration.

This is the user-approved follow-up to `PLAN_PROFILING.md`: its exclusion of graph
and timeline UI no longer applies to this extension. Preserve its recording,
real-time safety, toggle, interval and reset contracts. Coordinate with any
remaining validation on that branch; do not treat its draft PR as finished.

Out of scope: path cost mode, dependency-path sums or longest-path estimates,
critical-path attribution, graph editing, historical topology comparison,
additional capture history, export formats and device playback/signal latency.

User-approved addition (2026-11-04): correlate processor/group labels with the UI's
track and loop names in both views. Preserve processor role and slot for duplicate
labels; resolve through backend identities and update names without relayout.

## Immutable acceptance criteria

1. The existing profiling window offers Graph and Table tabs with shared immediate
   on/off, overall DSP bar, Live/Latest xrun and µs/% quantum-budget controls. The
   table retains its execution-order Node/Avg/Worst/Stddev presentation and values.
2. The graph represents the active compiled plan, including host inputs/outputs,
   audio/MIDI connections and distinct processor identities. It uses automatic
   left-to-right layout, zoom, pan, fit-to-view and collapsible track/loop/bus
   groups. Measurements updating must not move the layout.
3. Processor hover/pinned details show the existing complete-step Avg, Worst and
   population Stddev, sample count and identity. Link details show Avg, Worst and
   population Stddev of measured cumulative readiness from graph execution start
   to the source node's output-ready timestamp in each tick. All outgoing links
   of a processor share that source-readiness measurement. Labels distinguish
   readiness from link transfer cost, exclusive CPU time and audible latency.
4. Compute cumulative values per accepted tick before aggregating them. Include
   sequential work from independent branches that actually ran before the source.
   Never obtain cumulative worst/stddev by summing per-node statistics. Missing or
   unfinished measurements remain unavailable rather than becoming zero samples.
5. Nodes and links have separate, labeled color scales for processing duration
   and cumulative readiness, with Avg/Worst/Stddev selection and numeric legends.
   Hover highlights upstream dependencies and downstream affected outputs; click
   pins details. Structural reachability does not claim to establish xrun cause.
6. Latest xrun initially shows statistics over the actual retained capture interval.
   Users can select a captured callback and one of its ticks to see actual node
   durations and link readiness for that tick, synchronized with an execution
   timeline of recorded callback/tick/phase boundaries and the callback deadline.
   Selected-tick values are labeled actual, not Avg/Worst/Stddev. Preserve trigger,
   attribution uncertainty, receiving/truncation/loss and clock-resolution status.
7. Topology, statistics and capture details carry coherent engine/generation/graph
   identity. Every rebuild (including identical or failed builds), activation,
   replacement, reconnect and profiling reset invalidates stale measurements and
   selections according to existing lifecycle rules. Never attach old timings to
   reused node slots. Disabling retains a coherent final report and its topology
   until reset/disconnection, as the current report does.
8. Deliver through the existing native, process/socket, shared-memory and worklet
   application boundaries. Graph layout, aggregation and capture decoding run
   outside rendering. No added per-edge clock reads, blocking, render allocation,
   unbounded publication, or traffic that starves commands/recovery/media/meters.
   Preserve explicit timing-unavailable behavior and production audio/MIDI output.

9. Include callback entry/exit virtual nodes with dotted context dependencies to
   the first/last processors. Measure entry to the first graph start and final
   processor completion to callback exit per complete callback. Account for work
   between graph executions separately. Retain original callback budgets, mark
   partial callbacks, and leave missing boundaries unknown. This addition was
   explicitly approved for this PR by the user.

## Design rules and constraints

- Use the executable `PlanDescriptor.steps/connections`, not a reconstruction of
  the visible session or the external host connection graph. Export bounded
  metadata from prepared descriptors and associate publication with actual
  activation. Include host port/type/channel, execution order and structured group
  identity; do not infer groups or identities by parsing display labels.
- For node `v` in tick `k`, retain the existing duration between consecutive
  executor boundaries and add `ready(v,k) = node_ready(v,k) - graph_start(k)`.
  Use Welford accumulation for readiness independently of node duration. Fan-in
  does not sum predecessor values; fan-out reuses the source value.
- Host-source links use zero graph-relative readiness only where host input is
  already available at graph start; label this baseline explicitly. Processor-to-
  host links still show source readiness, not completed device output. Show final
  host routing in the timeline using its recorded phase boundary. Host-to-host
  bypass readiness is unavailable unless the recorded boundaries establish it;
  do not invent per-edge transfer timestamps.
- The timeline uses offsets in one recorded monotonic clock domain. Join host
  callbacks, split engine parts and ticks using their actual IDs/frame intervals
  and boundaries; do not assume the engine callback ID equals the outer host ID.
  Use complete outer callback timing/budget (complete offline engine callback as
  fallback), never a separate device deadline per tick or split engine part.
  Unmeasured gaps stay labeled gaps; preemption cannot be separated from work.
- Keep capture-interval aggregates separate from selected-tick actual values.
  Aggregated node/link maxima can come from different ticks; never present the
  aggregate graph as one observed iteration. Do not automatically identify a
  delayed host notification's callback as the offending callback.
- Percentages for node duration/readiness use the existing fixed quantum period;
  callback deadline/load uses that callback's actual budget. Carry the relevant
  sample rate/quantum/budget with the measurement context so frozen values cannot
  silently be reformatted against a different host configuration.
- Integrate `egui_graph` rather than implementing navigation, edge geometry and
  hit testing ourselves. Use its socket-aware layered layout and routed edges;
  resolve egui version compatibility and verify native/Wasm behavior, deterministic
  geometry, grouping and large-graph cost before pinning the dependency. Address
  gaps with a thin application adapter and small reviewed fixes where practical.
  Consider another widget or separate layout/custom rendering only if documented
  blockers make `egui_graph` unsuitable. Preserve a practical screen-space edge
  hover radius, cache geometry by topology/group expansion and keep stable ordering.
- Collapsed groups preserve visible cross-group connections and aggregate
  structural reachability. Do not sum member worst/stddev or invent group cost;
  retain access to member details. Clearly label bundled links and their members.
- Retain the existing raw recorder and timestamp density where possible. If
  metadata such as exact callback budget is absent, add only bounded context
  fields needed for truthful display. Preserve `no_std`, unsafe-code policy,
  independent profiling admission and existing off-path clock/storage behavior.
- Keep live aggregates bounded and refreshed at most five times per second.
  Reuse retained capture packets; expose a bounded callback index and requested
  detail pages through the app service. Do not repeatedly copy the up-to-16 MiB
  raw capture into the application snapshot, whose current maximum is 8 MiB.
  Define byte/count/work caps, cancellation and explicit partial states before
  implementation; a stalled client must not hold up capture or audio.

## Investigated implementation anchors

Paths below are relative to `src/rust/` unless stated otherwise.

| Area | Current implementation |
| --- | --- |
| Active plan and timing boundaries | `rt_runtime/src/{graph,executor}.rs`, `graph/activation.rs` |
| Plan construction, reset, metadata and forwarding | `shoop_rt_engine/src/{graph,jobs,profiling,endpoint,pool,cells,replacement}.rs` |
| Raw packets, statistics and retained capture | `rt_profiling/src/{lib,collector,recorder}.rs`; `Collector::raw_capture()` |
| Engine records and admission | `shoop_rt_protocol/src/{message,schema,codec}.rs`, `shoop_rt_transport/src/carrier.rs` |
| Backend/app projection and detail requests | `shoop_rt_backend/src/{incoming,backend}.rs`, `shoop_backend/src/lib.rs`, `shoop_app/src/service/{projection,commands}.rs`, `shoop_app_api`, `shoop_app_protocol`, `shoop_app_client/src/runtime.rs` |
| Window, drawing and interaction | `shoop_egui/src/{profiling,app_widget}.rs` |
| Existing evidence and docs | `rt_runtime/tests/profiling.rs`, `shoop_rt_backend/tests/profiling.rs`, engine protocol/transport profiling tests, `shoop_app_client/tests/projection.rs`, `scripts/benchmark_profiling.py`, `shoop_egui/examples/profiling_table_benchmark.rs`, root `docs/runtime_profiling.md` |

Current gaps: `Row/Report/View` contain duration aggregates only; layout records
enumerate active node keys but no edges; capture packets remain in the collector
and are not projected into the UI. Current protocol versions are engine 8/app 21.

## Graph widget evaluation (2026-11-04)

This is a source/API review, not a compiled integration or performance result.
The user selected `egui_graph` as the intended integration. Stage 0 validates that
choice and resolves compatibility; alternatives below are contingency references.

| Candidate | Fit and remaining work |
| --- | --- |
| [`egui_graph`](https://github.com/nannou-org/egui_graph) | Best initial fit: model-agnostic widgets, socket-aware left-to-right layered layout, routing waypoints, node/edge selection, edge hover responses, per-edge strokes/custom painting and `Graph::immutable(true)` preserving navigation/selection. Verify group projection, fit-to-view, hover under zoom and native/Wasm behavior. |
| [`egui-snarl`](https://github.com/zakarumych/egui-snarl) | Strong processor/pin presentation, custom node content, navigation, node hover and wire widgets. The inspected API does not supply automatic whole-graph layout or an equivalent single read-only switch; evaluate separate layout integration, wire-wide hover/pinning and suppression of edit gestures before choosing it. Individual node collapse is not track/loop/bus group collapse. |
| [`egui_graphs`](https://github.com/blitzarx1/egui_graphs) | Hierarchical layout, zoom/pan, node/edge hover/selection and custom renderers, with native/Wasm support documented. Its README explicitly says development is inactive; lower priority for a new dependency, and processor port/group presentation still needs evaluation. This is distinct from `egui_graph`. |

The inspected upstream manifests declare `egui = 0.36` for all three, whereas this
branch uses `egui = 0.37`. They are not currently drop-in dependencies: types from
the two egui versions cannot be mixed. Recheck releases during Stage 0; prefer a
compatible upstream release, otherwise assess a small reviewed/pinned adaptation
of the preferred widget and its dependencies. Do not downgrade the application's
egui version or assume a manifest-only bump works. `egui_graph`/`egui_graphs` use
MIT; `egui-snarl` uses MIT OR Apache-2.0. Sources: the repositories' README,
Cargo.toml and widget APIs (`src/lib.rs`, `src/edge.rs`, `src/ui/viewer.rs`).

Track/loop/bus collapse and upstream/downstream highlighting remain application
projection/interaction work; none of the reviewed APIs establishes those complete
profiling behaviors out of the box. Reuse widget layout/rendering where it fits,
while keeping profiling identities, metrics and group membership in our model.

## Staged implementation

### Stage 0 — Reconcile branch and finalize bounded contracts

- [x] Fetch the current profiling branch and existing PR state; preserve unrelated
  work and remaining profiling validation. Continue on `feat/runtime-profiling-graph` from the profiling branch;
  the user explicitly requested a separate branch and stacked PR.
- [x] Re-read repository build/test/style guidance and reconcile the anchors above.
  Inventory descriptor activation, reset paths, host callback matching and app
  detail-request patterns. Record exact topology/detail limits and publication
  ownership, plus how unavailable/partial data is represented.
- [x] Resolve `egui_graph` compatibility with egui 0.37: prefer a compatible
  upstream release; otherwise prototype the smallest reviewed adaptation, pinned
  to an exact revision or maintained repository dependency with license retained.
  Keep the existing application egui version and keep graph UI dependencies out
  of the runtime/recorder crates. Record any patch maintenance responsibility.
- [x] Prototype its `Graph`, `View`, `Node`, `Edge` and `layout_routed` APIs with
  representative branching/grouped profiling graphs. Verify source-colored links,
  edge-wide hover/pinning, `Graph::immutable(true)`, fit/zoom/pan, collapsed group
  projection and stable cached geometry on native and Wasm. Prefer adapter work
  and small fixes over switching libraries; document concrete blockers before
  considering a fallback. Record the pinned dependency and layout/cache strategy.
- [x] Specify generation-tagged topology, readiness aggregates, capture index and
  paged detail contracts, including original callback budgets and measurement units.

Verification: contracts fit the existing transport/snapshot caps, match actual
callback/activation semantics and require no per-edge instrumentation. The widget
prototype compiles with the application's egui version on native/Wasm and records
the interaction/grouping evidence or specific blockers. Commit the planning/
contract milestone before dependent behavior changes.

### Stage 1 — Publish the executable topology

Depends on Stage 0.

- [x] Export processor/host endpoints, typed port connections, execution order and
  structured group metadata from the prepared plan with bounded storage and
  work. Publish only a complete descriptor associated with the active graph.
- [x] Add bounded engine metadata chunks, collection/reassembly and resync;
  validate endpoint IDs, counts and complete identities. Keep old or partial
  descriptors from decorating new measurements, including activation mid-callback.
- [x] Cover branching, fan-in/out, audio/MIDI, host bypass, disconnected nodes,
  duplicate labels, failed/identical rebuilds, replacement and slot reuse.

Verification: compare exported nodes/edges directly with the executable descriptor;
exercise stale/missing/reordered chunks and stalled readers under RT safety guards.

### Stage 2 — Collect readiness and decode captured iterations

Depends on Stage 0; integrate with Stage 1 identities before publication.

- [x] Add per-node readiness accumulators to live/capture reports without changing
  table duration semantics. Validate complete records before accepting statistics;
  preserve counts and fault/partial markers independently for each metric.
- [x] Build a bounded capture callback/tick index from assembled raw records, with
  actual node durations/readiness, phases, original budgets and loss/gap status.
  Add the minimal bounded recording context only if current fields cannot supply it.
- [x] Add independent prescribed-time fixtures: unrelated 80 µs work between a
  20 µs source and 10 µs dependent gives 110 µs readiness, not 30 µs. Include
  anti-correlated spikes proving cumulative worst/stddev cannot be summed,
  plus fan-in/out, large clock origins, unfinished nodes and malformed packets.
- [x] Verify zero-/multi-tick callbacks, buffered and split host callbacks,
  differing host/engine frame origins, delayed notifications and truncated capture.

Verification: exact independent expected statistics and selected-iteration values
match raw boundaries; no fabricated tick deadlines or xrun attribution appear.

### Stage 3 — Carry coherent graph and capture data to the UI

Depends on Stages 1–2.

- [x] Extend backend/service/API/app protocol/client projection for immutable
  topology, readiness and capture index; add bounded on-demand selected detail
  requests through existing application boundaries, without direct egui/engine access.
- [x] Tag requests/responses with connection, generation, graph, capture and
  selection identity. Reject stale responses after reset, replacement or a rapid
  selection change. Define paging, cancellation, reconnect/resync and final-off data.
- [x] Bump affected protocol versions/compatibility descriptors and update codecs,
  socket/shared-memory/worklet adapters and independent admission limits.
- [x] Exercise oversized/malformed data, interrupted assembly, slow clients and
  rapid graph/capture changes; keep snapshots below their existing size limit.

Verification: production transport/client round trips preserve identical topology,
statistics and selected detail; saturation does not starve reliable or meter traffic.

### Stage 4 — Add graph presentation and interaction

Depends on Stage 3 and the Stage 0 graph widget/layout decision.

- [x] Add Graph/Table tabs to the existing window and shared selectors; retain
  immediate toggle, persistence behavior, overall DSP display and table values.
- [x] Add an `egui_graph` adapter mapping generation-scoped processor/host IDs to
  widget node IDs and audio/MIDI ports to sockets. Keep `View`, layout/routes and
  selection as UI state; keep authoritative topology and measurements in the
  application model. Set immutable mode so inspection cannot edit the DSP graph.
- [x] Use `layout_routed` on topology/group-expansion changes and render through
  `Node`/`Edge` widgets with cached left-to-right geometry, zoom, pan and fit.
  Project collapsed groups into summary nodes while preserving member/edge IDs
  for inspection and expansion. Handle large, empty, disconnected and
  timing-unavailable graphs without freezing the UI or moving live layouts.
- [x] Use widget node/edge responses for hover and pinned details, and per-edge
  strokes/custom node framing for metric colors and upstream/downstream emphasis.
  Add group/member inspection, separate legends and metric selection with numeric
  values, unavailable styling and clear readiness wording.
- [x] Add focused UI checks for hit testing after transforms, overlapping/bundled
  edges, collapse/expand reachability, stable geometry, unit conversion, source
  switching, lost-data feedback and reset/selection invalidation.

Verification: deterministic egui interaction checks plus native/browser visual
inspection demonstrate readable topology and correct values without layout jitter.

### Stage 5 — Add captured callback/tick selection and timeline

Depends on Stages 3–4.

- [x] Keep capture-interval aggregation as the default; add callback and tick
  selection, navigation back to interval statistics and bounded detail loading.
- [x] Synchronize selected-tick graph values with a compact timeline showing all
  retained ticks, node spans and recorded non-node phases within the selected
  callback, its actual deadline and any missing/uncertain regions.
- [x] Show actual values for selection, interval Avg/Worst/Stddev otherwise;
  retain separate node/link scales. Make trigger/attribution uncertainty and
  partial/receiving status visible in both graph and timeline.
- [x] Test rapid selection, capture replacement, incomplete parent callbacks,
  missing details and graph reset while a detail page is in flight.

Verification: an injected spike is visible in the independently known callback/tick
and processor; delayed host notifications never falsely identify an offender.

### Callback envelope addition — approved for this PR

- [x] Preserve the exact graph-execution count across split engine parts, mark
  partial callback records, and derive complete callback prefix/suffix/between
  costs outside rendering. Missing boundaries remain unknown.
- [x] Add virtual entry/exit nodes and dotted context dependencies, with interval
  statistics and actual whole-callback selection independent of selected tick.
- [x] Cover multiple ticks, split host callbacks, missing records and clockless
  selection with collector/recorder/UI checks; document the measured envelope.

### Stage 6 — Deliver and complete end-to-end validation

Depends on Stages 1–5; this is the final implementation/validation stage.

- [x] Extend user docs with readiness origins, separate color meanings, aggregation
  versus actual selection, grouping, units, unavailable timing and xrun limits.
- [x] Run affected collector/runtime/engine/backend/protocol/transport/app/UI tests,
  RT no-heap/lock and dependency-boundary guards, then the complete suite prescribed
  by current `.agents/info/test.md` in the selected build environment. Run formatting,
  warning-denying workspace build, test-attribute and tracing inventory checks.
- [x] Run native packaged socket/shared-memory and browser/worklet flows: enable,
  inspect graph/table, record/play, route/rebuild, switch units, inject xrun,
  inspect interval and selected callback/tick/timeline, disable/re-enable,
  reconnect/replace host and shut down. Exercise actual unavailable browser clocks
  and injected-clock contracts separately; report unavailable real facilities.
- [x] Compare release off/on/stalled performance with the recorded pre-graph
  profiling baseline across graph sizes and Q=1/16/32/64/128. Record callback tails/
  misses, publication bytes, collector memory/CPU, capture detail latency, layout
  time and steady UI frame cost. Resolve unexplained RT regressions and verify
  fixed bounds under dense topology, capture saturation and slow clients.
- [x] Commit meaningful milestones, push `feat/runtime-profiling-graph` and update its
  dedicated PR against `feat/fixed-quantum-runtime` with final graph scope and evidence.
  Basic profiling PR #938 is already merged. Do not merge this graph PR.
- [ ] Work through CI failures until required checks are green. Check for automated
  review as it arrives and repeatedly address applicable feedback through reviewer
  approval; explicitly record an unavailable reviewer. Re-run affected checks after
  fixes, keep this plan current and leave the final PR concrete and reviewable.

Verification: record evidence against every acceptance criterion, relevant native/
Wasm/browser checks, performance bounds and final CI/review state. Distinguish
unavailable facilities from passing checks; do not mark delivery complete early.

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

## Implementation decisions and evidence

Stage 0: baseline `ac4748a4` matches the remote profiling branch. Vendored
`egui_graph` 0.19.1 at `1a5e2b5385c8c81a49cbde836317848b06849966` under
`third_party/egui_graph`, retaining its MIT license. The manifest selects egui
0.37; five deprecated API uses were replaced with their current equivalents.
Shoop maintains this adaptation until a compatible upstream release is available.
Warning-denying native workspace build and Wasm widget check pass. Four focused
profiling UI tests pass, including grouped projection, cached geometry, capture
selection fencing and existing table formatting/reset behavior.

Contracts: visualization metadata is capped at 4,096 nodes and 16,384 edges;
exceeding that reports graph capacity unavailable while leaving audio/table timing
intact. Each endpoint poll forwards at most 16 topology records in the independent
profiling lane, using prepared active/staged arrays. Publication follows actual
activation acknowledgement. Captures retain the existing 16 MiB raw-packet bound
and expose at most 8,192 trace summaries; selected detail contains at most 256 node
steps and 64 phase boundaries per page. Topology/index/detail use immutable shared
ownership in app snapshots, not copies of the complete raw capture. Requests and
responses carry generation/capture/callback/tick/page identity; the existing app
connection authority fences reconnects. Switching selection logically cancels old
pages by rejecting responses that no longer match. Engine/app versions are 10/24;
the shared-memory descriptor layout and its independent profiling lane are unchanged.

Core evidence: 14 recorder/collector tests and the profiling wire codec test pass.
Independent fixtures verify 110 µs readiness despite a 30 µs signal-path node sum,
anti-correlated spikes, atomic topology assembly and original callback budgets.
Production backend round trips pass across Q=1/16/32/64/128 with typed topology,
readiness, automatic capture selection and rebuild invalidation. Final broad
native/Wasm/browser, visual, saturation and performance evidence remains pending.

Delivery refinement (user request): publish the current implementation to a new
`feat/runtime-profiling-graph` branch and draft PR against `feat/runtime-profiling`,
then continue implementation/validation there. The initial broad native run found
two protocol golden fixtures needing the intended 21-to-22 version update (fixed)
and a legacy graph playback test failure still under investigation. These are not
passing evidence; the draft remains incomplete until validation finishes.


Validation checkpoint (2026-11-04): PR [#939](https://github.com/SanderVocke/shoopdaloop/pull/939)
contains the graph implementation. The complete native suite passed 2,699 tests
with six facility-dependent skips. Subsequent UI fixes pass all eight profiling
widget tests, including pointer-driven zoom/pinning and group-selector collapse;
warning-denying workspace builds and the test/tracing/boundary/parity checks pass.
The native packaged socket and shared-memory probes and production browser
record/play/save/load/routing workflow pass (browser xruns: zero). The raw worklet
contract passes topology, original budget, clock/timestamp and toggle assertions.
The seven affected Node and Chrome Wasm packages pass; the final UI runs contain
240 tests each. An additional application Wasm check exposed a pre-existing
native-only import used by a cross-platform profiling settings test; its gate is
corrected, with the application rerun pending.

User label correlation is implemented through frozen topology-to-backend entity
mappings, then application projection to current track/loop/bus labels. Loop names
are qualified by track, and both graph/table retain processor role and slot.
Renaming does not reset timing or layout. Link details and collapsed groups use
the resolved names too. Independent native and encoded service tests verify the
mapping and rename behavior. Native screenshots in `docs/images/profiling_graph*.png`
use the prescribed-time preview fixture, not a production capture. Visual checking
also caught an unusable group popup; an explicitly identified group selector and
pointer-input regression test fix it.


Base integration: the profiling branch advanced to `af44ae10` during graph
validation. Merge its disabled-activation generation invalidation and table
layout/contrast fixes, retaining UI-qualified names. The PR cannot run CI while
it has merge conflicts; the resolved merge is undergoing renewed validation.
The original release comparison remains explicitly against `ac4748a4`.
A dense 4,094-processor / 16,293-link UI fixture exposed unnecessarily fine
curve sampling at far zoom. The adapter now selects curve detail and a six-pixel
hover radius in screen space using the widget's supported APIs. A pointer-input
test covers link hover/pinning across zoom transforms. Final measurements and
CI state will be recorded after the merge gates pass.


A subsequent base update, `30bc167f`, includes runtime recovery/publication fixes
and a 256-record browser profiling credit window. Preserve those changes while
returning credits for topology tag 11 as well as status/data/name tags 8–10.
The MessagePort consumption test now exercises graph records explicitly, and
the worklet artifact's normal and stalled credit accounting includes topology.
The earlier native merge passed 2,703 tests; the refreshed full native, affected
Wasm/browser and release comparisons are being rerun against this base.


Final identity and delivery corrections: newly decoded but semantically identical
topology must preserve the cached layout, pan/zoom and pins. The pointer-input
refresh fixture now uses a new topology allocation to cover encoded boundaries.
A zero-tick callback has no graph-start baseline; its host links stay unavailable
until a retained selected tick supplies that boundary.

The full browser gate exposed a missing `consumed` delegation in the audio-side
MessagePort wrapper. Forwarding it returns the profiling credits and unblocks
metadata after the 256-record window fills. Ownership metadata now additionally
carries each prepared strip/loop/bus slot generation from the staged model. The
backend resolves only matching generations and retries incomplete mappings after
creation acknowledgements, preventing reused slots from acquiring old names.
Engine/app protocols advance to 10/24 for this complete wire contract. Final
validation is in progress; the browser failure is not passing evidence.


### Callback envelope and identity validation (2026-11-04)

The user approved virtual callback entry/exit nodes for this PR. They now use
complete callback prefix/suffix measurements, with separate between-execution
accounting and exact graph counts across engine parts. Partial callback records
carry no independent device deadline. Existing timestamp hooks are reused.
Track/loop ownership includes prepared slot generations; incomplete name mappings
are retried, and identical serialized topologies preserve layout and selection.
The browser audio wrapper forwards consumed-record credits to prevent a long
profiling session from exhausting its metadata window.

The warning-denying workspace build passes. The full native CI-profile command
passes 2,781 tests with seven skips; the final graph cache adjustment and nonzero
wire-count fixture receive focused reruns. All 134 Python checks and the test
attribute, tracing (427 modules), parity and dependency-boundary gates pass.
Fresh Wasm/browser, final release measurements, screenshots and delivery checks
remain in progress; Stage 6 stays open until their evidence is recorded.


The fresh Node/Wasm run passes all ten affected packages (including 21 collector,
244 egui and 40 backend tests). The built worklet contract passes codec worker,
profiling timestamp/capture/toggle and production AudioWorklet checks. Fresh native
socket/shared-memory probes pass profiling names, toggles, recording/recovery and
shutdown. Updated synthetic screenshots show interval wrapper statistics and
actual whole-callback costs beside the selected-tick timeline. Dotted context
links use a neutral stroke that remains visible at overview zoom. Chrome and
isolated production browser runs, final measurements and CI remain pending.


Native socket and shared-memory canaries now require complete callback entry,
exit and between-execution statistics to reach the application snapshot with
matching sample counts; both pass. The injected-clock worklet artifact contract
also requires a nonzero execution count on captured host callbacks and passes.
These checks exercise the new envelope context across the real transport paths.


### Retarget after basic profiling merged (2026-11-05)

Per the user's instruction, PR #939 now targets `feat/fixed-quantum-runtime`.
Basic profiling was squash-merged as `a192b0ce` (#938), after render optimizations
`5b0c166b` (#944). Squash ancestry caused duplicate profiling conflicts. Resolving
against the exact shared profiling content at `5036ad06` preserves the graph
implementation and brings in exactly the new target's six-file content delta.
The final performance baseline moves to `a192b0ce` so the render optimizations
are present on both sides. Affected validation is repeated after reconciliation.


## Final local verification (2026-11-05)

PR #939 targets `feat/fixed-quantum-runtime` and resolves the profiling squash
merge. Both performance binaries include #944; baseline `a192b0ce`, implementation
`a7e2f7ee`. The merged target contained two global test allocators; retaining one
preserves allocation guards and fixes test compilation.

- Full prescribed native CI-profile command: **2,782 passed, seven skipped**.
  Warning-denying workspace builds, formatting, test policy, tracing inventory
  (427 modules), feature parity and runtime/engine/loop boundary checks pass.
  The Python suite passes 134 checks.
- All ten affected Node/Chrome Wasm packages pass. After retargeting, backend,
  host worklet and application checks pass again: Node **40/17/45** tests;
  Chrome **36/17/49** tests. Collector and egui suites contain **21/244** tests.
- Socket/shared-memory canaries require nonzero callback wrapper statistics with
  matching prefix/suffix/between sample counts; both pass. Debug and release
  built-worklet contracts pass, including exact callback execution counts.
- Production Chrome workflow and the **release** large-media round-trip pass at
  900×600, with zero xruns. The worklet's missing elapsed clock stays unavailable;
  injected-clock contracts test real timestamp semantics separately. An extra
  debug large-media attempt timed out after 120 seconds at LargeLoaded. CI runs
  that case in release only; no timeout was changed. The baseline debug attempt
  was interrupted by the environment restart and proves no attribution.
- Native synthetic screenshots cover interval wrapper statistics and actual
  callback/tick selection. Physical audio/MIDI hardware was unavailable locally.

### Acceptance evidence

| Criterion | Evidence |
| --- | --- |
| 1 — shared controls and table | Profiling UI control/table tests; native interaction preview; browser settings persistence. |
| 2 — executable topology and stable layout | Atomic descriptor/activation tests; transport canaries; native/Wasm graph interaction tests; semantic serialized-topology cache. |
| 3 — processor and link statistics | Independent collector fixtures; hover/pin tests; current UI names with role/slot identity. |
| 4 — per-tick cumulative statistics | Independent-branch and anti-correlated-spike fixtures; malformed/unfinished records remain unavailable. |
| 5 — color scales and influence | Pointer/zoom/pinning and grouping checks; native screenshots; bounded release UI measurements. |
| 6 — interval versus actual xrun selection | Original-budget, partial-parent, paging and selection fencing fixtures; actual callback/tick timeline preview. |
| 7 — coherent lifecycle | Connection/attachment/generation/epoch fencing, reused owner generations, mapping retries, reset and replacement checks. |
| 8 — deployment boundaries and RT bounds | Full native/Node/Chrome checks, packaged carriers, production browser/worklet workflows, no-heap guards and dependency checks. |
| 9 — callback wrapper context | Independent multi-tick/split-part envelope fixtures, missing-record checks, transported nonzero statistics and dotted-node UI tests. |

### Measurement evidence and limits

See `docs/profiling_graph_validation.md`, `docs/profiling_graph_benchmark.json`
and compressed raw arrays in `docs/profiling_graph_measurements.jsonl.gz`.
The final matrix contains 110 cases / 440 runs. Median case changes are −3.45%
off, −2.84% on and −1.91% stalled on this shared VM. These are measurements,
not evidence of a guaranteed speedup. On-mode control/collection cost increases
from 15.855 to 31.491 µs per callback; initial on-mode deadline misses are 124/160
baseline/current and are retained, not hidden.

Historical suspect cases and four largest fresh suspects receive ten alternating
pairs each. Fresh median paired changes are −1.83%, +5.54%, −0.96% and −0.05%.
The initial +30.75% off partitioned case does not reproduce. For the +5.54% paired
case, ratios span −21.49% to +15.25%; separate run medians are 11.883/11.818 µs.
This spread limits causal conclusions and does not establish hard real-time safety.
No added per-edge clock reads or render allocations are present; fixed bounds and
saturated-consumer behavior pass their guards.

Steady graph frame medians for 12/102/502 processors are 0.399/1.377/4.537 ms.
The 4,094-processor/16,293-signal-link stress case takes 1.088 s initially and
90.115 ms per steady frame: explicitly not 60 fps. Capture queries retain 8,192
summaries and 14,221,312 raw bytes; 128-node pages have 134.132 µs median query time.

### External delivery gate

Required current-head CI remains pending. Live checks:
<https://github.com/SanderVocke/shoopdaloop/pull/939/checks>.
Review and inline-comment queries currently return no reviewers or feedback;
reviewer approval is unavailable and is not claimed. The Codecov report is
coverage information, not review approval. Keep the remaining CI checklist item
open until required checks are green, then record final delivery state.
