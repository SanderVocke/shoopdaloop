# Indexed MIDI operations

`midi::{MidiRenderer, MidiCapture, Playback, RenderReport}` provide prepared operations, not another loop clock or the coordinating loop planner. Positions and discontinuities come from the resolver. Their large channel projections live in fixed, pre-touched boxes allocated during preparation so embedding processors in runtime command/ownership types does not multiply large inline payloads on the stack. Callback updates overwrite plain state in those boxes; they never replace or free the boxes.

## Playback

Prepare with the maximum interval size and the resource store's maximum packet size. Supply an empty, prepared output `MidiBuffer`. The interval must be non-wrapping and fit the pinned MIDI snapshot's duration. Zero frames do nothing.

On first use or an authoritative discontinuity, the renderer performs bounded indexed state positioning, cleans up its prior owned projection, restores state strictly before the selected offset, then emits original events at/after that offset. Equal-time ordering and complete packet bytes—including SysEx and note-on-zero—are preserved. Events at the interval end remain for the next interval. Trailing silence is retained by explicit resource duration.

A scoped cursor and one lookahead event survive contiguous intervals. Their cached end position is a consistency check, not a timeline authority: changing source or position without a discontinuity is refused. First use establishes the supplied state even if the consumer is newly bound to an already-running loop. Source/read faults invalidate the cursor cache; enclosing runtime faults do not authorize callback replay. The caller retains/releases snapshot ownership.

`state()` is the intended source projection, not device acknowledgement. Output overflow does **not** abandon the remaining original events: all are read, validated and applied, and the media cursor advances to the supplied interval end. `RenderReport::output_error` records the first output failure; later output attempts are counted but not retried. The processor must finish other required audio/capture work and then propagate that error to runtime recovery. `Ok(report)` with an output error is not lossless delivery. The renderer does not replay missed events on the next host cut; restoration occurs at the next supplied discontinuity or fresh binding. Parameter-history limitations from projected restoration remain explicit in the report.

Selecting no source cleans up once and clears the projection/cursor, even if the previous cursor was invalidated. Repeated stopped intervals emit nothing. Sufficient output capacity—including cleanup/restoration plus original events—is required for lossless reference comparisons. The runtime's global recovery path remains separate and its callback allowance must not be refreshed by internal intervals.

## Input tracking and capture

Prepare `MidiCapture` with matching interval/event/byte bounds. `process(..., capture: None)` still tracks connected input while stopped. Invalid input ranges/capacity leave its projection unchanged. Copy `state()` at the capture-start boundary before observing the current interval: that is the initial state of the new take, not a history scan or prerecording.

With a reserved `(CaptureIntervalId, member)`, the operation additionally stages the original input buffer. The planner still owns activation, group commit and freeze. Valid input remains observed even if later staging fails; a staging refusal is not retroactive input rollback. The reserved input buffer's recovery form is handled separately from ordinary event/byte limits, so all 32 CC120/CC123 packets remain visible even to a small ordinary-input configuration. Capture-group producer declarations must provision for that recovery data as well as normal output.

## Verification boundary

The package tests check an independent absolute packet oracle across three partitions, 257-byte SysEx across pages, equal-time records and trailing silence; exact seek/wrap/replacement cleanup→restoration→event order; stopped input tracking and immediately readable frozen MIDI with initial state; scope/range/continuity/capacity/zero-frame refusals; and 128 one-sample intervals driven by the production resolver with no diagnostic journal.

A real generic-runtime graph embeds these operations in a MIDI producer and a downstream input tracker. Deliberate output exhaustion advances both original notes in the producer, delivers reset packets to clear a seeded downstream held note, preserves audio samples, and drains all 32 host recovery packets. This is a focused generic-runtime recovery proof. The [loop planner](runtime.md) separately supplies planner/consumer integration and coherent mixed-media export; neither is an actual loop AudioWorklet.

The first native attempt exposed a stack overflow with large inline projections embedded in generic runtime processor/command types. Fixed prepared boxes reduce that footprint without raising stack limits, changing workloads, or weakening heap guards. The exact overflowing stack frame was not captured; the failure log is retained, and the revised representation is tested in native/Wasm/Miri configurations.

The planner's [coordinated_loop](../examples/coordinated_loop.rs) demonstrates mixed audio/MIDI record/playback/FX/capture, settled outcomes and immutable export including initial held-note state. Its callbacks have no allocator guard of their own; guarded integration tests provide separate allocation evidence. Historical test reports keep their original platform/source scope. Independent loop hosts and newer loop performance/deadline evidence are not supplied by these examples or package-Wasm tests.
