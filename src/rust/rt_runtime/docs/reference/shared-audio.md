# Shared audio history and copy-on-write takes

`resource::shared_audio::AudioStore` reserves prepared pages in a `ResourceStore`
and owns a persistent binary page index. It is an additional audio-only source;
ordinary uploads, captures and snapshots retain their existing interfaces.

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
occupied. Whole-root charging and power-of-two padding are explicit capacity
tradeoffs; this is not a final application memory policy.

Capture and edits use render accounting rather than background transfer credits.
A write copies the touched pages and their index paths; it never clones the whole
history map. Reads position once per page span. Work scales with touched pages,
index depth, frame count and channels. Transaction-local garbage can only contain
changed paths, because unchanged branches remain owned by the new root. Its cleanup
is part of required write work and does not depend on background maintenance.

On the native sizing fixture, 30 seconds at 48 kHz stereo with 4096-byte pages,
32-frame operations and four owner slots uses a 4096-page ring. The configured
20483-page reservation is 83,898,368 payload bytes (about 80.01 MiB), with 4,438,880
additional bytes of index, lease and scratch arrays (about 4.23 MiB). Common arena
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

## Evidence scope

Resource tests guard all execution against heap operations and check partial stereo
capture, five-channel frame padding, repeated history wraps, cross-page/wrap edits,
independent exports, stale/foreign handles, failure before publication and complete
credit recovery. A capacity fixture checks small-write work at short and 30-second
horizons.

The loop-library fixture adds real resolver decisions, capture and renderer DSP,
an independent sample oracle and callback-partition comparisons. It manually owns
the composition; this source is not yet integrated with `LoopPlanner` operations,
the media ledger or an application host. It implements neither MIDI history nor
application replacing/grab policy, and establishes no device deadline guarantee.
