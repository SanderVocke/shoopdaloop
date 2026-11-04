# Shared audio history and copy-on-write takes

`resource::shared_audio::AudioStore` reserves prepared pages in a `ResourceStore`
and owns a persistent binary page index. A prepared store can also live inside
`ResourceStore`, behind ordinary resource IDs, snapshots, cursors and read indexes.

## Ownership and publication

`append` consumes interleaved samples through `RenderIo`. Channels belong to one
frame interval; input must start at the previous committed end. Partial pages,
unaligned windows and callbacks longer than the history horizon are supported.
The complete operation is preflighted before any new history root is published.
Zero input at the expected frame changes neither content nor time.

`grab(start..end)` returns an owned editable take within committed history coverage.
`fork(take)` acquires another editable owner. `export(take)` acquires an immutable
owner of that revision. These operations retain an index root without walking or
copying history pages. Tokens check store scope and lease generation. Copying a
token acquires nothing; release each independent owner exactly once.

A borrowed read index follows its take's current revision. Export first when a
reader needs a revision to survive later edits. Releasing an owner invalidates its
read indexes immediately, including while physical reclamation remains pending.

`replace` changes a fixed-length take's logical timeline, wrapping at its end.
It stages complete replacement pages and publishes one new root after every
channel and touched page is ready. Untouched index branches and pages stay shared.
Original takes and exports remain readable. Shape, identity, frame, revision and
capacity failures preserve published content and existing owners.

## Capacity and required work

Prepare channel count, history duration, owner slots and a physical-page budget.
The ring size is the next power of two above the history's covering page count
plus boundary slack. Each page contains complete interleaved frames; unused page
padding is excluded from reads. Transaction scratch covers the largest prepared
interval's distinct touched pages.

The store reserves its configured pages so unrelated uploads cannot consume them.
Owner admission conservatively charges a whole history root, including pages
outside a requested window. This guarantees room for live capture, retained owners
and an edit transaction. Owner admission can refuse before every metadata slot is
occupied. Imported takes charge the larger of a history root and their logical
page count. Whole-root charging and power-of-two padding are explicit capacity
tradeoffs; this is not a final application memory policy.

Capture and edits use render accounting rather than background transfer credits.
A write copies the touched pages and their index paths; it never clones the whole
history map. Reads position once per page span. Work scales with touched pages,
index depth, frame count and channels. Transaction-local garbage can only contain
changed paths, because unchanged branches remain owned by the new root. Its cleanup
is part of required write work and does not depend on background maintenance.

On the native sizing fixture, 30 seconds at 48 kHz stereo with 4096-byte pages,
32-frame operations and four owner slots uses a 4096-page ring. The configured
20483-page reservation is 83,898,368 payload bytes (about 80.01 MiB), with 4,898,856
additional bytes of index, lease and scratch arrays (about 4.67 MiB). Common arena
metadata and allocator overhead are additional. Small edits copy one data page
and a logarithmic index path. These measurements describe this conservative
configuration, not every host or target layout.

## Deferred release and teardown

Released owners retain their capacity charge until queued index/page reclamation
settles. `maintain` spends at most the remaining callback validation allowance.
Each index node or page release consumes one step. Shared children are unreferenced
once and remain alive for their other roots.

After all owners and releases settle, `begin_cleanup` stops new work. Maintenance
releases the history root and returns the entire prepared page reservation. Final
object destruction remains outside rendering. There is no implicit callback-time
drop or automatic budget reset.

## Ordinary resource and persistence integration

`ResourceStore::prepare_audio_store` prepares a lifetime-scoped `AudioStoreId`.
`RenderIo::append_audio_history` writes its history. `grab_audio` returns a new
ordinary `ResourceId`; `fork_audio` returns a new editable resource from an
existing immutable snapshot. Both have `ResourceCommand` equivalents. Prepared
audio-store slots remain allocated until host teardown, including after cleanup.

Loaded ordinary PCM is retained as an immutable base, without an initial sample
copy. Its loop length may exceed the history horizon. Edits overlay touched pages
and preserve untouched base bytes. Importing a running capture snapshot can copy
its private tail under the ordinary background allowance. A shared snapshot can
be forked within its originating prepared audio store; cross-store shared forks
refuse. Channel shape must match the prepared store.

`RenderIo::replace_audio` checks the ordinary resource revision before editing.
Every ordinary `snapshot` acquires an immutable audio export, never a borrowed
mutable take index. `retain_snapshot` keeps exactly the selected revision even
after removal of its resource. Generic byte reads support arbitrary byte offsets
and chunk sizes, exclude frame padding, and obey the existing background copy
allowance. Required playback reads remain independent of that allowance.

The existing media ledger, `NodePairSnapshot`, `Adopt`, `Select`, export cursors
and `ConfigurationReplace` consume these snapshots unchanged. New full-loop
uploads can supersede a shared resource while already retained exports stay
readable. Session archives and audio files contain logical samples and ordinary
metadata; page identities, sharing, edit roots and prepared-store IDs are never
serialized.

`ResourceStore::reclaim` services registered audio retirement and ordinary
retirement under the same callback validation allowance, giving both turns.
Remove resources and release snapshots, drain retirement, then call
`cleanup_audio_store` and drain again to return the history reservation. An
imported base retains its snapshot credit until budgeted maintenance releases it.
Page, node, owner and transaction capacities still apply to every edit; a large
import does not enlarge prepared transaction scratch or index-node capacity.

## Evidence scope

Resource tests guard all execution against heap operations and check partial stereo
capture, five-channel frame padding, repeated history wraps, cross-page/wrap edits,
independent exports, stale/foreign handles, failure before publication and complete
credit recovery. A capacity fixture checks small-write work at short and 30-second
horizons.

The scalar loop fixture is a fixed Q=1 reference with independent sample
expectations. The production engine fixture uses Q=1/16/32/64/128 and host
partitions 1/7/31/128, continuous mixed input history, retained overlapping grabs,
private edits and coherent tick selection. Loaded/grabbed media also follows
ordinary adoption, export, family and complete configuration replacement paths.
Codec fixtures cover session archives, exact audio/MIDI and WAV, indexed reload
and further edits. See the [fixed shared-history contract](../../../shoop_loops/docs/shared-history.md)
for source completion, scheduling, finite capacity, shutdown and CI measurements.
Application P04/P05 policy and physical-device deadline acceptance remain outside
this foundation.
