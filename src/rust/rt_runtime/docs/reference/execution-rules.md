# Execution rules

[Book](../README.md) · [Reference](README.md)

These rules describe generic GraphRuntime. There is one fixed execution kernel. Convenience constructors use Q=1; `prepare_fixed_with_planner_and_resources` selects an explicit Q. Buffered hosts declare a constant Q-frame stream delay. Musical precedence, source/node-pair/media ownership and loop cleanup belong to the separate loop library's planner.

## Callback envelope and half-open time

A zero-frame call returns immediately without shape/frame validation, ingress, recycling, planning, resource work or output mutation. It is not a flush. A nonempty call validates host shapes/packet bounds and the expected absolute frame; bounded output clearing can precede refusal. Validated clock advancement occurs before processing, so a later fault can leave the next-frame clock advanced. An execution fault is terminal: silence subsequent calls and stop/extract the owners without replay.

For each valid nonempty callback:

1. Recycle execution credits and reset callback resource/diagnostic allowances.
2. Snapshot eligible transport work, apply ingress commands and incremental graph validation.
3. Deliver actions due at the next fixed tick, settle its domain, process the whole prepared quantum, then finish capture/state publication.
4. Repeat whole ticks while inside the host block (or as buffering completes each tick); reclaim with remaining validation work and publish bounded diagnostics.

Ticks are `[frame,frame+Q)`. An action at host callback end is settled by the next nonempty callback, not by the preceding finish. `callback_start` remains the host origin while `frame` advances internally. `graph_epoch` is the context-entry identity; after activation use the returned next epoch. Neither more intervals nor newly read results refresh callback ingress/recycle/validation/optional-copy/recovery/publication allowances. `budgets.subranges` is ignored.

The domain must establish finite same-tick settlement. A compatibility `Planner::plan` implementation must return the supplied tick end exactly; it cannot subdivide execution. Required reads/capture have independent work counters and physical/shape/time checks, not optional quotas.

## Transport order and cancellation observation

Normal and recovery lanes have separate admission credits. One admitted owner reserves its reliable completion/retirement space until the control endpoint receives that return. Telemetry is separate and lossy.

At cycle start, eligible recovery can park an already held normal command inline. Recovery is examined first; afterward that same normal head resumes before later normal traffic. Parking retains sequence, owning credit, validator progress and the original cancellation observation. It is not another dequeue.

Only original dequeue observes a ticket's cancellation flag: a request cancelled before its first dequeue completes Cancelled without attempting its operation, while a request cancelled after dequeue (e.g. a held validator that was parked for recovery) still completes with its original outcome. Cancelled dequeues spend operation/publication work too. Work arriving after the snapshot waits for another cycle. Recovery has independent admission, not unlimited processing or publication; if recovery spends the allowance, normal work resumes in a later callback. Indefinitely consumed recovery allowances can starve normal work; cross-lane fairness is not promised. A recovery release can overtake a normal resource read and legitimately make its handle stale. Prepared flag reuse additionally waits for every external flag reference; see [Submitting without allocation](../part-5-configure-and-operate-the-runtime/18-submitting-without-allocation.md).

## Scheduled order is not submission order

Requested times round upward to the prepared grid. The scheduler orders by `(effective_frame, action target/category/subkey, sequence)`:

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
| Normal `Command::Commit` | Incremental validation; old graph continues until ingress can publish at an eligible core tick. Completion returns displaced plan. |
| Normal `StageGraph` then planner permit | Owning completion means Ready only; later actual-boundary preflight/commit coordinates domain and graph. |

The planner admits the first two paths through `Planner::admit_graph`, and generic `Remove`/`Install` through `Planner::admit_processor_change`, before anything changes. A refusal fails preparation or rejects the command with the planner's error; the active graph, epoch and slots stay as they were. Both hooks default to admitting everything. Permit-based publication is checked by the planner that holds the permit.

Stage identities check runtime scope/slot/generation. Ready is neither active nor locked by copied/pending references. Preflight checks current/validation epochs, actual retained/displaced capacities, next epoch and pending MIDI compatibility. All fallible application preflight must precede domain mutation. An unused permit changes no graph state; it cannot undo arbitrary application mutations.

Buffered-host graph commits and generic capture start/stop/snapshot operations wait until the next collected tick is eligible for their original request boundary. A partial earlier tick retains its graph/capture state. Snapshot cuts use the core clock. Callback budgets and owning credits are retained while waiting.

Commit swaps the graph/epoch without freeing the displaced owner. The slot becomes Taken, retaining that owner—or legitimately none when no graph was active. Normal ReleaseGraph returns Ready/Taken ownership and retires the identity; reuse advances generation. Graph/catalog changes invalidate earlier candidate validation. Do not mutate borrowed metadata or merely rewrite an expected epoch to evade restaging.

Faults after publication are not rollback. Stopped extraction neither settles work nor invents finish/device cleanup; [Stopping, disconnecting, and reclaiming ownership](../part-5-configure-and-operate-the-runtime/19-stopping-disconnecting-and-reclaiming-ownership.md) gives both host policies.
