# 12. MIDI state and checkpointed seeking

<!-- rust-source: ../../examples/midi_seeking.rs -->

[Contents](../README.md) · [Previous: Seeking and reading MIDI events](11-seeking-and-reading-midi-events.md) · [Next: Planning processing intervals](../part-4-coordinate-processors-and-changes/13-planning-processing-intervals.md)

Seeking MIDI needs both a packet position and the protocol state **strictly before** the requested frame. Packets at that frame remain original playback events. Applying them during lookup as well would duplicate their effects.

`MidiState` is a fixed MIDI 1.0 channel/key projection. The resource index stores initial state and checkpoints; neither lookup nor the value type emits restoration packets.

## Run the example

The checkpoint half of [MIDI seeking](../../examples/midi_seeking.rs) uses an initial held note, five original records, and independently expected key/pedal states:

```sh
cargo run -p rt_runtime --example midi_seeking
```

It compares each result with a linear pre-target reference plus explicit velocity/held/pedal expectations, verifies equal-time packet order, and checks malformed-chunk/capacity refusal and empty indexed media. State/event lookup is heap-guarded. The legacy event-access half is explained in [Seeking and reading MIDI events](../part-3-work-with-stored-media/11-seeking-and-reading-midi-events.md).

## 1. Reserve checkpoint rows before use

Construct the store with concrete MIDI indexing capacities:

```rust
    let mut store = ResourceStore::with_midi_indexing(
        config(128)?,
        MidiIndexConfig {
            versions: 4,
            checkpoints_per_version: 3,
            replay_events: 2,
        },
    )?;
```

`config` is an example helper. The checkpoint case uses 128-byte pages; the event-copy case uses 16-byte pages to expose crossings.

Each indexed version reserves one mutable state slot and a full checkpoint row. Checkpoint zero holds the initial state, then another checkpoint follows each complete pair of records. With `C` checkpoints and replay span `K`, the capacity is **`C × K − 1` packets**: five here, not six. Opaque SysEx and realtime packets count too.

Approximate state storage is `versions × (1 + C) × size_of::<MidiState>()`, plus fixed metadata/free stacks. These are independent prepared rows, not a copy of all media pages. Arithmetic/capacity checks do not certify available OS memory or callback deadlines.

## 2. Supply history before the first recorded packet

The example's initial state contains held key 62 at velocity 40:

```rust
    let mut initial = MidiState::default();
    initial.apply_packet(&[0x90, 62, 40], 3)?;
```

Pass that value to `begin_indexed_midi_upload`. It is source history, not a synthetic note inserted at frame zero. The normal contiguous Write → Finish → Commit protocol publishes these originals with duration six:

| Relative frame | Packet |
|---|---|
| 0 | Note-on 60, velocity 100 |
| 1 | Sustain CC64 = 127 |
| 1 | Note-off 60, release velocity 45 |
| 3 | Sustain CC64 = 0 |
| 4 | Note-on 61, velocity 80 |

The stream preserves both frame-one packets in that order. `begin_indexed_midi_capture` supplies the same initial-state concept for single-resource recording; coordinated groups belong in [Recording resources together](../part-4-coordinate-processors-and-changes/16-recording-resources-together.md).

## 3. Seek without consuming equal-time originals

Acquire a snapshot and `ReadIndex`, then seek through required access:

```rust
            let mut position = store.render_io().midi_seek(index, target)?;
            assert!(position.replayed_events < 2);
```

`position.state` is pre-target state; `position.cursor` retains the first original event at or after the target:

| Target | State and next event |
|---|---|
| 0 | Only initial key 62; next is note-on 60 at 0 |
| 1 | Keys 62 and 60 held; next is sustain-down, then note-off, both at 1 |
| 2 or 3 | Key 60 released but retained by sustain; next is pedal-up at 3 |
| 4 | Key 60 gone, key 62 held; next is note-on 61 at 4 |
| 6 | Keys 62 and 61 held; cursor at EOF |

Beyond duration, seek returns `InvalidFrame`. Empty indexed media can return its initial state and EOF at frame zero. Legacy unindexed MIDI instead refuses state seek with `InvalidState`; it does not silently scan the whole history.

The index selects the last checkpoint whose final packet is strictly before the target and within the pinned prefix. Checkpoint search is logarithmic, with additional page positioning and fewer than `K` replayed records. Reconstruction reads indexed headers/short state-bearing bytes, not entire SysEx bodies or the whole resource. A broken replay invariant is `InvalidState`, not permission to drop musical work.

Retained old leases keep their version and row alive after replacement/removal. No index is rebuilt at freeze. Once the last reference is released, the row's identity/used length can recycle without clearing every checkpoint state; media pages still reclaim incrementally. Required reconstruction and its saturating diagnostic counters are independent of optional transfer allowance.

## 4. Refuse a whole upload chunk

The example attempts a sixth record while byte space remains. It receives Capacity, the upload is still incomplete, and `cancel_upload` recovers the reservation. A checkpoint limit is not a byte-page limit or timing allowance.

A separate chunk contains a valid first record followed by malformed data:

```rust
    assert_eq!(
        store.write_upload(upload, 0, &invalid),
        Err(Error::InvalidEvent)
    );
    assert_eq!(store.work().validation_steps, invalid.len());
    assert_eq!(store.work().copied_bytes, 0);
    store.write_upload(upload, 0, &valid)?;
```

No earlier packet from that rejected chunk was published: retrying the complete valid chunk still starts at offset zero. Validation work remains spent. Cancel unfinished/refused uploads explicitly; the example also releases every snapshot and verifies complete page/version/lease/row recovery.

For capture, checkpoint exhaustion instead halts with an incomplete reason and preserves the preceding complete prefix. It does not store part of the refused append or resume automatically after a gap. [Recording and exporting](10-recording-and-exporting.md) explains that per-resource failure policy.

## What the state represents

The fixed projection has sixteen channels and 128 keys per channel:

- Keys retain velocity, held/sostenuto status and observed poly pressure. Velocity zero means no projected sounding note. Note-on zero is note-off; repeated note-ons update one key projection while remaining separate original packets.
- Sustain CC64, hold CC69 and sostenuto CC66 retain released keys according to their distinct pedal rules. This is not a synthesizer's multiple voices or envelope state.
- Program, loaded-program bank, pending bank selectors, pitch, channel/poly pressure and known controller values remain distinct. Unobserved values are unknown, not invented device defaults.
- CC120 clears notes; CC123/mode changes release held notes while respecting holds. Reset All Controllers establishes its documented baseline while retaining unrelated program/control information. Momentary reset/all-notes commands and CC96/97 are not cached controls to replay after restoring notes.
- The last RPN/NRPN selector family is retained, but parameter-value/history reconstruction is explicitly untracked. System Reset clears the projection and records a marker. Arbitrary SysEx/device state is not reconstructed.

`ChannelState::projected_notes()` visits non-default key projections in ascending
key order; it includes known zero poly pressure and pedal-latched keys, not just
currently held keys. `known_controllers()` visits every known control in ascending
number order, including known zero values. Both borrow fixed bit indexes maintained
with state updates; neither allocates or scans media history. The `midi_seeking`
example checks these iterators under the heap guard. Omitted note keys have the
default projection; omitted controls are unknown. Dense `note`/`controller` lookups
remain available. The fixed indexes contribute to `size_of::<MidiState>()` and
therefore to the prepared checkpoint storage arithmetic above.

`MidiState::apply_packet` validates atomically before updating. `from_channels` only reassembles copied channel states and the reset marker: obtain all of them from the same immutable cut. It cannot make independently sampled live fields coherent.

## State lookup is not restoration

A renderer still needs cleanup → pre-target restoration → original equal-time events, with bounded sink capacity and an explicit failure policy. The separate loop library implements [projection/restoration helpers](../../../shoop_loops/docs/midi-restore.md) and [indexed MIDI rendering](../../../shoop_loops/docs/midi.md), including unsupported parameter-history reporting and runtime recovery integration.

Do not treat the generic state/index API as that renderer, or protocol restoration as a snapshot of an external synth. Keep the export lease until all state/packet reads are complete and release it once; copying state does not acquire media ownership.

[Next: Planning processing intervals](../part-4-coordinate-processors-and-changes/13-planning-processing-intervals.md)
