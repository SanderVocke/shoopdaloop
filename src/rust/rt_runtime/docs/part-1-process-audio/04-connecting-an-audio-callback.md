# 4. Connecting an audio callback

<!-- rust-source: ../../examples/callback_adapter.rs -->

[Contents](../README.md) · [Previous: Writing processors](03-writing-processors.md) · [Next: Commands, completions, and ownership](../part-2-control-a-running-graph/05-commands-completions-and-ownership.md)

Keep the prepared `GraphRuntime` with the callback owner. Pass borrowed host buffers to `process` and deliver the resulting outputs through your device adapter. Preallocate any channel conversion or interleaving scratch before starting callbacks.

The [callback adapter example](../../examples/callback_adapter.rs) exercises this boundary offline, including validation and a deliberate processor fault:

```sh
cargo run -p rt_runtime --example callback_adapter
```

## Adapt the host buffers

For the example's one input/output and no MIDI ports, the adapter is:

```rust
fn callback(
    runtime: &mut GraphRuntime<FailOnce>,
    input: &[f32],
    output: &mut [f32],
    frames: usize,
) -> Result<(), Error> {
    runtime.process(
        AudioBlock {
            frame: runtime.next_frame(),
            frames,
            inputs: &[input],
            outputs: &mut [output],
        },
        MidiBlock {
            inputs: &[],
            outputs: &mut [],
        },
    )
}
```

The temporary port arrays borrow existing storage. For more channels, supply one planar `f32` slice per channel. Input/output port-array counts must exactly match the configuration, and each channel slice must contain at least `frames` samples.

`frames` may vary up to `max_frames`. The runtime processes and clears only the selected prefix of each output channel; it does not overwrite a larger buffer's unused tail. MIDI ports likewise require exact counts and prepared buffers, covered in [Processing and scheduling MIDI](../part-2-control-a-running-graph/08-processing-and-scheduling-midi.md).

Use `next_frame()` for the engine frame. The engine clock starts at zero and advances contiguously; device timestamps or wall-clock times need their own host mapping.

Each valid nonempty callback validates/advances the clock and snapshots queued work once. Due actions and an optional planner can split it into internal intervals without renewing callback allowances; see [Planning processing intervals](../part-4-coordinate-processors-and-changes/13-planning-processing-intervals.md) and [Capacity and work budgets](../part-5-configure-and-operate-the-runtime/17-capacity-work-budgets-and-diagnostics.md).

The callback covers a half-open frame range. An action at its end affects the next nonempty callback's first sample/event; planner `finish` does not settle that next boundary.

## Invalid and zero-length blocks

Invalid shapes and noncontiguous frames return errors before clock or control-work progress. Output clearing on these paths is bounded by the configured limits and available buffers.

The example supplies an input slice shorter than the requested four frames:

```rust
    let invalid_shape = assert_no_heap(|| callback(&mut runtime, &input[..2], &mut output, 4));
    assert_eq!(invalid_shape, Err(Error::InvalidShape));
    assert_eq!(runtime.next_frame(), Frame(0));
    assert_eq!(output[..4], [0.0; 4]);
    assert_eq!(output[4..], [9.0; 4]);
```

A zero-frame call is a no-op: no shape validation, clock advance, output clearing, or command progress. A host that needs queued changes to settle must keep delivering nonempty callbacks.

## Processor faults do not roll back

The example's `FailOnce` deliberately changes its internal state and returns an error on its first processing call. The runtime clears the host output, but its clock has already advanced:

```rust
    let processor_error = assert_no_heap(|| callback(&mut runtime, &input, &mut output, 4));
    assert_eq!(processor_error, Err(Error::InvalidParameter));
    assert_eq!(runtime.next_frame(), Frame(4));
    assert_eq!(output[..4], [0.0; 4]);
    assert_eq!(
        runtime.diagnostics().render_error,
        Some(Error::InvalidParameter)
    );
```

This `FailOnce`/`NoPlanner` example permits continuation at `next_frame()` rather than replaying frame zero. Its following two-frame callback succeeds:

```rust
    output.fill(9.0);
    assert_no_heap(|| callback(&mut runtime, &input, &mut output, 2))?;
    assert_eq!(runtime.next_frame(), Frame(6));
    assert_eq!(output[..2], [1.0; 2]);
    assert_eq!(output[2..], [9.0; 6]);
    assert_eq!(runtime.diagnostics().render_error, None);
```

That continuation is not a universal fault policy. A domain may already have published graph/program/media state and retain an unfinished committed interval. Stop callbacks and use its explicit off-render extraction/cleanup in that case; do not fake `finish`, replay samples or roll back ownership. See [shutdown and ownership](../part-5-configure-and-operate-the-runtime/19-stopping-disconnecting-and-reclaiming-ownership.md).

Choose a host policy for reporting faults or stopping the stream; do not log or construct owning error reports through an unbounded callback path. MIDI output overflow uses a distinct recovery path rather than treating ordinary `Full` as a general processor fault.

## Start and stop

Prepare runtime, processor and host buffers outside callbacks, then transfer the runtime to its render owner. Keep the control endpoint elsewhere if needed. Read live-runtime diagnostics from that owner between calls, or publish bounded copies to another thread.

Stop and join callbacks before dropping the runtime. If accepted commands or resources require graceful cleanup, settle them while callbacks still run; [Stopping, disconnecting, and reclaiming ownership](../part-5-configure-and-operate-the-runtime/19-stopping-disconnecting-and-reclaiming-ownership.md) covers that sequence.

The crate supplies neither a device API nor an AudioWorklet adapter. A browser host must also bound its messaging and JavaScript-side work; Rust's prepared buffers do not remove those host responsibilities.

[Next: Commands, completions, and ownership](../part-2-control-a-running-graph/05-commands-completions-and-ownership.md)
