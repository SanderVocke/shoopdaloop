# Generalize in-app profiling

Implementation plan based on `feat/backend-replacement` at
`0b6812259ffc00b9a524bb6095cfc2c6b2112847`, with the current base updated to
`27c8927f51303519f621270bd9b521a687c134d7` (expensive-processor graph filter).
The engine measurement baseline is unchanged. Execution is in progress; unchecked
items are not certified complete.

## Goals and scope

Expose profiling from the audio engine, host and future platform-specific
producers through the same in-app facility. Adding a report type must require
only its producer/schema and application decoder/presentation, without changing
carrier implementations or the generic protocol vocabulary.

Deliver a common report/control contract across the engine and application
protocols, migrate existing audio profiling without losing functionality, and
add a FreeRTOS task snapshot report in the application. Keep audio collection
and aggregation in their existing owners. Add host providers at the app service,
without routing host measurements through the audio graph or RT engine.

Scheduler-event tracing, new Ethernet/USB instrumentation, cross-device clock
synchronization, report export and a dynamically generated UI are follow-up work.
Existing USB/network counters can become separate reports later. Physical USB
qualification and other unfinished work in `PLAN.md` remain separate.

## Immutable acceptance criteria

These goals, scope and criteria require explicit user approval to change.

1. **Extensibility:** both protocol boundaries carry bounded, versioned opaque
   report payloads and generic discovery/control. A test-only unknown type and
   schema version traverse framing/carriers without payload decoding or a new
   transport/protocol enum variant; an unsupported consumer skips them safely.
2. **Audio parity:** preserve enable acknowledgement, units, live statistics,
   topology/names/groups, readiness, callback budgets, automatic xrun capture,
   paged callback/tick inspection, loss indicators, reset/invalidation and final
   report retention on disable. Existing audio tests retain their behavioral
   assertions through the new path; no session-file timings are introduced.
3. **Independent host profiling:** the FreeRTOS host advertises and delivers a
   structured task report with identity/name, state, priorities, runtime counters
   and stack watermarks. CPU usage is computed over sample intervals. Missing
   capabilities, failed samples and counter resets are explicit, never zero
   measurements. Audio profiling can be disabled while host profiling is active.
4. **Correct lifecycle:** graph changes invalidate audio data only; host restart,
   engine replacement, detach/reattach and task reuse cannot relabel stale data.
   Reports carry their own identity/freshness and do not depend on a session
   revision arriving at the same time. Old capture responses stay fenced out.
5. **Bounded delivery:** reports do not travel in ordinary session snapshots.
   Per-source and total memory/work/admission limits hold under stalled clients,
   malformed payloads and source saturation. Reliable control/heartbeat/shutdown
   progress and admitted diagnostic sources receive fair service when the peer
   drains. Loss, coalescing and incomplete reports are observable.
6. **RT and portability:** no new allocation, destruction, wait, lock, kernel
   snapshot call or serialization enters the render kernel/ISR/scheduler hook.
   Preserve bounded worklet forwarding and its separate measured host costs.
   Portable contracts build for native, Wasm and the existing ARM firmware;
   FreeRTOS FFI remains platform-owned. Compare off/on/stalled costs with the
   baseline and investigate regressions before acceptance.
7. **Evidence and delivery:** native UI and actual FreeRTOS/QEMU reports work
   through the production connection; existing browser audio profiling still
   works. Required final-head CI is green, available automated review is addressed,
   and documentation states supported measurements and limits. Emulator evidence
   is identified as such; any physical timing claim needs board measurements.

## Current implementation and change points

| Owner | Current behavior | Planned responsibility |
| --- | --- | --- |
| `rt_profiling` | Portable bounded recorder, collector, graph/capture data and `View` | Retain audio semantics; own audio payload encoding/decoding |
| `shoop_rt_protocol::{message,codec,schema}` | Four audio-specific profiling messages and `Request::Profiling(bool)` | Generic report chunks and producer discovery/control |
| `shoop_rt_engine::profiling` | Forwards status, topology, names and packets | Encode those records in the audio payload schema |
| `shoop_rt_backend::{incoming,backend}` | Collects audio records and exposes a typed profiling view | Decode audio records; adapt them to an application-facing provider |
| `shoop_app::Service` | Projects audio profiling into session snapshots and resolves user labels | Own a provider registry and connection-scoped diagnostic delivery; retain audio label resolution |
| `shoop_app_protocol`, `shoop_app_connection` | Serialize `ProfilingView` in `SessionSnapshot`; write snapshot/media records | Independent bounded diagnostic messages/scheduling |
| `shoop_app_client`, `shoop_app_api`, `shoop_egui` | Project and render one audio-specific view | Typed consumer registry and Audio/Host panels |
| `shoop_host_freertos`, `platforms/zynq7000` | Run service/render tasks; log task stacks, memory and USB counters | Collect/encode task snapshots and register a host provider |

`shoop_transport` already carries opaque records and has `Lane::Profiling`.
Engine outboxes and MessagePort already isolate profiling admission. Application
connections currently use `post()` for their traffic, and the byte-stream writer
uses strict lower-lane ordering; extend those paths rather than assuming existing
lane support alone provides end-to-end isolation/fairness.

## Design rules

- Introduce a small neutral `no_std` contract crate, provisionally
  `profiling_protocol`, shared by both protocols. It owns open report-type IDs,
  source identities, bounded envelopes and generic control/status contracts, not
  audio/FreeRTOS schemas, host APIs, collectors or renderers. Keep dependency
  boundary checks intact. Report payload codecs belong to their report owners;
  the generic protocols must not import payload types to validate them.
- Use numeric open IDs with stable namespaced discovery names, independent schema
  versions, source ID/epoch, sequence, report ID and bounded fragment offsets/
  total length. Unknown IDs must survive decoding. Define ordering, completion,
  overflow and reset semantics; validate lengths before allocation or copying.
  Keep the existing 1,280-byte record ceiling, with payload space accounting for
  each outer protocol's headers. Freeze numeric source, report, assembly, queue
  and work limits before integrating producers.
- Discovery advertises schema versions, availability, supported collection modes
  and sampling bounds. Route subscribe/unsubscribe/snapshot and bounded
  report-specific inspection requests generically. Acknowledge requested versus
  effective state/rate and explain unsupported/refused operations. Bound catalog,
  command and status traffic as well as report data.
- Preserve separate audio wire records and application audio views. Keep
  `rt_profiling::Collector` in the backend, user label resolution in `Service`,
  and typed display state in the app. Different payload IDs distinguish raw audio
  records from derived views. Do not flatten graphs/captures into numeric tables
  or send the entire retained capture on each refresh.
- Scope source epochs to producer restarts, and retain audio graph/generation and
  capture identities in the audio schema. Preserve application authority/lease/
  attachment fencing on the new channel. Task IDs must distinguish deletion and
  reuse within a boot. Timestamps declare units and clock domain; unrelated clock
  domains cannot share an apparent timeline without synchronization.
- Use bounded queues and fair per-source scheduling within the profiling lane.
  Coalesce only whole replaceable snapshots; keep ordered streams and multipart
  assemblies coherent. Abandon an incomplete transfer explicitly and recover at
  a new report boundary. Separate capacity/loss counters for collection, service
  delivery and each client. Do not multiply collection work by client count.
- Reliable controls/status remain bounded and prioritized. Bound low-priority
  bytes queued ahead of controls; an already-started TCP record cannot be
  preempted. Do not claim hard latency guarantees on a stalled ordered connection.
- Keep optional sampling off by default, independent per source. Unsubscribe or
  detach releases demand/assemblies; effective producer demand combines only
  valid consumers. Preserve the existing audio preference and final disabled
  view. Kernel runtime accounting has a documented baseline cost even when
  report sampling is off.
- FreeRTOS collection runs in control context with fixed storage and a low initial
  rate (1 Hz). `uxTaskGetSystemState()` suspends scheduling and may scan stacks;
  low task priority alone does not make it harmless. Measure its worst observed
  cost on the actual configured stacks. Keep scheduler tracing out of this slice.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

Stages are ordered by dependency below. Keep intermediate commits building;
temporary adapters are permitted during migration but must be removed before
final validation. Before committing behavior changes, run the complete native
gates required by `.agents/info/test.md`; use focused tests within each milestone.
Do not reformat unrelated code. Stages 2–6 form one protocol migration milestone:
the producer, both incompatible outer protocol versions, service and consumers
must land together to keep the implementation commit usable. Delivery remains a
separate final gate.

## Stage 1 — Establish baseline and freeze the contract

Depends on: none.

- [x] Create `feat/backend-replacement-profiling` from the current reviewed
  `feat/backend-replacement` head, preserving unrelated work. Confirm applicable
  repository instructions and record any change from this plan's baseline.
- [x] Capture existing audio regression/benchmark evidence using
  `docs/runtime_profiling.md` and its benchmark guide: off, on, stalled collection,
  graph rebuild and automatic xrun capture. Record workload/configuration and
  callback tails, drops, storage and host forwarding costs under `work/`.
- [x] Specify the envelope, stable IDs/versions, discovery/control responses,
  lifecycle, complete-transfer rules and numeric memory/work/rate bounds. Budget
  the largest supported audio view/topology/capture page and task table; preserve
  table-only profiling where graph visualization is unavailable. Document CPU
  percentage denominator and clock/reset behavior.
- [x] Verification: check the contract against existing graph/capture capacities,
  strict engine/app version handshakes, all carrier record sizes and FreeRTOS
  memory budgets. Confirm baseline assertions pass before changing behavior.

## Stage 2 — Implement reusable report framing and controls

Depends on: Stage 1.

- [x] Add the neutral contract crate with bounded storage/codecs, open IDs,
  generic discovery/control/status and bounded incremental assembly. Keep typed
  audio/FreeRTOS payload decoders outside it. Add it to existing Wasm/dependency
  checks without weakening their rules.
- [x] Add generic engine/app message wrappers and generic command routing hooks.
  Enforce caps, overflow-safe offsets, monotonic identity/sequence handling,
  bounded outstanding inspections and transfer retirement on cancel/disconnect.
  Bump affected protocol/layout versions when compatibility changes; retain
  explicit rejection of incompatible outer protocols.
- [x] Verification: focused codec/assembly tests exercise maximum records, unknown
  IDs/versions, oversized/truncated payloads, duplicate/stale/out-of-order chunks,
  abandoned reports, exhausted assemblies and restart/cancellation. Verify bounded
  behavior from untrusted lengths and an opaque fixture through both protocols.

## Stage 3 — Migrate the engine audio stream

Depends on: Stage 2.

- [x] Move audio payload serialization from `shoop_rt_protocol::codec` to an
  audio-owned codec module; encode existing status/graph/name/packet records in
  generic chunks. Adapt `ControlEndpoint::forward_profiling` and enable/reset
  routing without changing recorder/capture semantics.
- [x] Decode at `shoop_rt_backend` and feed the existing collector. Keep topology,
  naming, generation and capture sequencing intact. Update native, worklet,
  socket and shared-memory record integration and MessagePort consumption credits
  wherever changed framing affects them; no report-specific carrier branches.
- [x] Verification: retain assertions in `shoop_rt_protocol/tests/profiling.rs`,
  `shoop_rt_transport/tests/profiling.rs`, `shoop_rt_backend/tests/profiling.rs`
  and runtime/engine profiling tests. Exercise unknown reports and profiling
  saturation alongside controls over direct and encoded carriers. Repeat render
  allocation guards and the actual worklet wire contract after migration.

## Stage 4 — Add service providers and independent app delivery

Depends on: Stage 3.

- [x] Add a portable bounded provider interface/registry to `shoop_app`, with a
  backend audio adapter and host registration at composition roots. Providers
  support discovery, demand, polling and routed inspection; collection advances
  once per service turn, independently of connection count.
- [x] Publish typed audio views after existing application label/group resolution.
  Send catalog/status/report/inspection data on the independent channel. Remove
  profiling payloads from `SessionSnapshot` and diagnostic-only session revision
  comparisons once client migration lands. Preserve live refresh throttling,
  final disabled views and paged detail requests.
- [x] Extend both ordinary and `with_network` connection writers to use bounded
  diagnostic queues and `post_on_lane`. Add per-source rotation and byte/record
  budgets; address starvation in `shoop_transport::stream` while retaining reliable
  priority and existing telemetry/bulk policies. Clear demand/queued assemblies
  on authority change, detach, shutdown and producer removal.
- [x] Verification: extend app service and connection session/network tests for
  concurrent sources, a stalled client, source fairness, late join/reconnect,
  authority rejection, graph-versus-host reset, and heartbeat/control/shutdown
  progress under diagnostic and bulk pressure. Diagnostic updates must reach a
  client without changing session state/revision.

## Stage 5 — Migrate the client and profiling window

Depends on: Stage 4; Stage 4's snapshot removal completes with this stage.

- [x] Add a bounded client report store and typed decoder dispatch, independent
  of snapshot assembly. Reject stale authority/source epochs and retain only
  complete current views. Unknown types/versions show unsupported availability
  without disconnecting or attempting typed decoding.
- [x] Update `shoop_app_client::{lib,runtime}`, API/intents, backend interfaces and
  native/browser/local paths. Populate the typed UI model from decoded reports;
  preserve connection/lease/attachment fencing and saved audio enable preference.
- [x] Add source selection with Audio and Host presentation. Reuse the current
  graph/table/timeline UI unchanged in meaning; add independent controls, effective
  sampling rate, freshness and loss/unavailable indicators. Closing/unsubscribing
  follows the documented demand policy without disabling other consumers.
- [x] Remove obsolete audio-specific outer protocol variants, commands, snapshot
  fields and temporary adapters. Keep report-specific types in schema/consumer
  modules, where adding a renderer is intentional.
- [x] Verification: extend client projection and egui behavior tests for audio
  parity, asynchronous snapshot/report arrival, stale capture detail, unsupported
  schemas, independent toggles, final disabled data and disconnect clearing.
  Exercise native UI and browser application projection through real codecs.

## Stage 6 — Implement the FreeRTOS task provider

Depends on: Stages 2, 4 and 5.

- [x] Add a portable typed task schema/codec shared by the host producer and app
  consumer, with no FreeRTOS headers/FFI dependency in portable/app crates. Include
  task ID/name, state, current/base priority where available, cumulative runtime,
  total runtime, counter width/frequency, sampling bounds and stack watermark in
  explicit bytes. Represent unsupported fields and invalid samples explicitly.
- [x] In the platform C adapter, enable/configure runtime statistics using a
  sufficiently fine monotonic counter. The existing `configGENERATE_RUN_TIME_STATS`
  is zero; reuse/scale the platform timer rather than the millisecond UI clock.
  Verify the pinned kernel's counter width/wrap and task identity semantics.
- [x] Provide a bounded FFI snapshot API and preallocated task storage. Collect
  structured `uxTaskGetSystemState()` data, not `vTaskGetRunTimeStats()` text.
  Normalize stack words to bytes; define overflow/capacity failure without an
  assertion or fabricated complete table. Copy names while their lifetime is
  protected. Handle task deletion/reuse, failed reads and counter reset/wrap.
- [x] Register the provider in `shoop_host_freertos::embedded::run`; sample only on
  demand, initially at 1 Hz with advertised bounds. Compute interval CPU usage in
  the typed consumer and render a task table. Label scheduler-accounted runtime:
  it is not a separate interrupt-time measurement. First/reset intervals are
  unavailable. Do not feed host data through `Request::Profiling` or graph nodes.
  Host reporting must work even if the render host has no audio profiling clock;
  preserve explicit audio timing unavailability in that case.
- [x] Verification: deterministic fake-platform snapshots test CPU deltas,
  denominator, wrap/reset, identity reuse, capacities and unavailable fields.
  Cross-build firmware; run real kernel API collection in QEMU. Measure collection
  and watermark scan time, runtime-accounting overhead, heap/stack margins and
  render overruns with sampling off/on while network and USB work continue.
  Wire the existing platform UI probe to display real task data, not fixtures.

## Stage 7 — Final end-to-end validation and delivery

Depends on: all previous stages. This is the final acceptance gate.

- [ ] Exercise production native socket/shared-memory and browser MessagePort
  audio profiling: enable, graph/table, xrun/callback/tick inspection, renames,
  rebuild, disable and reconnect. Exercise FreeRTOS/QEMU task profiling through
  the authenticated app connection/native UI with audio profiling independently
  toggled, network traffic and available USB fixtures. Stall/drain consumers and
  restart the same host endpoints; check bounds, truthful loss and fresh identity.
- [x] Repeat Stage 1 off/on/stalled release measurements with matched workloads.
  Include FreeRTOS collection costs and combined-source saturation. Keep raw
  results/configuration under `work/` or CI artifacts; summarize deltas and
  limitations on the PR. Resolve regressions before claiming acceptance.
- [x] Run the native gates from `VALIDATION.md` in the environment specified by
  `.agents/info/build.md`: formatting, warning-denying workspace build, full
  nextest suite, test-usage/feature-parity/dependency-boundary checks and script
  tests. Use targeted packages during earlier stages; do not repeatedly run the
  full workspace suite for unchanged evidence.
- [ ] Run relevant Wasm/Node and actual browser/worklet smokes, firmware ARM builds,
  `platforms/zynq7000/suite.py`, and affected USB/deployment gates. Add new report
  regressions to existing native/web and `zynq_freertos` workflows. Use current
  documented build entry points and pinned tools. Report unavailable manual or
  physical checks explicitly; compilation/QEMU cannot certify silicon latency.
- [x] Update `docs/runtime_profiling.md`, `ARCHITECTURE.md`, owning protocol and
  connection guides, and the platform guide: extension recipe, schemas/versions,
  source lifecycle, limits, UI behavior and measurement costs. No per-report
  carrier instructions should be necessary.
- [ ] Push milestone commits and open an implementation PR targeting
  `feat/backend-replacement`. Verify required CI on the final head, investigate
  failures, and repeatedly address available automated review feedback until
  approval; report absent/unavailable review rather than inventing approval.
  Recheck CI after fixes, map evidence to criteria 1–7, and leave the PR unmerged
  with a clean worktree. Do not merge autonomously.

## Testing cost and confidence

Use existing deterministic profiling assertions as the audio regression oracle;
add tests at genuinely new contracts: opaque extension, assembly, lifecycle,
fairness and kernel-to-UI measurements. Run focused package/test filters after
each change and the relevant carrier/RT guards at integration milestones. Run
required complete native gates before behavioral milestone commits; group local
iterations to avoid repeating them unnecessarily. Reserve full firmware/USB/
deployment gates and broad browser smokes for final integration or evidence of a
wider regression. Use CI for the existing cross-platform matrix. Measure
performance in matched release runs rather than adding unstable timing assertions
to unit tests. Do not mark an acceptance criterion complete when its required
integration or measurement is unavailable.
