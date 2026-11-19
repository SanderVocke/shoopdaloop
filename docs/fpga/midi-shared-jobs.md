# MIDI execution through SP and JP

The state processor (SP) owns musical time and loop modes. Its compiled work pass
places MIDI and audio jobs in one finite queue. The existing job processor (JP)
drains that queue; MIDI services receive explicit operands, not access to SP.
The event store, history, state interpreter and incremental comparison algorithms
remain reusable software components. Rust functions remain direct computations.

## Execution contract

A quantum first settles all loop states in the existing synchronization pass.
The work pass orders recovery, capture completion/start, input observation,
capture append, old-owner cleanup, ownership arbitration (established owners
first), restoration/read windows, ordered output, cursor completion and bounded
maintenance. Each channel operation names a stable prepared context slot and
its loop identity. Musical positions and lengths come from SP. Jobs resolve
content through the context at execution time, so a completed recording can be
played by a later job in the same queue without a stale view handle.

Queue capacity is reserved for the maximum admitted work before SP advances.
Enqueuing work expresses intent. Writer, restoration and output progress change
only when execution succeeds. Output acceptance drives destination state,
incremental differences, activity and cursor advancement. MIDI capacity/refusal
is reported separately from a fatal audio/pipeline fault. All captures obtain
pre-event starting snapshots before input observation. Cleanup for every old
owner precedes restoration for any new owner. Output selection retains the
source context and orders by timestamp then stable context identity, including
across destinations.

Grouped edits use prepared reservations and generation checks. At a completed
quantum boundary, publication changes bindings and SP lengths/revisions together.
Exports retain independent view references. Retirement and transfer preparation
are bounded and cannot consume capture/history reservations.

## Migration baseline

The starting source is `0e26c783b2bd1662fd156483e5658cf8b795430d`.
The old standalone Rust session is frozen under `midi::oracle` with `cfg(test)`;
it cannot be selected in a production build. Existing MIDI, pedal, content,
allocation and independent native loop tests remain behavioral requirements.
The old standalone RTL and its Vivado measurements remain historical evidence.
The reference/compiler baseline suite passed locally. A descriptive release-mode
3,000-event/128-binding workload is retained outside the repository during work.
