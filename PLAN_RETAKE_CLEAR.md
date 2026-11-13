# Deferred reclamation for retake, clear and remove

## Goal and scope

Make the foreground cost of clearing, removing or replacing a recording independent of its length. Transfer ownership of its existing DDR block-ID table to a bounded cleanup queue, then release its references and metadata incrementally.

Keep the current allocation blocks, per-block reference counts, independent slice tables, sample caches and DDR metadata-page allocator. Implement the change in the media store and automatic BeginCapture path. Future loop-level clear/delete controls can use the same primitive.

This plan was investigated against `proto/fpga-rust-reference` at `bc833fc310817648c9e9c1d4c7ae3d6d21ac24c6`. The relevant media RTL was unchanged from the saved block-allocation benchmark checkpoint. Verify the branch again before implementation.

## Execution status

Implementation and local FPGA validation are complete on `proto/deferred-retake-clear`.
The branch is pushed and [PR #982](https://github.com/SanderVocke/shoopdaloop/pull/982)
is open against `proto/fpga-rust-reference`. Remote CI and automated review
are in progress; GitHub access is restored. The full native suite
was run; its 18 host startup/recovery failures in unchanged host code remain
recorded as failures. No merge has been performed.

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

## Implemented data and command model

### Retired-table record

Reserve a configurable DDR queue region after the existing metadata-page arena. Start with one 64-byte metadata line per record. For example, 256 records occupy 16 KiB; the selected capacity must be justified by workload and exhaustion tests.

A record contains:

| Field | Purpose |
| --- | --- |
| `root_page` | Root of the existing DDR block-ID tree |
| `remaining_entries` | Length of its remaining dense prefix |
| Reserved bytes 8..63 | Alignment and future diagnostics; contents unspecified |

Keep queue indices and one active collector context, including active block and release/progress state, on chip. Any traversal stack is bounded by configured radix depth, rather than recording duration. Count the active record against queue capacity until its table is fully reclaimed.

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
5. Make the queued record eligible for cleanup after the serialized foreground commit, and acknowledge the command.

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
- [x] Check empty roots, maximum configured tree depth and invalid detached contexts.

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
- [x] Inject faults around transfer publication and verify that no old table becomes silently unowned.
- [x] Compare foreground operation counts across recording lengths.

### Stage 3: Implement resumable reference and metadata reclamation

Depends on Stages 1 and 2.

- [x] Consume detached tables from their tails.
- [x] Reuse the existing per-block ownership and reusable-ID stack mechanisms through a shared serialized arbiter.
- [x] Make count updates indivisible with respect to competing foreground ownership changes.
- [x] Track each entry through lookup, reference release, retirement, table trimming and checkpoint completion.
- [x] Make leaf/directory unlinking and free-page linking resumable.
- [x] Recycle queue records after all table metadata has been released.
- [x] Ensure cleanup can progress without allocating new table pages, including when both sample and metadata pools are full.

Verification:

- [x] Pause and resume at every meaningful collector state.
- [x] Cover shared ranges, duplicate ownership entries, partial blocks, directory boundaries and concurrent allocation.
- [x] Audit intermediate ownership, including the active transaction.
- [x] Drain after deleting all owners and verify exact resource recovery.
- [x] Exercise full metadata pools and prove that cleanup can release pages without additional allocation.

### Stage 4: Make last-reference retirement compatible with foreground audio

Depends on Stage 3.

- [x] Add a background retirement request/completion path in `fpga/chunk_memory/rtl/chunk_memory.sv`.
- [x] Keep a retiring block unavailable for allocation while matching dirty writes and accepted transfers finish.
- [x] Allow foreground commands for unrelated blocks to proceed while retirement waits.
- [x] Cancel queued hints/fills and prevent new stale work from repopulating a retiring block.
- [x] Reuse the resolver reset/cancellation path and add generation-tagged outstanding responses in the metadata bridge (see implementation refinement below).
- [x] Prevent responses from an old view mapping being accepted against its replacement table.
- [x] Preserve fault propagation and the coordinated reset epoch.

Verification:

- [x] Hold old writebacks and prefetch responses across several audio iterations while unrelated playback continues.
- [x] Verify cache invalidation, delayed response handling and reuse of the retired block.
- [x] Inject retirement/writeback errors and verify existing containment behavior.
- [x] Test reset with active cleanup and old traffic.

### Stage 5: Add maintenance scheduling and separate completion signals

Depends on Stages 3 and 4.

- [x] Update `fpga/media_store/rtl/media_audio_pipeline.sv` and `fpga/loop_pipeline/rtl/loop_pipeline.sv` so foreground completion does not require an empty cleanup queue.
- [x] Expose separate foreground-busy, cleanup-pending, cleanup-active and memory-quiescent observations.
- [x] Integrate maintenance grants into continuous audio scheduling and the relevant audio/AXI wrappers.
- [x] Add a configurable per-iteration cycle budget. Following the requested refinement, default to 50 microseconds (5,000 cycles at 100 MHz), with no block-count ceiling; retain an optional block ceiling for profiling.
- [x] Charge all queue, metadata, retirement and page-reclamation work to the maintenance budget.
- [x] Give foreground requests priority at safe transaction boundaries while measuring whether sufficient cleanup progress remains available.
- [x] Reserve a guard interval for the declared worst-case completion time of an already accepted, non-preemptible memory transaction.
- [x] Preserve pending collector context when its grant expires.
- [x] Stop new maintenance work during quiescence and include outstanding maintenance transfers in memory-quiescent reporting.

Verification:

- [x] Measure foreground latency with an empty and a large pending queue.
- [x] Check budget accounting and yield latency under fixed, stalled and realistic memory service.
- [x] Run sustained recording with simultaneous deletion and measure releases, genuinely freed blocks and net free-space change.
- [x] Verify audio completion while cleanup remains pending.
- [x] State the memory-latency assumptions required for any deadline claim.

### Stage 6: Integrate capacity, diagnostics, snapshots and configuration

Depends on Stages 2 through 5.

- [x] Extend arena-size calculations, address bounds, generated wrappers, simulation backends and source manifests for the queue region and new RTL.
- [x] Update the relevant AXI, direct-memory, audio-I/O, synthesis and board configurations.
- [x] Account for live and retired metadata pages together; add documented free-space watermarks/admission checks for new recordings.
- [x] Do not treat a just-detached recording's blocks as immediately available to its replacement.
- [x] Expose pending-table count, pending-reference count, queue high-water mark, blocks freed, metadata pages freed, cleanup cycles and capacity failures.
- [x] Keep ordinary Fence as a memory-ordering operation.
- [x] Add an explicit offline cleanup-drain operation for physical snapshots and diagnostics.
- [x] Update ownership audits to count live views, unprocessed retired entries and active transaction state.
- [x] Preserve logical production comparisons separately from deferred physical allocator observations.
- [x] Update `BLOCK_ALLOCATION.md`, `DDR_METADATA.md`, subsystem READMEs and the hardware verification contract.

Verification:

- [x] Exhaust queue records, sample blocks and metadata pages independently.
- [x] Verify normal snapshots do not secretly drain cleanup during measured runs.
- [x] Verify explicit drain plus quiescence yields a consistent complete physical snapshot.
- [x] Check minimum, odd and larger capacities, AXI32/64 and address bounds.

### Stage 7: End-to-end validation and review

Depends on all previous stages.

- [x] Extend focused media, metadata, block-retirement and production-pipeline tests with deferred reclamation cases.
- [x] Run exact production/RTL comparisons for retakes, retained exports and uninterrupted unrelated playback.
- [x] Exercise repeated record/clear/remove cycles with bounded capacity and prove eventual recovery.
- [x] Run long mono/stereo scenarios with many simultaneous retakes/deletions and report the exact take lengths, channels, allocation geometry and pending ownership counts.
- [x] Test cases whose old tables exceed metadata-cache capacity and cross multiple directory levels.
- [x] Record foreground command latency, whole-quantum latency, maintenance cycles, reclamation throughput, DDR traffic and resource high-water marks.
- [x] Run fault/reset recovery with queued and active cleanup, including late responses.
- [x] Use focused tests during iteration, then run the applicable broader native regression, formatting, warning-denying builds, RTL lint and synthesis gates before publication. Follow the current repository test instructions.
- [x] When Rust tests change, run `python3 scripts/check_shoop_test_usage.py` (passes after integrating the target branch's new Marlin tests).
- [x] Report resource/timing changes and any unqualified hardware-memory assumptions.
- [x] Push the feature branch and create a PR with implementation and validation evidence.
- [ ] Monitor CI and automated review; address failures and review feedback until resolved.
- [x] Leave merging as a separate user decision.

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

Detailed reports, exact workload geometry, source/model hashes and reproduction
commands are in [the deferred-reclamation evidence](fpga/media_store/measurements/deferred/README.md).

- Base: `bc833fc3`; feature branch: `proto/deferred-retake-clear`. Milestones commit the detached metadata interface, integrated collector, Cargo output-layout compatibility and capacity/recovery hardening separately.
- Final-controller gates: nine ownership/maintenance tests on native AXI32 and nine on DRAMsim3 AXI64 with single-line caches; three pool/publication-fault tests; two 30-second ownership campaigns; one integrated stereo retake/retained-export comparison. All 24 tests pass.
- Broader FPGA gates: 45 test cases pass, including AXI32/64 metadata caches and six-level tables, block retirement under native/DRAMsim3 memory, minimum/odd capacities, production sessions, continuous audio, demo recovery and board controls. The production run covers 21 sessions; four retained-export sessions also pass with intermediate offline drain disabled.
- One-block and 17-block clears take the same 62 native AXI32 cycles (57 in the selected DRAMsim3 AXI64 configuration), with zero old-table lookups. In the larger four-level geometry, all 32 clears take 95 cycles each, including the last with 31 tables pending.
- Long campaigns: 32 mono loops or 16 stereo loops; 30 seconds per channel at 48 kHz; 32 frames/chunk and 32 chunks/block; 1,407 blocks/take and 45,024 old references. They transfer existing ID tables totaling 180,096 bytes using 2,048 bytes of queue records, without copying those IDs. Forty recording iterations free 351 blocks while 1,280 replacement frames/channel remain exact. Final drain recovers all 45,052 issued block IDs exactly once and all metadata pages.
- The mixed 64-mono-loop production benchmark passes all 24 quanta. Its simultaneous retake replaces 384 frames/channel (8 ms) on 32 loops. Peak whole-quantum cost is 27,453 cycles (274.53 microseconds at assumed 100 MHz); audio completes with 21 cleanup records pending. It transfers 12,288 sample bytes and 4,160 metadata bytes in that quantum. The updated 96-block arena provides pending-ownership headroom; the former 72-block fixture reports MemoryFull. The saved 234.33-microsecond synchronous result used the smaller arena. This is not a short-take speedup claim or a physical DDR deadline qualification.
- Native fixed/stalled and DRAMsim3 gates cover dirty transfers, unrelated cached playback, stale hints, queue wrap/full refusal, duplicate entries, every ownership/checkpoint phase, full pools, post-publication errors and coordinated reset. Intermediate audits include the active released reference and verify unique reusable IDs.
- Review found and fixed a configuration/collector atomic-lock deadlock. The bridge completes a held atomic transaction while configuration waits for admission. A directed test protects it.
- Review also found that a finite maximum budget could strand an offline drain or leave automatic cleanup effectively unbounded afterward. Explicit drain now has a separate mode. Its expiration regression fails against the previous controller and passes on the final native/DRAMsim3 models; automatic grants retain their block/cycle ceilings afterward.
- Continuous 16-channel DRAMsim3 audio matches production media/state exactly, with no missed or discarded frames. Demo recovery passes input exhaustion, memory faults, accepted-response draining and four coordinated recoveries. Board control/watchdog/counter checks pass; board output correctness is not claimed by that measurement runner.
- Formatting, warning-denying workspace build, generated-wrapper checks, media RTL lint and generic/ECP5 synthesis pass. Default store/media-audio mappings add 1,270/2,396 LUT4 and 881 flip-flops each, with unchanged block RAM counts (15/18). No routed Zynq timing or board-fit result is claimed.
- The complete native suite reports 3,203 passes, 18 host startup/recovery failures and 11 skips. A focused host rerun reports seven passes, 18 failures and two skips. These failures remain unresolved; no base-branch native run establishes baseline status. No Rust tests changed.
- Dedicated CI gates are added for deferred ownership, retirement, exhausted pools and a short large-arena regression. Remote execution and automated review are in progress on PR #982; their checklist item remains open until checks and feedback are resolved.

Implementation refinements:

- DDR records checkpoint root/count; the active block, release state and bounded context stay on chip. A stored epoch is redundant under the existing destructive coordinated arena reset: roots, queue indices, collector and endpoints reset together. This does not add reset-time collector recovery.
- Root exchange reuses the resolver's existing reset/cancellation path. Metadata bridge generation tags reject late results; a second cancellation mechanism inside the resolver was unnecessary.
- Tail trim checkpoints after one entry, including bounded-depth ancestor unlink/free work. It never sweeps the rest of a recording in one admitted operation. The cycle budget charges active cleanup, including accepted memory-response waits, while preserving its allowance during foreground resource arbitration. Accepted indivisible tails finish; the guard still needs qualification against the actual memory service.
- The requested scheduling refinement makes time the primary limit: 5,000 cycles per iteration at 100 MHz, a 1,024-cycle guard, and no default block-count ceiling. The loop pipeline opens one allowance before control settlement, without refreshing it again at audio admission. The historical measurements above used the earlier 20-block/10,000-elapsed-cycle policy; current-policy evidence is recorded separately.
- A waiting processing iteration suppresses new collector transactions before its run handshake. Early job completion can therefore start the next ready iteration after any accepted transaction/atomic tail, without waiting for unused cleanup allowance. The standalone audio admission follows the same rule. Back-to-back runs replace the allowance; they do not accumulate it.
- Integration with the advanced target preserves opt-in reuse of invalid view IDs. Slice preflight checks the selected destination's metadata capacity before publishing references. The Marlin simulator waits for admission across temporary collector locks, includes the collector RTL and reserves its queue storage.
- Current-policy evidence in `measurements/deferred/cycle-budget/` records 32 focused FPGA test cases and 16 passing Marlin tests. Early empty runs complete in 33--74 cycles and the tested admissions take 1--52 cycles with cleanup allowance remaining. The repeated 30-second campaigns recover every issued ID/page and keep replacement captures exact; 355 blocks are freed during 40 recording grants. The new full native workspace run remains in progress; historical native failures above remain recorded.
- Remote FPGA/native-memory, DRAMsim3, reclamation and hosted-processor checks passed on `47d9f227`. The synthesis job exhausted its 25-minute job limit after the first generic audio variant took about 15 minutes; its limit is now 90 minutes to retain all three variants, mapping, demo and media checks. No synthesis assertion or test deadline was relaxed.
