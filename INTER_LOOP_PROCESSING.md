# Decision: inter-loop processing

## Decision

Preserve **sample-accurate interaction between loops within an audio block**. Do not introduce a fixed interaction quantum or a mandatory one-block delay between loops.

Process all required finite musical transitions at their intended sample positions. Do not impose an arbitrary per-block transition, propagation-round, or subdivision limit merely to keep the callback within a CPU allowance.

> The engine owns correctness and termination. The application owns the cost of the musical workload it requests.

An expensive sequence of transitions may cause an xrun, just as an expensive DSP processor may. The engine must not conceal that cost by silently delaying, dropping, or reordering musical transitions.

## Why not fixed-quantum interaction?

Deferring cross-loop effects to a fixed grid simplifies part of the executor, but introduces other complexity:

- Trigger times become quantized, and cascades either accumulate delay per hop or require special same-boundary resolution.
- The interaction grid and its phase become session semantics that must survive reopening.
- Device buffers need adaptation without accidentally introducing additional interaction boundaries.
- Simultaneous starts and composite transitions need exceptions or explicit grouping to preserve expected behavior.

Synchronization is fundamental to a looper. Musical boundaries should determine processing subdivisions, rather than processing subdivisions determining musical behavior.

## Coordination model

Keep loop and composite timing relationships under an **application-level timeline resolver**, represented as an explicit planning node in the execution graph. Keep its timing responsibilities separate from the audio/MIDI work performed by individual loop processors.

```text
Loop / composite timeline coordinator
  positions · sync relationships · planned transitions
                       │
             sample-timestamped decisions
                       ▼
Generic RT signal processing
  playback → effects → recording → outputs
```

Independent loop processors must not discover synchronization by reading one another's partially updated mutable state. The coordinator establishes coherent timing decisions.

Conceptually, processing proceeds as follows:

```text
Resolve transitions at the current sample boundary
                       ↓
Find the next relevant boundary or end of block
                       ↓
Advance/render the interval before that boundary
                       ↓
Gather boundary observations and resolve interactions
                       ↓
Repeat until the block is complete
```

Loops need not execute simultaneously. They may execute sequentially within an agreed interval, provided no participant crosses a relevant boundary before its effects have been resolved.

Where timing decisions depend only on known timeline/control state, the coordinator may prepare timestamped spans before signal rendering. Where a decision depends on rendering outcomes, coordination must happen at the relevant boundary. Preplanning must not assume away recording failures or other outcomes that can change loop state.

## Implementation direction: resolver before loop processors

Adopt a **separate loop timeline resolver followed by individual loop processors** as the design direction. The resolver's application-specific state machine belongs in the loop layer; the generic runtime supplies ordering, prepared decision storage, and interval execution facilities.

```text
                  timestamped commands / prepared session state
                                      │
                                      ▼
                            LoopTimelineResolver
                         owns inter-loop timing state
                           │          │          │
                 timing / ordering dependencies, not audio routes
                           │          │          │
                           ▼          ▼          ▼
                       Loop A      Loop B      Loop C
                       renderer    renderer    renderer
                           │          │          │
                           └──── signal DAG ─────┘
                              effects / recording / outputs
```

Every affected loop processor has an explicit dependency on the resolver. Ordering must not rely on node IDs, insertion order, or a dummy audio/MIDI connection. Existing audio/MIDI edges continue to determine signal dependencies between rendering operations.

### Responsibilities and ownership

| Component | Responsibility |
|---|---|
| `LoopTimelineResolver` | Own authoritative loop/composite timing state, resolve sync and simultaneous actions, and select the next interval |
| Per-loop renderer | Own playback/recording/DSP state and execute the decisions for its loop; do not independently resolve cross-loop triggers |
| Prepared decision storage | Publish a coherent, read-only interval view to consumers, including absolute frame range and relevant per-loop mode, position, and transition information |
| Executor | Run the resolver before its consumers, execute the signal graph for the selected interval, and return to the resolver when needed |

The resolver and consumers execute serially under the render owner. Publishing a decision view must not require locks, cross-thread messaging, or owning allocations. Storage and access paths are prepared before rendering. Timing state has one authority; renderers must not maintain competing versions of the synchronization state machine.

### Prefer incremental interval publication

The baseline design should not require a list of every transition in an entire host block. The resolver can publish one interval at a time into reusable storage:

```text
begin host callback once
        │
        ▼
resolver: settle this boundary and publish next interval
        │
        ▼
execute dependent loop renderers and signal DAG for that interval
        │
        ▼
collect any relevant execution outcomes
        │
        ├─ more samples remain → resolver runs again
        └─ otherwise → finish callback
```

Same-boundary cascades are settled before publishing the following nonempty interval. Reuse interval storage only after all its consumers have finished. A large number of successive transitions therefore increases processing work without requiring an equally large retained transition list.

The resolver is a **planning node with an executor-supported lifecycle**, not merely an ordinary processor that happens to run first for a full block. Today's `Processor::process` interface cannot publish an interval length that changes the subsequent traversal by itself. The runtime needs a generic planning/dependency facility or equivalent explicit execution phase; its exact Rust API is not decided here.

Control admission and callback-wide budget resets happen once at the host callback boundary, not once per internal interval. Internal musical intervals must not consume an arbitrary subdivision allowance simply because they are numerous.

### Separate timing order from signal order

Resolver decisions must be available before loop rendering, but this does not force all audio work into the resolver or one aggregate loop node. Playback, FX, and recording remain signal-processing operations. Where one loop's playback must feed FX whose output is recorded, expose separate playback and recording operations as needed rather than constructing an artificial `loop → FX → same loop` cycle.

If execution outcomes affect later timing decisions, return compact observations to the next resolver phase after the interval completes. This is a phase boundary in the executor, not a zero-delay signal feedback edge or a trip through the external control queue. Such outcomes must be observed at their semantically correct boundary; they cannot be postponed merely to make whole-block preplanning convenient.

### Optional optimization: publish longer plans

When timing is independent of intermediate rendering outcomes, the resolver may publish multiple timestamped spans ahead of consumers. This can let each loop renderer process its own spans without traversing the whole signal graph at every loop boundary.

Treat this as an optimization of the same semantics, not a requirement. It needs sufficient prepared plan storage and proof that no unobserved rendering outcome invalidates the plan. Incremental execution remains the baseline when those conditions do not hold; exhausting an optional planning buffer must not silently change musical timing.

### Implementation sequence

1. Extract the existing loop/composite timing behavior into a single-owner resolver, with tests for its timestamped decisions independently of audio rendering.
2. Define interval decision views and execution observations, separating authoritative timeline state from per-loop sample-processing state.
3. Add explicit resolver-to-consumer dependencies and an interval execution lifecycle to the runtime; retain the existing signal DAG ordering.
4. Implement separate loop renderers against those decisions, preserving playback/FX/recording dependencies.
5. Compare output and transition journals across different host block partitions, including finite cascades and dense one-sample-loop workloads. Verify termination handling and allocation-free processing.
6. Optimize planning batches or traversal scope only after semantic equivalence and representative performance are established.

The remaining design discussions concern the precise API, same-frame trigger semantics, and which execution outcomes affect timing—not whether independent loop processors should discover synchronization from one another during rendering.

## Termination, not an arbitrary work allowance

### Time-advancing work

```text
frame 0 → frame 1 → frame 2 → ... → end of block
```

A finite block already bounds the number of strictly time-advancing iterations. A one-sample loop may be expensive, but its repeated wraps are legitimate work. It should not fail solely because it exceeds a chosen wrap count.

### Same-frame propagation

```text
at frame 40:
A starts B → B starts C → C starts A → ...
```

Same-frame propagation needs explicit termination semantics. Finite cascades may resolve at the same sample; they must not acquire a mandatory quantum of delay per hop.

Prefer structural validation and defined event semantics over an arbitrary iteration counter. Possible mechanisms include rejecting instantaneous causal cycles and identifying trigger occurrences so the same occurrence cannot circulate indefinitely.

A cycle in **sync relationships** is not automatically an infinite instantaneous causal cycle. Do not reject valid mutually related loops merely because their relationship graph contains a cycle.

The following details still need specification:

- Identity and delivery rules for a trigger occurrence.
- Whether repeated same-boundary actions produce new occurrences.
- Deterministic ordering and conflict resolution for simultaneous actions.
- Detection and reporting of nonterminating instantaneous behavior.

An unrestricted application callback that can invent fresh same-frame events cannot be made termination-safe by a universal cycle detector. The coordination interface must constrain such behavior, or explicitly treat it as trusted application code. Arbitrary callbacks are not a substitute for defined trigger semantics.

## Limits that remain appropriate

Removing arbitrary musical-work limits does **not** mean allowing heap growth, blocking, or unlimited storage during rendering.

| Concern | Policy |
|---|---|
| Legitimate finite musical transitions | Execute all required work; the application owns its CPU cost |
| Prepared buffers, event queues, resource pools | Keep fixed capacities and explicit exhaustion behavior |
| External command ingestion | Service a finite snapshot/batch; do not chase a producer indefinitely |
| Same-frame causal feedback | Ensure termination through semantics and validation; report invalid behavior |
| Locks, waits, and external I/O | Keep them out of rendering |
| Deadline exceeded | An xrun is possible; do not alter musical ordering to hide it |

Transition count need not equal storage consumption. The coordinator can process successive boundaries using reusable scratch rather than materializing every event in the entire block. Work that does require retained events still needs sufficient prepared storage.

## Determinism

For the same initial state, engine timebase, and timestamped inputs, musical results should not depend on how the host partitions samples into callbacks, assuming valid inputs and sufficient prepared resources.

In particular:

- Do not defer a transition because a per-callback musical-work budget ran out.
- Define simultaneous-event ordering and conflict resolution explicitly.
- Preserve intended event timestamps independently of processor traversal order.
- Test equivalent executions under different host block partitions, including boundaries exactly at callback ends.

This does not promise identical live-input arrival times, xrun-free playback, or bit-identical audio across different sample rates and DSP environments. Sample-rate/timebase policy is a separate decision.

## Implications for `rt_runtime`

The current runtime already supports DAG signal dependencies and sample-frame scheduled actions, but it does not yet provide the complete inter-loop coordination interface described here.

Its scheduler currently couples admission to event/result credits and a conservative subdivision allowance. Those limits must **not automatically become limits on internal loop wraps or synchronization transitions**.

Retain bounded ownership transport and external command processing. Provide an integration path through which the application coordinator can drive the required finite musical work without an arbitrary internal-transition cap. Repeatedly calling the existing `GraphRuntime::process` is not automatically that integration: it also resets callback budgets and snapshots control work again.

The resulting processing contract should be stated accurately:

> Allocation-free, deterministic processing with finite application-dependent work—not a guaranteed per-callback CPU bound.

Expose transition/subdivision counts and timing diagnostics so the application can identify expensive sessions and apply its own workload policy. Such diagnostics must not change execution semantics.
