# Independent prepared example DSP

A portable deterministic fixture for engine composition, not a plugin host. Its
only production dependency is `rt_runtime`; it has no loop, old-engine, backend,
UI, device or browser dependency.

## Signal and preset semantics

Audio inputs are live L/R then playback L/R; outputs are stereo. A prepared preset
selects per-output live/playback coefficients, optional stereo source exchange
and a fixed delay. MIDI has two input ports and one output: merge by frame offset,
then input-port index, preserving each port's packet order. MIDI is not delayed
by the audio delay. Capacities are checked; oversize packets/events or full output
return a compact error, never grow storage. A `GraphRuntime` host translates MIDI
`Full` into overflow diagnostics and reserved recovery while audio continues.
Drain its separate MIDI recovery sink once per output/callback; ordinary MIDI
stays suppressed while recovery is pending.

`prepare` validates limits, dense generation-checked preset slots, finite
coefficients and the initial preset. It allocates and pre-touches the preset bank
and delay ring off-render. Initial revision is 1.
A successful selection changes the whole prepared configuration and resets delay
history to silence at its exact sample; selecting the same preset is still a new
revision/reset. This is a typed operation with a terminal result, not a float ID.

## Control and hosting

`prepare` returns `Prepared { processor, meters }`. Place the processor in a runtime
graph, wrapped in the host's node type if it has other processors. `Dsp` implements
`Processor<L, Operation>` for every planner `L`: its typed operation family is its
own, independent of any planner's vocabulary. A host schedules a selection as a
processor action at the frame where it must apply:

```rust
Command::Schedule {
    at,
    action: Action::Processor {
        node,
        operation: Operation::SelectPreparedPreset {
            preset,
            expected_revision,
        },
    },
    payload: Box::new([]),
}
```

The runtime delivers the operation immediately before sample `at`, inside any
loop-selected interval, and reports the outcome in the execution's
`processor_result`:

- `Ok(Receipt)` names the requested and actual frame, the previous and new revision
  and the preset.
- `Err(Refusal)` changes nothing: `Late` when delivery came after the requested
  frame (never clamped), `InvalidPreset`, `StaleRevision` or `RevisionExhausted`.

Admission, cancellation and disconnect are runtime behaviour. A ticket cancelled
before dequeue completes `Cancelled` with no execution; `Command::Cancel` terminates
a scheduled selection that has not been delivered yet with `Err(Cancelled)`. A fault
in an earlier interval of the same callback leaves the selection undelivered; one
delivered before a fault keeps its receipt. Destroy the processor only after
callbacks have stopped.

## Peak meters

`meters.read_peaks` independently reads/resets each channel's maximum published
magnitude. Publications occur at processing-interval ends. A racing publication
belongs to this or a later read; stereo fields are not a coherent transaction.
NaN samples do not contribute; infinite magnitudes saturate at `f32::MAX`, without
changing audio samples. Meter reads never submit reliable commands or affect DSP.
