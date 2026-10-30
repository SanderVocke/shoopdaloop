# 13. Planning processing intervals

<!-- rust-source: ../../examples/planned_pair.rs -->

[Contents](../README.md) · [Previous: MIDI state and checkpointed seeking](../part-3-work-with-stored-media/12-midi-state-and-checkpointed-seeking.md) · [Next: Application-specific operations](14-application-specific-operations.md)

A planner settles one domain before processors render a positive interval. Consumers borrow its immutable decisions, then report bounded observations about the work they performed. They do not advance another authoritative clock.

```text
at current frame:
    deliver due external actions
    plan → choose end and publish decisions
    process signal graph over [frame, end)
    finish → consume this interval's observations
repeat at end, if still inside the host callback
```

## Run the example

The [planned pair example](../../examples/planned_pair.rs) shares one alternating gain decision with two processors:

```sh
cargo run -p rt_runtime --example planned_pair
```

It asserts every sample, both producers' observations, callback origins and five intervals in one 32-frame callback. It also runs split/irregular callbacks, a zero call at a cut, reversed consumer declarations, and a one-sample planner. Every callback is heap-guarded; setup/results/destruction are off-render.

The older [planned gain example](../../examples/planned_gain.rs) remains a useful one-consumer variant, but has no heap guard of its own and no pending planner receipt demonstration. Those claims belong to the new examples here and in [Application-specific operations](../part-4-coordinate-processors-and-changes/14-application-specific-operations.md).

## 1. Prepare one authority and fixed decision storage

Implement `Planner` with Copy decision, observation, operation and result types. Its preparation-only specification declares concrete stores:

```rust
        Some(PlannerSpec {
            id: self.id,
            decision_slots: 1,
            observation_capacity: 2,
            operation_bytes: 0,
        })
```

This planner exposes one initialized `f32` decision and reserves two observations. Its decision slice length/signature must remain valid throughout rendering; do not resize or allocate it in `plan`.

The runtime owns absolute frames. `PlanContext::frame` is the boundary now being settled; `ceiling` is the latest permitted end, limited by external events and host callback end. The planner must return `frame < end <= ceiling`.

Here gain alternates between 1.0 and 2.0 every eight frames. The planner selects the next eight-frame boundary, clipped to the supplied ceiling. A processor multiplier change at frame 13 forces an additional cut:

```text
[0,8) [8,13) [13,16) [16,24) [24,32)
```

The scheduled change is an immediate typed processor operation used to expose that cut. The planner itself accepts no operations here, so its operation and result types are `()`; the processors declare their own family. Chapter 14 explains both families.

## 2. Declare each consumer's access and bind it

Each gain declares that it requires a decision and may write one observation:

```rust
    fn planning(&self) -> ConsumerAccess {
        ConsumerAccess {
            decision_required: true,
            observations: 1,
            operation_bytes: 0,
        }
    }
```

Prepare explicit bindings from both consumer NodeIds to decision slot zero. Pass them to `prepare_with_initial_graph`, or to `compile_planned` when preparing a later plan. The compiler checks planner identity, slot bounds and required bindings against prepared catalog metadata. The example verifies missing bindings return `InvalidGraph`.

A binding does not connect signal ports. The graph separately routes the input through each gain to its corresponding output. Reversing consumer declarations changes neither the canonical signal execution nor the expected samples.

## 3. Borrow decisions only for this process call

The processor checks that the borrowed `DecisionView` covers its actual `ProcessBlock` interval, then multiplies the input by that decision and its local multiplier. It reports its frame count through:

```rust
        block.observations.push(block.frames)
```

The writer stamps the producer NodeId. Its reservation is local to this consumer: a producer cannot spend another's unused observation allowance. Refusal preserves earlier observations; exceeding the declared interface is a fault to handle, not permission to grow storage.

Do not retain the borrowed decision or observation slice across mutation/another interval. `finish` receives the completed interval and observations; here it checks exactly one correctly sized contribution from each known producer. After stopped extraction, the example verifies each producer observed 32 frames.

## 4. Check samples independently of interval shape

For every absolute frame, the expected decision is `(1 + (frame / 8) % 2)`. Left output multiplies it by 1.0 before frame 13 and 0.5 afterward. Right output always multiplies it by 0.25. The example computes these expectations directly, not from the planner's decisions.

One callback produces the five intervals above. Splitting at `[8, 5, 3, 8, 8]` preserves those cuts; irregular `[3, 10, 19]` adds host boundaries without changing samples. A zero call at 13 neither settles that boundary nor changes diagnostics. A valid one-sample planner completes all 32 intervals with the same samples.

There is no arbitrary internal-subdivision allowance: this example sets `budgets.subranges: 0` and needs only one external event/result slot. Domain structural rules must establish same-frame termination and finite progress. Valid finite work can still be too expensive for a device deadline.

## Context identity and callback-wide work

`callback_start` stays at the enclosing nonempty callback's origin while `frame` advances internally. The example prepares an independent origin table from its host partitions and checks every plan call against it. It is useful for private callback-wide bookkeeping, not a second musical clock.

`graph_epoch` is the context-entry graph identity. `graphs` is scoped graph authority, not an owned runtime/control reference. Active/candidate views cannot survive mutation or escape the callback. If a permit publishes a new graph, use the epoch returned by its commit; later contexts observe that epoch. [Chapter 15](15-staging-and-coordinating-graph-activation.md) explains preflight/publication. Standalone prototype contexts may use `GraphAccess::disabled()`, which cannot manufacture runtime authority.

Command snapshots, execution-credit recycling, graph validation, optional transfer, recovery and telemetry remain one enclosing callback envelope. Calling `plan` again does not renew them. Required `RenderIo` work is independently capacity/shape/time checked, not a discretionary quota that can drop mandatory samples. Decisions, observations and retained execution results also occupy distinct stores; [Capacity, work budgets, and diagnostics](../part-5-configure-and-operate-the-runtime/17-capacity-work-budgets-and-diagnostics.md) sizes them.

`finish` consumes the interval; it does **not** settle its end boundary. At callback end that work belongs to the next nonempty callback. NoPlanner remains the ordinary default when no domain planner is needed.

A fault after a domain publication is not rollback. Do not replay the frames or synthesize `finish`; domain policy may require stopped extraction and explicit permanent cleanup instead of continuation.

[Next: Application-specific operations](14-application-specific-operations.md)
