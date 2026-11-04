# 13. Planning processing intervals

<!-- rust-source: ../../examples/planned_pair.rs -->

[Contents](../README.md) · [Previous: MIDI state and checkpointed seeking](../part-3-work-with-stored-media/12-midi-state-and-checkpointed-seeking.md) · [Next: Application-specific operations](14-application-specific-operations.md)

Every graph iteration covers one whole configured quantum. A render-owned planner
settles state in `begin_tick`, processors borrow that domain through `ProcessBlock`,
and `finish_tick` completes the tick. Host callback adaptation surrounds this loop;
it does not introduce smaller graph intervals.

```text
deliver due operations → begin_tick → render DAG for one quantum → finish_tick
```

## Run the example

The [planned pair example](../../examples/planned_pair.rs) shares an alternating
gain value with two processors:

```sh
cargo run -p rt_runtime --example planned_pair
```

It uses an explicit one-frame quantum so automation at frame 13 is exact. It checks
every sample, per-node completion, callback origins, whole/split/irregular callbacks,
a zero call at the cut and reversed processor declarations. Every callback is
heap-guarded. Preparation, results and destruction stay off-render.

## 1. Keep domain state in the planner

The planner owns the gain value and prepared per-node accounting. It updates gain
at the tick start and checks both consumers at the tick end:

```rust
    fn begin_tick(&mut self, context: PlanContext<'_>) -> Result<(), Error> {
        assert_eq!(context.graph_epoch, Epoch(1));
        assert_eq!(
            context.callback_start,
            self.callback_origins[context.frame.0 as usize]
        );
        self.values[0] = 1.0 + ((context.frame.0 / 8) % 2) as f32;
        Ok(())
    }
    fn finish_tick(&mut self, context: FinishContext<'_>) -> Result<(), Error> {
        for frames in self.observed {
            assert_eq!(frames as u64, context.interval.end.0);
        }
        self.finishes += 1;
        Ok(())
    }
}
```

`PlanContext::frame` is the tick start and `ceiling` is its exact end. The planner
returns success or an error, rather than choosing a shorter interval. The runtime
owns absolute frames; processors do not run another clock. A planner may validate
the prepared quantum through `validate_quantum` before rendering begins.

## 2. Borrow the domain during processing

The runtime supplies the actual node identity, current frame, quantum, signal ports,
resources and an exclusive domain borrow in the same block for every processor:

```rust
    fn process(&mut self, block: &mut ProcessBlock<'_, Clock>) -> Result<(), Error> {
        let slot = block.node.slot() as usize;
        assert_eq!(block.domain.observed[slot] as u64, block.frame.0);
        block.domain.observed[slot] += block.frames;
        let gain = block.domain.values[0] * self.0;
        let (input, output) = block.audio.pair(0, 0).ok_or(Error::InvalidShape)?;
        for (input, output) in input.iter().zip(output) {
            *output = input * gain;
        }
        Ok(())
    }
```

The example indexes its own fixed accounting by actual node identity. A production
domain must admit those physical identities and roles before installation, and
protect required membership through `Planner::admit_graph` and
`Planner::admit_processor_change`. These are domain safety checks, independent of
signal connections. See [graph activation](15-staging-and-coordinating-graph-activation.md).

Domain state and buffers cannot escape the process call. Signal-only processors
may ignore `block.domain`; `NoPlanner` is the empty domain for an ordinary graph.

## 3. Verify samples independently

For every absolute frame the expected gain is `(1 + (frame / 8) % 2)`. Left output
multiplies it by 1.0 before frame 13 and 0.5 afterward; right output multiplies it by
0.25. The processor's independent typed operation changes its local multiplier at
13 and produces one reliable result. The planner operation family is `()` here.
[Chapter 14](14-application-specific-operations.md) explains both operation families.

Splitting callbacks at `[8, 5, 3, 8, 8]` or `[3, 10, 19]` preserves all samples and
32 completed ticks. A zero call at 13 changes neither time nor diagnostics. Larger
quanta round operation delivery to a whole-tick boundary and may require host
buffering; they use this same lifecycle.

## Callback identity and bounded work

`callback_start` remains the enclosing callback origin while `frame` advances.
`graph_epoch` identifies context entry; a graph permit returns the newly committed
epoch. Borrowed graph/resource authority cannot escape its scope.

Ingress snapshots, recycling, validation, optional transfer and recovery allowances
belong to the enclosing callback. Another tick does not replenish them. Required
render and capture work remains separately shape/capacity checked. Domain settlement
must terminate within prepared structural bounds; valid finite work can still miss
a device deadline. See [capacity and diagnostics](../part-5-configure-and-operate-the-runtime/17-capacity-work-budgets-and-diagnostics.md).

`finish_tick` completes this tick without settling the next boundary. A change at
callback end belongs to the next nonempty call. A fault after publication is not
rollback: do not replay frames or invent a finish; stop and extract owners according
to the domain's cleanup policy.

[Next: Application-specific operations](14-application-specific-operations.md)
