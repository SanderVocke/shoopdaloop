# 17. Capacity, work budgets, and diagnostics

<!-- rust-source: ../../examples/coordinated_capture.rs -->
<!-- rust-source: ../../examples/stored_audio.rs -->

[Contents](../README.md) · [Previous: Recording resources together](../part-4-coordinate-processors-and-changes/16-recording-resources-together.md) · [Next: Submitting without allocation](18-submitting-without-allocation.md)

Size storage for everything simultaneously owned, then bound the work allowed in one host callback. These are different calculations. A prepared store can still run out, and finite work can still miss a deadline.

## Size a concrete recording task

Run the [coordinated capture example](../../examples/coordinated_capture.rs):

```sh
cargo run -p rt_runtime --example coordinated_capture
```

Its four-frame stereo/MIDI task has these independent costs:

| Store | Example requirement |
|---|---|
| Port/index space | One installed node; three host and three processor audio ports fit `max_ports: 8`. `max_nodes: 1` is slot space, not automatic installation. |
| Processor audio scratch | One input + two outputs, each four `f32`s: 48 bytes. Host buffers and processor-specific stores are additional. |
| Group staging | 32 stereo bytes + 30 encoded MIDI bytes fit 64 bytes. Members cannot borrow each other's regions. |
| Data payload | 24 × 16 = 384 bytes in the successful store. Old pinned data uses two pages; final new media uses four audio + four MIDI pages. Interval preflight also needs conservative free-page headroom. |
| Versions | Eight entries; retained old and prepared/active new versions coexist. Retired content remains occupied until reclaimed. |
| Snapshot slots | Two old + two promised final + four optional leases occupy all eight slots. |
| MIDI indexing | Four version banks, four checkpoints each, span two; each version supports seven packets, not a shared flexible total of 28. Old/new versions have independent initial states/rows. |

The same task with six pages keeps a complete two-frame prefix and refuses the next interval **before** dependent rendering; unrelated audio continues. Four pages refuse the initial all-member reservation without activation. These are Capacity refusals, not optional Budget exhaustion.

For the latter distinction, run [stored audio](../../examples/stored_audio.rs):

```sh
cargo run -p rt_runtime --example stored_audio
```

It exhausts optional transfer credit and receives Budget, while required reads still return the exact pinned bytes. Required access bypasses discretionary work allowances, **not** physical capacities, shapes, timestamps or lease checks. Neither example is a throughput benchmark.

## Validate configuration, then prepare every owner

`Config::prepare()` checks positive fields, port/storage representability and headroom constraints. It does not test available OS memory or prepare arbitrary processors, host adapters, graph owners or application banks. Further preparation checks typed element sizes/products before allocating and touching those stores. Retaining a mutable copy of Config cannot resize the prepared runtime.

The convenience values `audio_samples()`, `midi_bytes()` and `resource_bytes()` describe particular products, not total heap/RSS. Audio is `max_ports × max_frames` samples; MIDI is `max_ports × midi_bytes_per_port` bytes; resource payload is `resource_pages × page_bytes`. Add event descriptors, routed/injected node-input arenas, planner/queue/validator metadata, owner vectors, host buffers and processor-specific storage separately.

Each owned `MidiBuffer` adds reserved recovery storage for 32 events/96 bytes beyond its normal event/payload capacities. Node MIDI inputs have separate routed and injected arenas. Host recovery sinks are additional; spare recovery bytes do not enlarge an ordinary packet's admitted limit.

The [configuration/error reference](../reference/configuration-and-errors.md) lists every field/default and the validation rules.

## Separate admission, results and reusable flags

The runtime keeps several independent pools: scheduled action/result capacity, transport admission (`command_slots` plus one recovery owner), and prepared cancellation flags. Unread completions and results keep their credit occupied; reading a result recycles it at the next callback boundary. [Submitting without allocation](18-submitting-without-allocation.md) demonstrates Full versus Busy and exact payload reuse. The exact formulas for each pool are in [Configuration and errors](../reference/configuration-and-errors.md).

## Account for actual graph capacity

A descriptor's populated rows do not describe all its retained memory: `PreparedPlan::storage_bytes()` includes vector **capacities**, and returned owners retain them. Stage admission compares actual retained bytes against a conservative per-owner allowance derived from `max_nodes`/`max_edges`. The full accounting is in [Configuration and errors](../reference/configuration-and-errors.md); [Staging and coordinating graph activation](../part-4-coordinate-processors-and-changes/15-staging-and-coordinating-graph-activation.md) shows the lifecycle.

## Bound callback work without inventing musical quotas

One enclosing nonempty callback owns ingress snapshots, result recycling, command/recovery/publication allowances, optional copy/validation and maintenance. More planner intervals do not renew them. For `N` active nodes, `E` connections and `R` intervals, expect `N × R` processor calls and `E × R` route visits, plus actual sample/MIDI work. The capture example deliberately chooses `R = frames`; that is simple to inspect, not an efficient universal planning policy. `Budgets::subranges` is retained for compatibility and ignored.

## Read diagnostics from the owner

While the render owner is exclusively available, inspect `runtime.diagnostics()`, `resource_usage()`, `midi_index_usage()` and `capture_group_usage()`. While another thread owns rendering, use bounded publication instead of racing an owner borrow. `Control::try_telemetry()` is lossy; reliable owning completions/execution results decide request outcomes.

`Diagnostics` separates node/connection visits, scheduled deliveries, intervals, optional transfer/validation/reclamation and required render operations/copied bytes/validation. Resource and MIDI index work/state copies have their own counters. Capture's safe path counts three media copies: input → staging → stack scratch → pages; it is not zero-copy.

Diagnostic additions saturate and report overflow where exposed. Semantic identities, frames, revisions, token sequences and representable storage do not wrap into valid reused values; exhaustion is an error. A saturated counter is not permission to refuse a required sample or restart time.

## Measure separately from capacity

The existing [bounded runtime example](../../examples/bounded_runtime.rs) is a synthetic seven-workload smoke with heap guards and exact checks. Its capture workload intentionally uses legacy quota-based I/O; its historical timings do not measure required/group/loop throughput. See [historical repository performance evidence](../../../../../docs/history/pre-consolidation-fffc1faa/docs/rt_runtime/PERFORMANCE.md) for actual setup, distributions and limits.

Prepared storage and heap guards do not certify locks, third-party DSP, browser JavaScript, cold-cache behavior, platform deadlines or independent loop-host performance. Keep measurements and their conditions separate from the capacity/refusal contract.

[Next: Submitting without allocation](18-submitting-without-allocation.md)
