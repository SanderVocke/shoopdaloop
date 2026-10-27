# Chapter 11: byte positioning is not MIDI event/state positioning

Destination currently empty. See `../media-baseline.json` for reviewed source/archive
hashes and selected checks. Supply a small executable byte/event-access task, not
an indexed playback engine. Reserve protocol-state reconstruction for chapter12.

## Public-API walkthrough to implement

1. Pin a known audio revision and obtain `ReadIndex` from its snapshot. Use
   `RenderIo::cursor_at(index, byte_offset)` for offsets inside/between pages and
   exactly at byte length. Check returned bytes against a flat reference and
   reject offsets beyond EOF. A sample frame's byte offset is checked
   `frame × channels ×4`; the index itself only understands bytes.
2. Encode a short MIDI stream with a header and payload crossing small page
   boundaries, equal-time packets and a complete SysEx larger than one page.
   For example, page16 with records note-on@0, a37-byte SysEx@2, note-off@2 and
   duration5 forces both kinds of crossing. Set the packet arena/output bounds
   explicitly; prepare the output scratch before guarded processing.
3. Obtain `RenderIo::midi_cursor(index)` and enumerate with `midi_next`. For each
   event, assert `frame()`/`byte_len()` and copy the complete original packet with
   `midi_read`. Assert original tie order and EOF after the final packet, while
   duration still includes silence through5. Never treat arbitrary `cursor_at`
   byte offsets as validated MIDI event boundaries.
4. Deliberately pass a too-small packet output buffer; assert Capacity and unchanged
   sentinel bytes, retain the event token and retry with prepared sufficient space.
   `midi_next` already advanced its cursor when it returned that token; failed packet
   copying is not permission to call `midi_next` again and lose the original event.
5. Keep a prefix snapshot across append/index rotations and replacement; check old
   indexed bytes/events remain old. Release its lease, then verify the old opaque
   cursor/event/index refuses use. A foreign store must reject the scoped handles;
   raw snapshot-ID preparation still requires the same-store precondition.

Sequential event access works with legacy validated MIDI even without checkpoint
rows. `midi_seek` does **not** silently scan that entire stream to synthesize state;
it refuses unsupported unindexed state positioning. Introduce that distinction and
link chapter12 instead of bolting a linear-history fallback onto this example.

## Storage/work contracts

`read_index` is constant metadata work and does not acquire another lease. The page
index is maintained incrementally on append/upload in O(total pages) metadata, not
one page table per version. Positioning/insertion are logarithmic in version pages;
sequential reads keep their next-page link. Freeze/seek/wrap never rebuild the index
by scanning all content. Reclamation still handles bounded page work.

Required packet copies may span multiple pages, unlike one page-bounded raw read.
They copy complete declared packets into caller storage; the fixed packet bound is
not an arbitrary musical subdivision cap. Data/index counters are separate from
optional background allowances and reset once per host callback. Saturation is
observational, not a refusal reason or deadline guarantee.

The resource layout is explicit little-endian u64 frame/u32 length plus complete
packet bytes, not Rust struct layout or borrowed pointers. These local scope/lease
handles must not be passed between independent Wasm instances as wire capabilities.

## Existing checks to preserve/cite

`resource_index` checks all4097 offsets plus4096 seeded positions, immutable tails,
foreign/released views, empty/EOF and full reclaim. `midi_index` checks complete packet
copies, crossing SysEx, ties, prefix bounds and unsupported legacy seek. Their full
native workloads ran in both profiles in this review; the forthcoming teaching
example must have its own executable assertions/source markers. Miri and the actual
loop host remain separately tracked verification, not implied by these checks.
