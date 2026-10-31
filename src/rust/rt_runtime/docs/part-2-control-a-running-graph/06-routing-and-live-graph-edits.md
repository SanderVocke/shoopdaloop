# 6. Routing and live graph edits

<!-- rust-source: ../../examples/live_routing.rs -->

[Contents](../README.md) · [Previous: Commands, completions, and ownership](05-commands-completions-and-ownership.md) · [Next: Changing parameters at an exact sample](07-changing-parameters-at-an-exact-sample.md)

A graph selects installed processors and connects their ports. Audio fan-out copies a source into several destinations; fan-in sums sources into a destination. The runtime processes the signal DAG in a deterministic order.

This graph feeds two gains from the same input:

```text
host input 0 ──┬── gain a: 0.5 ────────────┐
               │                         ├──► host output 0: 0.75
               └── gain b: 0.25 ──┬───────┘
                                 └──────────► host output 1: 0.25
```

Both outputs assume an input of `1.0`. The graph routes signals; planner bindings and application sync relationships do not create audio connections.

## Run the example

The [live routing example](../../examples/live_routing.rs) starts with that graph, excludes `a` in a live edit, then replaces its physical slot with a new generation:

```sh
cargo run -p rt_runtime --example live_routing
```

It asserts old output throughout incremental validation, candidate return on stale-epoch rejection, processor retirement, and failure of an old scheduled target after replacement. Each callback is heap-guarded; compilation, result disposal and teardown are not callback work.

## 1. Describe the routes

`Endpoint::host` identifies a host input when used as a source, and a host output when used as a destination. `Endpoint::node` similarly selects a processor's output or input. Port indices are per kind and direction.

The example defines `host(port)`/`node(id)` as Audio endpoints and `audio(source, destination)` as a small `Connection` constructor. None is a library API:

```rust
    let graph = GraphDescription {
        nodes: vec![b, a],
        connections: vec![
            audio(host(0), node(a)),
            audio(host(0), node(b)),
            audio(node(a), host(0)),
            audio(node(b), host(0)),
            audio(node(b), host(1)),
        ],
    };
```

`GraphRuntime::prepare_with_initial_graph` installs this graph at startup, as in [Your first audio graph](../part-1-process-audio/02-your-first-audio-graph.md). With gains 0.5 and 0.25, its first callback verifies:

```rust
    assert_eq!(output, [[0.75; 8], [0.25; 8]]);
```

A graph need not include every installed processor. Unconnected audio inputs start at zero; host outputs without incoming routes stay silent. MIDI fan-in merges complete packets in stable timestamp/source order rather than adding bytes; [Processing and scheduling MIDI](../part-2-control-a-running-graph/08-processing-and-scheduling-midi.md) shows that order.

## 2. Compile off-render

`compile(runtime.catalog(), &description)` checks IDs, ports, capacities and the signal DAG, then returns an owning `PreparedPlan`. Do that outside the callback.

Description row order does not define processor order. The example reverses both lists and still obtains `[a, b]` for these independent nodes. Adding `a → b` and `b → a` fails with `Error::Cycle`. Even a musically meaningful feedback/sync relationship is not permission to put an instantaneous cycle in this signal graph; model its timing or delayed processing separately.

Compilation does not bypass render-side validation. The catalog may change before admission, and prepared descriptors still require validation against the actual runtime.

## 3. Commit a live replacement

The replacement keeps only `b`, routing it to both outputs. Submit the compiled plan with the current epoch:

```rust
    let plan = compile(runtime.catalog(), &only_b)?;
    let ticket = control
        .submit(Lane::Normal, Command::commit(runtime.epoch(), plan))
        .map_err(|refusal| refusal.error)?;
```

Ordinary Commit validates at callback ingress. With `budgets.validation_steps: 1`, this candidate cannot finish in one callback. The active graph keeps rendering normally:

```rust
    render(&mut runtime, &mut output)?;
    assert_eq!(output, [[0.75; 8], [0.25; 8]]);
    assert_eq!(runtime.epoch(), initial_epoch);
    assert!(control.try_completion().is_none());
```

The example continues finite callbacks, matching the completion by sequence. Each unfinished callback still outputs 0.75/0.25. Once Applied, both outputs contain 0.25 and `retired` owns the old plan. Drop or retain that owner on the control side.

The epoch is the runtime's checked publication/catalog identity, not a callback count. Read the actual value before preparing another request; do not predict how many callbacks validation needs. In a live host, send current epoch information to the control side rather than accessing the render owner concurrently.

Ordinary Commit publishes at callback ingress, not at an arbitrary scheduled sample. For a coordinated internal-boundary cut, use [staged graph activation](../part-4-coordinate-processors-and-changes/15-staging-and-coordinating-graph-activation.md). A staged Ready candidate alone changes neither routes nor epoch.

## 4. Remove an inactive processor, then replace it

Excluding `a` stops processing it but leaves its processor installed. Parameter changes can still address an installed inactive node; scheduled MIDI requires active membership.

Remove the node only after the exclusion is Applied. Removing an active node instead returns `Outcome::Rejected(Error::Busy)`. Successful removal returns its processor:

```rust
    assert_eq!(completion.outcome, Outcome::Applied);
    assert!(matches!(
        &completion.retired,
        Some(Retired::Processor(Gain(0.5)))
    ));
    drop(completion);
    let replacement = a.checked_next_generation()?;
```

Prepare the replacement off-render and submit `Command::Install { prepared: Some(...) }` with that new ID. The example uses gain 0.125, then compiles a new graph containing `replacement` and `b`. Its output is 0.375/0.25.

A physical slot retains its prepared port signature. Installation cannot enlarge it, and changing routes cannot construct arbitrary processors or resize their ports. Exhausted generations fail rather than wrapping.

Before removal, the example schedules a future gain change addressed to old `a`. After replacement, that action reports `Error::InvalidId` at its target frame; it does not change the new processor. The output remains 0.375/0.25. Generation checks prevent an old reference from silently retargeting reused storage.

## Stale candidates and retained owners

The example deliberately submits another plan with the initial epoch after the catalog has changed:

```rust
    assert_eq!(completion.outcome, Outcome::Rejected(Error::StaleEpoch));
    assert!(matches!(
        &completion.request,
        Command::Commit { plan: Some(_), .. }
    ));
    assert!(completion.retired.is_none());
```

Rejection preserves the active graph and returns the candidate. Review/recompile against the current catalog, then use the actual expected epoch; do not merely change a number to conceal a changed physical configuration. Successful off-render compilation or visually identical routes do not replace epoch/generation checks.

Read and dispose of returned plans/processors off-render. The example drains its scheduled execution, verifies zero outstanding commands, then destroys the stopped runtime. It does not claim graph changes perform MIDI note cleanup, resource transactions or domain rollback. Those require their own explicit policy.

[Next: Changing parameters at an exact sample](07-changing-parameters-at-an-exact-sample.md)
