# 8. Processing and scheduling MIDI

<!-- rust-source: ../../examples/midi_packets.rs -->

[Contents](../README.md) · [Previous: Changing parameters at an exact sample](07-changing-parameters-at-an-exact-sample.md) · [Next: Loading and reading resources](../part-3-work-with-stored-media/09-loading-and-reading-resources.md)

MIDI ports carry complete packets with sample offsets. A processor receives offsets relative to its current processing interval, even when one host callback is split by scheduled actions. The runtime rebases them when routing into and out of that interval.

## Run the example

The [MIDI packets example](../../examples/midi_packets.rs) passes host packets through one processor and injects a scheduled note at absolute frame 2:

```sh
cargo run -p rt_runtime --example midi_packets
```

It checks exact bytes/timestamps with callbacks `[4]`, `[1, 3]` and `[1, 1, 1, 1]`, including a zero call at frame 2 in the last partition. A second scenario deliberately overflows MIDI routing, verifies continuing audio and all 32 recovery packets, then resumes ordinary output. Processing and recovery draining are heap-guarded; this is an offline example, not device-delivery evidence.

## 1. Prepare packet storage

`MidiBuffer::new(events, bytes)` allocates two independent ordinary capacities. Clear and reuse it between callbacks. One large SysEx can exhaust bytes before event slots; many one-byte realtime packets can exhaust event slots first.

The example also checks these packet writes:

```rust
        packets.push(0, 4, &[0x90, 60, 100])?;
        assert_eq!(packets.push(1, 4, &[0x80, 60]), Err(Error::InvalidEvent));
        assert_eq!(packets.len(), 1);
        packets.push(2, 4, &[0xf0, 0x7d, 1, 0xf7])?;
        assert_eq!(packets.byte_len(), 7);
        assert_eq!(packets.push(3, 4, &[0xf8]), Err(Error::Full));
```

The note-off is incomplete and changes no prior packet. The SysEx includes both `0xf0` and `0xf7`; partial SysEx is not a streamable fragment here. A host must resolve running status and separate interleaved realtime bytes before submitting complete packets. Do not allocate a Vec per packet in the callback.

`push(offset, frames, packet)` requires nondecreasing offsets in `[0, frames)`. `insert` maintains stable timestamp order and permits inserting earlier offsets, with descriptor-shifting work bounded by event capacity. Both preserve existing contents on refusal.

Preparation writes every descriptor slot and byte, including recovery storage;
bulk initialization does not leave lazy/uninitialized packet slots. Routing may
copy an already-validated whole source in bulk when its rebased timestamps append
after existing packets. This preserves stable equal-time source order even when
payload bytes were originally inserted out of timestamp order. Partial windows
or interleaved timestamps retain ordinary stable insertion. Both paths preflight
capacity and use the same overflow/reset protocol; no packet or interval quota
is introduced.

Host input buffers must also fit the runtime's configured event/byte limits and callback frame range. Oversized/malformed host input is an invalid callback, not the internal fan-in overload recovery shown below.

## 2. Process only this interval's offsets

For ordinary packets, the pass-through processor copies complete packets using its supplied interval length:

```rust
            for index in 0..input.len() {
                let (offset, packet) = input.get(index).ok_or(Error::InvalidEvent)?;
                block.midi_outputs[0].push(offset, block.frames, packet)?;
            }
```

This branch borrows `block.midi_inputs[0]` as `input`; it neither parses a byte stream nor assumes a host block size. The processor separately recognizes an emergency input buffer through `is_recovery()` and verifies its resets. A real receiver must apply those resets to its MIDI state; this demonstration counts them rather than implementing a synthesizer.

Audio routing and MIDI routing are independent connections. The example copies audio input to output before handling MIDI, so a full MIDI path does not stop its audio.

## 3. Schedule a complete packet

The Schedule payload owns the packet until command completion. Admission copies it into prepared scheduler storage:

```rust
    let ticket = control
        .submit(
            Lane::Normal,
            Command::Schedule {
                at: Frame(2),
                action: Action::Midi {
                    node: NodeId::from_parts(0, 1)?,
                    port: 0,
                },
                payload: Box::new([0x90, 72, 100]),
            },
        )
        .map_err(|refusal| refusal.error)?;
```

The target must be installed **and active**, with that MIDI input port. Parameters can address inactive installed processors; MIDI injection cannot.

One packet must fit both `midi_bytes_per_port` and `budgets.transfer_bytes - resource_io_bytes`. Callback copy allowance can delay admission. After acceptance, internal subdivision does not add an arbitrary delivery quota. Use finite lookahead and collect both owning completions and execution receipts, as in [chapter 7](07-changing-parameters-at-an-exact-sample.md).

Here host input 0 sends a note at frame 0, another at frame 2, and a note-off at frame 3. Host input 1 also sends a note at frame 2. Canonical routed source order comes first, followed by local injection:

```rust
    assert_eq!(
        observed,
        [
            (0, [0x90, 60, 100]),
            (2, [0x90, 70, 100]),
            (2, [0x90, 71, 100]),
            (2, [0x90, 72, 100]),
            (3, [0x80, 60, 0]),
        ]
    );
```

The result is identical under all three callback partitions. Frame 2 at a callback end belongs to the next nonempty callback; a zero call at that cut neither injects it nor advances the clock.

## 4. Recover without stopping audio

The overload scenario gives each runtime MIDI port just one ordinary event/three bytes. Each host input individually fits, but their fan-in into one processor input does not. That input is replaced by reserved emergency messages: CC120 then CC123 for each of 16 channels, in order—32 messages/96 bytes. Ordinary `MidiBuffer::push` cannot use that emergency reserve.

A recorded overflow also requests recovery on every host MIDI output. Ordinary output is suppressed while that output's recovery remains pending; audio continues. This is a generic bounded reset mechanism, not loop restoration, pedal-history reconstruction or a snapshot of external synth voices.

A host's normal output loop is:

```text
process audio/MIDI
for each MIDI output:
    reserve downstream reset capacity
    clear its separate preallocated recovery sink
    drain_midi_recovery once into that sink
    deliver accepted reset packets, then ordinary output
```

A sink that can accept a complete reset in one drain needs **32 ordinary event slots and 96 ordinary bytes**. Its own internal emergency reserve does not count. The callback's `budgets.events` must also allow those attempts. Never acknowledge into a temporary sink and discard its accepted packets: the runtime cannot recover a reset the host then loses.

The example instead uses a one-event sink to expose backpressure. It records one accepted packet, then deliberately retries while the sink is still full:

```rust
        assert_no_heap(|| {
            assert_eq!(runtime.drain_midi_recovery(0, &mut sink, 4)?, 1);
            delivered[index].copy_from_slice(sink.get(0).unwrap().1);
            assert_eq!(runtime.drain_midi_recovery(0, &mut sink, 4)?, 0);
            Ok::<_, Error>(())
        })?;
```

That second call is a backpressure assertion, not the recommended host loop. A full attempt spends allowance but does not acknowledge the next packet. Drain attempts share one per-output allowance for the callback; repeated calls or a zero-frame `process` do not renew it. `drain_midi_recovery` itself requires a positive frame count no greater than `max_frames`.

The next callback retries the exact next packet. Repeated input overflow during the unfinished reset does not restart its sequence. Across 32 callbacks the example verifies all bytes, continuing `1.0` audio, and no ordinary MIDI output. After the final acknowledgement, ordinary packets resume on a subsequent render.

`Command::ResetMidi` can request host-output resets through independently admitted recovery work when normal commands are full. It does not reset arbitrary processor state or waive recovery admission/processing limits.

## Graph changes do not perform note cleanup

At one frame, planner operations precede processor-target actions. Within a processor target, parameter/MIDI/typed operations have a defined order; there is no global “all parameters before all MIDI” ordering across nodes. Submission order alone cannot reverse these categories.

If a planner operation removes an injection target first, that injection fails `InvalidId`. Planning occurs after due actions: if the packet has already been injected, removing its target is refused `Busy` before graph/domain publication. The old graph consumes the packet once; it must not disappear or replay on later reactivation. A retained target can consume pending input normally. [Chapter 15](../part-4-coordinate-processors-and-changes/15-staging-and-coordinating-graph-activation.md) covers these publication preconditions.

Arrange musical note/pedal cleanup explicitly before removing a role. Graph metadata does not synthesize it. Stored MIDI event/state seeking is a separate task in [chapters 11](../part-3-work-with-stored-media/11-seeking-and-reading-midi-events.md) and [12](../part-3-work-with-stored-media/12-midi-state-and-checkpointed-seeking.md).

[Next: Loading and reading resources](../part-3-work-with-stored-media/09-loading-and-reading-resources.md)
