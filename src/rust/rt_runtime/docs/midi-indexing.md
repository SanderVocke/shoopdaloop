# MIDI event and state indexing

[Resources](resources.md) · [Protocol-state policy](midi-state.md)

## Preparation and ownership

Construct `ResourceStore::with_midi_indexing(prepared_config, MidiIndexConfig { versions, checkpoints_per_version, replay_events })` before rendering. All three capacities must be positive and concrete storage arithmetic is checked against the common native/Wasm ceiling. Legacy `ResourceStore::new` allocates no MIDI checkpoint rows.

Each indexed version reserves one mutable protocol-state slot and a complete fixed row of checkpoints. The first checkpoint is the supplied initial state, before any recorded packet. Further checkpoints follow every `replay_events` complete packets. Storage is approximately `versions × (1 + checkpoints_per_version) × size_of::<MidiState>()`, plus fixed metadata/free stacks. The per-version reservation deliberately trades utilization for simple ownership: another version cannot consume a take's reserved checkpoint space. It is not a copy of the media bytes or an implicit version-by-media-page allocation.

Use `begin_indexed_midi_upload(resource, expected_revision, description, &initial_state)` for streamed content, or `begin_indexed_midi_capture(resource, expected_revision, absolute_frame, &initial_state)` for single-resource capture. These are structural store operations, not prepared render-side group activation tokens. Ordinary `ResourceIo` does not expose them. Initial state must describe the source before the first recorded packet; the index does not infer prior input history.

The normal finish/commit/cancel, capture append/stop, and snapshot APIs remain in use. Leases retain both the data version and its MIDI row. Replacement/removal does not invalidate old leased content. After the final reference is released, the plain-data row is recycled in constant metadata work without clearing or walking it; media pages continue through incremental reclamation. Row source identity and used length prevent stale contents from becoming visible on reuse.

## Incremental construction and capacity

Uploads validate a candidate parser and checkpoint capacity before publishing any received bytes or state. The accepted chunk is then interpreted for complete-record summaries. This second pass is bounded by the admitted chunk, not the whole resource or a whole spanning SysEx message. A malformed later packet cannot publish earlier packets from that rejected chunk. Validation work remains charged on refusal; the prefix and checkpoint state remain unchanged.

Capture preflights checkpoint needs before appending data. Every packet counts toward checkpoint spacing, including opaque SysEx and realtime messages. Capacity is a storage limit, not a subdivision or timing quota. With `C` checkpoints and span `K`, one version can retain at most `C × K − 1` packets; checkpoint zero is the initial state. A valid MIDI upload may exhaust this declared record capacity before its byte reservation fills. Refused uploads remain incomplete and cancellable. Capture exhaustion retains the previous complete prefix with an explicit incomplete reason.

This is per-resource atomicity only. Coordinated audio/MIDI preflight, reserved lifecycle/lease tokens, and group commit remain required before promising multichannel capture behavior.

## Lease-scoped access

Obtain a `ReadIndex` from a successful snapshot, then use `ResourceIo::render()` or `ResourceStore::render_io()`:

- `midi_cursor(index)` starts sequential event access, including for legacy validated MIDI without state checkpoints.
- `midi_next(&mut cursor)` returns an opaque `MidiEvent` with relative `frame()` and `byte_len()`, or EOF. It preserves equal-time order and advances only after successful metadata validation.
- `midi_read(event, &mut prepared_output)` copies the complete original packet and returns its byte length. Too-small output is refused before copying. Storage must cover the declared packet size.
- `midi_seek(index, relative_frame)` returns `MidiPosition { cursor, state, replayed_events }`. State is strictly **before** that frame; the cursor retains the first event at or after it. Seeking to duration yields the state through the captured prefix and EOF. Beyond duration is invalid.

Seek chooses the last checkpoint whose final packet is strictly before the target, constrained to the snapshot prefix. It replays fewer than `replay_events` packets and returns without scanning from the resource start. Search is logarithmic in checkpoint count; local reconstruction uses page-indexed headers and short state-bearing data, skipping large opaque payload bodies. A violated prepared replay invariant is a fault, not a musical-work cutoff. All original payload bytes remain available for playback.

The returned state follows the documented channel/key projection, not arbitrary external synthesizer/device state. Restoration **emission** is a separate renderer operation and is not implemented by seek. Legacy MIDI without checkpoints refuses `midi_seek` explicitly rather than falling back to a whole-history scan.

Cursor/event/index handles validate store scope, lease/version, format, packet/prefix bounds, and offsets. They borrow an existing lease rather than duplicating it. Release each acquired snapshot once; copied handles become invalid after release. Raw legacy IDs still require their same-store precondition and opaque handles are not portable between Wasm instances.

## Accounting

Required packet/seek reads do not consume background allowances. `MidiIndexWork` reports background/render indexed records, replayed records, and conceptual initial/checkpoint state copies, with saturating overflow. Data copies and page positioning retain their separate counters. Index work resets once per host callback; reborrowing or selecting an interval does not reset it. Diagnostics expose `midi_index_work`; store/runtime usage reports expose reserved/free MIDI version rows. These are work observations, not deadline promises.
