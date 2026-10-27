# Media and MIDI policy

[Book](../README.md) · [Reference](README.md)

Generic resources provide immutable/prefix storage and checked access. They do not choose loop roles, musical take identity, playback restoration or device state.

## Formats and publication

Audio is interleaved little-endian `f32`: `bytes = frames × channels × 4`, with positive channels and checked arithmetic. Byte access is not a resampler or channel converter.

Stored MIDI consists of consecutive records: little-endian `u64 frame`, little-endian `u32 payload length`, then one complete MIDI 1 packet. Frames are nondecreasing and strictly below the declared duration; equal-frame records retain input order. A packet may cross page/chunk boundaries. Length is positive and bounded; channel/system message lengths and data bytes are validated. SysEx is a complete F0…F7 packet, not a stream fragment; running-status fragments are not records.

Declared duration is independent of last event time. Positive empty MIDI extends silence; zero-frame media cannot contain records. Opaque/realtime records still occupy packet/index capacity even when they do not alter projected state.

Upload chunks are contiguous and at most one page; each Write is owning and returns its buffer. Finish validates completeness; Commit separately publishes the new revision. Old pinned versions remain old. A rejected malformed indexed chunk publishes no partial parser/state/index update. Physical data revisions are not musical take revisions or graph epochs.

## Lease and identity discipline

| Value | Ownership/lifetime |
|---|---|
| Resource/snapshot/transfer IDs | Raw local handles; same-store discipline remains a caller precondition. They are not authenticated/global wire identities. |
| Acquired Snapshot | One explicit lease until release; copying its fields acquires nothing and Drop releases nothing. |
| ReadIndex, byte/MIDI cursor, MidiEvent | Scoped views/tokens over an existing snapshot lease, not additional leases. Foreign store or released lease generations are refused. |
| Resource revision | Data publication identity; not proof of a settled application/musical cut. |

Currentness is observational. A Stale or ResourceRemoved snapshot can retain readable old bytes while its lease is valid. Required reads use the pinned snapshot's metadata, not newer active-resource length. Snapshot cut/duration must be interpreted with its own capture metadata; a host-supplied cut on noncapture data does not certify musical coherence.

Acquiring a capturing prefix can copy its partial mutable tail into a private page. It consumes optional byte/page/lease capacity; later append/status changes can make the old lease Stale without changing its bytes. Capture Stop does not increment the data revision. Release every actually acquired lease once and allow bounded retirement before assuming pages/versions are free.

## Byte and event access

Byte positions include EOF. A read copies at most the current page remainder, so a short positive read is not necessarily EOF. Required `RenderIo` access bypasses optional copy/step quotas, not index/lease/shape/offset checks.

`midi_next` advances its cursor and returns an event token. `midi_read` copies that complete packet; a too-small destination returns Capacity without modifying output. Retry **the same event token**, not another next call. Event metadata/body can cross pages, including long SysEx. Legacy media supports event enumeration; it does not silently acquire checkpointed whole-history state seeking.

## Strict-pre-target state

Indexed MIDI stores a declared initial state and checkpoint rows. With `C` checkpoints/span `K`, one version supports `C×K−1` records and lookup replays fewer than `K`. Rows belong to that version; old/new leases never retarget each other.

A seek to target F returns state after records strictly before F and a cursor preserving originals at F. Equal-time originals remain in order for playback; EOF state includes all records. Initial state and reset/channel markers must come from the same immutable cut.

`MidiState` is a bounded protocol projection, not synthesizer voices, envelopes, effects, arbitrary SysEx/device history or a restoration emitter. State lookup emits nothing. Cleanup → restoration → original-event ordering, channel-budgeted restoration and reconciliation after sink failure belong to loop/application policy. Unsupported histories must remain explicit rather than receiving invented state.

## Single-resource capture versus groups

Single-resource capture publishes complete append prefixes or refuses an append whole. Exhaustion preserves a truthful incomplete prefix and does not silently resume across a later gap. Independent audio can continue. A successful optional snapshot is not a mixed-media transaction.

Groups claim fixed unique members. Staging bounds are private per member; MIDI promises cover complete encoded packets and any recovery traffic that producer can emit. Preparation reserves versions, initial MIDI rows and final snapshot slots, not unlimited future data pages.

The lifecycle is Idle → Prepared → PreparedReserved → Reserved → Recording, with later reserve/commit cycling Recording ↔ Reserved. First reservation can precede activation while old media stays active. Activate at the exact reserved frame and carry the same token. Stage one complete contribution/member, including empty positive MIDI; commit at the reserved end publishes one common prefix and returns unused conservative page promises.

Cancel a PreparedReserved interval before its take; cancellation returns to Prepared without discarding final-lease promises. Freeze a Recording take at its actual common next frame into empty output slots, publishing promised final leases and returning Idle. Claims persist until explicit Idle group release; that release preserves active frozen data and outside leases. Legacy mutation cannot bypass claims.

`PlannerResourceIo::capture()` is frame/ceiling-stamped lifecycle authority. Ordinary consumer ResourceIo/RenderIo can stage required data but cannot reserve/activate/commit/freeze the shared store. Direct standalone `planner_io(frame, ceiling)` callers own stamp correctness. Missing/duplicate/malformed/over-bound contributions are interface faults, not permission to publish partial members or roll back emitted audio.

## Work and stopped ownership

Required access/staging/commit has separate saturating work/index/state-copy counters. Background quotas are not replenished by another node or interval. Current group media uses three counted copies, not zero-copy. Identity/frame/revision/storage exhaustion remains semantic even when diagnostic counters saturate.

After callbacks stop, retain the cleanup ledger alongside the store. Plain handles hidden in a dropped planner/processor are not automatically released by extraction. See [stored media chapters](../README.md#part-iii--work-with-stored-media), [coordinated capture](../part-4-coordinate-processors-and-changes/16-recording-resources-together.md) and [shutdown](../part-5-configure-and-operate-the-runtime/19-stopping-disconnecting-and-reclaiming-ownership.md).
