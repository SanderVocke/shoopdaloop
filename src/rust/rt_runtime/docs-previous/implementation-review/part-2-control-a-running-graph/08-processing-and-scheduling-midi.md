# Change instructions — chapter 8: graph changes and pending MIDI

Destination: the matching chapter under `docs/`. Baseline: empty; archive sources
`scheduling-midi.md` and `graph-activation.md`. The sections below cover the
activation compatibility rule and complete the MIDI/recovery handoff backfill.

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

## Packet and exact-frame walkthrough

Start with a prepared MIDI pass-through processor and a host note-on at offset0,
note-off at offset3, plus a scheduled complete packet at absolute frame2. Assert
exact bytes and absolute timestamps across [4], [1,3] and four one-frame callbacks.
Include equal-frame fan-in from two sources and a local injection; assert canonical
source order followed by the local injection. Within a processor target, scheduled
parameter/MIDI/typed-operation order is defined; there is no global all-parameters-
before-all-MIDI rule across different nodes. Chapter7 supplies admission/receipt
mechanics; distinguish the MIDI target's required active membership.

The host must parse running status and separate realtime bytes before submission.
A complete bounded SysEx is one packet, not an invitation to stream partial packets.
MidiBuffer has independent event and byte capacities. push requires nondecreasing
offsets, insert maintains stable timestamp order with descriptor-shifting cost;
both reject malformed/out-of-range packets without partial writes. Offsets are
half-open within the current process interval, rebased to/from host callback offsets.
Prepare buffers and packet storage off-render; do not allocate a Vec per packet.

A scheduled packet must fit min(midi_bytes_per_port,
transfer_bytes - resource_io_bytes). Staging shares callback copy allowance and may
wait before admission; accepted events are not deferred by arbitrary subdivision
credits. Long sequences need finite lookahead and drained execution receipts.

## Make overload recovery observable

Add a second small example/test profile with deliberately tiny ordinary event/byte
capacity. Keep processing audio even when an ordinary MIDI write refuses. An input
routing overflow replaces that input with reserved CC120 then CC123 on each of
16channels:32messages/96bytes beyond ordinary capacity. This is not loop-specific
cleanup, pedal/state restoration, or a model of external synth voices.

Reported overflow also requests resets on every host output. After process(), for
each output clear a separate preallocated recovery sink, call drain_midi_recovery,
deliver its accepted reset packets, then any ordinary output. Reserve downstream
capacity before acknowledging delivery into the sink; throwing away accepted resets
can strand notes. A full reset in one sink requires32ordinary slots/96ordinary bytes;
the sink's own emergency reserve is not available to this API. Use a smaller sink to
assert the exact next packet persists, repeated overflow does not restart an
unfinished sequence, and ordinary output remains suppressed while recovery is pending.
Audio continues, and ordinary output resumes on a subsequent render after recovery.

Drain attempts share budgets.events per output per callback, including repeated
calls; a full-sink attempt consumes allowance but does not acknowledge the packet.
Use one drain call per output in the recommended host loop. frames must be positive
and within max_frames; a zero-frame process call is not a new recovery budget.
Command::ResetMidi may use independent recovery-lane admission when normal is full;
it requests output resets, not arbitrary processor-state reset. Assert all32packet
bytes/order, no skipped reset under backpressure, continuing audio and recovery's
own admission limits. Guard render/drain, not only the ordinary processor call.

References: tests/midi.rs, tests/schedule.rs, tests/executor.rs and the same-frame
activation case above. ../control-baseline.json links source-matched native tests;
these proposed public examples still require implementation by the documentation
worker. Stored media/state seeking belongs to chapters11–12, not this packet lesson.
