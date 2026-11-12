# Deferred reclamation for retake, clear and remove

## Goal and scope

Make the foreground cost of clearing, removing or replacing a recording independent of its length. Transfer ownership of its existing DDR block-ID table to a bounded cleanup queue, then release its references and metadata incrementally.

Keep the current allocation blocks, per-block reference counts, independent slice tables, sample caches and DDR metadata-page allocator. Implement the change in the media store and automatic BeginCapture path. Future loop-level clear/delete controls can use the same primitive.

This plan was investigated against `proto/fpga-rust-reference` at `bc833fc310817648c9e9c1d4c7ae3d6d21ac24c6`. The relevant media RTL was unchanged from the saved block-allocation benchmark checkpoint. Verify the branch again before implementation.

## Immutable acceptance criteria

1. For admitted operations and a fixed hardware configuration, foreground CLEAR, REMOVE and BeginCapture perform a bounded number of metadata operations independent of recording length and cleanup backlog. They never walk the old block-ID table.
2. Detachment makes no second copy of the block-ID list or audio payload. It requires one fixed-size cleanup record for each nonempty detached table.
3. Every ownership reference is released exactly once. Retained ranges remain readable and unchanged. A block reaches the reusable-ID stack only after its final reference and required sample-cache/transfer retirement.
4. Cleanup yields across processing iterations. Its budget includes ownership operations, queue traffic, metadata traversal, metadata-page reclamation and retirement work. Audio completion does not wait for the pending queue to drain.
5. Capacity exhaustion produces a bounded, explicit failure. There is no synchronous full-sweep fallback and no silent loss of an old table.
6. After all live owners are removed and cleanup drains, the expected audio blocks, table/directory pages and cleanup records are recovered without leaks or double frees.
7. Existing production/RTL logical audio and supported state comparisons remain exact. Hardware allocation identities and the time at which deleted storage becomes reusable are separate diagnostic observations.
8. Deadline claims state the memory-service assumptions and include any non-preemptible transaction tail. A block-count limit or cycle counter alone is not treated as a proof against arbitrarily stalled DDR.

## Design rules and constraints

- An existing DDR table changes owner; it is not duplicated. A live view and its detached old table have independent lifetimes.
- Keep the existing live view-ID policy for this change. Pending cleanup does not consume a new live view ID.
- Existing slices have separate ID tables and already retain their own block references. Do not introduce persistent shared metadata trees or tree-page reference counts.
- Use the current coherent metadata access path for queue records and table accesses.
- Serialize refcount read/modify/write, reusable-ID stack changes and metadata-page allocator mutations. Never yield between a count read and a conflicting write without preserving exclusive ownership of that operation.
- Reclaim detached tables from the tail so their remaining entries stay a dense prefix. Metadata-page reclamation must also be resumable.
- Preserve progress across ordinary iteration boundaries. Keep the existing coordinated reset/fault-epoch contract; this change does not introduce recovery of a partially executed collector after reset or power loss.
- Pending tables occupy their existing metadata pages until consumed. New recording data and new table pages still need available pool capacity.
- Cleanup must be able to make progress when the metadata-page pool is exhausted; it must not require allocating new table pages to release old ones.
- Distinguish logical command completion, pending cleanup and full memory quiescence.

## Proposed data and command model

### Retired-table record

Reserve a configurable DDR queue region after the existing metadata-page arena. Start with one 64-byte metadata line per record. For example, 256 records occupy 16 KiB; the selected capacity must be justified by workload and exhaustion tests.

A record contains:

| Field | Purpose |
| --- | --- |
| `root_page` | Root of the existing DDR block-ID tree |
| `remaining_entries` | Length of its remaining dense prefix |
| `active_block` | Block being processed, when applicable |
| `progress_state` | Distinguish lookup, release, retirement and table/page cleanup |
| `session_epoch` | Reject state from a previous coordinated session |
| Reserved fields | Alignment and future diagnostics |

Keep queue indices and one active collector context on chip. Any traversal stack is bounded by configured radix depth, rather than recording duration. Count the active record against queue capacity until its table is fully reclaimed.

A FIFO with one resumable active record is sufficient for the initial implementation. Record queue occupancy and completion latency so a later scheduling-policy change can be evidence driven.

### Metadata operations

Extend the metadata service with internal capabilities equivalent to:

- Read/stage a live view's root and entry count.
- Detach that root and reset the live view's root/count/leaf translation.
- Read an entry through a detached root context.
- Shrink a detached dense-prefix table and return its updated root/count, with resumable page unlink/free operations.

Detached accesses must not depend on a still-valid live view ID.

### Foreground ownership transfer

For a nonempty table:

1. Validate the command and reserve a cleanup-record slot.
2. Capture the old root/count and stage an unpublished queue record.
3. Cancel or drain affected outstanding view lookups and invalidate its cached leaf translation.
4. Exchange the live root for an empty root and publish the new live descriptor.
5. Publish the cleanup record and acknowledge the command.

Serialize this sequence so other commands cannot observe a partial transfer. Before mutation, a capacity refusal leaves the original view unchanged. A memory error after mutation follows the existing sticky-fault/reset policy; do not retry an irreversible partial transfer as a fresh command.

The queue inherits the old ownership references without incrementing or decrementing their counters.

- CLEAR preserves a valid empty view and its existing kind.
- REMOVE invalidates the live view.
- BeginCapture preserves the current create-or-clear behavior for its stable recording view.
- An empty table needs no cleanup record.
- Explicit live-view Reclaim retains its existing semantics; detached-table cleanup uses an internal operation.

### Collector operation

For the last remaining table entry:

1. Resolve its block ID.
2. Perform a serialized reference release.
3. If other references remain, retain the physical block.
4. For the final reference, finish required sample-cache/transfer retirement before publishing reuse.
5. Remove the consumed table entry and incrementally recycle empty leaf/directory pages.
6. Checkpoint progress and continue only while the maintenance budget allows.

A progress state must prevent a reference from being decremented again if execution yields between reference release and removal of its table entry. Ownership audits must account explicitly for that in-flight state.

## Ordered implementation stages

### Stage 1: Establish the baseline and detached-root interface

- [x] Recheck the latest `proto/fpga-rust-reference` head and create a feature branch from it.
- [x] Read applicable repository instructions and reconcile upstream changes with this plan before editing.
- [x] Record current CLEAR/REMOVE/BeginCapture behavior and foreground timing boundaries.
- [x] Extend `fpga/media_store/rtl/media_metadata.sv` to support root transfer and detached-root access independently of live view IDs.
- [x] Invalidate cached leaf translations on root exchange.
- [x] Preserve the existing page allocator and independent slice-table ownership.

Verification:

- [x] Detach one-block and multi-page tables, reuse the live view, and inspect both mappings independently.
- [x] Assert that detachment neither visits block entries nor changes reference counts.
- [ ] Check empty roots, maximum configured tree depth and invalid detached contexts.

### Stage 2: Add retired-table storage and atomic command transfer

Depends on Stage 1.

- [x] Add `fpga/media_store/rtl/media_reclaimer.sv` with a configurable DDR record queue and one active context.
- [x] Add coherent queue-record access, bounded admission and explicit queue-full reporting.
- [x] Include the active collector record in queue capacity accounting.
- [x] Replace the table-walking CLEAR/REMOVE path in `fpga/media_store/rtl/media_store.sv` with the ownership-transfer transaction.
- [x] Route automatic BeginCapture through the same mechanism.
- [x] Ensure queue records become eligible for cleanup only after the live root/descriptor transfer commits.
- [x] Preserve pre-mutation state on capacity refusal; use existing sticky-fault handling for post-mutation memory faults.

Verification:

- [x] Exercise queue wraparound, queue exhaustion, empty clears and repeated clears of the same view.
- [x] Verify that queued roots survive live-view reuse without consuming extra live view IDs.
- [ ] Inject faults around transfer publication and verify that no old table becomes silently unowned.
- [x] Compare foreground operation counts across recording lengths.

### Stage 3: Implement resumable reference and metadata reclamation

Depends on Stages 1 and 2.

- [x] Consume detached tables from their tails.
- [x] Reuse the existing per-block ownership and reusable-ID stack mechanisms through a shared serialized arbiter.
- [x] Make count updates indivisible with respect to competing foreground ownership changes.
- [x] Track each entry through lookup, reference release, retirement, table trimming and checkpoint completion.
- [x] Make leaf/directory unlinking and free-page linking resumable.
- [x] Recycle queue records after all table metadata has been released.
- [ ] Ensure cleanup can progress without allocating new table pages, including when both sample and metadata pools are full.

Verification:

- [ ] Pause and resume at every meaningful collector state.
- [ ] Cover shared ranges, duplicate ownership entries, partial blocks, directory boundaries and concurrent allocation.
- [ ] Audit intermediate ownership, including the active transaction.
- [ ] Drain after deleting all owners and verify exact resource recovery.
- [ ] Exercise full metadata pools and prove that cleanup can release pages without additional allocation.

### Stage 4: Make last-reference retirement compatible with foreground audio

Depends on Stage 3.

- [x] Add a background retirement request/completion path in `fpga/chunk_memory/rtl/chunk_memory.sv`.
- [x] Keep a retiring block unavailable for allocation while matching dirty writes and accepted transfers finish.
- [x] Allow foreground commands for unrelated blocks to proceed while retirement waits.
- [x] Cancel queued hints/fills and prevent new stale work from repopulating a retiring block.
- [ ] Extend `fpga/media_store/rtl/media_prefetch_resolver.sv` and its bridge with cancellation and generation-tagged outstanding lookup responses.
- [x] Prevent responses from an old view mapping being accepted against its replacement table.
- [x] Preserve fault propagation and the coordinated reset epoch.

Verification:

- [x] Hold old writebacks and prefetch responses across several audio iterations while unrelated playback continues.
- [x] Verify cache invalidation, delayed response handling and reuse of the retired block.
- [x] Inject retirement/writeback errors and verify existing containment behavior.
- [ ] Test reset with active cleanup and old traffic.

### Stage 5: Add maintenance scheduling and separate completion signals

Depends on Stages 3 and 4.

- [x] Update `fpga/media_store/rtl/media_audio_pipeline.sv` and `fpga/loop_pipeline/rtl/loop_pipeline.sv` so foreground completion does not require an empty cleanup queue.
- [x] Expose separate foreground-busy, cleanup-pending, cleanup-active and memory-quiescent observations.
- [x] Integrate maintenance grants into continuous audio scheduling and the relevant audio/AXI wrappers.
- [x] Add configurable per-iteration block and cycle budgets. Use 20 block releases as an initial configurable ceiling, subject to timing measurements.
- [x] Charge all queue, metadata, retirement and page-reclamation work to the maintenance budget.
- [x] Give foreground requests priority at safe transaction boundaries while measuring whether sufficient cleanup progress remains available.
- [x] Reserve a guard interval for the declared worst-case completion time of an already accepted, non-preemptible memory transaction.
- [x] Preserve pending collector context when its grant expires.
- [x] Stop new maintenance work during quiescence and include outstanding maintenance transfers in memory-quiescent reporting.

Verification:

- [ ] Measure foreground latency with an empty and a large pending queue.
- [ ] Check budget accounting and yield latency under fixed, stalled and realistic memory service.
- [ ] Run sustained recording with simultaneous deletion and measure releases, genuinely freed blocks and net free-space change.
- [ ] Verify audio completion while cleanup remains pending.
- [ ] State the memory-latency assumptions required for any deadline claim.

### Stage 6: Integrate capacity, diagnostics, snapshots and configuration

Depends on Stages 2 through 5.

- [ ] Extend arena-size calculations, address bounds, generated wrappers, simulation backends and source manifests for the queue region and new RTL.
- [ ] Update the relevant AXI, direct-memory, audio-I/O, synthesis and board configurations.
- [ ] Account for live and retired metadata pages together; add documented free-space watermarks/admission checks for new recordings.
- [ ] Do not treat a just-detached recording's blocks as immediately available to its replacement.
- [ ] Expose pending-table count, pending-reference count, queue high-water mark, blocks freed, metadata pages freed, cleanup cycles and capacity failures.
- [x] Keep ordinary Fence as a memory-ordering operation.
- [x] Add an explicit offline cleanup-drain operation for physical snapshots and diagnostics.
- [x] Update ownership audits to count live views, unprocessed retired entries and active transaction state.
- [x] Preserve logical production comparisons separately from deferred physical allocator observations.
- [ ] Update `BLOCK_ALLOCATION.md`, `DDR_METADATA.md`, subsystem READMEs and the hardware verification contract.

Verification:

- [ ] Exhaust queue records, sample blocks and metadata pages independently.
- [ ] Verify normal snapshots do not secretly drain cleanup during measured runs.
- [ ] Verify explicit drain plus quiescence yields a consistent complete physical snapshot.
- [ ] Check minimum, odd and larger capacities, AXI32/64 and address bounds.

### Stage 7: End-to-end validation and review

Depends on all previous stages.

- [ ] Extend focused media, metadata, block-retirement and production-pipeline tests with deferred reclamation cases.
- [ ] Run exact production/RTL comparisons for retakes, retained exports and uninterrupted unrelated playback.
- [ ] Exercise repeated record/clear/remove cycles with bounded capacity and prove eventual recovery.
- [ ] Run long mono/stereo scenarios with many simultaneous retakes/deletions and report the exact take lengths, channels, allocation geometry and pending ownership counts.
- [ ] Test cases whose old tables exceed metadata-cache capacity and cross multiple directory levels.
- [ ] Record foreground command latency, whole-quantum latency, maintenance cycles, reclamation throughput, DDR traffic and resource high-water marks.
- [ ] Run fault/reset recovery with queued and active cleanup, including late responses.
- [ ] Use focused tests during iteration, then run the applicable broader native regression, formatting, warning-denying builds, RTL lint and synthesis gates before publication. Follow the current repository test instructions.
- [ ] When Rust tests change, run `python3 scripts/check_shoop_test_usage.py`.
- [ ] Report resource/timing changes and any unqualified hardware-memory assumptions.
- [ ] Push the feature branch and create a PR with implementation and validation evidence.
- [ ] Monitor CI and automated review; address failures and review feedback until resolved.
- [ ] Leave merging as a separate user decision.

## Execution contract

- Keep this plan current and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may change when new evidence warrants it.
- Design rules may change for a documented, well-supported reason.
- Goals and immutable acceptance criteria must not change without explicit user approval.

## Validation strategy

Use narrow, meaningful tests at each stage to verify its new ownership or scheduling boundary before integrating further. Reserve expensive long DRAMsim3 runs and broad regressions for the completed integrated implementation, or earlier when a failure requires them.

The final evidence must distinguish bounded foreground work, budgeted maintenance, eventual memory recovery and actual memory-service assumptions. Passing the old short retake fixture alone does not establish long-recording behavior. CI and automated-review follow-through are part of delivery; autonomous merging is not.

## Implementation evidence

- Base branch: `bc833fc3`; implementation branch: `proto/deferred-retake-clear`.
- Initial metadata milestone: coherent fixed-size DDR retired-root records, independent detached access, incremental tail trimming and queue-capacity refusal. No live media command uses deferred cleanup yet.
- `python fpga/media_store/simulate_metadata.py`: eight AXI32/64 cache/table configurations passed, including small and three-level metadata tables, queue wraparound/full refusal, view reuse, unchanged reference counts and complete metadata-page recovery. The additional no-table-walk assertion is checked in the focused rerun before committing.
- Warning-denying `cargo build --locked -p rt_backend_testbench` passed.

- Integrated milestone: deferred CLEAR/REMOVE/BeginCapture, resumable tail release, serialized ownership updates, asynchronous grouped sample retirement, maintenance grants, explicit offline drain and pending-owner audits.
- `python fpga/media_store/simulate.py --config small --block-chunks 32 --output work/reclaim-smoke`: existing block-ownership, snapshot restore, nested ranges and reuse passed.
- `python fpga/media_store/simulate.py --config wide --block-chunks 32 --retired-tables 2 --deferred --output work/deferred-tests`: three directed gates passed. One-block and 17-block clears both took 62 accepted-to-done cycles and performed zero block-table lookups; a three-block grant released exactly three entries.
- `python fpga/chunk_memory/simulate.py --config small --retire-chunks 32 --output work/background-retirement`: legacy and asynchronous retirement gates passed, including stalled dirty transfers, unrelated cached reads, safe reuse, writeback failure and reset recovery.
- `python fpga/verification/simulate.py --directed --random-cases 2 --block-chunks 32 --output work/reclaim-production`: 18 production/RTL scenarios passed exactly.
- Implementation detail: root/count checkpoints live in each 64-byte DDR queue record; active block/progress and bounded traversal state stay on chip across iteration boundaries. Extra record words remain reserved. A separate stored epoch is unnecessary for the existing destructive coordinated arena-reset contract; queue indices and collector state reset together with metadata and endpoints. This does not provide reset-time collector recovery.
