# Interactive runtime profiling graph implementation plan

Status: implementation in progress; core collection/transport and graph UI implemented, final validation pending.

Target branch: `feat/runtime-profiling-graph`, based on `feat/runtime-profiling` (profiling PR #938).
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

- [ ] Export processor/host endpoints, typed port connections, execution order and
  structured group metadata from the prepared plan with bounded storage and
  work. Publish only a complete descriptor associated with the active graph.
- [ ] Add bounded engine metadata chunks, collection/reassembly and resync;
  validate endpoint IDs, counts and complete identities. Keep old or partial
  descriptors from decorating new measurements, including activation mid-callback.
- [ ] Cover branching, fan-in/out, audio/MIDI, host bypass, disconnected nodes,
  duplicate labels, failed/identical rebuilds, replacement and slot reuse.

Verification: compare exported nodes/edges directly with the executable descriptor;
exercise stale/missing/reordered chunks and stalled readers under RT safety guards.

### Stage 2 — Collect readiness and decode captured iterations

Depends on Stage 0; integrate with Stage 1 identities before publication.

- [ ] Add per-node readiness accumulators to live/capture reports without changing
  table duration semantics. Validate complete records before accepting statistics;
  preserve counts and fault/partial markers independently for each metric.
- [ ] Build a bounded capture callback/tick index from assembled raw records, with
  actual node durations/readiness, phases, original budgets and loss/gap status.
  Add the minimal bounded recording context only if current fields cannot supply it.
- [ ] Add independent prescribed-time fixtures: unrelated 80 µs work between a
  20 µs source and 10 µs dependent gives 110 µs readiness, not 30 µs. Include
  anti-correlated spikes proving cumulative worst/stddev cannot be summed,
  plus fan-in/out, large clock origins, unfinished nodes and malformed packets.
- [ ] Verify zero-/multi-tick callbacks, buffered and split host callbacks,
  differing host/engine frame origins, delayed notifications and truncated capture.

Verification: exact independent expected statistics and selected-iteration values
match raw boundaries; no fabricated tick deadlines or xrun attribution appear.

### Stage 3 — Carry coherent graph and capture data to the UI

Depends on Stages 1–2.

- [ ] Extend backend/service/API/app protocol/client projection for immutable
  topology, readiness and capture index; add bounded on-demand selected detail
  requests through existing application boundaries, without direct egui/engine access.
- [ ] Tag requests/responses with connection, generation, graph, capture and
  selection identity. Reject stale responses after reset, replacement or a rapid
  selection change. Define paging, cancellation, reconnect/resync and final-off data.
- [ ] Bump affected protocol versions/compatibility descriptors and update codecs,
  socket/shared-memory/worklet adapters and independent admission limits.
- [ ] Exercise oversized/malformed data, interrupted assembly, slow clients and
  rapid graph/capture changes; keep snapshots below their existing size limit.

Verification: production transport/client round trips preserve identical topology,
statistics and selected detail; saturation does not starve reliable or meter traffic.

### Stage 4 — Add graph presentation and interaction

Depends on Stage 3 and the Stage 0 graph widget/layout decision.

- [ ] Add Graph/Table tabs to the existing window and shared selectors; retain
  immediate toggle, persistence behavior, overall DSP display and table values.
- [ ] Add an `egui_graph` adapter mapping generation-scoped processor/host IDs to
  widget node IDs and audio/MIDI ports to sockets. Keep `View`, layout/routes and
  selection as UI state; keep authoritative topology and measurements in the
  application model. Set immutable mode so inspection cannot edit the DSP graph.
- [ ] Use `layout_routed` on topology/group-expansion changes and render through
  `Node`/`Edge` widgets with cached left-to-right geometry, zoom, pan and fit.
  Project collapsed groups into summary nodes while preserving member/edge IDs
  for inspection and expansion. Handle large, empty, disconnected and
  timing-unavailable graphs without freezing the UI or moving live layouts.
- [ ] Use widget node/edge responses for hover and pinned details, and per-edge
  strokes/custom node framing for metric colors and upstream/downstream emphasis.
  Add group/member inspection, separate legends and metric selection with numeric
  values, unavailable styling and clear readiness wording.
- [ ] Add focused UI checks for hit testing after transforms, overlapping/bundled
  edges, collapse/expand reachability, stable geometry, unit conversion, source
  switching, lost-data feedback and reset/selection invalidation.

Verification: deterministic egui interaction checks plus native/browser visual
inspection demonstrate readable topology and correct values without layout jitter.

### Stage 5 — Add captured callback/tick selection and timeline

Depends on Stages 3–4.

- [ ] Keep capture-interval aggregation as the default; add callback and tick
  selection, navigation back to interval statistics and bounded detail loading.
- [ ] Synchronize selected-tick graph values with a compact timeline showing all
  retained ticks, node spans and recorded non-node phases within the selected
  callback, its actual deadline and any missing/uncertain regions.
- [ ] Show actual values for selection, interval Avg/Worst/Stddev otherwise;
  retain separate node/link scales. Make trigger/attribution uncertainty and
  partial/receiving status visible in both graph and timeline.
- [ ] Test rapid selection, capture replacement, incomplete parent callbacks,
  missing details and graph reset while a detail page is in flight.

Verification: an injected spike is visible in the independently known callback/tick
and processor; delayed host notifications never falsely identify an offender.

### Stage 6 — Deliver and complete end-to-end validation

Depends on Stages 1–5; this is the final implementation/validation stage.

- [ ] Extend user docs with readiness origins, separate color meanings, aggregation
  versus actual selection, grouping, units, unavailable timing and xrun limits.
- [ ] Run affected collector/runtime/engine/backend/protocol/transport/app/UI tests,
  RT no-heap/lock and dependency-boundary guards, then the complete suite prescribed
  by current `.agents/info/test.md` in the selected build environment. Run formatting,
  warning-denying workspace build, test-attribute and tracing inventory checks.
- [ ] Run native packaged socket/shared-memory and browser/worklet flows: enable,
  inspect graph/table, record/play, route/rebuild, switch units, inject xrun,
  inspect interval and selected callback/tick/timeline, disable/re-enable,
  reconnect/replace host and shut down. Exercise actual unavailable browser clocks
  and injected-clock contracts separately; report unavailable real facilities.
- [ ] Compare release off/on/stalled performance with the recorded pre-graph
  profiling baseline across graph sizes and Q=1/16/32/64/128. Record callback tails/
  misses, publication bytes, collector memory/CPU, capture detail latency, layout
  time and steady UI frame cost. Resolve unexplained RT regressions and verify
  fixed bounds under dense topology, capture saturation and slow clients.
- [ ] Commit meaningful milestones, push `feat/runtime-profiling-graph` and update its
  dedicated PR against `feat/runtime-profiling` with final graph scope and evidence.
  Keep the underlying profiling PR #938 separate. Do not merge.
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
pages by rejecting responses that no longer match. Engine/app versions are 9/22;
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
