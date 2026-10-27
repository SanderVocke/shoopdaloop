# 7. Changing parameters at an exact sample

Previous guide; the current [chapter 7](../docs/part-2-control-a-running-graph/07-changing-parameters-at-an-exact-sample.md) retains this teaching path in the nineteen-chapter book.

[Contents](../docs/README.md) · [Graph setup](getting-started.md) · [MIDI](scheduling-midi.md#midi-buffers-and-ordering)

A **scheduled action** specifies a change and its absolute sample frame. The runtime splits processing at that frame and applies the change between processor calls.

Here, a gain change at offset 64 divides a 128-frame host block into two processor calls:

```text
One host callback: 128 samples

Sample offset:       0                         64                        128
                     |-------------------------|-------------------------|
Gain:                           0.5                       0.25
Processor calls:     process 64 samples         process 64 samples
                                               ^
                                               set_parameter runs here
```

## Run the example

The snippets below come from the [gain graph example](../examples/gain_graph.rs), which uses the graph from [Getting started](getting-started.md). Run it from the repository root:

```sh
cargo run -p rt_runtime --example gain_graph
```

It renders offline, asserts the timing and output samples, and shuts down. No audio device is needed.

## 1. Give the processor a parameter

The gain processor stores its gain in `self.0` and accepts parameter ID `0` through its `Processor` implementation:

```rust
    fn set_parameter(&mut self, parameter: u32, value: f32) -> Result<(), Error> {
        if parameter != 0 || !value.is_finite() {
            return Err(Error::InvalidParameter);
        }
        self.0 = value;
        Ok(())
    }
```

Processors define their own parameter IDs, units, ranges, and smoothing. The default `set_parameter` implementation returns `Error::InvalidParameter`.

`set_parameter` runs on the render side under the same restrictions as `process`: no heap allocation or freeing, locks, waits, or external I/O.

## 2. Choose an absolute frame

Timestamps count sample frames from the runtime's frame zero. The example targets 64 frames after the start of its next callback:

```rust
    let at = Frame(runtime.next_frame().0 + 64);
```

If `next_frame()` is `Frame(1024)`, the change runs immediately before sample frame 1088. A target at the block's end belongs to the following callback.

The offline example reads `next_frame()` between callbacks. A live host should communicate frame information to the control side and submit with enough lead time.

## 3. Submit the change

The example sends this command using `Command` from `rt_runtime::executor` and `Action` from `rt_runtime::schedule`:

```rust
    let completion = apply(
        &mut control,
        &mut runtime,
        Command::Schedule {
            at,
            action: Action::Parameter {
                node,
                parameter: 0,
                value: 0.25,
            },
            payload: Box::default(),
        },
        &mut output,
    );
```

`node` identifies the gain processor; `payload` is empty because parameter changes need no extra bytes.

`apply` is an example helper, not a runtime API. It submits on `Lane::Normal`, renders blocks into `output`, and polls `control.try_completion()` until the command completes. In a live host, submit and collect results on the control side while callbacks run independently. Ordinary `submit` may allocate.

See [Commands, results, and backpressure](integration.md#commands-results-and-backpressure) for the submission API.

## 4. Check both acceptance and execution

Scheduling reports acceptance and execution separately:

```text
Control side                     Render side

submit Schedule ---------------> receive request
                                 reserve room for action and result
try_completion() <-------------- report Scheduled
                                 ... process audio until target frame ...
                                 call set_parameter
try_execution() <--------------- report its result
```

The command completion confirms acceptance into the schedule:

```rust
    assert!(matches!(completion.outcome, Outcome::Scheduled { .. }));
    drop(completion);
```

`Outcome::Scheduled` includes `effective_frame` and `late_by`, but does not confirm that the processor accepted the parameter. Check the execution result for that:

```rust
    let execution = control.try_execution().unwrap();
    assert_eq!(execution.actual_frame, at);
    assert_eq!(execution.result, Ok(()));
```

These assertions verify the delivery frame and `set_parameter`'s return value. The immediate `unwrap()` works because the helper has already rendered the target block. For future actions, `try_execution()` returns `None` until a result is available. With several requests in flight, match results by sequence number.

Collect both result streams: unread completions occupy command slots, and unread execution results occupy scheduler slots. Scheduler slots become reusable at a callback boundary after their results are read.

## 5. Check the samples

With an input of `1.0`, the output should contain 64 samples at each gain:

```rust
    assert_eq!(output[..64], [0.5; 64]);
    assert_eq!(output[64..], [0.25; 64]);
```

The runtime handles the split; the processor uses the supplied `block.frames` and slice lengths rather than assuming the host's block size.

An abrupt gain change may click. Exact scheduling provides timing, not smoothing; implement a ramp in the processor if needed.

## Late requests and failures

### Late requests

If a request is admitted after its target frame, it is scheduled at the current callback's start frame. Lateness is reported in `Outcome::Scheduled { effective_frame, late_by }` and later through `execution.actual_frame` and `execution.late_by()`.

Queueing and graph validation can delay admission. Choose lead time based on the host and expected queued work.

### There is no available slot

There are two opportunities for refusal:

1. `submit` can refuse the command before it enters the queue. The returned error includes the original command, so you can keep it for a later attempt.
2. A queued command can complete with `Outcome::Rejected(Error::Full)` if the scheduler has no available action/result slot.

Collect outstanding results before retrying, and reconsider the timestamp. Depending on the application, replacing the request with a newer value or reporting failure may be preferable.

The normal scheduler has `min(scheduled_events, budgets.events)` action/result slots: 64 with the defaults. Future actions and unread execution results both occupy slots, so use a limited lookahead window for long sequences. See [Capacity and performance](performance.md) for sizing.

### The processor rejects the change

An unsupported parameter ID can be scheduled successfully, then produce `Err(Error::InvalidParameter)` in its execution result.

Removing the target before execution also produces an execution failure; its `NodeId` will not address a replacement in the same slot. Installed but inactive processors can receive parameter changes.

### Cancellation

A submission ticket's `cancel()` is best effort before dequeue. It cannot remove an action already accepted into the scheduler. The completion, not the ticket flag, confirms cancellation.

[Next: MIDI](scheduling-midi.md#midi-buffers-and-ordering)
