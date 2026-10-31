# Chapter 9: upload a revision, pin it, and read without background starvation

The destination is currently empty. `../media-baseline.json` records that baseline,
reviewed archive/source inputs and fresh selected native checks. This is an actionable
chapter/example specification, not authored teaching prose or completed publication.
Use the approved chapter7 style: a small executable task, observable assertions,
then failures at their point of use. Do not paste the archived resource guide.

## Small public-API example to implement

1. Prepare a store with small pages and enough resource/version/snapshot capacity to
   retain old and new content together. Create an audio resource. Encode a known
   stereo pattern as **interleaved little-endian f32**; callback audio is planar.
   `Description` must have bytes=`frames × channels ×4`, with nonzero channels and
   checked arithmetic. There is no decoding, sample-rate conversion or playback DSP.
2. Use `begin_upload(resource, expected_revision, description)`, contiguous `write`
   chunks, `finish_upload`, then `commit`. Chunks must fit the remaining page and
   current copy allowance. Assert the exact accepted bytes and active revision.
   A new resource starts at revision0 with no content; Finish is not publication.
3. Acquire a snapshot, obtain `read_index(snapshot.id)` and a cursor, then read the
   original pattern into prepared output. Replace the active revision and verify
   the old snapshot still reads its original bytes and metadata. Inspect currentness
   without confusing Stale/ResourceRemoved with an invalid still-held lease.
4. Spend the remaining background copy allowance in a controlled setup. A legacy
   quota-based read should refuse, while `store.render_io().read` (or processor
   `block.resources.render()`) still reads the same prepared content under a heap
   guard. Assert that required access did not renew background credits. Metadata
   from `RenderIo::snapshot(index)` is the pinned prefix, not the newest resource.
5. Release each acquired snapshot exactly once, cancel any unfinished/refused
   upload, remove unneeded resources and reclaim within a finite prepared bound.
   Check full pool recovery before off-render destruction. Never put a retry-until-
   progress loop or owning result destruction inside a processor callback.

A direct-store setup is suitable for the first example. If using executor requests,
show `Command::resource(Request::...)`, command outcome **and** `ResourceCommand`
reply. `Read` returns the updated cursor and filled owning buffer in the returned
request (`request()`/`into_request()`), with written/eof in its reply. Queue admission
is not operation success, and Budget is a refused operation, not automatic retry.

## Contracts that must be explicit

- Snapshot/index/cursor values do not acquire extra leases when copied. Opaque
  `ReadIndex`/cursor/event handles validate store scope and lease lifetime. **Raw
  ResourceId/SnapshotId preparation still has a same-store caller precondition**;
  do not claim that coincident raw IDs from another store are universally detected.
  These values are not transferable capabilities between independent Wasm instances.
- `RenderIo` separates required work from optional transfer/validation allowances;
  it does not remove identity, shape, timestamp or physical-capacity checks and
  cannot create resources/uploads/leases, reset accounting or reclaim.
- A read copies at most one page remainder. A short read is not necessarily EOF;
  an empty output buffer can also return zero. Demonstrate finite cursor progress
  against the immutable byte length rather than treating any short copy as EOF.
- Reservations are inside total budgets. Optional snapshot-tail copies remain
  budgeted. Reborrowing or additional internal intervals do not reset the callback
  envelope; saturating diagnostics must never refuse required media work.
- Replacement requires simultaneous space for old/new/pinned/retiring content.
  Releasing a handle does not make every page immediately reusable. Store teardown
  belongs off-render, and explicit reclaim is needed if the store will be reused.

## Source/evidence and later links

Use `resource/{mod,request,render,index,format}.rs` and tests `resources`,
`resource_execution`, `resource_render`, `resource_index`. The review freshly ran
those existing tests in debug/release as part of63 cases/profile, not a new example.
Continue positional/MIDI access in chapter11, capture in10/16, sizing in17 and
extraction in19. The chapter's runnable example is still the documentation worker's
work; no new browser, whole-package or Miri pass is implied by this handoff.
