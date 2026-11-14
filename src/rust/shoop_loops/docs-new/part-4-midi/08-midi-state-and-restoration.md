# 9. MIDI state and restoration

<!-- rust-source: ../examples/midi_loop.rs -->

[Contents](../README.md) · [Previous: A MIDI loop](07-a-midi-loop.md) · [Next: Composite loops](../part-5-composites-and-programs/09-composite-loops.md)

MIDI is stateful: a note-on without a note-off leaves a key held on the instrument forever. `shoop_loops` tracks that state per loop and emits **cleanup** when playback stops, so your synth does not hang notes. This chapter uses the same [MIDI example](../examples/midi_loop.rs): the take holds note 60 and the sustain pedal (CC 64) at 127, and Stop at frame 7 must release both.

```sh
cargo run --locked --manifest-path src/rust/shoop_loops/docs-new/examples/Cargo.toml --bin midi_loop
```

## 1. What Stop emits

When a MIDI loop stops, the playback node runs `midi_restore::cleanup` on the loop's tracked state and sends the result at the stop frame — here frame 7:

```rust
    assert_eq!(events[events.len() - 2], (7, &[0xB0, 64, 0][..]));
    assert_eq!(events[events.len() - 1], (7, &[0x80, 60, 0][..]));
```

| Cleanup packet | Emitted when |
|---|---|
| `B0+ch, cc, 0` for CC 64/66/69 | The pedal/controller value was ≥ 64 (held). |
| `80+ch, key, 0` | A note is held or has nonzero release velocity. |

Only loop-owned channels are touched; unrelated voices are not reset. The next block after Stop is silent — the example asserts `output[0].len() == 0` for the second `process` call.

## 2. State and seeking

While a MIDI loop plays, the planner's playback node keeps a `MidiState`: which notes are held, controller values, and the program/bank selectors. A `Seek` (chapter 4) does not just jump the event offset; the restore path first re-establishes the projected state at the seek point — pedals, ordinary controllers, banks/program, bend and pressure — then delivers the recorded events from there. Cleanup always precedes restoration, so a seek cannot stack a second copy of a pedal.

The state is also queryable from the control side: `Operation::State` returns a `Cut` whose MIDI fields reflect the loop at that frame, and `Operation::MidiInitial { export, channel }` reads the initial state of an exported take.

`Reply::MidiInitial` includes the requested `ChannelState` and the exported
version's `system_reset_seen` marker. Read every channel from the same retained
export, then use `MidiState::from_channels` if a complete projection is needed.
The marker belongs to the whole projection: emit System Reset once before
channel restoration, not once per channel. `midi_restore::restore` does this;
`restore_channel` emits only that channel's state. Channel-local restoration must
therefore carry the reset marker separately.

Initial state precedes every original packet at the playback/seek frame. It is
not another group of events to interleave by an event order number. When exporting
to a file format that represents setup as frame-zero packets, preserve this
precedence explicitly. Live content import and initial-state ownership are
covered in [chapter 12](../part-6-content-and-graph/11-exporting-and-importing-media.md).

## 3. What restoration does not do

Restoration replays the supported protocol (notes, pedals, ordinary controllers, bank/program, bend and pressure) from recorded state. It does not reconstruct synthesizer voices or parameters that were never sent as MIDI, and relative parameter commands (data-entry increments) are not replayed as state. If a device needs more than that, send your own reset from the application after Stop.

Inspect `Emission::first_error` and recover from sink refusal before resuming
ordinary output. SysEx, parameter CC6/38/96–101 and other unsupported system
messages refuse before storage; their device state is not reconstructed. The
legacy `parameter_history_untracked` diagnostic remains available to helper
callers, but accepted production basic-MIDI media does not carry that history.

[Previous: A MIDI loop](07-a-midi-loop.md) · [Next: Composite loops](../part-5-composites-and-programs/09-composite-loops.md)
