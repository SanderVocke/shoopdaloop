# Generic real-time runtime: initial design and coordination evolution

The initial-design and implementation sections below describe the delivered crate. The final section proposes its next evolution for `INTER_LOOP_PROCESSING.md` and `PROPOSAL_LOOP_LAYER.md`; generic planner/delivery facilities are implemented in loop-layer Stage 3, while indexed resources and integrated loop/media execution remain later stages. Historical limits remain an accurate description of the initial API, not a requirement to cap future internal musical transitions.

## Purpose and boundary

Implement `rt_runtime` at `src/rust/rt_runtime` as an independent library in the existing workspace. It owns generic audio/MIDI graph execution and the control/render boundary, not Shoop's loops, tracks, buses, sessions, or musical policy. No existing application backend will use it in this milestone.

This is a fresh implementation informed by `shoop_engine`, not a compatibility layer or wholesale extraction. `PROPOSAL_RT_ENGINE.md` describes the broader direction; this document and the completed `DONE_INITIAL_CRATE.md` narrow it to the initial crate. Separate publication or a repository split is unnecessary.

Production and build dependencies must not include `shoop_*`, UI frameworks, device drivers, or browser bindings. The repository's shared test attribute is permitted through dev-dependencies only. Browser glue belongs in a test/example fixture, not the portable core.

## Problems addressed

| Problem | Initial solution |
| --- | --- |
| Native closure queues and browser messages have different semantics | Typed, bounded operations with identical sequencing, admission, activation, and completion semantics; host-specific transport only. |
| Topology changes can allocate or do expensive work on the rendering thread | Compile on the control side; incrementally stage/validate in preallocated storage; activate a complete plan at a block boundary. |
| Buffer reads/replacements can copy or destroy large objects during rendering | Revisioned immutable resources, bounded chunk transfers, snapshot leases, and explicit retirement. |
| A bounded queue can still consume an entire audio deadline | Separate configured limits for operations, transfer bytes, validation steps, events, and publication work per callback. |
| Full return queues or disconnected clients can force destruction on the callback | Credit-based admission and retained retirement storage; continue the last valid graph rather than blocking, dropping ownership, or leaking. |
| Generic abstractions can conceal MIDI timing loss or unbounded event work | Sample-frame timestamps, deterministic ordering, bounded payload/event arenas, and explicit late/overflow outcomes. |

## Architecture

```text
Application-defined policy (outside this crate)
                  |
Control: GraphBuilder / compiler / resource producer
                  |
        typed operations + relocatable plan/resource chunks
                  |
   native bounded SPSC     browser credit-limited MessagePort fixture
                  |                         |
                  +---- render ingress -----+
                              |
             RenderRuntime (single owner)
             fixed pools / staged transactions
             active ExecutionPlan / processor slots
                              |
              audio slices + timestamped MIDI
                              |
         completions / telemetry / snapshot chunks
                              |
                 control-side reclamation
```

Suggested modules: `config`, `ids`, `graph`, `processor`, `render`, `control`, `transport`, `resource`, and `diagnostics`. These are modules within one crate, not a new crate hierarchy. No internal scheduler threads, timers, runtime executor, or synchronous request/response waits are required.

### Processing contract

Initialization declares sample rate, maximum block size, graph/port/node limits, scratch capacity, MIDI event/byte limits, resource pools, queue credits, and per-callback work budgets. Reject invalid configurations before starting, including budgets too small for an indivisible operation or capture page. Changing capacities or sample rate requires stopping and re-preparing in this milestone.

`process` receives borrowed planar `f32` buffers, actual frame count, a monotonic frame position, and bounded timestamped MIDI input/output views. Support variable block lengths up to the configured maximum; zero frames is a documented no-op. Invalid shapes fail without panicking or leaving stale audio in writable output. The core does not implicitly clip, resample, or add a block of signal latency.

The entire render entry path—including ingress, command handling, staging, publication, rejection, and retirement bookkeeping—must be allocation/deallocation-free, lock-free, nonblocking, and bounded. No arbitrary queued closures, logging/formatting, I/O, spinning for another thread, heap growth, or allocation-guard exemptions. Pre-touch render storage during preparation. Bounds scale explicitly with admitted graph size, block length, and configured event/work limits; bounded does not mean constant-time DSP.

Processors have separate preparation and processing contracts. Processing gets disjoint borrowed buffers and bounded event writers, not access to the control plane. Safe Rust alone cannot enforce a third-party processor's real-time behavior: guarantees are conditional on conforming processors, and supplied processors are tested under guards. Panics are programming faults, not recoverable callback errors; normal capacity/validation failures use compact error codes.

### Graphs and processor lifetime

Start with typed audio/MIDI ports and a deterministic directed acyclic graph. Support fan-out, audio summation, MIDI merge, multichannel routing, and disconnected outputs as silence. Reject cycles, incompatible ports, invalid IDs, duplicate invalid connections, and capacity violations explicitly. Feedback scheduling, co-processing groups, and automatic latency compensation are deferred, not silently approximated.

The control-side compiler produces a flat execution plan and explicit scratch routing. Prefer simple, auditable buffer ownership over aggressive reuse in the first version. Use generational node/resource IDs; stale handles cannot address reused slots. Processor state lives in stable slots separate from the plan so rewiring does not reset oscillators, filters, capture cursors, or other retained state.

A prepared transaction contains its expected graph epoch, plan descriptor, and reserved lifetime/retirement credits. Staging and validation may span callbacks, while the old plan remains active. Validation must finish before activation: verify descriptor sizes, ranges, unique node execution, topological order, resource/processor references, and aliasing constraints. The receiver must not trust a serialized plan merely because a compiler produced it.

Activation changes the active plan handle and epoch in bounded work, without walking the entire graph or dropping the old plan. A stale transaction is rejected with ownership preserved. Creation/removal bookkeeping is staged; reclamation of removed nodes waits until no active/staged plan references them. No partially edited graph becomes audible.

### Control, timing, and overload

Use fixed-size operation headers and bounded payload handles. Structural/resource commits activate at block boundaries. Parameter operations and MIDI can carry absolute sample-frame timestamps; render them at the corresponding offset, splitting processing into bounded subranges when required. Define equal-time ordering by source/port order and sequence number, preserve FIFO within a source, and bound both scheduled-event storage and subrange count.

Queue acceptance is not application success or a timestamp reservation. Render-side scheduling admission reserves event/subrange capacity before promising exact timing; insufficient capacity produces a rejection, not a silently delayed accepted event. Completions distinguish accepted/applied/rejected outcomes and include sequence, effective frame, and epoch/revision. Reject duplicates, stale generations, oversized or incomplete transfers deterministically. Late operations report their lateness and follow a documented policy (apply at the next available frame); never claim an already missed timestamp was met. Cross-host equivalence applies to operations admitted before the same deadline, not to unpredictable message delivery latency.

Reserve completion and retirement credits before accepting work that requires them. If credits are unavailable, defer or reject new work; never block the callback. The consumer-stalled and disconnected cases retain bounded ownership until the host stops rendering and performs non-RT teardown. Telemetry may coalesce, but command completions must not silently disappear. Publish coherent fixed-size state snapshots rather than retrying indefinitely for consistency. Use only atomics supported lock-free on the target; do not introduce hidden lock-based wide-atomic fallbacks for frame counters.

MIDI 1.0 packets carry sample timing and bounded byte payloads, including explicitly limited SysEx. Merge stably without callback allocation. Do not coalesce or silently overwrite note events. Report refused/overflowed events and late timestamps; reserve capacity for bounded note-release/reset recovery, with a documented recovery policy on output overflow. The crate cannot guarantee lossless MIDI under unlimited input, so overload must be visible and testable.

### Resources and snapshots

Provide generic revisioned audio and MIDI resources, not loop content objects:

- Upload into reserved staging pages; validate format, lengths, ordering, and completeness incrementally.
- Commit a complete revision at a block boundary by swapping a handle. The old revision remains valid for readers and is retired outside rendering.
- Fetch through a lease on an immutable revision. Transfer bounded chunks asynchronously; copying/flattening a whole resource is control-side work.
- Supply a minimal append/capture writer to exercise data produced during processing. A snapshot selects a coherent prefix at a specified capture frame; published pages cannot subsequently change. Copying a partial tail is bounded by the configured page size, with publication allowed to finish later.
- Return revision, captured length/frame, and currentness (`current`, `publication pending`, `stale`, or explicit capacity failure). Never label mixed revisions or incomplete capture as current.

Use bounded pools and lease accounting rather than last-reference destruction on the callback. Snapshot retention consumes credits. If the reader stalls, refuse new snapshots or stop capture at capacity with an explicit incomplete status; existing playback and the last published snapshot remain valid. Do not overwrite leased pages or promise unlimited recording. Cancellation, failed admission, and endpoint shutdown return all credits and storage once safe non-RT teardown is possible.

Continuing audio during a swap means no callback pause, partial resource, or runtime-induced silent block. An intentional discontinuity between sample values may click; crossfading and musical transition policy are outside the initial resource API.

### Native and browser execution

Native control uses bounded SPSC channels, with one producer per endpoint; applications aggregate additional producers outside rendering. Owned prepared data can be handed over only under explicit credit/reclamation rules. Prefer an existing audited queue to new lock-free machinery.

The baseline browser fixture uses a separate Wasm instance in an actual AudioWorklet, MessagePort, and transferable buffer credits. It must work without `SharedArrayBuffer`, Wasm threads, or cross-origin isolation. A control-side compiler emits relocatable descriptors, never Rust pointers or trait objects. The worklet stages chunks into preallocated Wasm pools with bounded validation/materialization. Its message handler only admits bounded frames; it does not build graphs or flatten resources. Initialize processor instances and memory before audio starts; after startup, activate only prepared slots from a registered test processor catalog. Arbitrary dynamic plugin construction in a worklet is not promised.

Rust allocation-free processing does not make browser scheduling, structured cloning, or JavaScript GC hard-real-time. Recycle transferable buffers, limit in-flight messages, prohibit Wasm memory growth after startup, and measure actual worklet behavior. Keep these host limitations explicit rather than disguising them behind a thread abstraction.

## Implementation decisions established through stage 6

The executor now uses fixed processor slots with per-slot planar scratch and bounded MIDI arenas, rather than dynamically constructing port-reference vectors in callbacks. Preparation captures signatures and touches audio/MIDI storage. Graph plans contain only logical typed endpoints and execution ordering; the receiving validator checks every range, generation, dependency, and port type incrementally before activation. Rewiring retains slot state. Removing an unreferenced processor returns ownership to control and advances its generation; reinstalling requires the same prepared port signature. Increasing the slot/port capacity still requires stopping and preparing again.

Native ownership transport uses `rtrb` and reserves return capacity until control receives each completion. A commit retains its dispatch slot only for finite, budgeted validation of an already complete descriptor; it must not wait there for further upload commands. Structural changes share one normal FIFO lane. The independent recovery lane admits MIDI reset operations and is reserved for bounded release/recovery work; structural commands on it are rejected. Terminal and optional telemetry publication budgets are separate so telemetry cannot consume the terminal budget.

Scheduling now conservatively reserves one event, possible subrange split, and reliable execution-result slot for every outstanding operation. Capacity is the minimum of configured scheduled events, callback event budget, and subrange budget minus one. This deliberately limits far-future occupancy instead of guessing future host block boundaries. Admission and execution have distinct results; unread execution results retain credits. Exact-frame parameter/MIDI processing rebases events across subranges, and late delivery is explicitly reported. These reservations guarantee the limits under variable blocks without silently deferring accepted events.

MIDI routing includes reserved 32-message reset space per arena. Overflow resets internal processor inputs as well as external outputs, preventing dropped note-offs from leaving internal voices stuck. Host reset delivery uses an independent preallocated sink with acknowledged, budgeted draining; ordinary output remains suppressed until it finishes. Resets are CC120 followed by CC123 for all channels. Repeated faults do not restart a pending host reset. This conservative recovery policy is separate from musical/session behavior and is explicitly visible in diagnostics.

The resource substrate now uses a single-owner fixed-page store with generation-checked identities, upload reservations, streamed MIDI validation, immutable revision/prefix leases, and incremental reclamation. Audio/MIDI capture appends complete blocks; prefix snapshots share sealed pages and copy only a mutable partial tail. Pool/work exhaustion reports a stopped, incomplete capture instead of overwriting data or resuming after a gap. Native reads use the same bounded-copy semantics as non-shared-memory browser reads rather than requiring a native publication thread.

Resource requests are integrated with owning command completions, and processors receive a restricted resource I/O view. The legacy access path shares copy/validation budgets across control staging, graph validation, resource I/O, and retirement. The loop-layer Stage 4 accounting foundation adds `ResourceIo::render()` for required read/capture access with independent saturating work counters; append-maintained page-indexed byte positioning is implemented, with opt-in fixed MIDI checkpoint rows for bounded event/state positioning. Opt-in capture groups now provide reserved take/final-lease capacity, exclusive writer claims, staged all-member commits and frame-bound planner authority; complete group model/disconnect verification and loop adaptation remain pending. Configured headroom reserves processor work within those totals, so background upload/export cannot consume the work provisioned for ongoing playback/capture. Request and result buffers return through reliable completions; snapshot/capture metadata distinguishes queued requests, actual cut frames, stale revisions, and stopped/incomplete prefixes. Playback fixtures keep the prior valid lease if a new revision cannot yet be pinned.

The independent test-only host now shares its processor, bounded wire codec, and guarded render bridge across native and actual AudioWorklet execution. A control-side compiler in a separate Wasm instance sends versioned relocatable records. Receiver-side pools prepare seven reusable plans, lane-separated cancellation flags, fixed payload/response buffers, and generation-checked export cursors. Four normal transferable credits and one recovery credit bound ingress; no ordinary allocating submission is used on that receiver. Completion recycling stays under the heap guard; a consistency-fault owning completion is parked until callbacks stop.

A dedicated non-isolated localhost runner exercises real OfflineAudioContext and live AudioContext, checks independent ordinary Wasm memory and no growth, and observes actual Web Audio output. The deterministic offline journal is replayed in native offline and bounded two-thread hosts, comparing every response byte and callback sample hash. Native clock coordination occurs outside the guarded callback, not as a wait in rendering. Consumer stalls, delayed delivery, cancellation, malformed/stale transfers, and logical pool recovery precede explicit stop/dispose. JavaScript scheduling/GC and live transport lateness remain outside the Rust real-time claim. Browser unit tests remain separate from this host evidence. Seeded mixed/resource/wire models, selected isolated Miri tests, guarded release workloads, and deliberate native/worklet failure gates provide additional evidence. Native debug/release hosts pass in the Linux/Windows/macOS CI matrix. The complete Node/Chromium suites and real non-isolated worklet/failure gates also pass; `docs/rt_runtime/evidence/ci-stage8.json` records the verified source head and run.

## Initial delivery and exclusions

Deliver the crate, documentation, deterministic/offline and two-thread native fixtures, a minimal real AudioWorklet fixture, small reference processors, contract tests, adversarial tests, and reproducible benchmarks. The fixtures prove the API without modifying Shoop's backend, protocol, application, or production JavaScript hosts.

Defer JACK/CPAL/Web MIDI device adapters, Shoop integration, plugin hosting, unbounded recording, feedback graphs, parallel DSP, graph buffer-reuse optimization, MIDI 2.0, automatic latency compensation, and optional shared-memory browser acceleration. These omissions must remain explicit capability limits.

Validation combines sample-accurate reference comparisons, native/Wasm contract parity, allocator/deallocator canaries in debug and release, lifecycle/concurrency checks, saturated-queue and slow-reader stress, and real browser rendering during graph/resource updates. Performance reports record configuration, raw callback timings and tail latency, copy/validation work, ownership/page peaks, throughput, and deadline misses; noisy CI timing is not proof of hard-real-time deadlines or of superiority over Shoop's current engine. `docs/rt_runtime/GUIDE.md` documents capacity sizing, executable API examples, processor obligations, and stop/drain/dispose ordering. `docs/rt_runtime/PERFORMANCE.md` records reproducible measurements and deliberate gate-failure checks.

## Proposed evolution: resolver-driven interval execution

`INTER_LOOP_PROCESSING.md` governs the next integration: sample-accurate inter-loop interaction, no fixed interaction quantum or mandatory per-hop/block delay, and no arbitrary quota that truncates required finite musical transitions. This deliberately evolves the initial processing contract; it does not retroactively change the completed acceptance record or claim that the current scheduler already provides this behavior.

### Responsibility boundary

A Shoop-specific `LoopTimelineResolver` belongs in the loop layer. It owns authoritative loop/composite timing, trigger occurrences, simultaneous-action resolution, and musical interval selection. Per-loop renderers own media cursors, capture handles, MIDI state, and DSP state, and execute coherent decisions rather than independently discovering synchronization.

The generic runtime must provide the planning lifecycle, explicit resolver-to-consumer dependencies, prepared decision/observation access, and interval signal execution. It must not contain loop modes, synchronization policy, or composite semantics. “Application-level” does not imply a control-thread round trip: planning and rendering execute serially under the render owner.

### Executor lifecycle

The conceptual lifecycle is:

```text
begin host callback and admit external work once
  → settle the current boundary and finite same-frame cascades
  → publish a coherent decision view for the next nonempty interval
  → execute dependent render operations and the signal DAG
  → collect observations at their defined semantic boundary
  → repeat until the host block is complete
finish callback
```

Decision storage is prepared and reused only after its consumers finish. Successive intervals do not require retaining a whole callback's transition history. An outcome that can invalidate later timing must be observed before affected execution crosses its boundary; whole-block preplanning cannot assume away capture failures or other such outcomes.

Today's `Processor::process` cannot select the interval for the rest of the graph. Add a generic planning facility or equivalent explicit phase; the exact Rust API remains open. Repeatedly calling `GraphRuntime::process` is not that facility because it resets budgets and snapshots external work again. External timestamped actions and musical boundaries must be reconciled without duplicating admission or delaying already admitted actions.

### Dependencies and signal structure

Planner-to-consumer ordering must be explicit, not inferred from node IDs, insertion order, or dummy audio/MIDI routes. Retain the signal DAG independently. Playback and recording for one logical loop may be separate operations around an FX chain; do not force an artificial signal cycle by making the entire loop one indivisible processor.

Execution observations return through the next planning phase, not a zero-delay signal feedback edge or external control queue. Musical sync relationships, instantaneous trigger causality, executor ordering, and signal routes are distinct structures. The loop layer validates musical termination; generic graph validation must not reject every cyclic sync relationship as if it were a signal cycle.

### Work, storage, and termination

Retain fixed storage capacities, owning admission/return credits, finite external command batches, and incremental background maintenance. Preserve allocation/deallocation-free, nonblocking callback operation. Do not charge each internal wrap or trigger to an arbitrary external event/result or subdivision allowance.

The coordination contract is **allocation-free, deterministic processing with finite application-dependent work—not a guaranteed per-callback CPU bound**. An expensive valid workload may xrun; it must not be silently delayed, dropped, or reordered to fit a quota.

Time-advancing intervals make progress through a finite block. Same-frame termination requires constrained event semantics and validation, including occurrence identity, creation/delivery rules, and simultaneous-action ordering. Once-per-occurrence delivery is insufficient if fresh occurrences can be invented indefinitely. Arbitrary application callbacks are not a substitute for a specified terminating model.

Audit existing copy/validation quotas as well as subdivisions: distinguish required rendering work from throttled background transfers, and define preparation guarantees and explicit resource-failure behavior. Diagnostic counters and optional planning-buffer capacity must not become hidden musical-work limits. Appropriate counter-overflow reporting and interval-independent accounting remain API design work.

### Additional integration requirements and verification

Typed operations, prepared exact-frame capture activation/stop, coordinated multichannel failure semantics, efficient media positioning, and coherent domain state publication are also needed. Reuse the runtime's ownership and resource lifecycle rather than constructing parallel infrastructure in the loop layer.

First test the resolver independently, then the generic lifecycle with synthetic consumers/outcomes, then coordinated audio/MIDI renderers. Compare transition journals and rendered output across host partitions, including finite same-frame cascades, valid mutual synchronization, invalid instantaneous behavior, dense one-sample loops, callback-end boundaries, capture failures, and playback/FX/recording routing. Verify native and actual non-shared-memory AudioWorklet execution under heap/lifetime guards.

Longer preplanned span batches or reduced traversal are later optimizations. They require proof that intermediate observations cannot invalidate the plan; optional storage exhaustion must preserve incremental semantics. Implementation must update current API contracts and tests explicitly when these extensions land.

### Stage-1 integration decisions and Stage-3 generic implementation

`docs/loop_layer/RUNTIME_INTEGRATION.md` specifies one generic prepared planner per runtime, defaulting to `NoPlanner`, with typed plain-data decisions, observations, operations, and terminal results. Catalog/descriptor planning metadata and consumer bindings are separate from signal ports. Immutable decision borrows end before the planner consumes observations; a compile-checked safe-Rust prototype exercises that lifetime separation and rejects mutation while a decision view remains borrowed.

Pending musical requests retain generation-checked terminal receipts from the runtime's existing result machinery. They leave the timestamped due queue without releasing their terminal reservation; automatic wraps do not require such receipts. Recovery-safe cancellation/stop must remain admissible independently of normal pending-result capacity. One independently reserved recovery receipt now supports payload-free cancellation/stop, with terminal completion required before advancing past delivery-frame planning. Domain outcomes and requested/delivered/terminal frames are distinct. Fixed receiver-side program staging remains a later requirement for native/browser transfer without Rust pointers or a second ownership bus.

Resource work is divided into fixed storage/owners, external admission, budgeted background maintenance, counted required rendering, and non-semantic diagnostics. Prepared version-scoped indexes and group capture reservations must make freeze-to-play immediate. Failed preflight may re-preview an uncommitted boundary only by adding a new latched group/activation failure, giving a finite structural progress argument rather than an arbitrary retry allowance. No request outcome, generation, or musical event is published twice across previews. Required rendering cannot consume a throttled background-copy quota.

The loop layer's precise pulse/conflict/termination model belongs in `docs/loop_layer/SEMANTICS.md`, not the generic executor. The generic planner lifecycle and typed receipt implementation now have guarded native/Wasm tests, selected Miri coverage, a production-trait lifetime proof, and existing-host regression evidence. Indexed resources and prepared capture transactions now have dedicated APIs and focused tests, with milestone evidence recorded separately. Complete group ownership validation, the loop adapter, and a real loop AudioWorklet remain later-stage requirements; generic fixture regression is not evidence for those features.
