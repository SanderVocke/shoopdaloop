# 15. Staging and coordinating graph activation

<!-- rust-source: ../../examples/staged_routes.rs -->

[Contents](../README.md) · [Previous: Application-specific operations](14-application-specific-operations.md) · [Next: Recording resources together](16-recording-resources-together.md)

A staged graph separates owning validation from publication. A normal command makes a candidate Ready while the old graph continues. Later, a planner can preflight its domain change and publish it with the graph at one internal boundary.

This is different from startup installation in chapter 2 and ordinary callback-ingress Commit in chapter 6. A graph permit is not a general resource transaction, processor constructor or rollback mechanism.

## Run the example

The [staged routes example](../../examples/staged_routes.rs) starts with two constant-output consumers and a shared planner:

```sh
cargo run -p rt_runtime --example staged_routes
```

Initially each consumer has its own decision slot/output route, producing 0.5/0.25. The candidate swaps both bindings and routes; the domain publishes values 0.75/0.125 at the same cut. Independent output and producer-observation assertions detect mismatched bindings or routes.

It checks unused/refused/stale permits, whole/split/irregular/one-frame callbacks and a zero call at the cut. It releases every Ready/Taken owner explicitly, including a separate no-previous-graph case. All callbacks are heap-guarded; compilation, owning-result disposal and destruction stay off-render.

## 1. Prepare the owner bank before rendering

Opt in with `runtime.prepare_graph_staging(candidate_count)` once, while `next_frame()` is still zero. The example prepares one stage slot. Zero slots, re-preparation or preparation after rendering are refused.

This reserves fixed owner metadata and checks a compatible per-plan storage envelope. It does not compile future plans or allocate their descriptor vectors. Compile the candidate off-render with `compile_planned`, including its decision bindings.

Runtime admission accounts for **retained vector capacities**, not just populated rows. `PreparedPlan::storage_bytes()` includes descriptor storage, steps, connections, membership and bindings, excluding allocator overhead. A valid small description backed by oversized vectors can still be refused; so can an oversized displaced active plan at activation. [Chapter 17](../part-5-configure-and-operate-the-runtime/17-capacity-work-budgets-and-diagnostics.md) separates these costs from compiler/validator scratch and other queued owners.

## 2. Stage an owner, not an active graph

Submit `Command::stage_graph(slot, expected_epoch, plan)` on the normal lane. In the example, `stage` uses the same finite submit/render/poll pattern as earlier helpers. Its first real callback still renders the initial graph; this is not a hidden startup warm-up.

Validation uses the callback-wide allowance. Larger candidates/smaller allowances may need several callbacks; no partially validated graph is Ready. Recovery can preempt a held normal validator without losing its sequence/progress, as explained in [chapter 5](../part-2-control-a-running-graph/05-commands-completions-and-ownership.md).

On Applied, recover the stage identity from the changed owning request:

```rust
    let Command::StageGraph {
        plan: None,
        id: Some(id),
        ..
    } = &completion.request
    else {
        panic!("not a Ready stage")
    };
```

This means Ready, **not active**. Routes, decisions and epoch remain unchanged. The example observes `StageStatus::Ready { validated_epoch: Epoch(1) }` and old output 0.5/0.25. Rejection or pre-dequeue cancellation retains the original plan in the return.

A `StageId` identifies checked runtime scope, slot and generation; it is not the graph owner, a pointer, authentication or a cross-Wasm wire token. Ready/Taken slots cannot be overwritten. Application/topology IDs in the loop library are different identities.

## 3. Obtain a scoped permit at the actual boundary

Planner operation and plan contexts lend `graphs: GraphAccess`. Active/candidate views are borrowed metadata. `prepare_activation(id, expected_epoch)` checks:

- activation authority: recovery operation contexts cannot activate;
- scoped identity, generation and Ready state;
- current epoch and the candidate's validation epoch;
- representable next epoch and actual-capacity allowance for the displaced plan;
- compatibility with already pending injected MIDI.

The returned permit exclusively borrows graph state and exposes `current()`, `candidate()` and `next_epoch()`. It cannot escape the callback or coexist with mutation through another retained view.

The example first prepares a permit and returns without using it:

```rust
        let permit = graphs.prepare_activation(id, expected)?;
        if matches!(attempt, Attempt::Unused) {
            return Ok(Reply::Unused);
        }
        if matches!(attempt, Attempt::Refuse) {
            return Err(Error::InvalidParameter);
        }
```

Both branches leave graph/domain state and output unchanged. The next attempt supplies a deliberately wrong expected epoch and returns StaleEpoch, still preserving the Ready candidate. Returning a domain error while owning a permit does not publish anything.

## 4. Finish fallible preflight, then publish both halves

The successful preflight checks candidate membership, both new decision bindings and both signal routes. All refusals occur before changing domain values or consuming the graph permit.

Only then does it publish simple preflighted assignments and commit:

```rust
        let epoch = permit.next_epoch();
        self.values = [0.75, 0.125];
        self.swapped = true;
        self.active = true;
        self.epoch = epoch;
        assert_eq!(permit.commit(), epoch);
        Ok(Reply::Activated(epoch))
```

`commit()` infallibly swaps the validated graph/epoch and keeps the displaced owner in the stage slot. It does not allocate, reclaim, revalidate or scan the graph. If application publication still has an ordinary failure path after its first mutation, it has not established this coordinated cut.

Dropping an unused permit changes nothing; it does not undo arbitrary application mutations already made. A later rendering fault also does not roll back either publication.

## 5. Retain a pending request without locking the candidate

For the main cut, an operation retains only its receipt, StageId, expected epoch and target frame, then returns Pending. At that frame, `plan` rechecks the permit and resolves the receipt using the result. It does not trust the earlier Ready observation as a lock.

The test cut is three samples after the subsequent rendering batch starts. Every partition yields 0.5/0.25 before it and 0.75/0.125 from it onward. The terminal result records the actual cut:

```rust
    assert_eq!(execution.delivered_frame, Some(delivered));
    assert_eq!(execution.actual_frame, cut);
    assert_eq!(execution.result, Ok(()));
    assert_eq!(execution.operation_result, Some(Reply::Activated(Epoch(2))));
```

A zero callback at that boundary neither activates nor resolves the request; the next nonempty callback does. Operation-time publication happens during action delivery; plan-time publication happens after due actions and before processing the chosen interval. The new routes and bindings serve that interval.

The context's copied `graph_epoch` describes entry. Commit returns the new epoch; later contexts/final diagnostics observe it. Pending tokens and Ready graph owners have independent lifetimes: release/reuse/another activation may invalidate the pending reference. Recheck at publication, and resolve refusal without losing the domain outcome.

Any later graph/catalog edit invalidates earlier validation even if the routes still look compatible. Release and restage against the current catalog/epoch rather than just rewriting the expected number to evade that change.

## Release Ready and Taken owners independently

After activation, the slot is Taken and retains the displaced graph. `Command::ReleaseGraph { id }` on the normal lane returns it as `Retired::Plan`. Dispose/reuse it off-render. Release invalidates that identity; slot reuse advances its checked generation. The example verifies a second release returns InvalidId, then stages/releases a fresh unused candidate without another publication.

The separate initially-empty case legitimately releases a Taken slot with no retired plan: there was no previous active owner. This is a lifecycle case, not a reason to replace the simpler startup constructor in chapter 2.

Release does not undo the new active graph. Nor does disconnect release hidden application references, perform note cleanup or settle pending work. The example drains results and checks its pending ledger before stopped extraction.

## The planner admits generic changes

A planner's permit is not the only way to change the graph. `prepare_with_initial_graph` and normal `Command::Commit` publish graphs, and `Command::Remove`/`Install` change processors, without involving the planner's operations. A planner that depends on particular consumers protects them through two hooks, which run before anything changes:

- `Planner::admit_graph(candidate)` refuses a published graph that lacks a required consumer or binding;
- `Planner::admit_processor_change(node)` refuses removing that node's processor, or installing into its slot.

A refusal fails preparation, or rejects the command with the planner's error while the active graph, epoch and slots stay unchanged. Both hooks default to admitting everything. `admit_graph` also runs off-render in `prepare_with_initial_graph`; otherwise both run during callback ingress, so they obey the same no-heap, nonblocking contract as the rest of the planner.

## MIDI compatibility is not musical cleanup

This audio-only example does not exercise pending injection. The runtime nevertheless refuses a candidate that would remove a node whose already-injected MIDI still needs consumption: preflight returns Busy and the old graph consumes the packet once. At operation delivery, planner actions run before processor/MIDI actions; removing the node first instead makes its later injection fail InvalidId. Reversing submission order cannot reverse those categories.

Neither path synthesizes note/pedal cleanup or transfers arbitrary media/node-pair ownership. The application must preflight those responsibilities separately. Generic permits also cannot construct new processors or resize prepared port shapes. The loop library's constrained graph/media/configuration transactions build on this machinery; they are not guarantees supplied by the permit alone.

[Next: Recording resources together](16-recording-resources-together.md)
