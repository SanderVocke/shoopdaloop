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

## Prepared Rust layout and work bounds

A channel has 512 fixed u32 field locations: keys 0–127, controller values
128–255, pitch at 256, pressure at 257, and reserved padding. Bit 31 means known.
Note words contain velocity, key-down, sostenuto capture, sustain hold, and known
poly-pressure. Each state occupies 32 KiB; one is prepared per input, output, view,
and one seek scratch target. View generations protect the snapshot in that slot.
Snapshots are copied only at capture/import/range operations, not every quantum.

Each sequence/output has an 8,192-bit difference map, a 128-bit nonempty-word
summary, count, and 272 bytes of owner activity. The 128 × 8 comparison profile is
about 1.4 MiB in Rust. Each output/channel directory is a 128-bit sequence set.
A changed ordinary controller visits only its field and the relevant directory;
returning to the target value clears the bit immediately. Pedal/reset operations
may affect 128 keys on their channel. Restoration traverses dirty fields; only
complex pedal repair inspects the bounded channel group. A seek interprets the
stored prefix once and uses prepared scratch memory. None of these jobs allocates.

The compiled `State` binding supplies the owner generation, sequence, direct input,
outputs and channel mask. Prepare it outside rendering. SP submits restore on
play/start/repeat/seek and cleanup on leaving playback. JP performs concrete data
work without reading loop modes. Existing state bindings must be identical across
live image switches; changing them requires a stopped engine and fresh preparation.
This deliberately avoids an implicit route-change cleanup pass in the callback.

| Scenario | Checked behaviour |
| --- | --- |
| Control changes then returns | Bit sets, stays set, then clears without a restore. |
| Unrelated channel/output | No comparison visits. |
| Pressure-only difference | Pressure packet, no note restart. |
| Sustain/sostenuto repair | Pedal releases precede reconstruction; capture membership is correct. |
| Same-key velocity difference | Off/on then known pressure. |
| Recording starts on an input event | Snapshot precedes that event. |
| Repeat | Restore before ordinary frame-zero packets. |
| Refused packet | Accepted prefix updates output and all comparison maps. |
| Range/export followed by retake | Prefix-derived snapshot and retained original remain valid. |
| Unknown target | No invented value; raw events and owner stop cleanup still operate. |

RTL mapping review: bulk state/snapshots/bitmap words belong in DDR. Connection
masks and summary counts are bounded, prepared metadata. The RTL can serialize
field access and reuse its memory port while Rust uses normal indexed arrays and
word operations. Do not synthesize these heap arrays as resettable registers.
