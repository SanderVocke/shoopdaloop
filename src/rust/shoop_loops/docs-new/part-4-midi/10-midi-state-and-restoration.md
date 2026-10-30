# 10. MIDI state and restoration

<!-- rust-source: ../examples/midi_loop.rs -->

[Contents](../README.md) · [Previous: A MIDI loop](09-a-midi-loop.md) · [Next: Exporting and importing media](../part-5-content-and-graph/11-exporting-and-importing-media.md)

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

While a MIDI loop plays, the planner's playback node keeps a `MidiState`: which notes are held, controller values, and the program/bank selectors. A `Seek` (chapter 4) does not just jump the event offset; the restore path first re-establishes the projected state at the seek point — pedals, controllers, RPN/NRPN selectors — then delivers the recorded events from there. Cleanup always precedes restoration, so a seek cannot stack a second copy of a pedal.

The state is also queryable from the control side: `Operation::State` returns a `Cut` whose MIDI fields reflect the loop at that frame, and `Operation::MidiInitial { export, channel }` reads the initial state of an exported take.

## 3. What restoration does not do

Restoration replays the fixed protocol (notes, pedals, controllers, selectors) from recorded state. It does not reconstruct synthesizer voices or parameters that were never sent as MIDI, and relative parameter commands (data-entry increments) are not replayed as state. If a device needs more than that, send your own reset from the application after Stop.

[Previous: A MIDI loop](09-a-midi-loop.md) · [Next: Exporting and importing media](../part-5-content-and-graph/11-exporting-and-importing-media.md)
