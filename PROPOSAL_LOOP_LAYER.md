# Proposal: a portable loop-execution layer

## Recommendation and governing decision

Build a portable `shoop_loop_engine` around a **loop timeline resolver and separate audio/MIDI renderers**, rather than replacing the entire backend at once or starting with independently clocked loop processors.

Its purpose is:

> Resolve loop and composite timing coherently, then execute the resulting audio/MIDI intervals on top of `rt_runtime`.

`INTER_LOOP_PROCESSING.md` governs inter-loop behavior: preserve sample-accurate interaction within a host block, settle finite same-frame cascades without per-hop delay, and do not truncate legitimate musical work at an arbitrary transition or subdivision quota. The engine owns correctness and termination; the application owns the CPU cost of the musical workload it requests.

This proposal is not an implementation plan or a claim that the required runtime APIs already exist. `PROPOSAL_RT_ENGINE.md` and `DESIGN_RT_ENGINE.md` describe the generic foundation; the latter also identifies the proposed coordination extensions. `DONE_INITIAL_CRATE.md` remains the historical record of the delivered initial crate.

## Overall layering

| Layer | Responsibility |
| --- | --- |
| Application/session policy | Tracks, scenes, editing and preparing composite programs, user actions, undo, persistence, scripting |
| Loop execution — build next | Authoritative loop/composite timing, trigger resolution, coordinated transitions, and audio/MIDI execution of interval decisions |
| `rt_runtime` | Generic planning lifecycle and dependencies, signal DAG execution, prepared decision access, resources, ownership transport, admission, and retirement |

The planning facilities in the last row are proposed additions, not capabilities of today's `Processor::process` interface.

Host adapters connect these layers to native audio/MIDI facilities or an AudioWorklet. They must not contain another implementation of musical semantics.

- Session policy decides **what should happen** and prepares declarative programs or requests.
- The loop layer defines **exactly what those requests mean for timing, samples, MIDI, and recorded content**.
- The runtime supplies **the generic machinery for executing those decisions without callback allocation, locks, waits, or external I/O**.

“Application-level resolver” means Shoop-specific functionality, not code running on the GUI thread. The resolver executes serially with its consumers under the render owner.

## Scope and ownership

### 1. `LoopTimelineResolver`: authoritative musical timing

The resolver owns:

- Logical loop positions, lengths, modes, cycle counts, and pending transitions.
- Synchronization relationships and simultaneous launch decisions.
- Trigger occurrences, delivery, and same-frame propagation.
- Execution semantics of prepared composite timelines.
- Selection of the next relevant musical boundary.
- Reactions to recording failures and other execution outcomes that affect later timing.

The runtime retains the absolute engine-frame timebase. The resolver owns the musical interpretation of that timebase; renderers do not advance competing logical loop clocks.

Small per-loop state machines can remain independently testable components. They are owned and coordinated by the resolver, not independent processors discovering synchronization by reading one another's partially updated state.

The existing `BasicLoop` / `AudioMidiLoop` split is useful evidence for separating mechanics from media execution. Extract and specify the existing behavior, but do not port its queues, mirrors, backend dependencies, or incidental traversal-order semantics wholesale.

### 2. Audio/MIDI renderers: execute interval decisions

Renderers own media-processing state, not authoritative synchronization state:

**Audio:** prepared content access, playback cursors, gain, boundary smoothing, capture handles, and sample-processing state.

**MIDI:** timestamped event access, playback cursors, note/controller state, capture state, and defined cleanup/restoration on start, stop, wrap, and seek.

MIDI is not an audio-channel abstraction with a different sample type. Its discontinuities require state transitions as well as position changes.

A coherent decision view conceptually specifies:

> During absolute frames `[a, b)`, use this mode and content revision, play from this logical position, and capture these inputs.

Implementation cursors are permitted, but must follow the published decision rather than become another synchronization authority. Renderers return compact observations when an outcome can affect future timing.

### 3. One logical loop need not be one indivisible graph node

All channels of a logical loop share the resolver's position and transition decisions. That does not require playback and recording to execute together.

For example:

```text
Loop A playback → FX → Loop A recording
```

Playback and recording must be separable operations where routing requires it. Combining them into one indivisible processor would create an artificial signal cycle or force DSP into the coordinator. Their logical ownership and capture coherence remain coordinated even when their execution is separated.

## Timing dependencies and signal dependencies

```text
Timestamped requests / prepared musical programs
                       │
                       ▼
              LoopTimelineResolver
                │       │       │
                │ explicit decision/order dependencies
                ▼       ▼       ▼
              Loop A  Loop B  Loop C
              media execution operations
                       │
                signal DAG / FX / outputs
```

Each affected consumer has an explicit dependency on the resolver's decisions. Ordering must not depend on node IDs, insertion order, or dummy audio/MIDI connections.

Keep four concepts distinct:

1. Musical synchronization relationships.
2. Instantaneous causal relationships between trigger occurrences.
3. Executor ordering and decision-publication dependencies.
4. Audio/MIDI signal routes.

A cycle in synchronization relationships is not automatically a nonterminating instantaneous causal cycle or an illegal signal feedback path. The runtime can validate generic execution dependencies; the loop layer must validate musical causal semantics.

## Incremental interval execution

The baseline lifecycle is:

```text
Begin host callback once; admit a finite snapshot of external work
    ↓
Settle transitions and finite cascades at the current boundary
    ↓
Publish the next nonempty interval's coherent decisions
    ↓
Execute dependent renderers and the signal DAG for that interval
    ↓
Collect observations needed by subsequent timing decisions
    ↓
Return to the resolver, or finish the host callback
```

Decision and observation storage is prepared before rendering. Views are read-only to consumers and storage is reused only after all consumers finish. Successive intervals need not accumulate in a retained whole-block transition list.

No participant may cross a relevant boundary before its effects have been resolved. If an outcome can invalidate decisions partway through a proposed interval, its observation/activation boundary must be made explicit rather than reporting it too late. Recording failure, capture completion, and publication of newly playable content need precise contracts here.

Boundary-at-callback-end semantics must ensure an occurrence is neither lost nor delivered twice. For identical timestamped inputs and sufficient prepared resources, musical output and transition journals must not depend on callback partitioning.

The resolver is a planning node with executor support, not an ordinary processor that happens to run first for an entire block. Repeated calls to the existing `GraphRuntime::process` are not a substitute: they would snapshot external work and replenish callback budgets repeatedly.

## Termination and workload policy

### Time-advancing work

Each rendered interval must make progress toward the finite host-block end. A one-sample loop may legitimately wrap at every sample. Such work must not be deferred, dropped, or rejected merely because it exceeds a chosen wrap/subdivision count.

### Same-frame work

Define a constrained trigger model before implementation. Open semantic questions include:

- Trigger-occurrence identity and delivery rules.
- Whether a delivery can create a fresh occurrence at the same frame.
- Whether starting an already-playing loop creates another trigger.
- Ordering and conflict resolution for simultaneous start, stop, replace, wrap, and recording-completion actions.
- Validation and reporting of nonterminating instantaneous behavior.

Once-per-occurrence delivery can prevent circulation of one occurrence, but does not by itself prevent infinite creation of fresh occurrences. Termination must follow from the supported semantics and validation, not an arbitrary propagation-round counter. Do not reject all cyclic synchronization relationships as a shortcut.

Arbitrary callbacks capable of inventing same-frame events are not the baseline coordination interface. They would require a separate explicit trust contract, not a claim of universal cycle detection.

### Limits that remain appropriate

Retain fixed capacities for prepared storage, retained events, resource pools, snapshots, and owning completions. External command ingestion remains bounded and cannot chase producers indefinitely. Storage exhaustion remains explicit; capture failure must not silently produce a supposedly complete but inconsistent multichannel take.

Do not turn external admission credits or background-maintenance quotas into limits on required internal musical progression. An internal wrap should not consume an external command/completion slot merely to execute.

The processing contract is:

> Allocation-free, deterministic processing with finite application-dependent work—not a guaranteed per-callback CPU bound.

Expose workload counts, timings, and exhaustion diagnostics. Expensive valid sessions may xrun; diagnostics or optional optimization limits must not change transition ordering or timing.

## Responsibilities above the loop layer

Keep track/scene organization, GUI behavior, bindings, persistence, undo, plugin management, and device/browser integration outside this layer.

For composites, distinguish **editing and preparing the program** from **executing its timing semantics**. The former belongs above the loop layer; the latter belongs in the resolver so that primitive and composite timing do not become separate engines negotiating during rendering.

A request such as “launch these loops together at the next boundary of this synchronization source” is chosen by application policy and resolved precisely by the loop layer. Timing dependent on live playback or recording state must not round-trip through the UI or external control queue.

## Required generic runtime evolution

### Planning lifecycle, dependencies, and views

Add a generic planning/dependency facility or equivalent explicit execution phase. It must support interval selection, prepared decision publication, dependent signal execution, and observation collection. The exact Rust API remains open; loop modes, trigger rules, and composite semantics must not enter the generic crate.

Keep host-callback admission, accounting, and publication lifecycle distinct from internal musical intervals. Reconcile scheduled external actions and resolver-selected boundaries without delaying admitted actions or duplicating their execution.

### Typed operations and exact-frame resource activation

Do not encode recording or loop transitions as magic parameter numbers and `f32` values. Provide typed operations with explicit ownership, admission, and execution outcomes where needed.

Current resource capture start/stop commands operate at block boundaries. Exact-frame capture, coherent multichannel activation, and record-to-play transitions require prepared capacity and explicit activation/failure semantics. A compact rendering observation returns to the resolver phase, not through an asynchronous external command round trip.

### Positioning and media access

Current resource cursors are sequential. Audio seeking and MIDI event/state lookup need prepared indexing or bounded positioning primitives; they must not scan entire resources in a callback. Reuse runtime storage and leases rather than introducing a competing media store.

### Separate quota classes

Audit existing subdivision, copy, validation, and diagnostic-count limits. Preserve external admission, background transfer throttling, and ownership guarantees. Do not let those quotas silently cap required internal loop work or make musical behavior depend on host block partitioning.

In particular, distinguish work needed to execute admitted rendering from opportunistic uploads, exports, validation, and reclamation. Define preparation guarantees and explicit resource-failure behavior rather than treating a CPU quota as a musical transition rule. Diagnostic overflow needs a reporting policy, not altered musical execution.

### Coherent state publication

Publish mode, position, pending transitions, revision, and capture status through bounded mechanisms. Timing fields come from the resolver; renderer observations supply media execution status. Do not expose independently sampled mutable states as a coherent loop snapshot.

The musical resolver is necessary domain scheduling. It must not duplicate the runtime's generic signal executor, ownership transport, resource lifecycle, or retirement infrastructure.

## Implementation sequence and first milestones

1. **Specify and test resolver semantics without audio.** Extract the intended existing primitive/composite timing behavior; cover same-frame cascades, simultaneous actions, valid mutual synchronization, and invalid nonterminating behavior.
2. **Define decisions and observations.** Establish authoritative state ownership, interval-view lifetimes, exact observation boundaries, and coherent failure outcomes.
3. **Extend the generic executor.** Prove explicit planner-to-consumer dependencies and interval execution using simple renderers and synthetic outcomes; reset callback admission/accounting only once.
4. **Implement audio/MIDI renderers.** Keep playback and recording separable where signal routing requires it.
5. **Prove a coordinated vertical slice.** A loop with stereo audio and MIDI records and starts playback at an exact frame; A's boundary triggers B and B triggers C at the same frame; playback can feed an FX test processor and recording; capture failure reaches the resolver at the correct boundary; snapshots and teardown remain coherent.
6. **Verify partition-independent execution across hosts.** Compare transition journals and samples/events under different callback sizes, including one-sample loops, nonaligned lengths, callback-end boundaries, notes spanning transitions, finite dense cascades, storage exhaustion, and stalled consumers. Run native offline execution and a real non-shared-memory AudioWorklet under heap/lifetime guards.
7. **Optimize only after semantic equivalence.** Measure representative workloads before batching plans or narrowing graph traversal.

Single-loop tests remain useful building blocks, but inter-loop coordination is part of the initial architectural proof, not a feature appended after a self-contained loop processor is established. Full application integration, composite editing, and production plugin/device hosting remain later work.

## Performance and manageable complexity

Use one runtime graph, span-oriented media processing, prepared storage, and retained processor state. Start with one loop-layer crate containing clearly separated semantics, resolver, decision/observation, audio, MIDI, and runtime-integration modules. Create further crates only when an independently useful boundary emerges.

Longer preplanned span batches are optional. They are valid only when intermediate rendering outcomes cannot invalidate the plan and sufficient prepared storage exists. Exhausting that optional storage must fall back to semantically equivalent incremental execution, not delay or drop transitions.

The resolver is the architectural center, not a centralized DSP processor. It establishes coherent timing; individual renderers and the signal DAG execute media work. This makes synchronization fundamental without turning the next layer into another monolithic backend.
