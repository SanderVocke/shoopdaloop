# Rust reference MIDI components

MIDI execution here is a functional Rust reference, not an FPGA implementation.
The app-facing graph compiler and RTL target continue to reject MIDI graphs.

Managed messages are MIDI 1.0 channel messages and System Reset. Parameter
controllers CC 6, 38 and 96–101, SysEx and other system messages return explicit
unsupported errors. Input consists of complete messages, without running status.
Repeated note-ons remain distinct records; state retains the latest velocity for
a key rather than attempting to model synthesizer voices.

`Interpreter::begin` and `step` keep partial updates private. State queries and a
second update return `Busy` until completion. An ordinary note update takes one
step; a pedal release takes 129, and System Reset takes 4,226. `apply` is the
bounded completion convenience method. Counters saturate instead of wrapping.

`Emission` visits one field per step. Its cursor advances only after output
acceptance, or after a field that produces no message. Cleanup releases pedals
before keys. Restoration establishes resets, selected banks and program, pending
banks, other controls, pitch/pressure and pedal-held notes in protocol order.
Recovery sends All Sound Off and All Notes Off on each channel and retries the
same message under refusal. Recovery requires a dedicated MIDI destination;
sharing channels with unrelated sound producers is outside this milestone.

## Provenance

State and stepped-update logic were ported from `rt_runtime` at
`1d081bf8de54a50842cc48377a0bb395d11bd4d4`, with reference-owned message/error
types and an interpreter that hides partially updated state. No production
runtime dependency was added. Message policy preserves that branch's managed
subset. Restoration ordering derives from `shoop_loops` at
`9eb71d0406af63012e5f55a17ab5bfe994366838`, with unsupported parameter selectors
excluded and explicit acknowledged field-by-field progress replacing bulk output.

Tests include production-state comparisons, independently specified bank and
sostenuto ordering, a separate held-key map for sustain, and allocation guards.

## Takes

A take owns a prepared flat event buffer and its initial state. Equal-frame events
retain append order; silent windows still add duration. Invalid windows do not
mutate the take. Exhaustion freezes an incomplete take, preserving accepted records
and the duration of fully accepted windows; a partial final window may contain
records beyond that duration. Incomplete takes cannot play. A playback cursor pins
the take against replacement until explicitly released. Revisions reject stale
cursors after a new take begins.
