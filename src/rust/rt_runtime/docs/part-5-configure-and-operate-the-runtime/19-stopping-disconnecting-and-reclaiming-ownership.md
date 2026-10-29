# 19. Stopping, disconnecting, and reclaiming ownership

<!-- rust-source: ../../examples/shutdown_owners.rs -->

[Contents](../README.md) · [Previous: Submitting without allocation](18-submitting-without-allocation.md)

Choose whether requests must settle or may be abandoned. Then stop callbacks before consuming render-owned storage. Disconnect, cancellation, musical Stop, resource cleanup and destruction are different actions.

## Run the example

The [shutdown owners example](../../examples/shutdown_owners.rs) applies both policies to the same small fixture:

```sh
cargo run -p rt_runtime --example shutdown_owners
```

A gain graph runs beside an old snapshot and an unfinished replacement upload. A typed request is scheduled for frame 100. The graceful branch explicitly cancels it, collects both outcomes, publishes an empty graph and retires the processor. The abandonment branch leaves the future request, an unread retired graph and a queued owning Write when callbacks stop.

Both preserve the real graph cut and old audio bytes, recover every declared store pool and dispose of the processor exactly once off-render. Callbacks and the required snapshot read are heap-guarded. It is an offline synchronous example: a threaded host must additionally stop/join its callback owner before the extraction shown here.

## 1. Settle known work instead of rendering until idle

Stop admitting new application work; allow only the explicit shutdown protocol. Decide how future scheduled actions and delivered Pending requests terminate. A stopped domain's countdown need not ever reach its target, so an unconditional render-until-idle or blocking flush is not a shutdown policy.

The example first admits Wait at 100, renders one sample, and reads its Scheduled owning completion. No execution result exists yet. A zero callback neither advances time nor flushes that request.

Its recovery Cancel uses `ReceiptWriter::cancel_queued` on the undelivered typed sequence. Both results terminate at the current frame, 1:

```rust
                assert_eq!(execution.delivered_frame, None);
                assert_eq!(execution.operation_result, Some(Reply::Cancelled));
```

The cancellation's own result records delivery at 1 and `Reply::Done`. Cancellation can terminate future work before its requested frame; it does not pretend that Wait ran. Both results must be consumed. For delivered Pending work, resolve the retained token instead; [chapter 14](../part-4-coordinate-processors-and-changes/14-application-specific-operations.md) demonstrates that separate cut.

Ticket cancellation is only a pre-original-dequeue observation. It cannot universally unschedule admitted work, cancel a parked validator anew or undo a published effect. Recovery remains finite and must settle at its delivery boundary before positive samples.

## 2. Collect owning returns while callbacks still run

The graceful branch publishes an empty graph through ordinary Commit. Output becomes zero and the real epoch advances. Its completion returns the displaced graph as `Retired::Plan`; disposing of it does not undo publication.

Only after exclusion can it remove the inactive processor. The returned completion owns `Retired::Processor`. A Drop counter is still zero after the guarded callback and becomes one only when that completion is destroyed off-render. The processor's Box storage is not freed by processing.

Freeze/stop active captures at their valid boundaries and perform any required downstream cleanup before a device is stopped. Then collect reliable command and execution results. Lossy telemetry cannot substitute for those outcomes. The example has no active capture/device; its unfinished upload remains a deliberate stopped-store cleanup obligation.

## 3. Abandon explicitly after the callback owner stops

The second branch still publishes the empty graph, but retains its unread owning completion and queues a Write without another callback. It stops before frame 100. No imaginary execution/acknowledgement is generated for either abandoned request.

After consuming the runtime, the host can still read the **already existing** Applied completion and dispose of its retired graph. The queued Write has no terminal result; its remaining admission credit is abandoned with the endpoint, whose queued payload is destroyed off-render. A new submit returns Disconnected with its original value.

Do not drop the render receiver merely to signal a live callback to stop. Queued commands, a current recovery command, a parked normal command and unread returns may all own allocations. The callback owner must stop first; stopped endpoint/runtime destruction can then dispose of those owners safely. Eligible recovery need not discard a held normal owner to proceed.

Abandonment is not rollback. The empty graph and advanced frame/epoch really occurred. Metadata destruction neither reactivates the old graph nor synthesizes notes, terminal receipts or a final processing interval.

## 4. Retain the ledger needed to reuse the store

The example chooses:

```rust
    let stopped_frame = runtime.next_frame();
    let (mut ledger, mut store) = runtime.into_planner_and_resources();
```

`into_resources()` would preserve the store but drop the planner and other owners. It cannot discover leases/uploads/group promises represented only by plain IDs hidden in them. `into_planner_and_resources()` keeps the planner ledger; it still destroys other runtime owners off-render. A processor with hidden leases needs its own returned/external ledger too.

Extraction settles no requests and sends no device output. The example checks Wait was never delivered, the actual stopped frame is below 100, and the old snapshot still reads 0.5. Graph publication did not publish the unfinished replacement upload or change the resource revision.

It cancels that incomplete upload, releases the acquired snapshot, removes active content and reclaims retired pages/versions. Both branches recover eight pages, four version slots and four snapshot slots, with zero reserved pages and an empty live-lease/upload ledger. Reclamation is bounded maintenance, not an automatic consequence of dropping a copied handle.

## Match each owner to its cleanup path

| Owner/state | Graceful path or explicit stopped policy |
|---|---|
| Queued/current/parked commands and unread owning returns | Receive/dispose their real completions, or abandon them to stopped endpoint/runtime destruction. Retain refused values. |
| Future typed actions and delivered Pending tokens | Cancel queued work or complete retained tokens while running; otherwise declare abandonment without fabricated outcomes. |
| Tickets/prepared flag references | Receiving returns releases admission; flag reuse waits for external references. Final last-owner destruction stays off-render. |
| Uploads | Finish/commit intentionally or cancel; release remaining reservations and reclaim retired data. |
| Acquired snapshots/playback/export pins | Explicitly release each acquired lease once. Copying IDs/views acquires nothing; Drop releases nothing. |
| Prepared group interval/take | Cancel interval before take. Active groups instead freeze their actual common prefix; release final leases and Idle claims. See [chapter 16](../part-4-coordinate-processors-and-changes/16-recording-resources-together.md). |
| Ready candidates / Taken retired graphs | Normal-lane ReleaseGraph returns the appropriate owner; Taken can have none after first activation into an empty graph. Views/pending StageIds do not own or lock candidates. See [chapter 15](../part-4-coordinate-processors-and-changes/15-staging-and-coordinating-graph-activation.md). |
| Active/retired resource content | Remove/reclaim for reuse, or intentionally destroy the whole stopped store. Release does not instantly recover all pages. |

Graph retirement does not clean processors, resource claims or held notes. Loop topology/node-pair/source/media owners have additional planner-specific release paths, not generic graph metadata semantics.

## A fault is not a final successful interval

A processing error may follow time advance, graph/domain publication or partial staging. Inspect the actual stopped state; never replay frames, call a fake `finish`, backdate a receipt or invent a successful Stop. Continuation in the [callback adapter](../part-1-process-audio/04-connecting-an-audio-callback.md) is specific to its FailOnce/NoPlanner policy, not a generic domain guarantee.

The separate loop library's planner supplies `LoopPlanner::cleanup`, applied to the planner and store returned by `into_planner_and_resources()`, to permanently quiesce its resolver. That policy handles unpublished reservations, actual captured prefixes, leases and claims without rollback/replay; repeating successful cleanup is safe. It is not an API on generic GraphRuntime, permission to resume the consumed runtime, or reconciliation of arbitrary downstream synthesizer/device state.

Once a device is stopped, destroying Rust owners cannot send it reset messages. Keep live musical cleanup, stopped storage recovery and host device disposal as separate responsibilities.

[Reference](../reference/README.md) · [Contents](../README.md)
