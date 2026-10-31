> **Historical snapshot — not an active plan or current acceptance claim.**
> [History index](../../../README.md) · [Current plan](../../../../../PLAN.md) · [Original at `fffc1faa`](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/MIDI_INDEX_DESIGN.md)
> Original text follows; only relative link destinations are adapted for browsing.
> Exact original bytes remain in the snapshot bundle. Old tasks, pauses and status apply only to that snapshot.

# MIDI event/checkpoint integration design

Status: implemented single-resource indexing; historical Stage 4 verification is in evidence/midi-index-stage4.json, its provenance record and [VALIDATION.md](VALIDATION.md). Generic groups and the separate loop facade/restoration are also implemented, but have their own authority/evidence. Public usage is in the [MIDI access chapter](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/src/rust/rt_runtime/docs/part-3-work-with-stored-media/11-seeking-and-reading-midi-events.md) and [checkpoint chapter](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/src/rust/rt_runtime/docs/part-3-work-with-stored-media/12-midi-state-and-checkpointed-seeking.md). No new tests/platform campaign is implied here.

## Prepared storage

ResourceStore::with_midi_indexing supplies opt-in preparation with a fixed indexed-version count, an explicit checkpoint count reserved per version, and a positive declared event replay span. Legacy stores need not allocate these state-heavy pools. Check concrete storage arithmetic against the common native/Wasm ceiling and initialize/touch every slot before rendering.

An indexed resource version owns one mutable protocol state, an immutable initial-state checkpoint, and append-only checkpoints after each replay-span group of complete records. Each checkpoint retains the state *after* its last record, that record's media timestamp, and the next record's byte offset; its cumulative event ordinal follows from its row position and replay span. Checkpoints occupy a contiguous prepared row per indexed-version slot and support binary search. Reserving the complete row at version creation prevents other versions from consuming its promised checkpoint capacity. This explicitly declared version-count × checkpoints-per-version storage trades utilization for simpler reservation and constant-time metadata recycling; it is not an implicit copy of the whole media-page pool per version. Existing immutable snapshot leases retain the resource version and therefore its MIDI metadata. New index handles remain store-scoped; raw legacy IDs retain their same-store precondition.

The event span is an index-local replay bound, not a timing quantum, external-event credit, or musical-work cutoff. It trades storage for positioning work. Every packet counts toward the span, including opaque SysEx and realtime messages whose projected channel state does not change. With V indexed versions, C checkpoints/version (including the initial checkpoint), and span K, projection storage is V×(1+C) states and each version holds at most C×K−1 packets. Byte/page capacity may exhaust earlier; row exhaustion is explicit, not an unbounded-replay fallback.

## Construction and refusal

Uploads use transactional streaming parsing. Validate a candidate parser first, determine the number of checkpoint slots needed, and refuse insufficient preparation before changing the received prefix or checkpoint state. After validation/capacity checks, replay the accepted chunk through the parser's complete-record summaries to update state/checkpoints without a whole-packet SysEx rescan. Metadata updates are bounded overhead of admitted chunks; parser/copy allowances remain effective.

Indexed capture starts from an explicitly supplied initial protocol state. Preflight checkpoint needs along with append capacity before mutating data/state. Fold validated arena events in their existing order, including equal-time events. No checkpoint construction or history scan is deferred to stop/freeze. This single-version mechanism is not itself the coordinated group transaction: group reservation/commit must still prevent one channel from succeeding while another fails.

On refusal, preserve earlier complete data and metadata consistently. Upload cancellation and final version release return the plain-data MIDI row in constant metadata work, without clearing/scanning it or destroying owners. Its used length and source identity prevent reading old entries after reuse. Media pages keep their existing incremental maintenance reclamation. Old leases remain readable through replacement/removal until explicitly released.

## Positioning and packet access

A MIDI cursor/event handle is opaque and tied to an existing read lease. Copying it acquires nothing, and Drop releases no lease; retain/release the actual snapshot/export owner explicitly. Validate store scope, lease/version, format, offsets, packet bounds, and snapshot prefix. Metadata iteration preserves complete packet boundaries; packet copying uses caller-prepared output storage and the existing indexed pages. There is no second media store. A too-small output leaves bytes untouched; retry midi_read with the same event token, not midi_next on the already-advanced cursor.

For a target media frame, select the rightmost checkpoint whose last event timestamp is strictly less than the target, or the initial state. Bound selection by the immutable snapshot cut. Copy that checkpoint state and replay only the declared local event region. Stop before the first event at or after the target and return its cursor with the state before the target. Equal-frame packet groups must never be skipped by choosing a checkpoint that already includes target-frame events.

For C checkpoints, P media pages, and declared replay span K, seek uses O(log C + K log P) index work plus fixed-shape state operations. Page-index access retrieves record headers/short state-bearing data. Large opaque packets need not be rescanned to reconstruct the channel projection; playback still retrieves their complete original payload. A violated prepared replay invariant is a fault, not permission to silently omit events or delay music.

## Historical verification scope

- Independent full-history state/event oracle versus indexed seek at every boundary, dense equal-time groups, trailing silence, empty content, and long packets spanning pages.
- Initial states, reset markers, immutable prefix cuts, later capture appends/tree rotations, replacement/removal, stale/foreign handles, malformed chunks, exact checkpoint exhaustion, cancellation, and complete pool recovery.
- No whole-resource build at stop/freeze; measured local replay bounds; unchanged callback-wide background allowances and independent saturating diagnostics.
- Native debug/release, package Node/Chromium, heap guards, selected Miri, and existing real-host regressions before marking this index work verified. Group lifecycle/facade evidence is recorded separately; this index milestone does not prove restoration emission, arbitrary synth/parameter history or an independent loop AudioWorklet. That host is not supplied in this snapshot.
