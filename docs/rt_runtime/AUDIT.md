# Historical callback audit and current contract notes

The original final audit is retained below with its initial-runtime source/evidence scope. It is **not a renewed full callback-reachable audit of the loop implementation**. Its split-credit range bound and aggregate diagnostic-counter rejection are superseded. The current documentation source review checked the distinctions below against planner/executor/transport, graph activation, resource/index/group and loop lifecycle interfaces while writing the public guides; it did not rerun models, canaries, Miri, platforms, benchmarks or CI.

## Current documentation corrections

| Path/contract | Current argument and review boundary |
|---|---|
| Scheduled intervals | Normal capacity is min(scheduled_events, budgets.events), plus one independent declared-planner recovery receipt. Positive intervals number at most host frames, not external-events+1. Planner same-frame termination is an application obligation; safe Rust/fixed storage does not prove arbitrary planner termination or deadlines. |
| Planning views/observations | Prepared immutable decisions, validated exact-generation bindings and per-producer observation reservations. No view survives mutation; consumers do not own another musical clock. Recovery must terminate at delivery-frame planning before positive samples. |
| Held normal ingress | Callback-snapshotted recovery parks one inline owner, preserving validator progress, original cancellation observation and sequence. Resume before later normal traffic. No extra credit/dequeue, refreshed allowance or normal fairness guarantee. |
| Staged graphs | Checked scopes/generations, actual retained vector capacities, expected/validated/current epochs and pending MIDI compatibility. Ready references do not lock owners. Unused permits publish nothing; final preflighted swap retains displaced ownership without callback destruction. |
| Required data/index access | Independent saturating counters; physical/shape/time checks still refuse. Append-maintained page trees, lease-scoped access and fixed MIDI replay spans avoid whole-history seek/freeze scans. Legacy/optional work retains background quotas. |
| Capture groups | Exclusive claims, prepared version/index/final leases, first interval before activation, fixed per-member staging and all-member common-prefix commit. Unexpected producer/staging faults are not ordinary exhaustion. Interval-before-take cancellation and explicit stopped recovery preserve actual ownership. |
| Diagnostics/configuration | Allocation/storage arithmetic remains checked against representability ceilings. Traversal/work counters saturate with overflow diagnostics; preparation no longer rejects a configuration merely because worst-case accumulated diagnostic work could overflow. Semantic frame/ID/revision exhaustion remains checked. |
| Loop policy | The separate resolver/facade owns finite SCC/action settlement, latched-failure retries, discontinuities, fixed requests/journals, coherent queries and prepared configuration/program publication. The selected documentation review does not certify a fresh complete loop callback audit. |
| Fault/teardown | Time/state/publication can precede a propagated error. No replay, rollback or fake finish. Preserve the cleanup ledger through stopped extraction; permanent domain quiescence recovers owners without device cleanup or fabricated abandoned outcomes. |

See the [execution reference](../../src/rust/rt_runtime/docs/reference/execution-rules.md), [coordinated capture](../../src/rust/rt_runtime/docs/part-4-coordinate-processors-and-changes/16-recording-resources-together.md), [loop facade](../../src/rust/shoop_loop_engine/docs/runtime.md) and [historical validation](VALIDATION.md). Heap guards establish observed heap behavior, not absence of arbitrary application waits/I/O, platform deadlines or independent loop-host evidence. No loop host/benchmark is supplied in this snapshot.

## Original initial-runtime final audit (historical)

All findings, diff claims and bounds in the remaining sections refer to that original audit, not a fresh source-head certification.

Scope: portable runtime, supplied reference/host processors, and the independent wire bridge. The core forbids unsafe code. The allocator shim is test-only and separately exercised under Miri. This audit complements heap instrumentation; heap guards do not detect locks, I/O, or waits.

## Entry paths and bounds

| Path | Work and ownership argument |
| --- | --- |
| `GraphRuntime::process` | Zero-length calls return without work. Invalid shapes clear only configured writable prefixes. Valid host MIDI inspection is bounded by configured ports/events. Clock validation precedes graph work; diagnostics are Copy values. |
| `apply_commands` | At most `budgets.commands` attempts. Complete candidate plans validate incrementally under shared step credits; no waiting for more transfer chunks while holding dispatch. Old active plans and removed processors move into owning completions. |
| `run_scheduled` | Effective event credits are bounded by schedule/event/split capacity. No new admission occurs during rendering. Each nonempty subrange advances at least one frame; there are at most `min(max_frames, effective_events + 1)` ranges. Event payload storage and result queues are preallocated. |
| `run_graph_range` / routing | Each validated node and edge executes at most once per subrange. Audio loops are frame-bounded; MIDI insertion is event-capacity-bounded. Endpoint generations, types, ordering, and scratch ranges are certified before activation. Borrowed source/destination slices are disjoint. |
| MIDI recovery | Each internal arena has 32 reserved reset messages / 96 bytes. Host recovery has a per-port, per-callback attempt budget; repeated calls cannot replenish it. Full output records overflow without aborting unrelated audio. |
| Transport | Admission reserves command, completion, and retirement ownership through receipt. Normal/recovery lanes have independent credits. Dequeue snapshots and operation/publication budgets bound loops. Push failures retain payloads; no spinning or final-reference destruction on the callback. |
| Scheduler | Pending/free vectors cannot exceed private slot-token capacity. Removal shifts Copy indices, not owners. Result/release queues retain private credits; defensive publication failure parks a Copy record without repeating execution. |
| Resources | Fixed identity/version/lease/page arrays. Upload/read copies fit one page remainder and available byte budget. MIDI parsing charges bounded attempted bytes even on malformed input. Capture reserves the entire block before appending; its page loop strictly reduces remaining bytes. |
| Snapshot/reclamation | Snapshots pin coherent revisions/prefixes; only a partial mutable tail is copied. Final release queues a descriptor/page index rather than traversing its chain. Every reclamation iteration spends one step and returns at most one page or descriptor. |
| Prepared ingress | Cancellation-flag search is bounded by lane capacity. Retained base owners prevent callback-side final Arc destruction. Inactive plan insertion checks actual capacity; stamped reset clears only Copy descriptor elements without allocating or dropping owners. |
| Host bridge | Five fixed packet/payload credits, bounded plan chunks/cursor slots, and bounded ingress/completion loops. Admitted owners return to their fixed pools; a consistency fault parks ownership and stops ingress rather than overwriting it. Browser envelopes never convey Rust pointers. |

Configuration now also checks the aggregate worst-case route/port/processor/event counter bound across all possible subranges against the common native/Wasm ceiling. Individually representable capacities could otherwise overflow `usize` diagnostics on wasm32 after billions of operations. Preparation rejects such configurations without allocation; portable contract tests cover the product, aggregate bound, and a large valid configuration. This is preparation-time arithmetic, not a change to normal callback work.

## Owning drops

- Runtime preparation, ordinary control submission, control compilation, and final stopped destruction may allocate/free; they are not callback operations.
- Current commands move to the return queue with their original owning buffers and optional retired plan/processor. Cancellation and rejection use the same owning path.
- A disconnected control endpoint does not destroy render-owned requests or returned data: queue storage remains owned by the render endpoint until the host stops and disposes it.
- Unexpected completion failure parks retirement in the executor's single emergency slot and permanently faults ingress, preventing overwrite. Normal credit accounting makes that path unreachable, but it still preserves ownership.
- Resource slot contents, leases, descriptors, page indices, and cursor updates are Copy data. Retired/free stacks are preallocated to the total corresponding physical pool; a live item belongs to exactly one lifecycle state. Generation exhaustion permanently removes a slot from reuse rather than wrapping.
- Processor removal returns the processor, not its private resource leases. The host explicitly releases those leases and waits for incremental reclamation before final disposal.
- The worklet releases buffer/plan credits only after reliable terminal completion and recycles owners under the guard. Successful stop/dispose happens outside `process` after logical pool recovery. Intentionally failing canary sessions are not used as evidence of graceful shutdown.

## Supplied processors and programming faults

The exact worklet processor is imported by portable guarded regression tests. Capture pool/budget/revision exhaustion leaves explicit incomplete metadata while audio continues. Its audio/state advancement completes before returning MIDI overflow so reset propagation does not desynchronize its retained frame count. Gain and benchmark processors have bounded frame/event loops and use only prepared or stack storage.

Remaining assertions/`expect` calls in resource append/publication concern private invariants: reserved page credits, materialized tails, validated MIDI descriptors, and active capture identities. Public invalid IDs, malformed chunks, capacity exhaustion, and budget refusal take compact error paths before those points. Callback-safe application behavior still depends on third-party processors honoring the documented contract; a processor panic is a programming fault, not a recoverable real-time event.

No callback-reachable mutex, condition variable, wait, sleep, logging, filesystem/network operation, dynamic graph compilation, or unbounded retry was found. `rtrb` supplies the SPSC transport; no custom lock-free queue was introduced. Native fixture clock coordination occurs outside the guarded call. JavaScript allocation/GC, messaging, and OS scheduling are explicitly outside the Rust hard bounds.

## Evidence and integration boundary

Portable contract, lifecycle, prepared-ingress, graph, scheduling/MIDI, resource/capture, mixed/property, and exact host-processor tests exercise successful and refused paths under active guards. Native tests add destructor probes, Loom, and two-thread stress. Miri checks selected ownership/aliasing paths. Actual offline/live worklet runs include guarded ingress/render/recycling, no memory growth, stalled consumers, cancellation, stale identities, and complete pool recovery. Deliberate native and worklet violations prove guards and runner failures are effective.

The implementation diff against `master` is limited to the new crate, test-only helper, generic tooling, CI, documentation, and lockfile metadata. No existing Shoop backend/application/protocol implementation or production browser host imports or calls `rt_runtime`. Upstream dependency updates were merged without changing application sources. See `VALIDATION.md` for commands/results and remote CI status; configuration is not a substitute for completed platform checks.
