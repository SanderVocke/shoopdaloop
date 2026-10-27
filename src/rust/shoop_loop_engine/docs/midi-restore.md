# MIDI cleanup and projected-state restoration

`midi_restore::{cleanup, restore}` emit complete MIDI 1.0 packets through a bounded sink callback. They borrow a prepared `MidiState`; they do not allocate, mutate it, advance time, read media, acquire leases, or own a device. The caller supplies the exact event offset, performs cleanup before restoration, and delivers original events at that offset afterwards. Indexed playback/capture is provided separately by [MIDI operations](midi.md); the [runtime facade](runtime.md) supplies coordinated planner/consumer integration.

## Ordering and projection

Cleanup releases active CC64/66/69 holds and sends note-off for every projected held/sounding key. It does not send a normal-operation all-sound-off that would erase unrelated channel voices. MIDI 1.0 cannot distinguish separate owners of the same channel/key; hosts must route/channelize independently controlled parts appropriately.

Restoration uses fixed protocol work:

1. Emit a recorded System Reset marker before channel state, and each channel's Reset All Controllers marker before its known controls.
2. Select the bank that was observed when the current program was loaded, then emit that program. Restore any later bank selectors afterwards: those may be pending for a future program change and must not retroactively change the restored program's bank. Unknown bank components are not invented.
3. Restore known controls and current channel modes before notes. Only the latest member of each opposed omni-on/off and mono/poly pair is retained in the projection.
4. Emit cached Data Entry MSB/LSB only with RPN/NRPN selectors temporarily nulled, then restore known selector values with the last observed family last. This preserves subsequent packet interpretation without applying a stale Data Entry value to an unrelated parameter. Relative increment/decrement messages (CC96/97) are commands, not persistent state, and are never synthetically replayed. Original recorded packets remain unchanged.
5. Restore pitch, channel pressure and sustain/hold. Establish sostenuto off, emit exactly the latched sounding keys, then press sostenuto before emitting other sounding keys. Finally release keys whose projected state is pedal-held rather than physically held and restore known poly pressure.

The state retains a program-bank snapshot and selector-family ordering hint. CC121 establishes a canonical null-RPN baseline but does not erase the warning that parameter history occurred; System Reset clears the projection. Controller values from an inactive opposed mode are no longer presented as current state. These refinements are necessary because latest-value arrays alone cannot determine correct restoration order; they do not change the original packet stream or add a second clock.

This is the existing **channel/key projection**, not external synthesizer state. Per-voice envelopes, vendor SysEx and RPN/NRPN parameter-value histories are not reconstructed. `Emission::parameter_history_untracked` makes observed Data Entry/relative-command history explicit. Nulling selectors is a deliberate protective operation, including components whose previous values were unknown. Only known selector values are restored afterwards; unknown components may remain null. This prevents accidental parameter writes, not recovery of missing history or a claim that unknown values were reconstructed. The caller must not present this warning as full device-state restoration. Opaque SysEx and all original parameter packets remain part of later media playback; they are not synthesized from this projection.

## Capacity and failure

`CLEANUP_MAX_EVENTS` is **2096** and `RESTORE_MAX_EVENTS` is **8449**, conservative fixed-protocol bounds rather than musical work quotas. Three bytes per event is a safe byte bound; ordinary program/pressure/reset packets can be shorter. A capture producer's reservation must also include original playback events and the runtime's separate recovery events/bytes. These helpers impose no arbitrary subdivision or event-processing cutoff.

`Emission` reports attempted and accepted packets and the first sink error. After an error it completes its fixed state traversal/counting without retrying the sink or changing source state. An accepted prefix is **not** successful restoration. In a runtime processor, `Error::Full` must reach the runtime's MIDI reset path after other required processing has completed; other errors retain their normal fault handling. The helpers do not themselves claim reserved reset delivery, and must not be used to write partial output directly to a device. Output capacity sufficient for all required emissions is a precondition for lossless comparisons.

## Verification boundary

Tests compare cleanup and sostenuto/hold reconstruction with an independent key/pedal machine, independently check loaded versus pending banks and selector/data ordering, exercise every channel/key/controller/pressure against fixed capacity bounds, and verify full/refusing sinks without heap activity or retries. Runtime state tests cover the new ordering metadata, mutually exclusive modes, relative-command treatment and reset/copy behavior. Full native/Wasm/Miri workloads are retained.

These are restoration building blocks. The separate MIDI operations module now supplies positioning/rebasing/playback/capture, input tracking and a focused runtime recovery proof. The coordinated loop adapter is implemented separately. No independent loop AudioWorklet is supplied in this snapshot, and no device-state or deadline guarantee follows from these helpers or offline examples.
