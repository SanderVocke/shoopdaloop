# MIDI checkpoint state foundation

`midi_state::MidiState` is a fixed, Copy, allocation-free projection of MIDI 1.0 channel/key messages. It has no clock, device, loop, or renderer authority. It is suitable as initial/checkpoint metadata; checkpoint pools, indexed event positioning, and restoration emission are not implemented by this type.

## State and updates

- Sixteen channels, each with 128 key records and controller slots; no dynamic maps or callback allocation.
- A key records the latest note-on velocity, whether it is held, its sostenuto latch, and observed poly pressure. Velocity zero means no projected sounding note. Note-on velocity zero is a note-off. Repeated note-ons update that key's velocity; they remain separate packets in the media stream. This is not a model of a synthesizer's per-voice allocation/envelopes.
- CC64 sustain and CC69 hold retain released keys until released. CC66 latches keys held on its off-to-on edge, not keys pressed later. Releasing one pedal respects the other holds. Held keys survive pedal release.
- CC120 clears notes immediately without resetting controllers. CC123 and mode changes CC124–127 release held keys while respecting holds. Momentary CC120/121/123 commands are not cached as persistent controller values to replay accidentally after restoring notes.
- Programs, pitch bend, channel pressure, and poly pressure are retained. Unobserved program/pressure/controllers remain unknown rather than acquiring invented defaults. Initial pedal states are off and pitch is centered.
- CC121 establishes a reset baseline: pitch and pressure reset, pedals release, held keys remain, and recommended known control values are recorded. Bank, volume, pan, sound/effects controls, program, and channel-mode settings remain retained. Device-specific defaults remain unknown. `reset_controllers_seen()` records the baseline operation for eventual restoration before known control values.
- System Reset clears the projection and sets `system_reset_seen()`. Other system messages and opaque SysEx do not fabricate channel state. Their packets are still part of the media stream; this projection does not reconstruct device-specific effects or RPN/NRPN parameter histories.

The reset policy follows the MIDI Association's [Response to Reset All Controllers (RP-015)](https://midi.org/response-to-reset-all-controllers/) and [Control Change definitions](https://midi.org/midi-1-0-control-change-messages). Known reset values include modulation 0, expression 127, holds/portamento/soft pedal off, and null RPN/NRPN selectors. Unknown device-specific defaults are not represented as zero.

`apply(Message)` accepts an already validated complete packet; `apply_packet(bytes, max_bytes)` first validates and leaves state unchanged on malformed/oversized input. Ordinary notes/program/pressure/pitch updates use fixed work; pedal/mode/reset operations traverse only the fixed protocol channels/keys. Copying state is bounded by this fixed representation, not recording duration. The type itself does not retain packets, consume scheduler credits, or impose a musical-work quota.

## Integration still required

The planned index must preserve an initial state and checkpoints built incrementally while records arrive. Choose a declared maximum local replay span and reserve checkpoint storage before promises. Position before the requested media frame, restore tracked state, then deliver all equal-time events in original order. Neither this projection nor a byte cursor alone satisfies that contract.

Future restoration must preserve reset markers, distinguish held from pedal-latched keys, order bank/program/controller/pressure/note operations deliberately, and expose output-capacity failures through runtime MIDI recovery. Protocol-state restoration is not a snapshot of an external synthesizer or device. No renderer or loop-worklet integration is claimed here.
