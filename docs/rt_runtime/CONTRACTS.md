# Runtime contracts and validation matrix

This is the implementation contract table for the initial runtime. `PLAN.md` A1–A8 remain the acceptance criteria. Rows describe intended behavior until their test group is implemented; they are not claims of completed verification.

## API and execution contracts

| Area | Contract |
| --- | --- |
| Preparation | Validate configuration and checked arena-size arithmetic before allocating render storage. Preallocate/pre-touch all callback-reachable storage. Capacities are immutable while running. Initialization failure does not create a partially usable runtime. |
| Blocks | Planar `f32`; actual frames in `1..=max_frames`; fixed configured host channel counts. Zero frames is a no-op, including no command consumption or clock advance. Invalid shape returns a compact error and clears the writable output prefix without indexing beyond slices. Work on malformed inputs is capped by configured channels/frames. |
| Clock | Unsigned 64-bit absolute sample frames, starting at zero. Nonzero successful blocks advance exactly by their frame count. Reject noncontiguous host positions or arithmetic overflow. Frame positions travel as plain data; no required 64-bit atomic or hidden lock-based fallback. |
| IDs | Typed slot plus nonzero generation. Validate both. Reuse increments generation with checked arithmetic; exhaust a slot rather than wrap. Sequence/epoch/revision exhaustion rejects new mutations; no silent ABA. |
| Graph | Typed DAG; mono audio ports can be grouped/routed as multichannel signals. Deterministic node/source order; sum fan-in without clipping. Missing input/output connections are silence/empty MIDI. Reject cycles, invalid ports, bad references, illegal aliasing, or capacity violations. |
| Transactions | Prepare/stage privately; expected-epoch/revision checked at commit. All reference/range/topological/alias validation finishes before activation. Swap a plan/resource handle at a block boundary; do not scan or destroy the displaced graph. Retained processor slots preserve state. |
| Scheduling | Absolute-frame parameter/MIDI operations. Queue submission does not promise scheduling. Render admission reserves future-event and split capacity before issuing an accepted scheduling result. Apply on-time admitted events at exact frames; late arrivals apply at the next available frame with lateness reported. Reject excess work before acceptance. |
| Equal times | Stable source/port order, then monotonically increasing per-source sequence. Preserve input FIFO. Parameters at frame F affect sample F; events at block end belong to the next block. |
| MIDI | MIDI 1.0 short/system messages and bounded SysEx bytes; no silent truncation. Fixed event and byte arenas per prepared port. Reject invalid message lengths/data; deterministic merges. Report overflow and reserve capacity for bounded note-release/reset recovery, never silently overwrite note-offs. Recovery uses a separate reserved path emitting CC120 (all sound off), then CC123 (all notes off), on each of 16 channels. Refuse ordinary traffic while recovery is pending; acknowledge each reset packet only after downstream admission. Repeated overflow does not restart an unfinished reset. Runtime integration must preserve these credits independently of ordinary event capacity. |
| Budgets | Independent limits for command processing, scheduled events/subranges, transfer bytes, descriptor-validation steps, and publications per callback. Validate minimum indivisible-work requirements at preparation. DSP work is bounded by admitted nodes/edges/ports and frames; no drain-until-empty loops. |
| Completion | Distinguish submission, scheduling admission, application, and rejection. Include sequence/effective frame/epoch or revision. Reserve terminal-completion storage at submission. Reliable outcomes never compete with lossy/coalesced telemetry for capacity. |
| Ownership | Reserve retirement credits before activating work that displaces ownership. A full queue returns ownership to the caller or retains it in a preallocated slot. No callback final-reference destruction, including errors, cancellation, or dropped peers. |
| Disconnect | Continue the last valid graph and retain bounded storage if the control endpoint disappears. The host owns final teardown and must stop/join callbacks before destroying the render endpoint. No intentional leak fallback. |
| Resource upload | Reserve fixed pages/transfer state; validate bounded chunks, ranges, and sequence. Reject incomplete, malformed, stale, or oversize uploads. Cancellation returns storage through the same safe lifecycle. |
| Resource commit | New revision becomes visible atomically at a block boundary. Playback never observes partial data or a runtime-induced silent block. Sample-value discontinuities are not automatically crossfaded. |
| Capture | Append audio or timestamped MIDI within prepared capacity. At exhaustion, stop capture and mark incomplete, leaving playback and previous snapshots valid. No unlimited recording claim. |
| Snapshot | Lease immutable pages for a coherent revision/prefix cut at an explicit capture frame. A partial tail copy is page-bounded and budgeted. Publication may be pending; never report mixed revisions as current. Retaining leases consumes credits and may refuse further capture/snapshots. |
| Publication | Fixed-capacity coherent state slots, bounded attempts, no seqlock retry loop waiting for another thread. Optional telemetry can coalesce with a counter; reliable completions cannot disappear. |
| Processor | Preparation may allocate; process uses borrowed disjoint views and bounded writers only. No RT allocation/deallocation, locks, waits, I/O, logging, or panic on normal errors. User-supplied DSP must uphold this obligation; the type system alone cannot guarantee it. |
| Browser | No shared memory, Wasm threads, or isolation requirement. Wire plans are relocatable descriptors, never heap pointers. Initialize fixed worklet processor slots and Wasm storage before audio. Bound ingress and materialization; recycle transfer buffers by explicit credit. |
| Shutdown | Stop callbacks, cancel/drain pending transfers, release leases, then reclaim all owned objects on a non-RT context. Verify destructor location and credit balance; do not use callback teardown as the reclamation path. |

## Test groups

| Acceptance | Named group / evidence |
| --- | --- |
| A1 | `boundary`: production/build dependency audit; external public-API fixtures; unchanged existing Shoop consumers. |
| A2 | `portable`: common native/Node/Chrome tests; Linux/Windows/macOS CI; actual AudioWorklet on a non-isolated origin. |
| A3 | `rt_contract`: release-active allocator/reallocator/deallocator canaries, guarded render paths, source/call-path audit, deterministic work counters. |
| A4 | `graph`: scalar reference executor, seeded DAG generation, invalid descriptors, stale transactions, retained state, variable block partitions, saturated retirement. |
| A5 | `timing_midi`: exact output samples/events, timestamp boundaries, equal-time ordering, late outcomes, byte/event/subrange exhaustion and note recovery. |
| A6 | `resources`: revision/prefix immutability, incremental upload/export, large buffers, pinned leases, capture exhaustion, cancellation and stale commits. |
| A7 | `lifecycle`: small-capacity state machine, peer drops, full queues, destructor probes, two-thread stress, applicable Miri/model checking. |
| A8 | `end_to_end`: same admitted journal across offline/native/worklet, persisted benchmark configuration/results, runnable CI gates and failure canaries. |

Every group must exercise tiny capacities (one/two slots and the minimum valid budget), exactly-full and one-over-limit inputs, and post-failure recovery. Timing tests include different block partitions and long frame counters; resource lengths exceed per-callback transfer limits by at least 16 times. Validation errors must be observable under the same RT guard as success paths.

## Benchmark workloads

Use recorded seeds/configurations, 48 kHz, and block sizes 32, 128, and 512 where supported. Measure idle, gain chains, fan-in/out DAGs, representative 16-node and near-configured-capacity graphs, short-MIDI/SysEx bursts, sustained bounded capture, and concurrent plan/resource staging. Report duration distribution/max and nominal deadline misses, operation/byte/split counters, transfer throughput, peak live storage, and refusal counts. Keep the active graph/data workload identical when comparing baseline versus concurrent control traffic. Timing is diagnostic on a named host, not a brittle CI pass threshold or a claim about the old engine.
