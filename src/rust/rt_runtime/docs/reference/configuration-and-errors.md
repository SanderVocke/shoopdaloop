# Configuration and errors

[Book](../README.md) · [Reference](README.md) · [Sizing task](../part-5-configure-and-operate-the-runtime/17-capacity-work-budgets-and-diagnostics.md)

`Config::prepare()` produces an immutable Copy `PreparedConfig`. It validates representability and basic relationships; actual graph/node/planner/resource/transport preparation checks further typed products and allocates/touches their stores. It does not promise OS allocation success, a deadline, or permission to change prepared limits while rendering.

## Config fields

Defaults are from `Config::default()`; capacities are element counts unless a unit is shown.

| Field | Default | Meaning |
|---|---:|---|
| `sample_rate` | 48,000 | Positive frames/second; `u32`. |
| `max_frames` | 128 | Maximum nonempty host block; smaller positive blocks are valid. |
| `audio_inputs` / `audio_outputs` | 2 / 2 | Host audio port counts; each may be zero. |
| `midi_inputs` / `midi_outputs` | 1 / 1 | Host MIDI port counts; each may be zero. |
| `max_nodes` | 64 | Catalog/physical slot-index space, not automatic processor construction. |
| `max_ports` | 128 | Prepared host + installed-node signature port space, including inactive nodes. |
| `max_edges` | 256 | Graph connection bound. |
| `midi_events_per_port` | 256 | Normal events per owned port arena, independent of payload bytes. |
| `midi_bytes_per_port` | 4096 | Normal payload bytes per owned port arena; scheduled MIDI packets have a separate enforced limit. |
| `scheduled_events` | 256 | Requested normal action/result capacity; effective capacity also uses `budgets.events`. |
| `resource_slots` | 32 | Separate resource identity, version and snapshot pools each have this many entries. |
| `resource_pages` | 256 | Data-page count. |
| `page_bytes` | 4096 | Bytes/page, divisible by `sizeof(f32)`. |
| `resource_io_bytes` | 0 | Optional legacy processor copy headroom inside the callback's transfer total. |
| `resource_io_steps` | 0 | Optional legacy processor validation headroom inside the callback's validation total. |
| `command_slots` | 64 | Normal transport admission/completion owners. GraphRuntime adds one independent recovery slot. |
| `budgets` | below | Work allowances for one enclosing host callback, not every internal interval. |

Config requires the capacities/work fields to be positive except host port counts, resource-I/O headroom and ignored `subranges`. Each positive capacity/work value is at most `i32::MAX`; checked products must also fit the storage ceiling. Each host port count must fit `max_ports`; catalog preparation additionally checks the complete host/prepared-node port sum and identities/signatures.

Transfer bytes must be at least one page. `resource_io_bytes <= transfer_bytes − page_bytes`; `resource_io_steps < validation_steps`. Thus legacy headroom cannot consume the last page of unreserved copy or the last unreserved validation step. Headroom is **inside**, not in addition to, the totals. Required `RenderIo` access is not throttled by these discretionary allowances.

`PreparedConfig::audio_samples()` is `max_ports × max_frames` samples, `midi_bytes()` is `max_ports × midi_bytes_per_port` bytes and `resource_bytes()` is `resource_pages × page_bytes` bytes. They are not totals for all allocator overhead, recovery arenas, vectors, queues, host buffers or processor-specific memory.

## Budgets fields

| Field | Default | Meaning |
|---|---:|---|
| `commands` | 8 | Transport operations per callback, including recovery/cancelled work. |
| `events` | 64 | Normal action/result capacity and delivery bound; a planned runtime adds one independent recovery delivery. |
| `subranges` | 65 | Compatibility field, ignored. Zero is valid; internal intervals have no quota. |
| `transfer_bytes` | 4096 | Optional callback copy allowance; also caps prepared scheduled payload stride. |
| `validation_steps` | 32 | Shared graph validation, optional resource validation and reclamation allowance. |
| `publications` | 8 | Per-cycle terminal-return allowance and a separate optional telemetry allowance. Telemetry cannot steal a terminal publication's reservation. |

GraphRuntime prepares transport with one recovery operation per callback and three telemetry slots. Recovery remains subject to the total operation/publication envelopes. Snapshot/recycle boundaries, eligibility and held-normal parking are in [execution rules](execution-rules.md).

## Separate storage classes

### Actions, results and flags

Normal scheduler slots are `n = min(scheduled_events, budgets.events)`. Declaring a planner adds one recovery execution slot `r = 1`; otherwise `r = 0`. Event/result records and queues hold `(n+r)` elements with additional typed-size/product checks.

Payload storage is:

```text
(n + r) × min(max(midi_bytes_per_port, maximum declared operation_bytes),
              budgets.transfer_bytes)
```

The maximum includes planner and prepared-consumer operation declarations. MIDI admission still limits packets to `min(midi_bytes_per_port, transfer_bytes)`. A larger typed-operation declaration does not enlarge MIDI packets.

A scheduled future action, retained Pending token or unread result occupies its execution slot. Result reading returns credit through callback-boundary recycling. This is independent of transport admission: queued, current, parked and unread owning command returns occupy normal/recovery transport credits until received.

`prepare_submissions()` adds one reusable cancellation flag per lane credit. A retained ticket/completion can make a prepared submit Busy after admission becomes free; a full admission pool is Full instead. Preparing flags does not prepare arbitrary future payload objects or make their final Drop render-safe.

### Audio, MIDI and planner storage

Processor audio scratch uses `(audio inputs + outputs) × max_frames × sizeof(f32)` per prepared node. Add host buffers separately. Owned MIDI arenas reserve their normal events/bytes **plus 32 reset events/96 bytes**; node MIDI inputs have routed and injected arenas. Arbitrary processor-owned buffers and downstream host reset sinks are additional.

Planner decision/observation storage uses the fixed associated types and declared counts, with typed representability checks. Consumer observation reservations are per producer; another consumer cannot spend unused slots. A Copy type is neither a wire encoding nor permission to escape its borrowed lifetime.

### Graph owners

`PreparedPlan::storage_bytes()` counts `sizeof(PreparedPlan)` plus each retained vector capacity times its native row size: steps, connections, membership and bindings. It excludes allocator overhead. Compact populated rows can coexist with oversized retained capacity; row-count validation alone is insufficient for staging admission.

With `k` stage slots, `GraphStages` computes a conservative per-owner allowance `B` from configured node/edge maxima and native internal row sizes, then checks `(k+1)×B + k×sizeof(internal slot)` against checked arithmetic/`i32::MAX`. Some inline metadata is counted twice deliberately. These internal sizes/formula are not exported row-construction APIs or portable measured constants.

Candidates and displaced active owners must fit `B`. Returned plans retain their original capacities. Compiler temporaries, validator scratch and queued command owners have separate costs. Reusable inactive descriptor storage is still subject to validation before execution; [lower-level APIs](lower-level-apis.md) distinguishes materialization from certification.

### Callback work allowances

One enclosing nonempty callback owns ingress snapshots, result recycling, command/recovery/publication allowances, optional copy/validation and maintenance. Planner intervals do not renew them. For `N` active nodes, `E` connections and `R` intervals, expect `N × R` processor calls and `E × R` route visits plus actual sample/MIDI work; dense stable MIDI insertion can require quadratic descriptor work. Finite prepared structures must establish same-frame termination.

Eligible snapshotted recovery can park one held normal validator inline; parking adds no admission credit and preserves progress and the original cancellation observation. Cancelled recovery also costs operation/publication allowance, and an endless recovery stream need not be fair to normal traffic.

Legacy quota-based resource I/O reserves optional processor headroom (`resource_io_bytes`/`resource_io_steps`) **inside** the callback transfer/validation totals — not in addition to them. Example: for a stereo 128-frame read plus write, 1024 + 1024 = 2048 bytes of headroom is needed before any optional snapshot copy; with 4096-byte pages, transfer 6144 / headroom 2048 leaves a page for control work, and at least one unreserved validation step should remain. This is an illustrative legacy budget, not a required-render throughput ceiling. Upload MIDI validation charges bytes; arena-capture checks charge events, so their units differ.

### Resource, MIDI index and group storage

Count concurrent active/uploaded/prepared versions, old pinned prefixes, copied mutable tails, per-interval reservations and retired content—not only current media. Releasing the last reference can enqueue retirement instead of immediately freeing pages/metadata. Reclamation shares remaining validation work.

`PoolUsage::free_pages` includes physically free pages already promised by reservations; immediately available data pages are `free_pages − reserved_pages`. Do not add reserved pages as extra physical storage. Promised final snapshot slots are removed from the free pool before a public Snapshot exists.

An indexed version with `C` checkpoints and replay span `K` supports `C×K−1` complete packets. Checkpoint storage is approximately `versions × (1+C) × sizeof(MidiState)` plus metadata; each version's rows are private, not a flexible global quota. Lookup replays fewer than `K` records and returns strict-pre-target state.

Group staging is the sum of `max_frames × channels × 4` per audio member and `12 × midi_events + midi_bytes` per MIDI member. Member regions/bounds cannot be borrowed. Take versions, initial index rows, promised final leases and interval worst-case data pages are additional. MIDI interval preflight uses the full producer bound even for a short/silent interval; commit/cancel returns unused conservative page promises. Required group work counts three media copies and separate checkpoint-state copies.

Loop topology/node-pair/media/program banks have additional planner-specific storage; none is secretly included in the generic runtime formulas.

## Error meanings

Errors are compact Copy values. Representative causes follow; each operation's state/ownership contract still determines which check applies first.

| Error | Representative cause |
|---|---|
| `InvalidConfig` | Zero required field, invalid page alignment/headroom or invalid preparation dimensions. |
| `Capacity` | Unrepresentable storage/product; page/version/lease/checkpoint/member/arena capacity exceeded. |
| `Exhausted` | Frame, identity, generation, revision or sequence space cannot advance without wrapping. |
| `InvalidShape` | Wrong host block shape, channel/sample count, state/output array length or member format. |
| `InvalidFrame` | Not the next callback frame; wrong stamped lifecycle boundary or out-of-window end. |
| `InvalidId` | Missing/stale identity, scoped foreign access or invalid lease/token generation. Raw resource/snapshot IDs still require same-store discipline. |
| `InvalidEvent` | Invalid packet/range/timestamp or a rejected typed-operation encoding/category. |
| `Full` | Bounded transport/scheduler/result admission credit unavailable. |
| `Disconnected` | Peer endpoint unavailable; refused submitted values remain caller-owned. |
| `InvalidState` | Wrong lifecycle phase, absent prepared flags, duplicate terminal completion or occupied freeze output. |
| `InvalidGraph` | Invalid descriptor, missing required binding, planning/signature/catalog mismatch. |
| `Cycle` | Signal graph contains a cycle. |
| `PortType` | Incompatible route port kinds. |
| `StaleEpoch` | Expected/validation graph epoch no longer matches. |
| `Busy` | Processor still active, stage slot occupied, or reusable flag still externally owned. |
| `InvalidParameter` | Unsupported parameter/value or application preflight refusal. |
| `InvalidOffset` | Noncontiguous upload, invalid byte position/member ordinal. |
| `Incomplete` | Unfinished upload/record or missing group contribution. |
| `Budget` | Optional callback copy/validation allowance exhausted; not required-render throughput refusal. |
| `StaleRevision` | Expected resource revision no longer matches. |
| `Cancelled` | A scheduled parameter, MIDI or processor action was cancelled by `Command::Cancel` before delivery. |

Admission refusal returns the original value in `SubmitError::value`. An admitted owning completion has Applied/Scheduled/Rejected/Cancelled separately from later execution results. An execution has a runtime `Result<(), Error>` and optional typed domain results: `operation_result` from the planner and `processor_result` from a processor. A domain can report Refused while runtime delivery is `Ok(())`; neither success flag implies general domain publication.

Rendering faults can occur after frame/state/epoch publication. They are not rollback or authorization to replay the block. Diagnostic counters saturate rather than wrap; semantic exhaustion stays explicit. See [shutdown](../part-5-configure-and-operate-the-runtime/19-stopping-disconnecting-and-reclaiming-ownership.md) for graceful versus stopped-abandonment ownership.
