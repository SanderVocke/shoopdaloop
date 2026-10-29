# Capacity and performance

[Overview](../README.md) · [Host integration](integration.md) · [Scheduling and MIDI](scheduling-midi.md) · [Resources](resources.md)

The main tuning distinction is **storage capacity versus work allowed per callback**. More storage handles larger bursts or longer retention; it does not automatically increase processing throughput.

```text
                    TOO SMALL                         TOO LARGE
capacity      refusals / short capture          more prepared memory
work budget   slower control / refused I/O      more callback CPU / less headroom
block size    less time per callback            more device/block latency
```

Choose realistic upper bounds, then measure a mixed workload—not just an idle graph.

## Configuration reference

`Config::prepare()` validates capacities and arithmetic; it does not allocate the full runtime. `GraphRuntime::prepare()` allocates its pools, queues, and processor scratch and can report additional preparation errors. Prepared limits cannot be resized while running.

| Capacity | Default | Size it for |
|---|---:|---|
| `sample_rate` | 48,000 | Host engine rate; this field does not resample buffers |
| `max_frames` | 128 | Largest host callback; smaller callbacks are accepted |
| `audio_inputs`, `audio_outputs` | 2 each | Exact host channel counts |
| `midi_inputs`, `midi_outputs` | 1 each | Exact host port counts |
| `max_nodes` | 64 | Processor slot index space; declare reusable slots at preparation |
| `max_ports` | 128 | **Sum** of all host and prepared-node audio/MIDI inputs and outputs |
| `max_edges` | 256 | Connections in a candidate/active plan |
| `midi_events_per_port` | 256 | Ordinary events in each MIDI arena, including worst-case fan-in |
| `midi_bytes_per_port` | 4096 | Ordinary packet bytes per arena, including SysEx |
| `scheduled_events` | 256 | Storage ceiling; effective outstanding capacity is also limited by event/split budgets |
| `command_slots` | 64 | Outstanding normal commands, **including unread completions** |
| `resource_slots` | 32 | Size of each separate identity, version, and snapshot pool |
| `resource_pages`, `page_bytes` | 256 × 4096 | Fixed resource payload storage; includes retained/reserved content |
| `resource_io_bytes`, `resource_io_steps` | 0 each | Callback headroom protected for processor resource I/O |

| Per-callback budget | Default | What it limits |
|---|---:|---|
| `commands` | 8 | Command dispatch/dequeue work; cancellations also consume transport work |
| `publications` | 8 | Terminal publication allowance; telemetry has a separate allowance so it cannot steal completion capacity |
| `events` | 64 | Scheduler admission/execution bound; also host recovery attempts per output |
| `subranges` | 65 | Maximum scheduled render partitions |
| `transfer_bytes` | 4096 | Resource copies and scheduled MIDI payload staging, **not ordinary audio/MIDI routing** |
| `validation_steps` | 32 | Graph validation, MIDI upload byte validation, MIDI capture checks, and resource reclamation |

Host port counts and resource I/O reservations may be zero. Other capacities/budgets must be positive. `page_bytes` must be divisible by four, and `transfer_bytes` must cover at least one page. `GraphRuntime` also needs `subranges >= 2` for its scheduler, even without planned automation. Storage checks use a common `i32::MAX` byte/index ceiling; passing those checks is not a guarantee that the machine has enough memory.

### Common surprises at the defaults

```text
scheduled event/result slots       min(256, 64, 65 - 1) = 64
MIDI upload validation throughput  at most 32 bytes/callback before other work
resource payload pool             256 × 4096 = 1 MiB (not total runtime memory)
reserved processor resource I/O    zero — background commands can use the budget
```

## Estimate memory before preparing

These are payload estimates, not a complete allocator/RSS accounting:

| Component | Approximate storage |
|---|---|
| Processor audio scratch | Prepared-node audio input/output ports × `max_frames` × 4 bytes |
| Each MIDI arena | `(midi_events_per_port + 32) × size_of::<Event>() + midi_bytes_per_port + 96` |
| Scheduled payloads | Effective event capacity × `min(midi_bytes_per_port, budgets.transfer_bytes)` |
| Resource payloads | `resource_pages × page_bytes` |
| Additional storage | Processor state, slot/catalog/plan metadata, queues, resource metadata, scheduler records, host buffers, staging buffers |

Node MIDI inputs have both a routed-input arena and a scheduled-injection arena; node outputs have another arena. Host-owned buffers and recovery sinks are additional. At the defaults, the scheduler alone reserves 256 KiB of payload bytes, even if only parameter actions are used.

`PreparedConfig::{audio_samples, midi_bytes, resource_bytes}` describe configuration-derived storage quantities, **not total runtime memory**. Plans and command payloads may also remain owned while awaiting completion consumption. Measure live memory with representative prepared processors and outstanding work.

### Example: reserve playback + recording headroom

Assume stereo playback reads and stereo recording both process 128 frames per callback:

```text
playback read   2 × 128 × 4 = 1024 bytes
capture write  2 × 128 × 4 = 1024 bytes
                            ──────────
processor minimum               2048 bytes/callback
```

With 4096-byte pages, use at least `resource_io_bytes = 2048` and `budgets.transfer_bytes = 6144`. That leaves one page for control progress. Add headroom for concurrent voices, extra reads, and snapshot tail copies; this is not a universal preset. MIDI capture needs step headroom too.

One minute of stereo 48 kHz audio requires `60 × 48000 × 2 × 4 = 23,040,000` payload bytes, or **5625 pages** of 4096 bytes. Replacement, snapshots, and reclamation lag require additional space. The default 1 MiB pool is not a long-recording preset.

## What affects callback cost?

```text
callback cost ≈ bounded control work
              + audio sample processing / route mixing
              + per-subrange graph dispatch and MIDI work
              + resource copying / capture / maintenance
              + host adapter work
```

| Workload change | Performance consequence |
|---|---|
| More audio edges | More sample copies/adds; fan-out is not free |
| More distinct scheduled frames | More processor calls and graph traversals per host callback |
| Larger scheduler capacity | More payload memory and bounded ordered-list shifting at admission/execution |
| Dense MIDI fan-in | Stable insertion shifts descriptors; worst-case ordering can approach quadratic descriptor work per destination |
| More MIDI bytes | More packet validation/copying; event count alone misses SysEx cost |
| Larger graph validation budget | Faster activation, but more control work before rendering |
| Oversized `max_nodes` | More preparation memory and validation work even for small candidate graphs |
| Larger transfer budget | Faster resource movement, but more per-callback memory traffic |
| Long-lived snapshots | Retained pages/versions; pool exhaustion despite low current transfer traffic |
| Small callbacks | Less wall-clock time and more per-second callback overhead, though control work gets serviced more frequently |

For a graph with `N` active nodes, `E` edges, and `R` rendered subranges, expect `N × R` processor calls and `E × R` route visits. Audio sample work remains proportional to the total samples processed; per-call overhead and MIDI merge work increase with splitting. Processor algorithm cost is outside runtime budgets—`budgets.events` is not a general CPU instruction limit.

MIDI resets have separate bounded work: up to 32 internal reset messages per affected port, plus host drain attempts per output. Include overload/recovery in benchmarks.

### Signal latency versus control latency

| Latency | What determines it |
|---|---|
| Audio signal | Host/device buffering and processor algorithms; graph routing adds no extra signal block |
| Graph edit | Queue snapshot, preceding commands, validation budget, publication limits; activation at a callback boundary |
| Scheduled action | Admission lead time, then the absolute sample frame; lateness is explicit |
| Upload/export | Command and byte/validation budgets, page boundaries, competing processor work |

A held multi-callback validation or payload-staging command can delay later commands. Larger queues absorb backlog; they do not remove that latency. Recovery admission does not preempt the held command.

## Measure and diagnose

At 48 kHz, 128 frames represents about **2.67 ms** of audio. That is a nominal callback interval, not a safe runtime CPU allowance: the host, other processing, and scheduling jitter need headroom too.

The crate includes an offline benchmark with sample/reference checks and heap guards:

```sh
cargo run -p rt_runtime --release --example bounded_runtime -- \
  --iterations 20000 --raw-dir measurements
```

It exercises idle rendering, 16-node and dense 64-node audio graphs, MIDI routing, stereo capture, graph replacement, and resource roundtrips. Output includes JSON timing percentiles/maxima, work counters, and throughput; `--raw-dir` writes per-callback CSV files. Timing includes the benchmark's render guard/host wrapper. Linux process memory/scheduler counters are optional and are not per-runtime memory accounting. These synthetic offline workloads do not cover arbitrary DSP or real device scheduling.

For your own measurements:

1. Use release builds and realistic processors, graph fan-in/out, MIDI density, variable block sizes, and scheduled splits.
2. Exercise resource upload/capture/export concurrently, with slow completion consumers and nearly full pools.
3. Record callback-duration distributions and maxima outside the measured render scope; never print or allocate timing records in the callback.
4. Check samples/event results as well as speed. Include overload, cancellation, and shutdown paths.
5. Repeat on the actual device/browser/thread environment. Native or offline measurements alone do not establish browser callback timing.

| Observation | Check first |
|---|---|
| `Full` on command submission | Drain completions; check outstanding normal/recovery credits |
| `Busy` on prepared submission | Release completed tickets/results so cancellation flags can be reused |
| Schedule rejected `Full` | Drain execution results; reduce lookahead or raise all effective-capacity limits |
| Resource `Budget` | Shared copy/validation load and reserved processor headroom |
| Resource `Capacity` | Active/retained versions, uploads, snapshot leases, and unreclaimed pages |
| Repeated `midi_overflows` | Destination fan-in capacities, processor output handling, host recovery delivery |
| Slow graph activation | Validation budget, queued/held commands, completion publication cap |
| `StaleEpoch` / `StaleRevision` | Refresh state and reconsider the edit; blindly retrying the same expectation will not fix it |
| `Exhausted` | Counter/generation space cannot wrap; plan explicit reinitialization after safe shutdown |

`Diagnostics` exposes nodes/connections processed, events/subranges, copied bytes, validation steps, MIDI overflows, reclamation counts, and render errors. Validation steps include resource maintenance—not just graph checks. `resource_usage()` gives pool counters to the runtime owner between callbacks. Telemetry is bounded and lossy; it is neither a timing profiler nor an authoritative command acknowledgement.

## Check the RT contract

From this crate directory in the configured build environment:

```sh
cargo test -p rt_runtime
cargo test -p rt_runtime --release
cargo run -p rt_runtime --release --example gain_graph
```

The [contract tests](../tests/contracts.rs) install allocation/reallocation/deallocation canaries; [executor tests](../tests/executor.rs) check routing, timing, and overload; [resource execution tests](../tests/resource_execution.rs) exercise competing resource work. These are useful checks, not a certification of arbitrary processor implementations.

Allocation guards do not detect locks, waits, external I/O, OS scheduling delays, or every possible input path. Audit callback-reachable host/processor code and measure target-specific timing separately. The library does not promise a hard-real-time deadline or a portable throughput figure.
