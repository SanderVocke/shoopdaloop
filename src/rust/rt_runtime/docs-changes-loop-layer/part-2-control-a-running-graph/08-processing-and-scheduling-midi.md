# Change instructions — chapter 8: graph changes and pending MIDI

Destination: the matching chapter under `docs/`. Baseline: empty; archive sources
`scheduling-midi.md` and `graph-activation.md`. Full MIDI/recovery backfill remains
pending; this note captures the new activation compatibility rule.

Add a concise cross-reference to chapter 15 where graph edits interact with timed
injection. Planner operations precede processor/MIDI actions in the scheduler's
same-frame category order; submission order alone does not reverse this. If an
operation-time activation removes the target, the later injection fails InvalidId.

Planning occurs after due actions. If an injection has already been delivered to a
node's input, a candidate that removes it is refused Busy before graph/domain
publication. The old graph consumes that frame. The candidate remains Ready for a
later request; the packet must neither vanish nor replay when the node returns.
A target retained in the new graph can consume its pending input normally.

This is not automatic note/pedal cleanup or external synthesizer-state recovery.
A musical application must arrange cleanup for removed roles. Keep complete packet,
SysEx, capacity and host-recovery policy explanations separate from this metadata
precondition; the generic runtime does not provide loop-library MIDI restoration.

Reference: `tests/graph_activation.rs::same_frame_injected_midi_prevents_removal_until_delivery_and_never_replays_on_return`.
If demonstrating both orders, defer one activation into planning to create the
already-delivered-input case; reversing command submission order is insufficient.
