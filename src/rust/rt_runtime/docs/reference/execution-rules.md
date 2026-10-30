# Execution rules

[Book](../README.md) · [Reference](README.md)

These rules describe generic GraphRuntime. Musical precedence, source/node-pair/media ownership and loop cleanup belong to the separate loop library's planner.

## Callback envelope and half-open time

A zero-frame call returns immediately without shape/frame validation, ingress, recycling, planning, resource work or output mutation. It is not a flush. A nonempty call validates host shapes/packet bounds and the expected absolute frame; bounded output clearing can precede refusal. Validated clock advancement occurs before processing, so a later fault can leave the next-frame clock advanced.

For each valid nonempty callback:

1. Recycle execution credits and reset callback resource/diagnostic allowances.
2. Snapshot eligible transport work, apply ingress commands and incremental graph validation.
3. Deliver due scheduled actions, plan a positive interval up to the external/host ceiling, process its graph, then finish its observations/resource publication.
4. Repeat internal intervals while inside the host block; reclaim with remaining validation work and publish bounded diagnostics.

Intervals are `[frame,end)`. An action at host callback end is settled by the next nonempty callback, not by the preceding finish. `callback_start` remains the host origin while `frame` advances internally. `graph_epoch` is the context-entry identity; after activation use the returned next epoch. Neither more intervals nor newly read results refresh callback ingress/recycle/validation/optional-copy/recovery/publication allowances. `budgets.subranges` is ignored.

A planner must establish finite same-frame settlement and return `frame < end <= ceiling`. Required reads/capture have independent work counters and physical/shape/time checks, not optional quotas.

## Transport order and cancellation observation

Normal and recovery lanes have separate admission credits. One admitted owner reserves its reliable completion/retirement space until the control endpoint receives that return. Telemetry is separate and lossy.

At cycle start, eligible recovery can park an already held normal command inline. Recovery is examined first; afterward that same normal head resumes before later normal traffic. Parking retains sequence, owning credit, validator progress and the original cancellation observation. It is not another dequeue.

Only original dequeue observes a ticket's cancellation flag: a request cancelled before its first dequeue completes Cancelled without attempting its operation, while a request cancelled after dequeue (e.g. a held validator that was parked for recovery) still completes with its original outcome. Cancelled dequeues spend operation/publication work too. Work arriving after the snapshot waits for another cycle. Recovery has independent admission, not unlimited processing or publication; if recovery spends the allowance, normal work resumes in a later callback. Indefinitely consumed recovery allowances can starve normal work; cross-lane fairness is not promised. A recovery release can overtake a normal resource read and legitimately make its handle stale. Prepared flag reuse additionally waits for every external flag reference; see [Submitting without allocation](../part-5-configure-and-operate-the-runtime/18-submitting-without-allocation.md).

## Scheduled order is not submission order

The scheduler orders by `(effective_frame, action target/category/subkey, sequence)`:

- Planner-target operations precede processor-target actions at the same frame.
- Processor targets are ordered by identity. Within one target, parameter actions precede MIDI, which precedes typed processor operations; parameter/port indices form subkeys.
- Sequence breaks otherwise equal keys. Transport recovery priority is not a different scheduler sort key.

Signal graph order is separately canonical and acyclic. Decision bindings are not signal edges. Routed MIDI preserves stable source ordering; equal-offset injected packets follow the routed source contribution. Complete packet order does not imply arbitrary downstream synthesizer state is reversible.

Operation-time graph removal precedes later processor/MIDI delivery, making an old target InvalidId. Planning-time removal after injection is Busy if it would discard unconsumed input; the old graph consumes that packet once. Submission order cannot reverse these categories, and graph changes synthesize no note/pedal cleanup.

## Four frames and two result streams

| Identity | Meaning |
|---|---|
| Requested | Host timestamp. |
| Effective | Timestamp accepted by admission, clamped to ingress when late. |
| Delivered | Actual typed-operation delivery, or None if never delivered. |
| Terminal (`actual_frame`) | Runtime completion/domain receipt frame. May be later than delivery, or earlier than the requested frame for queued cancellation. |

An owning Schedule completion reports admission and effective frame; it is not the later execution result. Match both by sequence. `Execution::late_by()` measures delivery minus requested, saturating at zero; undelivered work reports zero. It does not measure a Pending domain wait.

Immediate typed completion and retained Pending tokens have separate lifetimes. Receipt writers validate scope, sequence and terminal state and stamp the current context frame; they do not permit arbitrary backdating. Runtime `Ok(())` can carry a domain Refused result.

Cancellation cuts are distinct: ticket before dequeue; `cancel_queued` for undelivered normal typed planner actions; `Command::Cancel` for undelivered normal parameter, MIDI and processor actions; explicit receipt completion for delivered Pending domain work. Dropping a token, copied payload or StageId settles nothing. A trusted recovery-safe classification must be payload-free, nonfuture and finitely bounded. A recovery token returned Pending must resolve during planning at its delivery frame before positive samples. Recovery operation contexts cannot activate graphs.

## Three graph publication paths

| Path | Publication and ownership |
|---|---|
| `prepare_with_initial_graph` | Startup epoch 1/frame 0, no warm-up callback or queued owner. |
| Normal `Command::Commit` | Incremental validation; old graph continues until callback-ingress publication. Completion returns displaced plan. |
| Normal `StageGraph` then planner permit | Owning completion means Ready only; later actual-boundary preflight/commit coordinates domain and graph. |

The planner admits the first two paths through `Planner::admit_graph`, and generic `Remove`/`Install` through `Planner::admit_processor_change`, before anything changes. A refusal fails preparation or rejects the command with the planner's error; the active graph, epoch and slots stay as they were. Both hooks default to admitting everything. Permit-based publication is checked by the planner that holds the permit.

Stage identities check runtime scope/slot/generation. Ready is neither active nor locked by copied/pending references. Preflight checks current/validation epochs, actual retained/displaced capacities, next epoch and pending MIDI compatibility. All fallible application preflight must precede domain mutation. An unused permit changes no graph state; it cannot undo arbitrary application mutations.

Commit swaps the graph/epoch without freeing the displaced owner. The slot becomes Taken, retaining that owner—or legitimately none when no graph was active. Normal ReleaseGraph returns Ready/Taken ownership and retires the identity; reuse advances generation. Graph/catalog changes invalidate earlier candidate validation. Do not mutate borrowed metadata or merely rewrite an expected epoch to evade restaging.

Faults after publication are not rollback. Stopped extraction neither settles work nor invents finish/device cleanup; [Stopping, disconnecting, and reclaiming ownership](../part-5-configure-and-operate-the-runtime/19-stopping-disconnecting-and-reclaiming-ownership.md) gives both host policies.
