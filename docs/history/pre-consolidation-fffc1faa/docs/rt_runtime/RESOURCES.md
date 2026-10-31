> **Historical snapshot — not an active plan or current acceptance claim.**
> [History index](../../../README.md) · [Current plan](../../../../../PLAN.md) · [Original at `fffc1faa`](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/rt_runtime/RESOURCES.md)
> Original text follows; only relative link destinations are adapted for browsing.
> Exact original bytes remain in the snapshot bundle. Old tasks, pauses and status apply only to that snapshot.

# Resource implementation contract

Fixed-page uploads, capture, immutable prefix/revision snapshots, cursors, reclamation, executor requests, and legacy shared-budget access were delivered by initial runtime Stage 5; its historical evidence remains in `VALIDATION.md` and `DONE_INITIAL_CRATE.md`. Host-fixture evidence is recorded separately. This specializes `DESIGN_RT_ENGINE.md` without changing the historical A1–A8 contract.

The first loop-layer Stage 4 milestone adds restricted required data access through `ResourceIo::render()` / `ResourceStore::render_io()`. Reads and active single-resource audio/MIDI appends use the existing checked storage paths without charging background copy/validation credits. They still reject invalid identities/shapes/frames and physical exhaustion. An append-maintained balanced page index now provides store-scoped logarithmic byte positioning. Opt-in MIDI checkpoint rows now provide bounded event/state positioning and incremental initial-state/checkpoint retention. Opt-in capture groups now reserve version/index/final-lease credits, enforce exclusive writer claims, and stage coordinated interval commits before publishing a common prefix. Planner-only boundary authority checks frame/ceiling; consumer views provide staged writes. Existing ownership/content models and the seventeen declared lifecycle-cut disconnect cases cover the generic mechanisms, including first-interval preparation. The separate [loop facade](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/src/rust/shoop_loop_engine/docs/runtime.md) supplies musical adaptation. See [coordinated capture](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/src/rust/rt_runtime/docs/part-4-coordinate-processors-and-changes/16-recording-resources-together.md) for the public lifecycle.

Choose stopped extraction before destroying a domain ledger: `into_resources()` drops the planner/other owners and cannot release plain handles hidden there; `into_planner_and_resources()` preserves that ledger with its store for explicit cleanup. Neither settles future work, invents outcomes/finish nor sends cleanup to a stopped device. Stop/join callbacks first.

## Ownership and fixed capacities

Raw ResourceId/SnapshotId values require same-store caller discipline. Scoped ReadIndex/cursors/MIDI event handles additionally validate store and lease lifetime; copying any handle acquires nothing, and Drop releases no lease.

The resource store has one owner, normally the render runtime. Control never receives references into mutable render storage. Fetching uses bounded copies through admitted buffers; control may assemble immutable snapshots or flatten data outside rendering. This deliberately makes the initial native and non-shared-memory worklet semantics identical rather than requiring native publisher threads or shared `Arc` chunks.

Prepare all storage before rendering:

- `resource_pages * page_bytes` bytes of page storage, explicitly touched.
- Fixed page metadata (sequential link, used length, and balanced index links/ordinal/height) and a free-page stack. Version index roots/counts are fixed metadata; no per-version dense page array is allocated.
- `resource_slots` resource identities, `resource_slots` version/transfer descriptors, and `resource_slots` snapshot leases, each with checked generational reuse. These are separate fixed arrays using the same initial limit, not dynamically growing collections.
- Fixed retirement bookkeeping and any incremental parser state.

An active revision, an upload, or a capture occupies a version descriptor. Snapshots reference an existing version descriptor, not a deep-copied page list. Pinned old revisions and concurrent transfers therefore compete visibly for descriptor/page capacity. All limits and constant recovery overhead must be documented in capacity sizing. Exhaustion is an explicit result; never grow, spin, leak, or invalidate a reader.

## Formats

- Audio: interleaved little-endian `f32` data, explicit positive channel count and frame length. Checked `frames * channels * 4` must equal the declared byte length. This storage encoding is independent of planar graph ports. No implicit clipping/resampling.
- MIDI: a sequence of complete records: little-endian `u64` relative frame, little-endian `u32` payload length, then one validated MIDI 1.0 packet. Frames are nondecreasing and strictly less than the resource's duration; equal-frame record order is preserved. Duration is independent of the final event. SysEx has an explicit configured byte limit.

Use a streaming parser so validation work scales with each admitted chunk, not with the full resource. Header and message state are fixed-size. Check the declared packet length before consuming its body; validate each payload byte incrementally, including SysEx framing. Chunk validation is transactional: scan using a copy of parser state, and only then update pages/parser metadata. Malformed data must not leave a supposedly complete version.

## Upload state machine

`free -> reserved upload -> receiving -> complete -> active -> retired -> free`

1. Begin with a resource ID, expected revision, format, duration, and exact byte length. Reserve the version descriptor and logical page credits before accepting.
2. Materialize a page only as its first chunk arrives; do not initialize or walk every page in the begin operation. Capture cannot consume pages already reserved for uploads.
3. Chunks carry the transfer identity and exact next byte offset. Bound each chunk by both remaining page space and the current copy budget. Reject duplicate/reordered/out-of-range chunks without advancing state.
4. On receiving the declared length, check parser completeness and format invariants. A partial final MIDI header/body is not a complete upload.
5. Commit only a complete version with a still-matching expected revision. At a block boundary, swap the active descriptor handle and advance the revision with checked arithmetic.
6. Retire the displaced version. Cancellation releases unused logical reservations immediately and sends allocated pages to incremental retirement. A stale commit leaves the prior active content untouched and retains the rejected upload for explicit cancellation/retry.

Zero-length resources consume a descriptor but no pages; they still obey generation and revision rules. No data path may use zero-length versions to bypass descriptor limits.

## Capture and immutable prefix snapshots

Capture appends only complete audio frames or MIDI blocks. It records its starting engine frame, expected next engine frame, resource duration, byte count, and revision. Reject discontinuous frame positions and arithmetic overflow. Empty MIDI blocks still advance duration. Reserve required page capacity before mutating a block; if capacity is exhausted, stop capture with an explicit incomplete status rather than overwriting previously captured content.

Snapshots select a coherent prefix at an explicit boundary. Ordinary executor commands run at callback ingress; planner lifecycle/coherent domain cuts may occur at an exact internal boundary:

- A lease stores resource identity, version identity, revision, byte/frame length, format, and capture cut frame.
- Complete pages are append-only. A snapshot retains the version descriptor, so replacement/removal cannot reclaim its page chain.
- If the prefix ends in a mutable partial tail, copy only that tail into one reserved snapshot page. The copy is bounded by `page_bytes` and charged to the callback copy budget. Refuse/defer before acquiring the lease if that work/storage cannot be reserved.
- Subsequent capture may append to the original tail; the snapshot reads its private tail copy. It never observes later bytes through a changed length or mixed revision.
- A version retained by a prefix lease may retain later appended pages as well. This intentionally conservative initial policy consumes bounded capacity rather than adding complex shared-page reclamation. Long-lived readers can exhaust capacity; that is visible and testable.

Currentness is checked against the resource's live identity/revision and capture status. Accepted appends advance the content revision. Stopping capture or marking an incomplete prefix changes capture status without changing the bytes/revision, and still makes earlier capture-status snapshots stale. A capture prefix can be current at its cut and stale after another append; it is never represented as the final content of an ongoing capture. Removal/replacement leaves its bytes readable but changes currentness. Capture-active and incomplete status are separate from immutable snapshot metadata.

## Reading and playback

Use generation-checked sequential cursors over leased versions. Each cursor tracks its current page and offset so reading the next chunk is bounded by chunk size/page crossings, not by repeatedly walking from the resource head. Export and playback use the same immutable content semantics. No callback API flattens or scans a whole resource to seek. `ReadIndex` pins no extra owner: it refers to an existing snapshot lease, checks instance scope/generation, and positions by byte offset in logarithmic tree steps. Sequential cursors also check store scope. Index nodes are maintained as pages arrive and reclaimed with their pages; freezing does not rebuild them. `IndexWork` separately counts background/render lookup and maintenance steps with saturating overflow. This bounded metadata work does not replenish background credits or introduce an internal work quota. Opt-in `MidiIndexConfig` reserves a fixed checkpoint row per indexed version. Uploads validate parser/checkpoint capacity before publication; capture preflights metadata capacity before appending. `RenderIo::midi_seek` selects state strictly before the target and replays fewer than the declared event span, preserving equal-time packets. Scoped MIDI cursors/events access complete original packets without whole-history scans. Rows remain pinned by version leases and recycle in constant metadata work; data pages remain incrementally reclaimed. With V indexed versions, C checkpoints per version and replay span K, prepared projection storage has `V × (1+C)` states and each version can index at most `C×K−1` packets, including opaque/realtime records. This is a storage bound, not a truncation rule. See [event positioning](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/src/rust/rt_runtime/docs/part-3-work-with-stored-media/11-seeking-and-reading-midi-events.md) and [checkpointed state](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/src/rust/rt_runtime/docs/part-3-work-with-stored-media/12-midi-state-and-checkpointed-seeking.md).

A fetch is a lease plus bounded chunk requests into preallocated buffers. Validate requested lengths/offsets and return end-of-prefix explicitly. Cancellation/release invalidates the lease generation and reclaims its private tail page incrementally. A dropped client must be drained/cancelled during host teardown; do not automatically reuse a live handle based on elapsed time.

A resource-backed playback fixture must continue using its previous valid cursor if a new revision cannot yet be leased. Successful revision switching happens at the caller's validated boundary (ingress for ordinary commits, an internal cut for planner publication) and never exposes partial bytes or a runtime-generated silent block. Sample-value discontinuities between valid versions are not implicitly crossfaded.

## Prepared all-member capture

Claim every fixed member or none; an Idle group remains claimed until explicit release. `prepare_capture_take` reserves versions, MIDI rows and final lease slots without replacing active media. Reserve its first interval with `reserve_prepared_capture_interval` before activation: PreparedReserved preserves old active revisions, then the same token/credits activate at exactly the stamped frame. Cancel interval before take if abandoning that promise.

Planner-lent authority checks current frame/ceiling and cannot be constructed by an ordinary consumer. Each member contributes once within its own prepared shape/staging bounds, including the full conservative MIDI promise for short/silent intervals; unused promises return on commit/cancel. Commit publishes only the common completed prefix after all contributions. Interface/producer/stale-token faults are not ordinary late storage exhaustion or rollback.

Freeze uses promised final leases into empty output slots, returning Idle without depending on optional export credits/background copy headroom. Optional active-prefix snapshots can still refuse Budget/Capacity without interrupting required capture. Required work is not zero-copy: the safe staging path copies input → prepared staging → 64-byte stack scratch → pages, with independent projection/state-copy work. Physical pages, reservations, metadata and leases are separate; immediately available pages are `free_pages − reserved_pages` because free_pages includes promised free pages.

## Reclamation and work accounting

Version references count active ownership and snapshot leases. When the last reference is released, mark the descriptor retired in constant work. Reclaim its chain one bounded page step at a time, recycling pages and finally the descriptor generation. Do not traverse/free the entire chain in a release, replacement, or cancellation operation.

Legacy resource-context methods share callback transfer/maintenance budgets across commands and processors; constructing a new context does not replenish them. `resource_io_bytes` and `resource_io_steps` reserve processor headroom within the total budgets. Control leaves these credits untouched; at least one page/validation step remains available for control progress. Rendering can use reserved plus unused control capacity; reclamation runs afterward within the remaining maintenance steps. Capacity planning must account for reclamation delays when work budgets remain fully consumed. Page allocation, chunk copying, parser steps, partial-tail copying, cursor reading, and retirement all have explicit charges. Resource lengths at least 16 times the callback transfer limit must pass the same no-heap render guard as small resources.

Required `RenderIo` data operations instead use independent `RenderWork` counters: attempted operations, copied bytes, and charged MIDI event-validation units. These saturate with an overflow flag rather than failing the render. The executor reports them separately and resets them only once per host callback. The capability has no budget reset, upload, lease-allocation, structural lifecycle, or reclamation methods. Optional snapshot-tail copies, transfers, and maintenance retain their quotas; required data access cannot replenish them. Per-operation bounds and fixed pool capacity remain, but there is no arbitrary aggregate render-work quota or deadline guarantee.

## Initial Stage-5 evidence requirements (verified in VALIDATION.md)

- Audio/MIDI byte/sample/event round trips, including duration with no final MIDI event.
- Captured-prefix immutability across further appends, partial-tail mutation, replacement, removal, and resource-ID reuse.
- Current/stale/pending/incomplete status and exact capture-cut metadata.
- Missing/reordered/duplicate/malformed chunks, parser boundaries across pages, oversized metadata, stale commits, cancelled uploads/exports, and generation exhaustion.
- Many pinned revisions, slow readers, reserved-page competition, pool exhaustion, and stopped capture without corrupting existing playback.
- Work counters proving bounded copying/validation/reclamation independent of total length, and no owning destruction in successful or rejected callbacks.
- A public-API playback/capture fixture exercising graph rendering concurrently with upload/commit/fetch, plus deterministic reclamation of every credit/page after leases and endpoints are released.
