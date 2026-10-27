# 11. Seeking and reading MIDI events

<!-- rust-source: ../../examples/midi_seeking.rs -->

[Contents](../README.md) · [Previous: Recording and exporting](10-recording-and-exporting.md) · [Next: MIDI state and checkpointed seeking](12-midi-state-and-checkpointed-seeking.md)

A byte position, a MIDI event boundary and the protocol state before a musical frame are different things. Use the page index for bytes, the MIDI event cursor for complete packets, and checkpointed seeking for state.

## Run the examples

The [stored audio example](../../examples/stored_audio.rs) from chapter 9 checks byte positions inside a page and at EOF. The first half of [MIDI seeking](../../examples/midi_seeking.rs) reads packets across small pages:

```sh
cargo run -p rt_runtime --example stored_audio
cargo run -p rt_runtime --example midi_seeking
```

Its event iteration and packet copies are heap-guarded. Packet encoding, scratch allocation and final destruction are off-render. The checkpoint half continues in chapter 12.

## 1. Position bytes through a pinned index

`store.read_index(snapshot.id)` borrows an existing snapshot lease and prepares a `ReadIndex` with constant metadata work. `store.render_io().cursor_at(index, offset)` positions a byte cursor without scanning content.

The audio example starts at byte 4 of a 16-byte page, reads 12 bytes, and ends at position 16. Starting at the immutable prefix's byte length is valid EOF; positioning beyond it returns `InvalidOffset`. A short raw read can end at a page boundary rather than EOF.

For interleaved `f32` audio, compute a frame's byte offset with checked `frame × channels × 4` arithmetic and validate it against that snapshot's metadata. The index itself understands only bytes, not sample alignment or MIDI records.

The page index is maintained as pages arrive: logarithmic positioning/append metadata, O(total pages) storage, and direct next-page links for sequential reads. It is not a page table copied for every version. Seeking, wrapping and freezing do not rebuild it by scanning all media; reclamation still performs bounded page work.

## 2. Store an explicit MIDI record stream

The MIDI example uploads these records with duration five:

| Relative frame | Packet |
|---|---|
| 0 | Note-on 60, velocity 100 |
| 2 | Complete 37-byte SysEx |
| 2 | Note-off 60 |

Each serialized record is little-endian `u64 frame`, `u32 packet_length`, then complete packet bytes. `encode_midi_record` writes into caller-provided storage; it is not Rust struct serialization. Records are nondecreasing by frame and strictly before the declared duration. Trailing silence remains part of that duration.

The example uses 16-byte pages and a prepared maximum packet size of 64. The SysEx header and body cross page boundaries; upload chunks remain contiguous and page-bounded. Parser validation spends one step per uploaded byte, even if a malformed chunk is refused. Choose chunks for the remaining copy **and** validation allowance, not just the page size.

## 3. Enumerate metadata, then copy the complete packet

Derive an event cursor from the pinned index:

```rust
    let index = store.read_index(old.id)?;
    let mut cursor = store.render_io().midi_cursor(index)?;
    let mut scratch = [0xa5; 64];
```

Each `midi_next` returns an opaque event token and advances the cursor past that record. The example compares its metadata with the independent input records:

```rust
            let event = store.render_io().midi_next(&mut cursor)?.unwrap();
            assert_eq!(event.frame(), frame);
            assert_eq!(event.byte_len(), packet.len());
```

Copy the original packet into prepared scratch:

```rust
            let count = store.render_io().midi_read(event, &mut scratch)?;
            assert_eq!(&scratch[..count], packet);
```

Unlike one raw byte read, this required packet copy can span several pages. It copies one complete declared packet, never a partial packet sized to the current page. Its bound is the prepared packet capacity, not an arbitrary musical subdivision allowance.

The example verifies SysEx before note-off at their equal frame, then EOF. EOF means no more recorded events; it does not remove the declared silence through frame five.

## 4. Retry the same event after small-buffer refusal

Before copying the SysEx, the example deliberately uses an eight-byte destination:

```rust
                let mut too_small = [0xa5; 8];
                assert_eq!(
                    store.render_io().midi_read(event, &mut too_small),
                    Err(Error::Capacity)
                );
                assert_eq!(too_small, [0xa5; 8]);
```

The destination remains unchanged. Retain `event` and retry it with sufficient scratch, as the example does. Calling `midi_next` again would skip the unread SysEx: returning the event already advanced its cursor. Copy refusal does not rewind it.

## Keep the lease until every cursor/event is finished

Replacing the active revision does not retarget old event tokens. The example replaces its resource with an empty five-frame stream, then still copies the original 37-byte SysEx through the old lease. Captured-prefix byte immutability across appends is demonstrated in [chapter 10](10-recording-and-exporting.md).

A foreign store rejects the scoped event/index. Releasing the original snapshot makes its retained event and cursor return `InvalidId`. Copies of those handles do not keep the lease alive or acquire another one. Raw resource/snapshot preparation still requires same-store discipline; this stronger scoped validation is not a claim that all raw IDs detect foreign stores.

Release each acquired lease once, remove unwanted identities, and reclaim retired pages before relying on their reuse. Index/cursor/event handles cannot be transported as capabilities between independent Wasm instances.

## Event access does not reconstruct state

Sequential `midi_cursor`/`midi_next`/`midi_read` works on validated legacy MIDI without checkpoint rows. The example's legacy stream explicitly refuses `midi_seek(index, 2)` with `InvalidState`; it does not fall back to scanning all earlier packets.

Required byte/event work and its page-index counters are separate from optional background allowance. They reset once per host callback, not per borrowed view or internal interval. Diagnostic saturation is observational, not a reason to refuse required work or a deadline guarantee.

For bounded state reconstruction and retaining equal-time originals for playback, continue with [MIDI state and checkpointed seeking](12-midi-state-and-checkpointed-seeking.md).

[Next: MIDI state and checkpointed seeking](12-midi-state-and-checkpointed-seeking.md)
