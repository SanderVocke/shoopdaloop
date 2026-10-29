> **Historical snapshot — not an active plan or current acceptance claim.**
> [History index](../../../README.md) · [Current plan](../../../../../PLAN.md) · [Original at `fffc1faa`](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/rt_runtime/GUIDE.md)
> Original text follows; only relative link destinations are adapted for browsing.
> Exact original bytes remain in the snapshot bundle. Old tasks, pauses and status apply only to that snapshot.

# Host and processor guide

`rt_runtime` owns bounded graph execution and resource/command state. It does not own an audio device, application state, a browser messaging protocol, or an OS thread. The host decides when to call it and when stopped objects may be destroyed.

## Reading path and executable examples

Use the completed [nineteen-chapter book](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/src/rust/rt_runtime/docs/README.md) for focused tasks and its [reference](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/src/rust/rt_runtime/docs/reference/README.md) for exhaustive configuration/execution/media/host rules. Examples are independent, not one growing application.

```sh
cargo run --locked -p rt_runtime --example first_graph
cargo run --locked -p rt_runtime --example planned_pair
cargo run --locked -p rt_runtime --example gain_graph
cargo run --locked -p rt_runtime --release --example gain_graph
python3 scripts/run_rt_runtime_hosts.py --native-only --profile debug
python3 scripts/run_rt_runtime_hosts.py --verify-failures
python3 scripts/benchmark_rt_runtime.py
```

`examples/gain_graph.rs` is a small complete public-API example: prepare a processor/slot, compile and submit a graph, poll its reliable completion, render guarded blocks, change gain exactly halfway through a block, consume the separate execution result, commit an empty graph, remove the processor, and destroy returned owners outside rendering. Its assertions run in both profiles. The independent host fixture adds chunked wire materialization, uploads, capture/snapshots, backpressure, actual browser execution, and complete pool accounting. The benchmark provides synthetic stereo/MIDI/legacy-quota resource workloads, not required/group/loop throughput or a production adapter. Existing commands/evidence do not imply a new host/benchmark campaign is needed for documentation.

`first_graph` installs startup metadata at epoch 1/frame 0 without a warm-up callback. `planned_gain` is the existing 32-sample/five-interval, one-consumer example with no allocator guard of its own; `planned_pair` adds guarded two-consumer decisions/observations. `pending_operations` teaches retained receipts and requested/effective/delivered/terminal frames; `staged_routes` and `coordinated_capture` teach graph/capture authority, refusal and independent cleanup. Their chapters state exact test scope.

| Graph path | Publication and ownership |
|---|---|
| Off-render initial graph | Fully validated during preparation; no ingress credit, receipt or hidden processor/planner call. |
| Ordinary Commit | Incremental live validation, then callback-ingress publication at the expected epoch; old graph continues while pending and refusal returns the candidate. |
| StageGraph → planner permit | Owning completion means Ready, not active/locked. Actual operation/planning-cut preflight rechecks scope/generation/current and validated epochs, retained capacities and pending MIDI. Commit follows all fallible domain work; displaced ownership returns separately. |

See [staged activation](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/src/rust/rt_runtime/docs/part-4-coordinate-processors-and-changes/15-staging-and-coordinating-graph-activation.md). Recovery cannot activate graphs. Unused permits change no graph owner, but cannot undo arbitrary application mutations.

A typical application prepares all storage and processors before starting callbacks. Graph compilation can allocate on the control side. The receiver incrementally validates every submitted plan against live generations/signatures/epoch; compile success is not permission to bypass validation. Keep a control-side catalog/identity mirror for concurrent compilation rather than borrowing the live runtime across threads. Rejected/refused commands retain their owning payloads.

## Capacity sizing

Memory capacities and work budgets are separate. A large queue does not authorize draining it in one callback. Configuration is checked once by `Config::prepare`; arithmetic overflow, impossible reserves, and common native/Wasm storage ceilings are errors, not truncation. Traversal diagnostics now saturate with `counter_overflow`; worst-case work estimates are not preparation limits or internal musical-work quotas.

| Setting | Size for |
| --- | --- |
| `max_frames` | Largest host callback; processors must also accept smaller scheduled subranges |
| `max_nodes`, `max_ports`, `max_edges` | Prepared slots, **all host plus node port counts**, and every typed route; fan-in/out adds edges |
| `midi_events_per_port`, `midi_bytes_per_port` | Ordinary per-port messages and payloads, including worst-case fan-in; each arena additionally reserves 32 resets / 96 bytes |
| `command_slots` | Maximum accepted normal-lane owning commands plus undrained terminal completions; the graph executor separately reserves one recovery slot. The lower-level `TransportConfig` exposes `normal_slots` and `recovery_slots` |
| `scheduled_events` | Future events **plus unread execution results**, subject to the effective credit limit below |
| `resource_slots` | Separate fixed arrays of identities, version/upload descriptors, and snapshot leases; replacement needs old and new versions concurrently |
| `resource_pages`, `page_bytes` | Resident versions, reserved uploads, active capture, pinned old versions, snapshot tail copies, and bounded reclamation lag |
| `budgets.commands`, `publications` | Command/dequeue and reliable publication work per callback; telemetry cannot steal terminal capacity |
| `budgets.transfer_bytes`, `validation_steps` | Resource copies / scheduler payload staging and graph/resource validation / reclamation |
| `resource_io_bytes`, `resource_io_steps` | Legacy quota-based reads/capture, optional snapshot-tail copies, and MIDI checks, reserved **inside** the shared callback budgets |

A conservative audio scratch estimate is `max_ports × max_frames × sizeof(f32)`; the runtime also owns MIDI metadata/payloads, node state, plans, queues, scheduler slots, and resource metadata. Host input/output arrays and returned owners are additional. Resource page bytes are exactly `resource_pages × page_bytes`; this is not total runtime memory. `PreparedConfig` reports checked storage counts, and the benchmark reports actual whole-process RSS separately.

Legacy quota-based example: at 48 kHz, a 128-frame stereo reader needs 1,024 bytes per callback, and a stereo recorder needs another 1,024. Reserve at least **2,048 processor bytes**, plus worst-case snapshot tail copies. With 256-byte pages, the total transfer budget must leave at least another 256 bytes for control; a 4,096-byte total permits additional headroom. Multiple processors/subranges share these totals; reborrowing `ResourceIo` never replenishes them. MIDI capture requires validation steps for its events. MIDI uploads consume a validation step per serialized byte, so chunk size must fit both the page remainder and unreserved byte/step budgets.

One minute of stereo `f32` at 48 kHz is 23,040,000 bytes, or 5,625 pages of 4,096 bytes. A concurrently staged replacement requires another full logical reservation; pinned old versions retain their pages too. A capture-prefix snapshot shares full pages and can require one private partial-tail page. Budget for the maximum outstanding leases, not merely the current revision. Resource length does not increase a single read/write command's copy limit. Recording is finite: exhaustion preserves a complete prefix and explicitly stops recording; it does not silently overwrite old data or resume after a gap.

Normal scheduled credits are `min(scheduled_events, budgets.events)`; a configured planner has one additional independent recovery receipt. Every external event reserves delivery and a reliable result, even if timestamps are equal. Internal planner intervals consume no such slots; `budgets.subranges` is retained but ignored. Increasing only `scheduled_events` cannot bypass the external event limit. Drain **both** command completions and execution results; admission and application are distinct. Future events can intentionally occupy all credits until their results are consumed.

The host fixture separately bounds five 512-byte wire credits, plan materialization records, and export cursor slots. These are host protocol costs, not hidden extra core copy credits. A real host must likewise bound decoding, JS message queues, buffer ownership, and completion recycling around the core.

Required `ResourceIo::render()` access instead uses independently counted reads and active audio/MIDI capture. It neither consumes nor refreshes background allowances; diagnostics saturate rather than refusing rendering. The capability cannot create leases, change lifecycle, or reclaim. Store-scoped `ReadIndex` supports logarithmic byte positioning; opt-in fixed MIDI checkpoint rows provide bounded event/state positioning. Opt-in capture groups add reserved lifecycle and staged coordinated commits through planner-only frame-bound authority; see [coordinated capture](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/src/rust/rt_runtime/docs/part-4-coordinate-processors-and-changes/16-recording-resources-together.md). The separate [loop facade](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/src/rust/shoop_loop_engine/docs/runtime.md) supplies musical roles/restoration, source/role banks and prepared-template publication; no independent loop host/benchmark is supplied in this snapshot. Size physical storage and workload cost separately; optional snapshots still need background headroom.

## Processor obligations

- Prepare and touch all processor-owned buffers before callbacks. Neither `process` nor `set_parameter` may allocate, reallocate, deallocate owning objects, lock, wait, log, perform I/O, or retry without a fixed bound. The crate cannot enforce arbitrary third-party implementations by type alone.
- Respect borrowed audio/MIDI/resource view lifetimes. Do not retain callback slices or pointers. Audio is planar; resource audio is interleaved little-endian `f32`. There is no implicit clipping, resampling, latency compensation, or added signal block.
- Bound work by configured frames/ports/events, including the separately reserved MIDI reset messages. A callback may split into several processor calls; use `block.frame` and `block.frames`, not a private assumption of fixed 128-frame blocks.
- Consume CC120/CC123 resets on every channel even when ordinary MIDI capacity is exhausted. Finish unrelated audio/state advancement when ordinary output refuses; signal MIDI overflow according to the processor contract so the runtime can propagate recovery. Hosts drain `drain_midi_recovery` into separately reserved storage before delivering ordinary MIDI.
- Handle capture failure without abandoning unrelated audio. Incomplete capture status is retained in resource metadata and requires an explicit new recording. Missing/stale playback leases and budget failures need an explicit application policy; do not hide them behind allocation or spinning.
- Retained leases embedded in processor state need explicit release after removal/replacement. Returning the processor in an owning completion does not let the library inspect its private resource handles.

Allocator guards are active in release and Wasm and deliberately tested to fail. They detect heap operations, not locks or I/O; source auditing remains necessary. JavaScript GC, MessagePort delivery, browser scheduling, and OS/device deadlines are not made hard-real-time by bounded Rust code.

## Ownership and stopping

Choose graceful settlement of known work while callbacks run, or explicit stopped abandonment. Do not render-until-idle indefinitely, render merely to reach abandoned future timestamps, replay a faulted callback, or invent outcomes for dropped work. An error may follow advanced time or published domain/graph state; clearing outputs is not rollback.

1. Stop admitting new application work. Account for every accepted command, staged host transfer, future event, export cursor, and lease.
2. Drain command and execution results for known work. Ticket cancellation is observed at original dequeue; queued typed cancellation and delivered-domain cancellation use different mechanisms. Eligible callback-start recovery parks a held normal owner/progress/original cancellation observation, then resumes it before later normal work under the same finite service/publication allowances. Endless recovery need not be fair; dependent reads may become stale.
3. Stop captures, cancel unfinished uploads, close host cursors, and release snapshots. Commit a graph that no longer references processors before removing them. Receive their owners and release processor-owned leases explicitly. Retired page chains recycle incrementally under the maintenance budget; drive bounded maintenance for known retirements and verify the required pool recovery; do not use arbitrary future requests as a drain condition.
4. Decide and report the host-level fate of intentionally abandoned future work; the runtime does not manufacture callbacks to reach a future frame. A disconnected control endpoint leaves render-owned data parked, not destroyed in the callback.
5. Stop/join callbacks before dropping the runtime, endpoint owners, unconsumed completions, or processor state. If a planner owns a cleanup ledger, use into_planner_and_resources; into_resources drops that ledger without releasing its hidden plain handles. Neither extraction settles requests, rolls back publication or sends device cleanup. There is no hidden blocking drain. In the worklet fixture, stop acknowledgement precedes Rust disposal outside `process`; closing a live context follows disposal. Retaining a live Wasm instance alone is not shutdown evidence.

Ordinary control-side `submit` may allocate. Hosts enqueuing on a render/worklet thread must use `prepare_submissions` / `submit_prepared`, bounded inactive plan storage, and preallocated payload buffers. A retained ticket/completion can prevent flag reuse and cause `Busy`; drop it only where final destruction is permitted. Never discard an owning refusal or retirement error in a callback.

## Historical support matrix and deliberate exclusions

This table records initial-runtime evidence, including source head `fefe626b1ff866daaa16d55de9091668defc32e3` / workflow `40145050855`, not current-loop-branch Windows/macOS CI or freshly available local tools. [VALIDATION.md](VALIDATION.md) retains exact run provenance; [loop validation](../loop_layer/VALIDATION.md) and [Miri jobs](../loop_layer/MIRI_JOBS.md) have separate scopes. Package Wasm is not a worklet, and the generic NoPlanner worklet is not an independent loop host.

| Host | Scheduled verification | Recorded evidence |
| --- | --- | --- |
| Linux x86-64 | Debug/release tests, examples, native replay, benchmarks, Miri | Passed on the recorded Nix host |
| Windows x86-64 | Debug/release tests, examples, native replay | Both CI cells passed; exact native replay hashes match Linux |
| macOS ARM64 | Debug/release tests, examples, native replay | Both CI cells passed; exact native replay hashes match Linux |
| Node Wasm | Shared core test suite | Passed with Node 22.22.2; not AudioWorklet evidence |
| Chromium Wasm | Shared core tests on relevant PR changes and nightly/manual runs | Passed with Chromium 147 |
| Real Chromium AudioWorklet | Independent offline/live worklet plus native equivalence and failure gates on web-debug CI | Passed without shared memory, threads, or cross-origin isolation |

The browser helper is a test host, not a supported production device/worklet adapter. Safari/Firefox, Wasm threads/shared memory, plugins, feedback graphs, parallel DSP, automatic latency compensation, resampling, MIDI 2.0, and unlimited recording are outside this initial release. No Shoop backend/application/protocol integration is included. See `VALIDATION.md` for actual commands/results and `PERFORMANCE.md` for measurement limitations.
