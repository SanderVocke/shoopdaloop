> **Historical snapshot — not an active plan or current acceptance claim.**
> [History index](../README.md) · [Current plan](../../../PLAN.md) · [Original at `fffc1faa`](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/CRITIQUE_LOOP_LAYER_ARCH.md)
> Original text follows; only relative link destinations are adapted for browsing.
> Exact original bytes remain in the snapshot bundle. Old tasks, pauses and status apply only to that snapshot.

# Loop layer architecture: scope and composability critique

## Summary

The expected boundary—loop semantics implemented through a set of processors, a
planner for `rt_runtime`, and control-side APIs—is a sound architecture. The current
implementation has much of that internal separation, but its complete public
integration is broader: `shoop_loop_engine::runtime::Engine` owns an entire
`rt_runtime::GraphRuntime` and mediates its construction and mutation.

There are legitimate reasons to coordinate loop timing, media ownership, capture
and selected graph changes. Those reasons do **not**, by themselves, establish
that the loop library should own the application's entire signal graph.

The important distinction is between:

1. **A coordinated loop subsystem**, which is necessary for the implemented
   semantics and media guarantees.
2. **A graph-owning facade**, which is one way to enforce that subsystem's
   integration contract, but need not be the only public integration path.

This document describes the current implementation and assesses its boundary. It
is not an implementation plan or a claim that the proposed component API already
exists.

## Current scope

The crate has three distinguishable layers:

| Layer | Responsibility |
| --- | --- |
| Loop semantics | `Resolver`, commands, decisions, compiled programs and timing topology: positions, wraps, synchronization, triggers, composite execution and arbitration. |
| Media execution and ownership | Audio/MIDI render and capture operations that follow supplied decisions without maintaining another loop clock; a ledger that retains media ownership. |
| Integrated runtime facade | `runtime::{Engine, Controller, Stopped}` assembles the planner, playback/capture processors, resources, signal graph, control transport and lifecycle. |

The semantic resolver and low-level media operations are separately exposed.
However, the complete production planner/processor integration is facade-first.

In [`runtime/mod.rs`](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/src/rust/shoop_loop_engine/src/runtime/mod.rs), the ownership
relationship is explicit:

```rust
pub struct Engine<P: Processor<LoopPlanner>> {
    runtime: GraphRuntime<Node<P>, LoopPlanner>,
}
```

`Engine::prepare` accepts a complete `GraphDescription`, prepared effects,
resources, loop definitions and bindings. It constructs the loop planner and
consumers, then constructs the `GraphRuntime` itself.

Although `LoopPlanner` is a public type, its fields are private and it has no
public standalone preparation API. The playback/capture node constructors are
also internal. An application cannot currently take the production planner and
loop-role processors and install them independently into its own `GraphRuntime`
through the public API. Building another integration from the resolver and
low-level renderers would reproduce substantial coordination logic.

### Timing topology is not the signal graph

The existence of a loop topology is not itself evidence of excessive scope.

[`TopologyDescription`](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/src/rust/shoop_loop_engine/src/topology.rs) contains
logical loops, synchronization sources, trigger relationships and composite
programs. It contains no audio/MIDI signal routes. Its source explicitly describes
prepared timing dependencies as independent of audio/MIDI signal routes.

This topology represents **who clocks or controls whom**, whereas the runtime
signal graph represents **which signal output connects to which input**. A
semantics-focused loop library needs the former without necessarily owning the
latter.

### Runtime ownership versus routing policy

The application still supplies signal routes and non-loop processors. Playback
and capture remain distinct nodes, allowing playback → FX → capture, monitoring,
mixing and other supported routes.

Nevertheless, the facade owns the actual runtime and resource store, creates and
validates the loop-role processors and bindings, restricts generic control access,
and mediates supported live graph changes. These changes operate over prepared
physical roles rather than unrestricted live processor construction or port
resizing.

The facade is therefore a **loop-aware runtime assembly and lifecycle owner**. It
is not a second graph executor: processing still delegates to `rt_runtime`. It is
also not a complete application backend: devices, thread creation, UI and
production host adapters remain outside this facade.

## What invariants does the integration make easier to enforce?

Most benefits come from coordinating the loop subsystem. The additional benefit
of graph ownership is preventing application code from bypassing that
coordination.

### 1. Required consumers and decision bindings remain present

For every assigned primitive loop, the facade requires:

- its playback processor is present in the active graph;
- its capture processor is present;
- both consume the decision slot belonging to that loop;
- no additional, unexpected decision bindings exist.

These checks appear in `LoopPlanner::validate_graph()` in
[`runtime/planner.rs`](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/src/rust/shoop_loop_engine/src/runtime/planner.rs).
Initial preparation constructs the corresponding pair and bindings.

A generic graph validator cannot infer that two processors form a required
musical pair. Removing a capture node could leave a structurally valid signal
graph but break recording and input-state tracking. Capture tracks connected MIDI
even while stopped, because a later take needs the input projection immediately
before its start.

**What the facade simplifies:** it creates the pairs and mediates graph
activation, so supported changes cannot bypass these domain checks.

**What is actually required:** loop-specific validation on relevant graph
activations, or restrictions protecting loop-owned nodes and bindings. Exclusive
ownership of unrelated graph nodes is not intrinsically required.

### 2. A program and its associated routing publish together

Suppose a new arrangement and its routing must start at frame F. Independent
commands to activate the program and then activate the graph are insufficient:
the first could succeed while the second fails because its expected graph epoch
is stale. The new arrangement would then execute through old routes.

The paired program/graph path in `LoopPlanner::plan()` instead:

1. Selects the actual eligible musical boundary.
2. Obtains an exclusive activation permit for the prepared graph.
3. Preflights capture under the candidate musical decisions.
4. Settles media and commits the resolver.
5. Commits the graph through the already-validated permit.
6. Reports the new epochs and renders with the coordinated configuration.

A capture-preflight refusal does not publish the paired graph change. A pending
candidate must also remain valid at the actual cut; initial readiness is not a
permanent lock on a graph candidate.

**What the facade simplifies:** one implementation owns the pending transaction
and publication sequence. The application cannot accidentally split it into
independent operations.

**What is actually required:** a planner with coordinated activation authority.
The generic runtime already supplies the graph activation permit used here; a
publicly constructible loop planner could use the same mechanism inside an
application-owned runtime.

This is controlled publication after preflight, not arbitrary rollback if a later
processor or interface fault occurs.

### 3. Reassigning physical roles cannot leave mixed logical identities

The configuration path can assign a prepared physical playback/capture pair to a
new logical loop generation. It must not leave the resolver using a new identity
while the ledger, capture state or decision bindings still describe the old one.

`LoopPlanner::activate_configuration()` in
[`runtime/configuration.rs`](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/src/rust/shoop_loop_engine/src/runtime/configuration.rs)
checks, among other things:

- the previous logical owner is already stopped;
- no active/prepared capture or outstanding interval remains in that row;
- capture groups are idle;
- the candidate topology agrees with the initial media length and loop kind;
- the candidate graph contains the correct consumers and decision bindings;
- the media-owner changes can be committed.

It then publishes the coordinated changes, resetting MIDI input projection when
required. A queued same-frame Stop is not equivalent to an already-settled stopped
owner.

**What the facade simplifies:** these representations are private, and the
supported API changes them through one checked transaction.

**What is actually required:** coordinated authority over logical identities,
media roles, capture lifecycle and the relevant graph publication—not ownership
of unrelated effects or routes.

### 4. Capture and musical state agree before affected samples render

For Record at frame F, capture resources must be reserved before the musical
start is committed. If reservation fails, the resolver must re-preview that
boundary with a refusal. Otherwise dependent triggers or composites could act on
a recording start that never actually happened.

Similarly, record-to-play must freeze a common audio/MIDI prefix and make it
readable before playback processes sample F. Exhaustion of an existing take must
also affect decisions before rendering crosses the failure boundary.

The planner's `preflight()`, `settle()`, `plan()` and `finish()` coordinate these
steps. Coherent state/export queries similarly combine resolver state and media
state at a defined boundary rather than independently sampling them.

**What the facade simplifies:** timing, the ledger, capture lifecycle and decision
publication share a controlled transaction.

**What is actually required:** the coordinated loop planner and resource
capabilities. This is not specifically an argument for a graph-owning `Engine`.

## Architectural assessment

The strongest benefit of the current facade is **preventing bypasses**. There is
one public route for construction and mutation, and that route performs the loop
checks. This reduces the integration contract the application must satisfy.

This is not principally about locks or concurrent graph mutation. The runtime
already has serial render ownership. It is about preventing valid generic
operations that are invalid for the loop subsystem, and coupling operations that
must not succeed independently.

The trade-off is real:

- **Benefit:** a controlled assembly and mutation path makes invariants easier to
  enforce and provides a turnkey loop runtime.
- **Cost:** unrelated DSP, routing and future domain integrations must fit inside
  the loop facade's exposed API and processor wrapper. Application processors
  also see the `LoopPlanner` generic parameter. With one planner per runtime,
  independent planning domains require explicit composition rather than simply
  installing another planner.

This is reasonable for a loop-centric runtime product. It is a stronger
commitment than a reusable loop-semantics extension to a generic runtime.

## Preferred boundary for a composable loop layer

If application-owned graph architecture is the destination, a preferable public
boundary would be:

1. **A reusable coordinated loop subsystem:** a preparable planner, loop-role
   processors, bindings and typed control operations installable into an
   application-owned `GraphRuntime`.
2. **Explicit integration contracts:** validate required consumers, protect
   loop-owned roles and bindings, and provide coordinated activation hooks where
   timing, media and routes must change together.
3. **An optional `Engine` facade:** assemble those same components for callers
   wanting a complete loop-centric runtime.

This does not mean exposing unrestricted mutable access to runtime internals and
hoping callers preserve the invariants. The component API must encode or validate
those obligations, including later graph changes and ownership recovery.

The current implementation's guarantees are requirements for such an API, not
proof that the loop layer must own the application's entire graph. Preserve the
coordination; make whole-runtime ownership optional rather than the only complete
public integration path.
