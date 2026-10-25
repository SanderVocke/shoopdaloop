# Host and processor guide

`rt_runtime` owns bounded graph execution and resource/command state. It does not own an audio device, application state, a browser messaging protocol, or an OS thread. The host decides when to call it and when stopped objects may be destroyed.

## Executable API examples

```sh
cargo run --locked -p rt_runtime --example gain_graph
cargo run --locked -p rt_runtime --release --example gain_graph
python3 scripts/run_rt_runtime_hosts.py --native-only --profile debug
python3 scripts/run_rt_runtime_hosts.py --verify-failures
python3 scripts/benchmark_rt_runtime.py
```

`examples/gain_graph.rs` is a small complete public-API example: prepare a processor/slot, compile and submit a graph, poll its reliable completion, render guarded blocks, change gain exactly halfway through a block, consume the separate execution result, commit an empty graph, remove the processor, and destroy returned owners outside rendering. Its assertions run in both profiles. The independent host fixture adds chunked wire materialization, uploads, capture/snapshots, backpressure, actual browser execution, and complete pool accounting. The benchmark example provides stereo/MIDI/resource workloads, not a production device adapter.

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
| `resource_io_bytes`, `resource_io_steps` | Worst-case processor reads, capture, snapshot-tail copies, and MIDI event checks, reserved **inside** the shared callback budgets |

A conservative audio scratch estimate is `max_ports × max_frames × sizeof(f32)`; the runtime also owns MIDI metadata/payloads, node state, plans, queues, scheduler slots, and resource metadata. Host input/output arrays and returned owners are additional. Resource page bytes are exactly `resource_pages × page_bytes`; this is not total runtime memory. `PreparedConfig` reports checked storage counts, and the benchmark reports actual whole-process RSS separately.

Example: at 48 kHz, a 128-frame stereo reader needs 1,024 bytes per callback, and a stereo recorder needs another 1,024. Reserve at least **2,048 processor bytes**, plus worst-case snapshot tail copies. With 256-byte pages, the total transfer budget must leave at least another 256 bytes for control; a 4,096-byte total permits additional headroom. Multiple processors/subranges share these totals; reborrowing `ResourceIo` never replenishes them. MIDI capture requires validation steps for its events. MIDI uploads consume a validation step per serialized byte, so chunk size must fit both the page remainder and unreserved byte/step budgets.

One minute of stereo `f32` at 48 kHz is 23,040,000 bytes, or 5,625 pages of 4,096 bytes. A concurrently staged replacement requires another full logical reservation; pinned old versions retain their pages too. A capture-prefix snapshot shares full pages and can require one private partial-tail page. Budget for the maximum outstanding leases, not merely the current revision. Resource length does not increase a single read/write command's copy limit. Recording is finite: exhaustion preserves a complete prefix and explicitly stops recording; it does not silently overwrite old data or resume after a gap.

Normal scheduled credits are `min(scheduled_events, budgets.events)`; a configured planner has one additional independent recovery receipt. Every external event reserves delivery and a reliable result, even if timestamps are equal. Internal planner intervals consume no such slots; `budgets.subranges` is retained but ignored. Increasing only `scheduled_events` cannot bypass the external event limit. Drain **both** command completions and execution results; admission and application are distinct. Future events can intentionally occupy all credits until their results are consumed.

The host fixture separately bounds five 512-byte wire credits, plan materialization records, and export cursor slots. These are host protocol costs, not hidden extra core copy credits. A real host must likewise bound decoding, JS message queues, buffer ownership, and completion recycling around the core.

## Processor obligations

- Prepare and touch all processor-owned buffers before callbacks. Neither `process` nor `set_parameter` may allocate, reallocate, deallocate owning objects, lock, wait, log, perform I/O, or retry without a fixed bound. The crate cannot enforce arbitrary third-party implementations by type alone.
- Respect borrowed audio/MIDI/resource view lifetimes. Do not retain callback slices or pointers. Audio is planar; resource audio is interleaved little-endian `f32`. There is no implicit clipping, resampling, latency compensation, or added signal block.
- Bound work by configured frames/ports/events, including the separately reserved MIDI reset messages. A callback may split into several processor calls; use `block.frame` and `block.frames`, not a private assumption of fixed 128-frame blocks.
- Consume CC120/CC123 resets on every channel even when ordinary MIDI capacity is exhausted. Finish unrelated audio/state advancement when ordinary output refuses; signal MIDI overflow according to the processor contract so the runtime can propagate recovery. Hosts drain `drain_midi_recovery` into separately reserved storage before delivering ordinary MIDI.
- Handle capture failure without abandoning unrelated audio. Incomplete capture status is retained in resource metadata and requires an explicit new recording. Missing/stale playback leases and budget failures need an explicit application policy; do not hide them behind allocation or spinning.
- Retained leases embedded in processor state need explicit release after removal/replacement. Returning the processor in an owning completion does not let the library inspect its private resource handles.

Allocator guards are active in release and Wasm and deliberately tested to fail. They detect heap operations, not locks or I/O; source auditing remains necessary. JavaScript GC, MessagePort delivery, browser scheduling, and OS/device deadlines are not made hard-real-time by bounded Rust code.

## Ownership and stopping

1. Stop admitting new application work. Account for every accepted command, staged host transfer, future event, export cursor, and lease.
2. Continue callbacks while draining command and execution results. Cancellation is observed at dequeue and cannot undo already applied work. Recovery requests can overtake normal requests, so dependent reads may explicitly become stale.
3. Stop captures, cancel unfinished uploads, close host cursors, and release snapshots. Commit a graph that no longer references processors before removing them. Receive their owners and release processor-owned leases explicitly. Retired page chains recycle incrementally under the maintenance budget; continue callbacks until the required pool recovery is observed.
4. Decide and report the host-level fate of intentionally abandoned future work; the runtime does not manufacture callbacks to reach a future frame. A disconnected control endpoint leaves render-owned data parked, not destroyed in the callback.
5. Stop/join callbacks before dropping the runtime, endpoint owners, unconsumed completions, or processor state. There is no hidden blocking drain. In the worklet fixture, stop acknowledgement precedes Rust disposal outside `process`; closing a live context follows disposal. Retaining a live Wasm instance alone is not shutdown evidence.

Ordinary control-side `submit` may allocate. Hosts enqueuing on a render/worklet thread must use `prepare_submissions` / `submit_prepared`, bounded inactive plan storage, and preallocated payload buffers. A retained ticket/completion can prevent flag reuse and cause `Busy`; drop it only where final destruction is permitted. Never discard an owning refusal or retirement error in a callback.

## Supported matrix and deliberate exclusions

| Host | Scheduled verification | Recorded evidence |
| --- | --- | --- |
| Linux x86-64 | Debug/release tests, examples, native replay, benchmarks, Miri | Passed on the recorded Nix host |
| Windows x86-64 | Debug/release tests, examples, native replay | Both CI cells passed; exact native replay hashes match Linux |
| macOS ARM64 | Debug/release tests, examples, native replay | Both CI cells passed; exact native replay hashes match Linux |
| Node Wasm | Shared core test suite | Passed with Node 22.22.2; not AudioWorklet evidence |
| Chromium Wasm | Shared core tests on relevant PR changes and nightly/manual runs | Passed with Chromium 147 |
| Real Chromium AudioWorklet | Independent offline/live worklet plus native equivalence and failure gates on web-debug CI | Passed without shared memory, threads, or cross-origin isolation |

The browser helper is a test host, not a supported production device/worklet adapter. Safari/Firefox, Wasm threads/shared memory, plugins, feedback graphs, parallel DSP, automatic latency compensation, resampling, MIDI 2.0, and unlimited recording are outside this initial release. No Shoop backend/application/protocol integration is included. See `VALIDATION.md` for actual commands/results and `PERFORMANCE.md` for measurement limitations.
