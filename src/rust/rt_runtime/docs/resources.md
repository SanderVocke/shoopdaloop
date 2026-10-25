# Resources: upload, capture, and read

`RenderIo::snapshot(ReadIndex)` returns the checked metadata for the pinned prefix, not a newer active resource version. It borrows the existing scoped lease without acquiring another lease or consuming background transfer credit. Audio renderers can validate format and duration before touching output.

[Overview](../README.md) · [Host integration](integration.md) · [Capacity and performance](performance.md)

`ResourceStore` holds audio/MIDI content in a fixed page pool. It provides revisioned storage and bounded copying—not a sampler, file decoder, resampler, or sequencer. Processors implement playback and recording behavior using that storage.

```text
ResourceId (stable identity)
    │
    └─► active revision ──────────────────────► pages
            │ replacement
            ▼
        old revision ◄── Snapshot lease ─────► old pages remain readable
                              │ release
                              ▼
                       incremental reclamation
```

## Choose an access path

| Context | API | Budget/lifecycle owner |
|---|---|---|
| Running host control | `Command::resource(Request::...)` | Executor applies requests at callback boundaries and returns owning completions |
| Processor/planner, legacy/background access | `ResourceIo` | Shared callback budgets; bounded read/capture/snapshot/metadata access |
| Required data access | `ResourceIo::render()` (`RenderIo`) | Fixed-capacity read/capture; counted independently, never throttled by background allowances |
| Before startup / custom integration | `ResourceStore` directly | Caller manages `begin_callback`, reclamation, and final destruction |

To preload content or playback leases, prepare a store and pass it to `GraphRuntime::prepare_with_resources`. Its configuration must match the runtime. Processors can retain IDs, snapshot IDs, and cursors from that same store.

**IDs and cursors are scoped to one store.** Do not reuse them across independent runtime instances. Copying a snapshot handle does not create a new lease; release each acquired lease exactly once.

## Required rendering versus background work

`RenderIo::read`, `capture_audio`, and `capture_midi` use the same prepared storage, cursor/lease validation, frame/shape checks, and explicit physical-capacity failures as the legacy methods. They do not consume or replenish background byte/validation allowances. Reads copy at most one page remainder; audio appends remain bounded by configured frames and resource channels; MIDI appends remain bounded by configured arena capacities. They do not promise a CPU deadline.

`ResourceStore::render_work()` reports attempted data operations, copied bytes, and charged MIDI event-validation units. These counters saturate and latch `counter_overflow`; they cannot refuse work. The executor publishes the same counts as `Diagnostics::render_operations`, `render_copied_bytes`, and `render_validation_steps`, merging overflow into `counter_overflow`. All counters reset once at the host callback boundary, not when a view is reborrowed or a planner chooses an interval.

The capability cannot upload, allocate a snapshot lease, begin/stop capture, reclaim, or reset accounting. Optional snapshots and exports still use background allowances. Existing `ResourceIo` methods retain their quota-based behavior and reservations. Separate opt-in [capture groups](capture-groups.md) provide lifecycle reservations, exclusive claims, and staged coordinated commits. Planner contexts add frame-bound lifecycle authority; consumers receive only staged writes through `RenderIo`. Single-resource append methods reject managed members and are not group transactions.

## Positional byte access

`ResourceStore::read_index(snapshot_id)` or `ResourceIo::read_index` prepares a `ReadIndex` in constant metadata work. It borrows the existing lease rather than allocating or duplicating ownership. `RenderIo::cursor_at(index, byte_offset)` positions a cursor in logarithmic page-index steps; `ResourceStore::cursor_at` is also available to custom integrations. Reads then use the same page-bounded copy operation. Offset equal to prefix length denotes EOF; larger offsets fail. There is no content scan at prepare, seek, wrap, or freeze.

A balanced append-only search tree is maintained as upload/capture pages arrive. Its nodes occupy the existing page metadata pool; storage is O(total pages), not a version-by-page cross product. Insertion/rebalancing and positional lookup take O(log version pages); sequential reads retain their direct next-page link. Snapshot cuts retain the version index and private-tail semantics across further appends, rotations, replacement, and removal. Reclamation still clears at most one data/index page per maintenance step.

`ReadIndex` and all cursors carry an opaque store-instance scope and reject foreign/released handles. Legacy raw snapshot/resource IDs still require the documented same-store precondition at preparation. Scopes are allocated only before rendering, with checked exhaustion; handles are not portable between independent Wasm instances.

`IndexWork` separately reports background/render metadata steps and saturating overflow through `Diagnostics::resource_index_work`. Upload index maintenance is bounded overhead of admitted page materialization, not a separate content scan or additional quota; existing transfer, parser, and reclamation budgets remain effective. Required lookup/append metadata work is counted without background throttling. These counters reset once per host callback.

The page index positions bytes. Opt-in [MIDI checkpoint indexing](midi-indexing.md) adds event-boundary cursors, complete packet reads, and bounded state positioning. Seeking to an arbitrary MIDI byte is not event positioning. Restoration emission remains separate work; coordinated capture uses the explicit capture-group API rather than raw single-resource append.

## Upload an immutable revision

```text
Create → BeginUpload(expected_revision, description)
                 │ reserves all required page credits
                 ▼
       Write(offset 0) → Write(next offset) → ...
                 ▼
              Finish        checks completeness
                 ▼
              Commit        checks expected revision again
                 ▼
          new active revision; existing snapshots stay readable

Before commit: CancelUpload releases the unused reservation and retires written data.
```

A newly created resource has revision zero and no content. Only a finished upload can become active. Concurrent replacement may make `Commit` fail with `StaleRevision`; cancel an upload you will not use.

### Content layout

| Format | Bytes | Duration |
|---|---|---|
| `Format::Audio { channels }` | Interleaved little-endian `f32`; exactly `frames × channels × 4` bytes | Explicit frame count; channels must be nonzero |
| `Format::Midi` | Repeated little-endian `u64 frame`, `u32 packet_length`, complete packet bytes | Explicit frame count, including trailing silence; records ordered by frame and strictly before the end |

Use `encode_midi_record` with caller-provided storage to encode a record. Resource audio is **interleaved**, unlike planar callback audio. Decode/interleave into preallocated processor scratch as needed. There is no sample-rate metadata conversion.

### Chunk sizing

```text
chunk bytes ≤ remaining bytes in current page
            ≤ remaining callback copy budget
MIDI also:  ≤ remaining callback validation steps (one step per byte)
```

Write offsets must exactly match the received prefix. Chunks cannot cross page boundaries, but MIDI records can span multiple chunks/pages. Empty writes and malformed chunks fail without advancing the prefix. A failed MIDI parser attempt still consumes validation work.

Executor read/write buffers cannot exceed `page_bytes`; an oversize buffer is refused before queue admission. With default budgets, a MIDI upload chunk is at most **32 bytes before other validation work**, despite a 4096-byte page size.

### Receiving resource replies

```text
submit Command::resource(request)
       ↓
try_completion()
       ├─ Applied → returned ResourceCommand::reply() contains Reply
       └─ Rejected / Cancelled → no reply; request ownership still returns
```

Inspect both the completion outcome and the reply. A queued request is pending, not a successful operation. `Request::Read` returns the **advanced cursor and filled buffer inside the returned request**; consume only `Reply::Read { written, eof }`'s written prefix. Use `request()` or `into_request()` to recover the data.

`Budget` means that operation was refused, not automatically queued for retry. Retry a returned request in a later callback when appropriate; do not busy-loop on the control thread expecting more work in the same callback.

## Snapshot and export

```text
Snapshot(resource) → Cursor(snapshot) → Read → Read → ... → ReleaseSnapshot
                                         │
                                  at most one page remainder per call
```

A snapshot pins an immutable uploaded revision or a coherent captured prefix. Each cursor advances sequentially; indexed positioning is explicit and reads do not perform a hidden whole-resource seek. A short read may merely reach a page boundary, not EOF. With the direct API, compare cursor position with snapshot byte length; an empty output buffer also returns zero.

| `Currentness` | Meaning |
|---|---|
| `Current` | Snapshot still matches active content and capture status |
| `Stale` | Content was replaced/appended or capture status changed |
| `ResourceRemoved` | The identity no longer exists |

All three states permit reading a still-held snapshot. Release the lease even if export is cancelled. Dropping a copied `Snapshot` value does **not** release its pool reservation.

## Capture

```text
BeginCapture → append contiguous blocks → StopCapture → frozen content
                       │
                       └─ storage/copy/validation exhaustion
                              ▼
                     stopped + incomplete prefix
                              │
                     start a NEW capture explicitly
```

- `BeginCapture` activates a fresh revision. Processor calls to `capture_audio` or `capture_midi` append at contiguous absolute engine frames.
- Audio capture takes interleaved samples. MIDI offsets become resource-relative frames; empty MIDI blocks still extend duration.
- Successful nonempty appends advance the revision. `stop_capture` freezes bytes without changing their revision, but its status change makes prior status snapshots stale.
- Storage exhaustion (or work-budget exhaustion through the legacy access path) stops recording atomically: none of the refused append is stored. Earlier successful appends—including earlier subranges—remain valid. Capacity recovery does not silently resume after a gap.
- Report capture failure through application state without abandoning unrelated audio processing. Propagating a capture error from `Processor::process` will instead fail/silence the host block.

A running-capture snapshot shares complete pages and copies only its partial mutable tail. It needs one spare page when that tail exists, plus copy budget for its used bytes. Refusal leaves capture unchanged. `Snapshot::cut_frame` is the **actual captured prefix end**, not necessarily the time a control request was submitted. Inspect `capture.active` and `capture.incomplete` through snapshot metadata or `info`.

## Budget and pool planning

```text
budgets.transfer_bytes
┌─────────────────────────────────────┬───────────────────────────┐
│ control + scheduled payload staging │ resource_io_bytes reserved│
└─────────────────────────────────────┴───────────────────────────┘
                                     processors may use reservation
                                     PLUS unused control allowance

budgets.validation_steps
┌─────────────────────────────────────┬───────────────────────────┐
│ graph / upload validation           │ resource_io_steps reserved│
└─────────────────────────────────────┴───────────────────────────┘
                                     unused steps can reclaim later
```

Reservations are **inside**, not on top of, the total budgets. Defaults reserve zero. For legacy quota-based access, size byte headroom for all processor reads, captures, and snapshot-tail copies in the worst callback; size step headroom for MIDI capture event checks. Required `RenderIo` access does not need this headroom, but still needs sufficient physical storage; optional snapshot-tail copies remain budgeted. Reborrowing `ResourceIo` for another node or subrange does not reset counters. Background work cannot consume reserved headroom.

Preparation requires at least one page of unreserved copy budget and one unreserved validation step. Reclamation runs after rendering with remaining validation steps, so sustained consumption can postpone page reuse.

| Pool | What competes for it |
|---|---|
| `resource_pages × page_bytes` | Active content, reserved uploads, old leased versions, capture snapshot tails, retired pages awaiting recycling |
| `resource_slots` identity pool | Live resource IDs |
| Separate `resource_slots` version pool | Uploads, active/captured revisions, retained and retiring revisions |
| Separate `resource_slots` snapshot pool | Outstanding leases |

An atomic replacement needs room for **old and new content together**, plus any older pinned revisions. Retained leases can exhaust pools even when no upload is active. Removing a resource or releasing a lease need not make its pages immediately reusable.

Executor reclamation is automatic and bounded; custom direct-store hosts must call `reclaim` under the remaining budget. Direct users call `begin_callback` once per callback, never per operation. Final store destruction is non-RT.

Upload cancellation, snapshot release, and capture stop may use the recovery lane. They can overtake queued normal reads/writes, which may then fail with stale handles. Removing a processor does not release leases hidden in its state: retrieve the processor, then explicitly release those leases through control requests while the runtime is running, or reclaim everything after callbacks stop.
