# MIDI checkpoint state foundation

`midi_state::MidiState` is a fixed, Copy, allocation-free projection of MIDI 1.0 channel/key messages. It has no clock, device, loop, or renderer authority. It is suitable as initial/checkpoint metadata; the resource store's [MIDI index](midi-indexing.md) supplies checkpoint pools and indexed event/state positioning. Restoration emission is not implemented by this type.

## State and updates

- Sixteen channels, each with 128 key records and controller slots; no dynamic maps or callback allocation.
- A key records the latest note-on velocity, whether it is held, its sostenuto latch, and observed poly pressure. Velocity zero means no projected sounding note. Note-on velocity zero is a note-off. Repeated note-ons update that key's velocity; they remain separate packets in the media stream. This is not a model of a synthesizer's per-voice allocation/envelopes.
- CC64 sustain and CC69 hold retain released keys until released. CC66 latches keys held on its off-to-on edge, not keys pressed later. Releasing one pedal respects the other holds. Held keys survive pedal release. Repeated values still update the observed controller value but do not rescan keys when the pedal's logical up/down state has not changed. Releasing unheld keys is a no-op while another sustain/hold pedal remains down.
- CC120 clears notes immediately without resetting controllers. CC123 and mode changes CC124–127 release held keys while respecting holds. Momentary CC120/121/123 commands are not cached as persistent controller values to replay accidentally after restoring notes.
- Programs, pitch bend, channel pressure, and poly pressure are retained. The bank selectors observed at program change are retained separately from later pending bank selectors. The last RPN/NRPN selector family is retained as an ordering hint; parameter Data Entry/relative-command history is explicitly marked as untracked. CC96/97 are momentary commands, not cached values to replay. Of each opposed CC124/125 and CC126/127 mode pair, only the latest setting remains current. Unobserved program/pressure/controllers remain unknown rather than acquiring invented defaults. Initial pedal states are off and pitch is centered.
- CC121 establishes a reset baseline: pitch and pressure reset, pedals release, held keys remain, and recommended known control values are recorded. Bank, volume, pan, sound/effects controls, program, and channel-mode settings remain retained. Device-specific defaults remain unknown. `reset_controllers_seen()` records the baseline operation for eventual restoration before known control values.
- System Reset clears the projection and sets `system_reset_seen()`. Other system messages and opaque SysEx do not fabricate channel state. Their packets are still part of the media stream; this projection does not reconstruct device-specific effects or RPN/NRPN parameter histories.

The reset policy follows the MIDI Association's [Response to Reset All Controllers (RP-015)](https://midi.org/response-to-reset-all-controllers/) and [Control Change definitions](https://midi.org/midi-1-0-control-change-messages). Known reset values include modulation 0, expression 127, holds/portamento/soft pedal off, and null RPN/NRPN selectors. Unknown device-specific defaults are not represented as zero.

`apply(Message)` accepts an already validated complete packet; `apply_packet(bytes, max_bytes)` first validates and leaves state unchanged on malformed/oversized input. Ordinary notes/program/pressure/pitch updates use fixed work; pedal/mode/reset operations traverse only the fixed protocol channels/keys. Copying state is bounded by this fixed representation, not recording duration. The type itself does not retain packets, consume scheduler credits, or impose a musical-work quota.

`MidiState::from_channels([ChannelState; 16], system_reset_seen)` reassembles copied
channel projections and the global reset marker without message replay. This is
useful for fixed-size channel-wise export replies. The caller must obtain all
channels and the marker from the same immutable snapshot; the constructor does
not certify that independently sampled live values form a coherent cut.

## Renderer integration

The resource index preserves an initial state and checkpoints built incrementally while records arrive, with reserved per-version rows and a declared maximum local replay span. Positioning returns state before the requested frame and retains equal-time events for playback. Renderer integration must still emit restoration and those events in the specified order; the value type alone is not a renderer.

Restoration must preserve reset markers, distinguish held from pedal-latched keys, order bank/program/controller/pressure/note operations deliberately, and expose output-capacity failures through runtime MIDI recovery. The loop crate now provides bounded cleanup/projected-state emission helpers with those ordering rules and an explicit untracked-parameter-history report; indexed playback/capture and runtime recovery are supplied by the loop crate, not by this value type. Protocol-state restoration is not a snapshot of an external synthesizer or device. No renderer or loop-worklet integration is claimed here.
