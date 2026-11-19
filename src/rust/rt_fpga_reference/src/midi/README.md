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
The source state remains fixed throughout an emission. Recovery sends All Sound Off and All Notes Off on each channel and retries the
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

## Pipeline integration

Prepare `Engine::prepare(loop_id, record_event_capacity, input_events_per_quantum)`
and attach it while the bound loop is stopped. Pass it, callback-relative input
events and an accepting/refusing output sink to `Pipeline::process_midi_block`.
The engine consumes the state processor's settled loop record after audio jobs;
it does not advance time or resolve loop transitions. When SP holds the loop at
its end awaiting synchronization, MIDI releases held output and stays silent
until SP resumes playback at the beginning. Input remains observed
while stopped and playing, but is captured only during recording and is not
monitored to output. Playback emits the saved take.

Input must be ordered, with offsets inside the callback. A boundary event belongs
to the next callback. Invalid input, excess input density, stale loop identities,
discontinuous calls and unsupported seek plans are refused before processing.
Do not interleave calls that bypass MIDI processing while this engine is attached.
`ProcessError::Pipeline` retains the audio pipeline's fault behavior; MIDI
capacity failures instead appear in `Report::error` and do not interrupt audio.
`Report::work` contains cumulative saturating work counters.

Ordinary playback or restoration refusal abandons overdue events and enters
reserved recovery. Stop/repeat cleanup instead retains its field cursor and a
frozen output projection until all cleanup messages are accepted;
`cleanup_pending()` distinguishes this from emergency recovery.
The 32 all-sound/all-notes-off messages retain acknowledged progress across calls.
The engine does not replay old note events late. If playback continues after
recovery, ordinary output waits for the next loop beginning, where initial state
is restored; the skipped windows report `Busy`. Stopping releases the take after
recovery. On a fatal audio fault, callers can explicitly use `drain_recovery`
without advancing the failed pipeline, then replace both pipeline and MIDI engine.
Partially restored output is never presented as a complete musical state.

Allocation is confined to preparation. An input quantum admits at most its
configured event limit; a take admits at most its record limit. Boundary emission
visits at most 10,433 restoration fields or 2,096 cleanup fields; each field can
emit at most one message. Step counts describe software work, not hardware cycles.
Copying a boundary state is a fixed-size operation; ordinary state
updates do not copy a whole state bank. `prepared_bytes` reports engine storage,
including its event allocation, excluding the caller's audio and output buffers.

Run the focused suite with:

```sh
cargo test --locked -p rt_fpga_reference --all-features
cargo run --locked --release -p rt_fpga_reference --example midi_profile
```

The example times 8,192 events at 256 events per 128-frame window through capture,
cursor reading and state updates. Preparation and input construction are excluded;
take reset, capture, finalization and reads are included. It also prints state
work, boundary work and memory use. This is a CPU microbenchmark on a shared host,
not a complete callback measurement or a hardware deadline result.

Local validation and the sample resource profile are recorded in
[the milestone evidence](../../../../../docs/fpga/midi-reference-milestone.md).
