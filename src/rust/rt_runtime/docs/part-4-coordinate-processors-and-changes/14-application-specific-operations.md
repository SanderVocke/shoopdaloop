# 14. Application-specific operations

<!-- rust-source: ../../examples/pending_operations.rs -->

[Contents](../README.md) · [Previous: Planning processing intervals](13-planning-processing-intervals.md) · [Next: Staging and coordinating graph activation](15-staging-and-coordinating-graph-activation.md)

Typed operations let a domain distinguish receiving a request from completing its meaning. A processor operation returns immediately. A planner operation can retain a receipt token and resolve it at a later boundary while audio continues.

## Run the example

The [pending operations example](../../examples/pending_operations.rs) implements a tiny gain domain:

```sh
cargo run -p rt_runtime --example pending_operations
```

`Wait` changes the domain value at a selected future boundary; the processor's `Multiply` changes its gain immediately; `Cancel` and `Stop` provide bounded recovery. The example asserts separate command/domain results, four different frames, all three planner cancellation points, finite recovery capacity and distinct refusal types. Callbacks—including delivery, planning and receipt completion—are heap-guarded.

## 1. Define Copy operation/result families

A planner and the processors each have their own family. `Planner::Operation` and `Planner::Result` are Copy application types. A processor names its family as the second parameter of `Processor<L, Q>`, where `Q: ProcessorOperation` supplies the result type:

```rust
struct Multiply(f32);
impl ProcessorOperation for Multiply {
    type Result = f32;
}
```

The runtime carries the processor family as its third parameter, `GraphRuntime<P, L, Q>`; it defaults to `()` for processors without operations. `Action::Planner { planner, operation }` and `Action::Processor { node, operation }` carry the two families through `Command::Schedule`, so a processor operation cannot be addressed to the planner or the other way round. An application with several kinds of processor uses an enum for `Q`, just as it does for the processor type itself.

Additional payload bytes are separately bounded by preparation declarations (`PlannerSpec::operation_bytes` or `ConsumerAccess::operation_bytes`) and callback staging allowance. The enum's Rust size is not its payload-byte limit. Delivery borrows the payload; it does not transfer a long-lived reference into the planner.

The example's planner declares four payload bytes. Its Payload operation decodes an explicit little-endian `f32`, rejecting malformed/nonfinite values. Do not treat Copy as serialization, authentication, pointer transport, or permission to move a Box/Vec/trait object between independent Wasm instances.

## 2. Retain only the validated pending data

On Wait delivery, the planner validates the requested future boundary/value and checks its one prepared pending slot. It then copies the needed fields:

```rust
                self.pending = Some((context.token, until, value));
                Ok(Disposition::Pending)
```

The owning command has already completed Scheduled, but the execution reservation remains occupied. Do not retain `OperationContext`, its payload slice, or borrowed graph/resource views. Pending domain storage and a held normal transport command are different lifetimes: the latter may still be validating before any Scheduled completion exists.

At the selected frame, `plan` takes the pending token, publishes the value, and completes its receipt:

```rust
            let (token, _, value) = self.pending.take().unwrap();
            self.value[0] = value;
            context
                .receipts
                .complete(token, Ok(Reply::Applied(value)))?;
```

`ReceiptWriter` supplies the context's current frame. It checks runtime scope, token sequence and terminal state; there is no arbitrary backdating API. Preserve every retained token until explicitly completed or recovered under stopped-domain policy. If replacing pending work, resolve the displaced request rather than losing its reservation/outcome.

## 3. Distinguish requested, effective, delivered and terminal frames

The example first renders to frame 10, then submits a request for frame 1 whose domain boundary is 20:

| Frame | Meaning |
|---|---|
| Requested: 1 | Original host timestamp |
| Effective: 10 | Admission adjusted the late timestamp to callback ingress |
| Delivered: 10 | Planner received the typed operation |
| Terminal: 20 | Planner published the value and resolved the receipt |

The command completion reports `Scheduled { effective_frame: Frame(10), late_by: 9 }`. Rendering `[10,20)` still outputs 1.0 and produces no execution result. A zero call at 20 does nothing; the next nonempty callback settles that boundary and outputs 3.0:

```rust
    assert_eq!(execution.requested_frame, Frame(1));
    assert_eq!(execution.delivered_frame, Some(Frame(10)));
    assert_eq!(execution.actual_frame, Frame(20));
    assert_eq!(execution.late_by(), 9);
    assert_eq!(execution.result, Ok(()));
    assert_eq!(execution.operation_result, Some(Reply::Applied(3.0)));
```

`actual_frame` is the terminal frame, not necessarily delivery. `late_by()` measures delivery lateness, not the domain's ten-frame wait. Match both result streams by sequence.

The later processor Multiply operation runs at frame 25 and immediately returns `0.5`, reported in the execution's separate `processor_result` field. Its output changes from 3.0 to 1.5 at that frame. The longer pending protocol belongs to the planner, not the processor callback.

## Cancellation has three separate cuts

| Request state | Mechanism | Observable result |
|---|---|---|
| Before transport dequeue | `ticket.cancel()` | Command Cancelled if observed; no scheduled execution existed |
| Scheduled typed planner action, not delivered | `receipts.cancel_queued(sequence, value)` | Terminal execution with `delivered_frame: None` |
| Delivered pending domain work | Complete its retained token | Terminal execution retains its real delivery frame |

The example exercises each. Its Cancel operation first looks for the delivered pending token; otherwise it cancels a queued normal typed action. A duplicate completion of a still-terminal token returns `InvalidState`, not a second outcome. Scoped sequence checks also prevent an old token resolving a reused slot.

Parameter, MIDI and processor actions have no planner to cancel them. Submit `Command::Cancel { sequence }` on either lane to terminate one that has not been delivered yet; its execution reports `Err(Cancelled)` with `delivered_frame: None`. Cancelling a planner action this way refuses with `InvalidId`, as does cancelling work that was already delivered: use the planner's own cancellation for its operations.

The ticket flag is best effort only before its original dequeue. It cannot unschedule an accepted action, cancel a retained token, or undo a published domain effect. Parking normal validation for recovery does not create another cancellation observation; see [chapter 5](../part-2-control-a-running-graph/05-commands-completions-and-ownership.md).

## Recovery has an independent, finite receipt

This runtime has one normal action/result slot. The example leaves it occupied by a delivered Wait, then submits another normal action and a recovery Stop. The normal action completes Rejected(Full); Stop has a separately prepared recovery receipt.

Stop stores its recovery token in a separate fixed slot and returns Pending. At the same delivery frame, `plan` cancels the old pending request, publishes value zero and completes Stop **before returning any positive interval**. Output is zero from that frame. Completing later in the callback would violate the recovery contract.

Both results remain unread. Another recovery Stop is refused Full: independent does not mean unlimited. After the host consumes results, their credits recycle at the next callback boundary. Internal one-sample intervals continue without renewing or consuming those external reservations.

Classify only payload-free, nonfuture, finitely bounded operations through `Planner::recovery_safe`. This example classifies Cancel/Stop. Unsafe, future or payload-bearing recovery is rejected; a recovery request that remains pending beyond planning at its delivery frame faults. Classification is a trusted semantic contract, not an escape from no-heap/nonblocking rules.

Recovery operation contexts cannot activate graphs. Staging/releasing owners remains normal-lane work. A copied StageId in pending application data also does not own or lock a Ready candidate: revalidate at the actual cut in [chapter 15](15-staging-and-coordinating-graph-activation.md).

## Refusal is not one undifferentiated success flag

The example checks:

| Failure | Command completion | Execution |
|---|---|---|
| Stale planner identity | Rejected(InvalidId) | None: not admitted into schedule |
| Three-byte payload for four-byte decode | Scheduled | Runtime `Err(InvalidEvent)`, no domain value |
| Nonfinite Multiply delivered to the processor | Scheduled | Runtime `Err(InvalidEvent)`, no processor value |
| Explicit domain Refuse | Scheduled | Runtime `Ok(())`, domain `Some(Reply::Refused)` |

An identity can also become stale after admission, as the removed processor example in [chapter 6](../part-2-control-a-running-graph/06-routing-and-live-graph-edits.md) demonstrates. Generic `Ok(())` means successful runtime delivery/completion, not necessarily application publication.

Drain owning completions and execution results off-render, retaining payload owners on refusal when retrying. The example extracts planner/resources only after all pending/recovery slots are empty. A faulted domain may need a retained cleanup ledger instead; stopped extraction does not invent terminal receipts, roll back changes or authorize replay.

[Next: Staging and coordinating graph activation](15-staging-and-coordinating-graph-activation.md)
