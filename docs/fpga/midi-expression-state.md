# MIDI expression-state contract

This implementation adds optional performance-state jobs to the existing SP/JP
pipeline. Raw event mode remains available. Source revision for the baseline is
`cc410f27`; earlier event-only simulation and Vivado evidence remains unchanged.

The default controller set is 1, 2, 4, 11, 64, 66, 67, 74. Pitch bend, channel and
polyphonic pressure, note velocity, key-down and pedal-held states are managed.
Other ordinary absolute CCs can be selected per output. Bank/program selection,
parameter selectors/data entry, tuning, and high-resolution controller pairs are
not automatically restored. Configuration is assumed stable.

Notes use one logical voice per channel/key. Sustain holds released notes;
sostenuto captures keys down on its off-to-on edge. Repeated on values do not
capture new keys. A new note replaces that key's logical voice. Note On velocity
zero releases the key. Soft pedal changes sound state, not note holding. Pressure
belongs to an active voice. Controller reset establishes only its documented
performance defaults; device-specific values remain unknown. All Notes Off
releases keys under existing pedals; All Sound Off silences voices without
changing controllers. Incoming commands are observed, not generated as dumps.

Starting snapshots belong to content views. Snapshot and view generations agree;
retained exports retain both. A snapshot has known bits, so absent knowledge never
silently becomes a reset value. Direct input state is observed once per input job,
including while loops are stopped. Capture samples that context before the first
recorded quantum. Merged-source captures require an explicit initial context or
retain unknown initial values; no graph-wide state interpreter is introduced.
Ranges and nonzero seeks derive state by applying the event prefix strictly before
the new origin. This work happens only at the discontinuity.

Each sequence/output comparison has a directly addressed bitmap and nonempty-word
summaries. Active membership is indexed by output/channel. Current output state
and all relevant comparisons advance only after packet acceptance, including
restoration packets. A refusal faults the quantum as in event-only mode; accepted
prefixes remain truthful. No automatic recovery scheduler is added.

Matching controls emit nothing. Pressure-only changes do not restart notes.
Simple note differences use targeted off/on repairs. When note holding/capture or
pedal edges require reconstruction, the affected channel's known note group is
conservatively rebuilt: release pedals/keys, restore sound controls, start captured
notes, engage sostenuto, start other notes, release target key-up notes, then apply
per-note pressure. This may reattack matching notes in that channel. It is a
chosen musical approximation, not a claim of optimal or envelope-preserving repair.
Unknown target fields preserve known current values during group reconstruction.

Channel-wide state follows the accepted stream. Loops sharing a channel cannot
have independent pedals or independent same-key synthesizer voices. Cleanup is
targeted to tracked playback activity; unrelated outputs/channels are untouched.
Use separate channels for independent articulation. The SP decides transitions;
JP receives concrete owner, sequence, position, scope, and destination information.
