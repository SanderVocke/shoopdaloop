> **Historical snapshot — not an active plan or current acceptance claim.**
> [History index](../README.md) · [Current plan](../../../PLAN.md) · [Original at `fffc1faa`](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/PROPOSAL_RT_ENGINE.md)
> Original text follows; only relative link destinations are adapted for browsing.
> Exact original bytes remain in the snapshot bundle. Old tasks, pauses and status apply only to that snapshot.

# Proposal: A Generic Real-Time Graph Runtime Crate

## Status and implemented next layer

This document retains the original architectural rationale for the generic runtime. The initial crate is delivered; `DESIGN_RT_ENGINE.md` describes its implementation and `DONE_INITIAL_CRATE.md` preserves the completed acceptance record. The original roadmap below is historical, not a statement that application integration is already complete.

The next-layer direction is defined by `INTER_LOOP_PROCESSING.md` and `PROPOSAL_LOOP_LAYER.md`: a Shoop-specific timeline resolver owns authoritative loop/composite timing, while separate audio/MIDI renderers execute coherent interval decisions. Composite editing remains application policy; executing prepared composite timing belongs with primitive loop timing in that resolver.

The runtime now supplies Planner/NoPlanner, explicit DecisionBindings and typed borrowed decisions/observations, interval execution within one callback, retained typed results/recovery, independently counted required media, page/MIDI indexes, prepared capture groups and staged graph permits. These are implemented extensions, not capabilities obtained simply by placing an ordinary processor first in the graph. The separate loop facade supplies musical policy, media ownership and prepared-template publication. Playback and recording can be separate signal operations; timing dependencies must not be encoded as dummy audio/MIDI edges.

Preserve fixed storage, ownership credits, bounded external admission, and throttled background maintenance. Do not use those limits to impose arbitrary caps on required finite internal loop transitions or same-frame cascades. Their semantics must guarantee termination, and their execution must preserve sample timing without mandatory quantum/per-hop delay. For this coordination path the contract becomes allocation-free, deterministic processing with finite application-dependent work, not a guaranteed callback CPU bound. Expensive valid workloads may xrun rather than silently change musical behavior.

This evolved the initial scheduler and work-accounting contracts: normal capacity is min(scheduled_events, budgets.events); internal intervals ignore subranges and renew no callback-wide allowance. Required RenderIo is independent of optional background quotas, and diagnostics saturate rather than truncate musical work. It does not require moving musical semantics into the generic crate or duplicating its transport, media store, signal executor, or retirement system. The [current implementation map](DESIGN_RT_ENGINE.md#current-implementation-map), [runtime book/reference](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/src/rust/rt_runtime/docs/README.md) and [loop guides](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/src/rust/shoop_loop_engine/README.md) describe the APIs. Historical generic-host/CI and [loop Stage 6 evidence](docs/loop_layer/STAGE6_READINESS.md) retain their source/run scope; they are not a new audit/platform campaign or an independent loop host/benchmark, which this snapshot does not supply.

The broad architecture, alternatives and migration roadmap below preserve proposal history, not delivered adapters/service integration or acceptance gates for the documentation plan. Diagram/service names and CommitResourceRevision are conceptual, not exported constructors/variants. The crate was freshly implemented, not extracted wholesale from shoop_engine. JACK/CPAL, ShoopEngineService/Session/backend/UI/protocol migration, production worklet/plugin hosting and independent packaging are not shipped here. Constant-work owner swaps do not make preparation/preflight or callbacks constant-time, crossfade sample discontinuities or guarantee browser deadlines.

## Feasibility verdict

A separate crate with a clean dependency and API boundary is feasible and potentially valuable. It does not need to be a separate repository or independently managed project.

The reusable crate should provide a **generic real-time graph runtime and control substrate** within the Shoop workspace. Shoop's loops, tracks, buses, composites, persistence, routing policy, and processor catalog should remain in Shoop-specific crates and use that runtime.

Moving the current approximately 56,000 lines under `shoop_engine/src` wholesale into a new crate would only create a Shoop-specific library with a generic name. The boundary therefore requires a layered refactor rather than a directory move.

## Why direct extraction is unsuitable

Several useful mechanisms already exist, but their interfaces are tied to Shoop:

| Existing component | Assessment |
| --- | --- |
| `shoop_engine/src/engine.rs` | Strong basis for bounded command and reclamation queues, but commands are closures over `Session`, so they cannot cross a worklet boundary. Command execution may also allocate. |
| `shoop_engine/src/graph.rs` and `graph_scheduler.rs` | Good foundations, but native schedule preparation and browser schedule application differ substantially. |
| `shoop_engine/src/content_snapshot/` | Closest match to the desired buffer snapshot API. Its contracts are useful; its runtime assumes native threads and concrete Shoop audio/MIDI types. |
| `shoop_engine/src/state_mirror.rs` | The publication mechanism is reusable, but mirror payloads are loop- and composite-specific. |
| `shoop_engine/src/session.rs` and `graph_build.rs` | Primarily Shoop domain behavior. These should not define the generic runtime crate. |
| `shoop_engine/src/app_backend.rs` | Mixes control API, domain orchestration, drivers, scheduling, snapshots, and native threading. It must be decomposed rather than exported. |
| `shoop_audio_protocol` | Entirely Shoop-specific despite being a transport protocol. |
| Worklet and client crates | Contain reusable transport ideas, but currently compile Shoop commands and backend APIs directly into the integration. |

The current native command queue also intentionally permits allocation while executing commands, and topology description allocates on the audio thread. That may be an accepted application trade-off, but it is not a suitable default guarantee for the new runtime's callback contract.

## Original recommended architecture

```text
Shoop UI
   |
   v
ShoopClient + Shoop command/event schema
   |
   +-- native in-process transport
   `-- browser MessagePort transport
   |
   v
ShoopEngineService                 remains in Shoop
   |
   v
Generic RT Runtime crate          clean internal boundary
   |
   +-- graph execution and prepared-plan swaps
   +-- bounded command/event queues
   +-- state publication primitives
   +-- revisioned bulk-buffer resources
   +-- reclamation outside the callback
   |
   +-- JACK/CPAL host adapter
   +-- AudioWorklet host adapter
   `-- deterministic/offline adapter
```

### 1. Real-time core

The core should be platform-neutral and unaware of threads, browsers, loops, tracks, buses, or sessions. It should provide:

- Block/process context.
- Audio and timestamped-event port views.
- Stable node and resource identifiers.
- Immutable execution plans.
- Prepared plan activation at callback boundaries.
- Bounded command application.
- Explicit reclamation of displaced plans and buffers.
- Counters and diagnostics.
- No locks, allocation, blocking, or destruction in the callback path.

The core should not spawn threads. This makes it usable both from native callback threads and from an `AudioWorkletProcessor`.

### 2. Control and resource layer

Reusable non-real-time facilities should include:

- Command sequence numbers and acknowledgements.
- Request/response correlation.
- Latest-state mirrors and bounded event streams.
- Plan compilation.
- Revisioned immutable resources.
- Chunked upload and download.
- Explicit overload policies. Coalesce/drop-oldest are appropriate only where the contract allows loss (such as optional telemetry), never for accepted reliable results, owning returns or required musical transitions.
- Reclamation queues.

Payload types should be supplied by the embedding application. The generic crate should not define commands such as `CreateTrack` or `TransitionLoop`.

### 3. Host adapters

Small platform-specific implementations should provide:

- Native callback adapters for JACK and CPAL.
- An AudioWorklet callback adapter.
- MessagePort framing and transferable-buffer pooling.
- Native SPSC/in-process transport.
- An offline/dummy driver.

These adapters cannot be identical, but they can implement the same contracts. The goal should be **one semantic implementation with small transport-specific edges**, not zero platform-specific code.

## Command model

Keeping a native closure queue was an original migration alternative, not the delivered portable command API or an allocation exemption. Current core operations use typed prepared data with explicit owning returns; there are no arbitrary queued closures in rendering.

A better model is:

1. The application submits an application-defined command.
2. Expensive validation and preparation happen outside the callback where possible.
3. The runtime receives a bounded prepared real-time operation.
4. At a callback boundary it performs only bounded mutation or pointer/index swaps.
5. Displaced objects are returned through a reclamation queue.
6. Completion is published asynchronously.

Commands should be classified:

- **Parameter operations:** fixed-size and cheap.
- **Topology operations:** activate a prepared plan.
- **Resource operations:** commit an already staged buffer revision.
- **Maintenance operations:** incremental and budgeted.
- **Queries:** served from mirrors and snapshots rather than synchronous callback round-trips where possible.

This classification is especially important for the browser, where parsing a large command or rebuilding a graph in the worklet can itself cause an audio glitch.

## Safe buffer fetching and replacement

`content_snapshot` provides a strong conceptual starting point. It should be generalized into a revisioned resource store based on:

```text
ResourceId + Revision + Format + Length
```

### Replacement

1. Reserve a staging resource.
2. Upload bounded chunks.
3. Validate completion outside the callback.
4. Queue `CommitResourceRevision`.
5. Swap the active revision at a block boundary.
6. Reclaim the old revision outside the callback.

The callback never copies or frees the full buffer.

### Fetching

1. Request a resource snapshot and expected minimum revision.
2. Pin an immutable revision in constant time.
3. Publish metadata and chunks asynchronously.
4. Report whether the result is current, stale, busy, or saturated.
5. Release the pinned revision after transfer.

Sharing immutable Arc chunks with a native publisher was an alternative, not the implemented store. Current native and non-shared-memory fixtures use the same single-owner store and bounded-copy/explicit-lease semantics; final destruction is off-render. Browser transferable framing belongs to the test host/adapter, not production runtime dependencies. Both can expose the same revision/currentness contract.

### Browser limitation

Without WebAssembly shared memory and cross-origin isolation, a worker cannot construct Rust heap objects that can simply be moved into the worklet's Wasm instance. Therefore:

- Large data can be prepared externally and streamed into preallocated worklet staging memory.
- Activation can still be constant-time.
- Graph and resource construction inside the worklet must be incremental or pooled.
- No-interruption guarantees are feasible for steady-state processing and commits, but arbitrary unbounded processor construction cannot be made free merely by adding a worker.

This is the main architectural constraint, not a missing abstraction.

## Original extraction boundary

After decoupling, the generic runtime crate should contain:

- Generic graph representation and plan compiler.
- Runtime ownership and callback boundary.
- Bounded command, return, event, and reclamation queues.
- Generic state mirror primitives.
- Generic revisioned snapshot and resource contracts.
- Bulk-transfer framing and chunking.
- Sequence and acknowledgement infrastructure.
- Generic native, worklet, and offline host interfaces.
- Real-time invariant checks and transport contract tests.

## What should remain in Shoop

The following should remain Shoop-specific:

- `Session` semantics.
- Loop modes, synchronization, composites, and timelines.
- Tracks, buses, mixer policy, and monitoring behavior.
- Session loading and saving.
- Backend IDs and frontend-facing state types.
- Built-in FX and OxiSynth policy and catalog.
- Shoop command and event enums.
- External port naming and automatic routing policy.
- Shoop-specific audio and MIDI snapshot metadata.

Individual DSP processors can later become separate reusable packages, but they should not be part of the generic runtime API.

## Reducing native/browser duplication

Shoop should introduce one shared service:

```text
ShoopCommand -> ShoopEngineService -> generic runtime
ShoopEvent   <- ShoopEngineService <- mirrors/events
```

Both native and browser clients should use that service:

- Native passes typed commands through an in-process bounded transport.
- Browser encodes the same commands through a versioned binary protocol.
- Both receive the same logical events and state revisions.
- The JavaScript worklet host becomes a thin generic audio/message bridge.

This would replace the current situation where native `app_backend.rs` and browser `EngineBackend` independently implement significant portions of Shoop orchestration.

## Original optional migration roadmap

### 1. Characterize current behavior

- Replay identical command journals against native and browser backends.
- Compare state, events, audio hashes, graph generations, and snapshot revisions.
- Record callback timing and queue saturation behavior.

### 2. Create an internal generic runtime crate

- Keep it in the Shoop workspace initially.
- Give it no dependencies on `shoop_*` crates.
- Extract queue/reclamation, sequence IDs, and generic publication primitives first.

### 3. Put the existing `Session` behind the runtime

- Initially treat the whole Shoop session as runtime-owned state.
- Do not attempt to generalize every loop and port immediately.

### 4. Create the shared `ShoopEngineService`

- Move common browser/native command dispatch and object identity handling into it.
- Make the current backend implementations thin clients or adapters.

### 5. Generalize bulk resources

- Refactor `content_snapshot` into generic mechanisms plus Shoop-specific audio/MIDI adapters.
- Add chunked browser staging and transferable-buffer pooling.

### 6. Extract prepared graph plans

- Move topology lowering and scheduling outside the callback.
- Activate plans through bounded swaps.
- Preserve old-plan reclamation outside the callback.

### 7. Stabilize and enforce the crate boundary

- Keep the generic runtime in the Shoop workspace unless independent distribution later becomes useful.
- Add focused examples and native/Wasm CI for the crate itself.
- Enforce that the crate has no dependencies on Shoop-specific crates.
- Treat moving it to a separate repository as an optional future packaging decision, not an architectural requirement.

## Original suggested service-migration criteria

These broad proposal criteria are retained for service migration, not used to
redefine the delivered initial crate or present loop/documentation acceptance.
The originally proposed extraction would be ready when:

- The generic crate has no `shoop_*` dependencies or domain terminology.
- Native and browser use the same Shoop service semantics.
- Large buffer operations perform bounded callback work.
- Plan and resource destruction never happens in the callback.
- The baseline browser path works without Wasm threads or cross-origin isolation.
- Transport contract tests run against in-process and MessagePort implementations.

## Conclusion

A separate crate with a clean boundary is both feasible and potentially valuable, but it should be a **small generic real-time runtime**, not today's complete `shoop_engine` under a new name.

The safest path is to establish and retain that boundary inside the existing workspace, then make the native and browser Shoop backends converge on one service. A later repository split remains possible, but is not necessary for the architecture or its maintainability.
