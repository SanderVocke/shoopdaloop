# 10. Recording and exporting

<!-- rust-source: ../../examples/recording_prefixes.rs -->

[Contents](../README.md) · [Previous: Loading and reading resources](09-loading-and-reading-resources.md) · [Next: Seeking and reading MIDI events](11-seeking-and-reading-midi-events.md)

Capture appends contiguous audio or MIDI intervals to a resource. A snapshot pins a coherent prefix without stopping recording. Later appends cannot change its bytes, duration or capture cut.

## Run the example

The [recording prefixes example](../../examples/recording_prefixes.rs) retains an audio tail across later appends, freezes the full recording, and captures MIDI with trailing silence:

```sh
cargo run -p rt_runtime --example recording_prefixes
```

A second one-page store demonstrates optional snapshot refusal and an incomplete capture without losing the preceding prefix. These are direct-store, heap-guarded data operations; the example's host owns the callback boundaries. It is not a live device recorder.

## 1. Begin at an explicit frame

The first capture starts at absolute frame 100:

```rust
    let capture = store.begin_capture(
        audio,
        Revision(0),
        Format::Audio { channels: 1 },
        Frame(100),
    )?;
```

Begin activates a fresh physical revision. Required `render_io().capture_audio` appends interleaved samples at the capture's next frame. Here three samples advance that frame to 103. Each append must start exactly where the previous one ended and fit the prepared maximum frames/shape.

In a processor, use `block.resources.render().capture_audio`; prepare interleaving scratch off-render if callback inputs are planar. Legacy `ResourceIo::capture_audio` remains available but uses optional copy allowance.

## 2. Snapshot the actual prefix

The first three samples occupy 12 bytes of a 16-byte page. A snapshot requested with frame 110 still describes the actual captured end:

```rust
    let prefix = store.snapshot(audio, Frame(110))?;
    assert_eq!(prefix.cut_frame, Frame(103));
    assert_eq!(prefix.description.frames, 3);
    assert_eq!(prefix.revision, Revision(2));
```

A request frame earlier than the captured end is invalid. A later request does not invent missing duration or samples.

A running snapshot shares complete pages and copies its partial mutable tail into a private page. It needs a snapshot slot, a spare unreserved page when there is a partial tail, and optional copy allowance for the used tail bytes. Refusal leaves capture unchanged; do not retry until success inside the callback.

The example appends samples 4, 5 and 6, mutating the old tail and crossing its page boundary. Exporting `prefix` still returns exactly samples 1, 2 and 3. `Currentness::Stale` is compatible with an immutable readable lease.

`export` is an example helper, not a store API: it allocates the destination off-render and copies through the snapshot's page-bounded cursor with explicit progress/EOF checks. Running hosts can instead use the owning Read commands from [chapter 9](09-loading-and-reading-resources.md).

## 3. Freeze without changing the recorded bytes

After the second append, the active revision is 3. Stop freezes it without another data revision:

```rust
    let before_stop = store.snapshot(audio, Frame(106))?;
    assert_eq!(store.stop_capture(capture)?, Revision(3));
    assert_eq!(store.currentness(before_stop.id)?, Currentness::Stale);
    let frozen = store.snapshot(audio, Frame(120))?;
    assert_eq!(frozen.cut_frame, Frame(106));
    assert_eq!(frozen.description.frames, 6);
    assert_eq!(frozen.revision, before_stop.revision);
    assert!(!frozen.capture.unwrap().active);
```

The pre-stop status snapshot becomes stale even though no further bytes were appended. The new snapshot contains all six original samples, not request-time silence through frame 120.

Begin and successful positive-length appends advance physical resource revisions. Stop does not. These are not the loop library's musical take revisions; splitting an append into more callbacks can produce a different physical revision count for the same samples.

## 4. Record MIDI duration, not just packets

`capture_midi(capture, frame, frames, input)` stores complete packets with offsets in `[0, frames)`. Stored timestamps are relative to the capture start, using explicit little-endian `u64 frame`, `u32 packet_length`, and packet bytes.

The example starts at absolute 200 and records note-on/note-off packets both at offset 1 of a four-frame interval. Their stored frames are both 1, preserving input order. It then appends two positive silent frames:

```rust
    assert_eq!(store.info(midi)?.description.frames, 6);
    assert_eq!(store.info(midi)?.description.bytes, 30);
    assert_eq!(store.stop_capture(capture)?, Revision(3));
```

The bytes remain the same two 15-byte records; duration increases from four to six. The held earlier snapshot retains duration four. The example checks exact serialized bytes for both exports.

A zero-length append extends nothing. Invalid shape/time calls are not successful recording, but not every invalid call halts capture. This unindexed capture supports sequential event reading; bounded protocol-state seeking is opt-in in [chapter 12](12-midi-state-and-checkpointed-seeking.md).

## Storage exhaustion preserves a complete earlier prefix

In the one-page scenario, the first three samples fit. A running snapshot cannot allocate its private tail and refuses Capacity while capture remains active. The next two-sample append would require another page and is refused **as a whole**.

The example handles that auxiliary failure instead of propagating it through audio processing:

```rust
    assert_eq!(refused, Err(Error::Capacity));
    assert_eq!(output, [0.25, 0.5]);
    let info = store.info(resource)?;
    assert_eq!(info.description.frames, 3);
    assert_eq!(info.description.bytes, 12);
    assert_eq!(info.revision, Revision(2));
    assert!(!info.capture.unwrap().active);
    assert_eq!(info.capture.unwrap().incomplete, Some(Error::Capacity));
```

`output` is the example's independent audio copy, not a claim about a device backend. A real processor should similarly report recording failure through application state while continuing unrelated DSP. Returning the capture error from `Processor::process` would fault/silence the host block instead. Do not reinterpret resource refusal as downstream MIDI overflow.

Storage exhaustion—and optional work exhaustion through legacy capture—halts the take with an explicit incomplete prefix. Replenishing capacity or resetting the next callback's allowance does not resume it after a gap; later appends return Incomplete. Starting a new capture is explicit. The stopped prefix remains exportable without a mutable-tail copy.

## Release every export

The example releases each acquired snapshot exactly once, removes both resource identities, and checks finite reclamation returns all page/version/snapshot capacity. Copying a snapshot does not acquire a lease; dropping its Copy value releases none.

Sequential audio/MIDI captures are **not** a coordinated mixed-media transaction. They do not promise a common cut or atomic failure across resources. Use [recording resources together](../part-4-coordinate-processors-and-changes/16-recording-resources-together.md) for prepared groups, common-prefix publication and reserved final leases. Whether an empty frozen take may play is also domain policy, not generic storage policy.

[Next: Seeking and reading MIDI events](11-seeking-and-reading-midi-events.md)
