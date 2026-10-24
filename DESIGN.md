# Generic real-time runtime: initial design

## Purpose and boundary

Implement `rt_runtime` at `src/rust/rt_runtime` as an independent library in the existing workspace. It owns generic audio/MIDI graph execution and the control/render boundary, not Shoop's loops, tracks, buses, sessions, or musical policy. No existing application backend will use it in this milestone.

This is a fresh implementation informed by `shoop_engine`, not a compatibility layer or wholesale extraction. `PROPOSAL.md` describes the broader direction; this document and `PLAN.md` narrow it to the initial crate. Separate publication or a repository split is unnecessary.

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

## Initial delivery and exclusions

Deliver the crate, documentation, deterministic/offline and two-thread native fixtures, a minimal real AudioWorklet fixture, small reference processors, contract tests, adversarial tests, and reproducible benchmarks. The fixtures prove the API without modifying Shoop's backend, protocol, application, or production JavaScript hosts.

Defer JACK/CPAL/Web MIDI device adapters, Shoop integration, plugin hosting, unbounded recording, feedback graphs, parallel DSP, graph buffer-reuse optimization, MIDI 2.0, automatic latency compensation, and optional shared-memory browser acceleration. These omissions must remain explicit capability limits.

Validation combines sample-accurate reference comparisons, native/Wasm contract parity, allocator/deallocator canaries in debug and release, lifecycle/concurrency checks, saturated-queue and slow-reader stress, and real browser rendering during graph/resource updates. Performance reports record configuration and tail latency; noisy CI timing is not proof of hard-real-time deadlines or of superiority over Shoop's current engine.
