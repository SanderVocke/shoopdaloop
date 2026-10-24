# Resource implementation contract (stage 5)

Status: the standalone fixed-page upload, immutable-revision snapshot, cursor, and reclamation APIs are implemented and tested. Capture/prefix-tail copying, shared executor budgets, and transport integration are still pending. The sections below remain the full stage-5 contract, not a claim that the entire resource feature is complete. This specializes `DESIGN.md` without changing PLAN.md A1–A8.

## Ownership and fixed capacities

The resource store has one owner, normally the render runtime. Control never receives references into mutable render storage. Fetching uses bounded copies through admitted buffers; control may assemble immutable snapshots or flatten data outside rendering. This deliberately makes the initial native and non-shared-memory worklet semantics identical rather than requiring native publisher threads or shared `Arc` chunks.

Prepare all storage before rendering:

- `resource_pages * page_bytes` bytes of page storage, explicitly touched.
- Fixed page metadata (next-page index and used length) and a free-page stack.
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

Snapshots select a coherent prefix at a callback boundary:

- A lease stores resource identity, version identity, revision, byte/frame length, format, and capture cut frame.
- Complete pages are append-only. A snapshot retains the version descriptor, so replacement/removal cannot reclaim its page chain.
- If the prefix ends in a mutable partial tail, copy only that tail into one reserved snapshot page. The copy is bounded by `page_bytes` and charged to the callback copy budget. Refuse/defer before acquiring the lease if that work/storage cannot be reserved.
- Subsequent capture may append to the original tail; the snapshot reads its private tail copy. It never observes later bytes through a changed length or mixed revision.
- A version retained by a prefix lease may retain later appended pages as well. This intentionally conservative initial policy consumes bounded capacity rather than adding complex shared-page reclamation. Long-lived readers can exhaust capacity; that is visible and testable.

Currentness is checked against the resource's live identity/revision. A capture prefix can be current at its cut and stale after another append; it is never represented as the final content of an ongoing capture. Removal/replacement leaves its bytes readable but changes currentness. Capture-active and incomplete status are separate from immutable snapshot metadata.

## Reading and playback

Use generation-checked sequential cursors over leased versions. Each cursor tracks its current page and offset so reading the next chunk is bounded by chunk size/page crossings, not by repeatedly walking from the resource head. Export and playback use the same immutable content semantics. No callback API flattens a whole resource or performs an unbudgeted random seek.

A fetch is a lease plus bounded chunk requests into preallocated buffers. Validate requested lengths/offsets and return end-of-prefix explicitly. Cancellation/release invalidates the lease generation and reclaims its private tail page incrementally. A dropped client must be drained/cancelled during host teardown; do not automatically reuse a live handle based on elapsed time.

A resource-backed playback fixture must continue using its previous valid cursor if a new revision cannot yet be leased. Successful revision switching happens at a block boundary and never exposes partial bytes or a runtime-generated silent block. Sample-value discontinuities between valid versions are not implicitly crossfaded.

## Reclamation and work accounting

Version references count active ownership and snapshot leases. When the last reference is released, mark the descriptor retired in constant work. Reclaim its chain one bounded page step at a time, recycling pages and finally the descriptor generation. Do not traverse/free the entire chain in a release, replacement, or cancellation operation.

The resource context shares callback transfer/maintenance budgets across commands and processors; constructing a new context must not replenish them. Page allocation, chunk copying, parser steps, partial-tail copying, cursor reading, and retirement all have explicit charges. Resource lengths at least 16 times the callback transfer limit must pass the same no-heap render guard as small resources.

## Required evidence before completing stage 5

- Audio/MIDI byte/sample/event round trips, including duration with no final MIDI event.
- Captured-prefix immutability across further appends, partial-tail mutation, replacement, removal, and resource-ID reuse.
- Current/stale/pending/incomplete status and exact capture-cut metadata.
- Missing/reordered/duplicate/malformed chunks, parser boundaries across pages, oversized metadata, stale commits, cancelled uploads/exports, and generation exhaustion.
- Many pinned revisions, slow readers, reserved-page competition, pool exhaustion, and stopped capture without corrupting existing playback.
- Work counters proving bounded copying/validation/reclamation independent of total length, and no owning destruction in successful or rejected callbacks.
- A public-API playback/capture fixture exercising graph rendering concurrently with upload/commit/fetch, plus deterministic reclamation of every credit/page after leases and endpoints are released.
