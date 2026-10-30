# 5. Commands, completions, and ownership

<!-- rust-source: ../../examples/owning_commands.rs -->

[Contents](../README.md) · [Previous: Connecting an audio callback](../part-1-process-audio/04-connecting-an-audio-callback.md) · [Next: Routing and live graph edits](06-routing-and-live-graph-edits.md)

A `Command` transfers an owned request to the render side. Its **completion** returns the request, its outcome, and any displaced owner to the control side. Keep collecting completions even when no user interface needs their status: unread returns occupy admission slots.

```text
control                         render
prepare request
submit -----------------------> dequeue; apply or retain work
          callback(s) continue
try_completion <--------------- request + outcome + retired owner
reuse or dispose off-render
```

## Run the example

The [owning commands example](../../examples/owning_commands.rs) starts with the gain graph from [Your first audio graph](../part-1-process-audio/02-your-first-audio-graph.md):

```sh
cargo run -p rt_runtime --example owning_commands
```

It checks refusal without losing a plan, recovery ahead of incremental validation, both sides of the cancellation cut, and separate scheduling/execution results. Callback processing is heap-guarded. Submission, compilation, polling and destruction happen outside that guard; this is not an allocation-free submission example.

## 1. Submit an owner, retain its sequence

Compile plans and prepare processors or resource buffers off-render. This request replaces the active graph with an equivalent one:

```rust
    let first = control
        // Submit an owning request: the runtime returns it with its outcome and any retired object.
        .submit(
            Lane::Normal,
            Command::commit(runtime.epoch(), compile(runtime.catalog(), &description)?),
        )
        .map_err(|refusal| refusal.error)?;
```

`first.sequence()` identifies the admitted command. The offline example alternates control work with calls to its guarded `render` helper. A live host runs these sides independently; do not wait for a completion inside the audio callback.

Ordinary `submit` may allocate a cancellation flag. [Submitting without allocation](../part-5-configure-and-operate-the-runtime/18-submitting-without-allocation.md) covers `prepare_submissions`/`submit_prepared`, including flags retained by old tickets or completions.

## 2. Refusal returns the request

The example has two normal admission slots. After filling them, another submission returns `SubmitError { error: Error::Full, value }`. Here `value` still owns the candidate plan:

```rust
    assert_eq!(refused.error, Error::Full);
    assert!(
        matches!(&refused.value, Command::Commit { plan: Some(plan), .. } if plan.contains(node))
    );
    drop(refused);
```

That request never entered the queue. Keep it for a deliberate retry or destroy it off-render, as here. Reconsider its expected epoch or timestamp before retrying. The example's `map_err` paths also dispose of refused requests on the control side; a host that must retain them should handle `value` explicitly.

A request that enters the queue may still complete with `Outcome::Rejected(error)` or `Outcome::Cancelled`. It still has an owning return. Do not infer success from submission alone.

## 3. Receive the changed request and retirement

A successful Commit takes the candidate out of the request and returns the former active plan separately:

```rust
    let completion = control.try_completion().unwrap();
    assert_eq!(completion.sequence, first.sequence());
    assert_eq!(completion.outcome, Outcome::Applied);
    assert!(matches!(
        &completion.request,
        Command::Commit { plan: None, .. }
    ));
    assert!(matches!(&completion.retired, Some(Retired::Plan(_))));
    drop(completion);
```

`try_completion()` is nonblocking; `None` means no return is available yet. The immediate unwrap is specific to this finite offline example, which has already driven enough callbacks. With multiple requests, match `sequence`, then inspect `outcome`, the returned `request`, and `retired` together.

Removing an inactive processor similarly returns `Retired::Processor`. A rejected Commit retains its candidate in `request`. A successful `StageGraph` instead returns `plan: None, id: Some(id)`: the candidate is Ready, **not active**. `ReleaseGraph` recovers its unused or displaced plan. [Chapter 15](../part-4-coordinate-processors-and-changes/15-staging-and-coordinating-graph-activation.md) covers that separate protocol.

Receiving a completion releases its command admission credit, even if you retain the returned value afterward. Until it is read, the credit remains occupied even when the render-side work has finished. The example leaves all returns unread for 64 callbacks and verifies that normal admission is still Full.

## Recovery can pass held normal work

The example limits validation to one step per callback. The first Commit remains held after dequeue; a later normal Remove must wait behind it. A recovery-lane `ResetMidi` submitted afterward is observed at the next callback snapshot and returns first, then the held Commit resumes before the Remove:

```text
normal:   held Commit → later Remove
recovery: ResetMidi

returns:  ResetMidi → original Commit → later Remove
```

Recovery temporarily parks the held normal request inline, retaining its validation progress and owning return. It is not a global FIFO across lanes, and it has independent admission — not unlimited processing. The full rules (cancellation observation, allowance costs, fairness limits) are in [Execution rules](../reference/execution-rules.md).

## Scheduling has another result stream

A Schedule command completes once admitted; its action may execute later. The example changes gain two samples into a callback, then checks a separate execution receipt:

```rust
    let execution = control.try_execution().unwrap();
    assert_eq!(execution.sequence, scheduled.sequence());
    assert_eq!(execution.actual_frame, at);
    assert_eq!(execution.result, Ok(()));
    assert_eq!(output, [0.5, 0.5, 0.25, 0.25, 0.25, 0.25, 0.25, 0.25]);
```

`Outcome::Scheduled` is admission, not the processor's result. Read both streams. Unread/future execution reservations occupy scheduler capacity independently of command slots; consuming an execution result makes its credit reusable at a later callback boundary. `control.outstanding()` counts commands, not those execution reservations.

A typed planner operation can remain pending after delivery and report a domain outcome later still; [Application-specific operations](../part-4-coordinate-processors-and-changes/14-application-specific-operations.md) separates those frames and results. Telemetry is lossy Copy diagnostics, never a substitute for either acknowledgement.

## Owning resource replies and shutdown

A `Command::Resource` returns its `ResourceCommand`, including a reply and the post-operation request. For a Read, recover the advanced cursor and buffer from that request, then inspect the reply's written/EOF information as well as the transport outcome. Budget refusal is not an automatic retry. [Loading and reading resources](../part-3-work-with-stored-media/09-loading-and-reading-resources.md) demonstrates the bytes and ownership.

Resource handles inside application state need explicit lease management: copying one acquires nothing, and dropping a plain handle releases nothing.

This example has no future work left at teardown and drops both endpoints outside rendering. A live host must first stop/join callbacks. Disconnecting or extracting stopped state does not settle pending operations, invent terminal results, or send MIDI cleanup; see [Stopping, disconnecting, and reclaiming ownership](../part-5-configure-and-operate-the-runtime/19-stopping-disconnecting-and-reclaiming-ownership.md).

[Next: Routing and live graph edits](06-routing-and-live-graph-edits.md)
