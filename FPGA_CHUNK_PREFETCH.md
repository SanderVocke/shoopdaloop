# Early chunk prefetch

Implemented in Rust and RTL on the existing [chunk-memory subsystem](FPGA_CHUNK_MEMORY.md).
Prefetch is speculative; ordered job execution and coherent chunk memory determine
the data observed by reads. No dependency-aware prefetch scheduler is required.

## Current implementation

- Rust and RTL generate a quantum's jobs before executing them. In RTL, the
  state engine already exposes each job as it enters the job FIFO.
- State-processor enqueue events feed a bounded read-intent FIFO. A separate
  `media_prefetch_resolver` walks each actual read window, sharing descriptor and
  chunk-table RAM read ports with foreground operations, which have priority.
  Job-processor next-job hints remain a fallback for newly published mappings.
  Metadata currently resides in bounded RAMs;
  moving chunk tables to DDR is separate work.
- Chunk memory already has separate read/write pools, an independent prefetch
  input, buffered write completion and forwarding from the write pool. A write
  invalidates matching read entries and supersedes matching speculative fills.
  Required demand fills and writebacks retain their existing ordering.
- Prefetch acceptance acknowledges a hint, not completed residency. Hints may be
  dropped; demand reads already wait for usable data. `Fence` drains writeback
  and active transfers to the external memory interface; it is not a
  prefetch-readiness fence. AXI completion guarantees visibility and ordering,
  while the DDR controller may still hold posted writes internally.

## Earlier hints and a small resolver

A logical read intent is forked whenever the state processor successfully enqueues
a read job. The normal job FIFO and execution order are unchanged. The hint FIFO
has `JOB_DEPTH` entries and resets at quantum admission; a full FIFO drops early
advisory hints without stalling state execution.

The resolver translates the intent's view and chunk range into physical chunk
IDs and sends them to the existing prefetch interface as soon as possible. Reuse
the media frontend's descriptor/table lookup logic and arbitrate its metadata RAM
ports. Keep allocation, publication and ownership in the media frontend; keep
sample access, buffering and coherence in chunk memory. Long chunk-ID tables stay
outside the state processor.

Hints can therefore start transfers during state processing, before job execution
begins. There is no requirement to overlap the state and job processors themselves
or to collect or finish all iteration reads before executing jobs. Existing demand
priority, writeback scheduling and demand-read gating remain applicable.

## Chunk units

At the compiler/state/job boundary, express media ranges and durations as chunk
counts, and positions and view offsets as chunk indices. The primary configuration
has chunk size equal to the quantum: one whole chunk per channel per playback or
capture job. Multiple-chunk windows remain expressible, including loop wrap.
Generated accesses are aligned by construction, with no alignment-error path.

`Graph` and `Program` explicitly specify `chunk_frames`; the quantum is an integral
number of chunks. Wire format 2 names playback fields `position_chunks` and
`length_chunks`; old format-1 documents are rejected. The length remains the loop
wrap bound, while `ReadIntent.chunks` specifies the actual window. Transition
records retain frame clocks and are converted at the state-processor input.
Arithmetic ramps, lower-level sample streams and existing partial-span APIs
retain sample units. `MediaStore::slice_chunks` prepares ranges in chunk units.

## Coherence across intervening writes

An early hint may fetch old contents. When the ordered job processor later writes
that chunk:

- Remove a matching queued prefetch and invalidate a completed read-cache copy.
- Mark a matching in-flight speculative fill obsolete. Let accepted external
  transactions drain, but suppress their cache publication and prevent late data
  from restoring the old entry. Retain ownership of its buffer until drained.
- Buffer the full-chunk write without waiting for that obsolete read transfer.
  On write completion, its write-pool entry is authoritative. Subsequent reads
  forward from it even before DDR writeback; new hints recognize that entry.

Distinguish speculative fills from demand fills and writebacks: this cancellation
rule does not cancel required operations. Retain retirement-before-reuse, memory
fault reporting and backpressure. Normal job ordering ensures an earlier demand
read finishes before a later write modifies its contents.

Early address resolution is also advisory. A capture can allocate or publish a
different chunk after a hint was resolved. Keep the actual read's existing
execution-time lookup; never bind it to a speculative ID. Missing or unpublished
mappings simply produce no hint. This preserves read/write/read dependencies
without teaching the resolver about graph topology or loop transitions.

No new prefetch-completion barrier is needed. If completion tracking is added
later, superseded hints must retire from it; demand readiness still means that
the current authoritative contents are available.

## Rust reference and verification

The original separate Rust FPGA model has been retired. The production runtime
now supplies logical playback/capture and arithmetic expectations; RTL lowering
emits the persistent state program. The production CPU path does not model
physical prefetch or BRAM caches.

Standalone cocotb tests cover queued, completed and in-flight
prefetch/write conflicts, including delayed stale responses, repeated writes and
retirement/reuse. An isolated resolver checks table indexing under stalled metadata
and prefetch interfaces. Production Rust/RTL comparisons and local protocol fixtures cover ordered read/write/read
and capture/publication/read cases, checking samples and media contents rather
than identical hint schedules. Wire/schema fixtures use the new units explicitly.
Aligned Q=32 timing runs use moving playback positions and media larger than cache
capacity. Reproduction and measurements are in [the media README](fpga/media_store/README.md).
