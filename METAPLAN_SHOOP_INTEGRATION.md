# Metaplan: integrating the new stack into ShoopDaLoop

## Context

Written on **2026-10-27**, after the initial [ARCHITECTURE.md](ARCHITECTURE.md) and
[PROPOSAL_TRANSPORT.md](PROPOSAL_TRANSPORT.md) were drafted, while a separate
executor was still implementing [PLAN_LOOP_LAYER.md](PLAN_LOOP_LAYER.md).

The proposal answers what should follow completion of that plan. The long-term
goal is ShoopDaLoop feature parity with no noticeable user-facing difference,
while preserving room for new capabilities such as separating UI and engine into
different processes. It weighs further architecture design, protocol
implementation/demonstrators and continued vertical stack development.

This is a proposed sequence for subsequent work, not a change to the ongoing
loop-layer plan, authorization to start implementing later milestones, or a
claim that the described integration exists. The original proposal is reproduced
verbatim below.

---

**I would make the next milestone a parity-driven engine-composition and communication slice—not a full transport implementation, and not simply another layer built in isolation.**

The next milestone should answer:

> Can the new engine support the application’s actual composition and control needs, while preserving existing behaviour, through a small interface that works locally and remotely?

That combines a little architectural design with concrete implementation. It avoids spending months completing an elegant stack before discovering a compatibility problem near the top.

## First: completion of the loop plan is not completion of loop compatibility

This is the biggest consideration.

`PLAN_LOOP_LAYER.md` deliberately excludes, among other things:

- Full legacy behaviour compatibility.
- In-place replacing/overdub modes.
- Prerecord, preplay and latency compensation.
- Full dry/wet application-mode policy.
- Production plugin/device integration.
- Persistence, undo, scripting and UI integration.

Those exclusions are reasonable for the current milestone. But several are fundamental to the final user experience. They are not all features that can simply be added by putting a session layer on top.

**Before committing to the next major abstraction, we should understand where those missing capabilities will fit.**

## 1. Start with a focused parity and architecture exercise

I would first extend `ARCHITECTURE.md` enough to identify the remaining responsibilities—not attempt to design every API.

The deliverables should be:

### A feature-parity map

For each current user-visible capability:

| Capability | Existing behaviour/reference | Intended owner in new stack | Gap or architectural risk |
|---|---|---|---|
| Dry/wet recording | Existing scenarios/tests | Application policy plus loop/media mechanisms | Which mechanisms are missing? |
| Prerecord/latency compensation | Existing timing tests | Explicit capture/playback mapping | Does the current storage lifecycle accommodate it? |
| Live waveform display | Existing publication behaviour | Telemetry/media adapter | Throughput and freshness |
| Plugin state replacement | Existing host lifecycle | Plugin component plus coordinator | Preparation/activation constraints |
| Save/load and undo | Existing session operations | Higher-level orchestration | Media retention and atomicity |

This should be grounded in the current application and tests, not just an inventory of API methods.

### A responsibility map for the remaining stack

Identify where the following will live:

- Track/bus/routing composition.
- Processor catalog and lifecycle.
- Application-facing engine service.
- Device and browser hosting.
- Persistence/undo integration.
- Protocol adapters.

That would also distinguish **existing modules we can reuse** from genuinely new components. We should not assume everything above the loop layer needs rewriting.

### An explicit compatibility policy

“No noticeable difference” should include timing, MIDI behaviour, UI responsiveness and load/save behaviour—not just available buttons.

Where the new engine intentionally improves or changes existing semantics, those differences should be identified explicitly. We should preserve intended user behaviour without reproducing every incidental limitation of the old implementation.

**This is a bounded design task, not a request to finish the entire architecture on paper.**

## 2. Resolve the highest-risk composition questions with one executable slice

I would then implement a small **engine composition/service boundary** that combines:

- The loop engine.
- A processor supplied by another crate.
- A meaningful custom operation for that processor—not only a floating-point gain parameter.
- Coherent graph/configuration activation.
- State/meter publication.
- Media import and export.

This directly tests the current gap we discussed: the loop facade fixes the planner and operation/result types, while independently developed processors may need their own control vocabulary.

We should settle that through a real example before freezing a general protocol-adapter API.

The important result is **one engine and signal graph with clear ownership**, not several nested engines whose timing and lifetimes must be reconciled.

### Include a parity-sensitive recording scenario

For example:

```text
live audio/MIDI ─┐
                ├── custom processor ──┬── output
loop playback ──┘                      └── capture
```

Exercise record → immediate playback, retained content replacement, a custom processor operation, and state/export reads.

Also include a focused experiment around an excluded but important feature—such as latency-compensated dry/wet capture or prerecord. It need not implement all of that feature yet. It should establish whether the proposed ownership and timing boundaries can support it without a fundamental redesign.

**The custom component can stand in for a plugin host initially.** Bringing up a complete production plugin host would introduce too many unrelated variables at this stage.

## 3. Implement the minimum reusable protocol needed for that slice

Once the local composition contract is credible, implement a deliberately small part of `PROPOSAL_TRANSPORT.md`:

- Common envelope and routing.
- Loop and custom-processor domains.
- Admission versus terminal outcomes.
- Bounded result retention and recovery.
- Telemetry.
- Chunked bulk transfer.

Then run **the same scenario** through:

1. Native in-process typed calls.
2. Separate Wasm instances in an actual AudioWorklet.
3. A native engine process reached through local IPC.

I do think the IPC demonstration is worth doing early. It tests whether we have genuinely separated control semantics from local Rust ownership. It also exercises one of the expansion capabilities you actually want.

But I would keep it small:
- One cooperating client.
- One engine process.
- Defined disconnect behaviour.
- No general session resumption system.
- No cross-machine deployment yet.

**Network transport can follow later.** Once the framing, identities and IPC boundary are sound, basic network carriage is comparatively straightforward; robust remote operation is a separate project.

### Do not make these three unrelated demos

They should be three deployments of one application scenario, with comparable semantic results.

That gives much stronger evidence than three “hello world” applications.

## 4. Make the following milestone use the actual ShoopDaLoop application

After that foundation, I would avoid continuing indefinitely with independent demonstrators.

The next milestone should connect an **opt-in new backend to the existing application-facing boundary**, reusing the current UI and relevant application modules wherever practical.

Start with a restricted real workflow, then expand it.

Keep the old backend selectable as:
- A working fallback.
- A behavioural reference.
- A performance baseline.

I would prefer backend selection at a clear lifecycle boundary, such as startup, rather than silently sending individual unsupported operations to the old engine. Mixing ownership and musical state between two engines would obscure the very problems we need to expose.

This is where the parity map becomes the implementation order: close the gaps required by real workflows, extending lower layers where necessary.

## What should count as success for the next milestone?

I would require evidence that:

- **Composition works:** loops and an independently supplied processor coexist and can both be controlled.
- **The API boundary survives deployment changes:** one logical scenario works in-process, in a real worklet and over native IPC.
- **The musical result is correct:** samples, MIDI and transition outcomes match expectations for equivalent admitted inputs.
- **The UI paths are viable:** meters remain responsive, and buffer transfers have measured throughput/freshness.
- **Lifecycle is sound:** stalled clients, cancellation, disconnect and shutdown preserve bounded ownership and reliable outcomes.
- **Performance is understood:** compare against the current engine on representative workloads, including callback tails—not merely average CPU.
- **There is a credible path for the excluded parity features:** no unresolved assumption that “the session layer will somehow handle it.”

It should **not** require a finished session implementation, every protocol operation, real plugin hosting, shared-memory acceleration or production networking.

## My recommended order

| Step | Main purpose |
|---|---|
| **1. Parity map + targeted architecture extension** | Identify missing behaviour and assign responsibilities |
| **2. Local composition slice** | Resolve planner, custom-operation and lifecycle extensibility |
| **3. Minimal modular protocol + native/browser/IPC versions of that slice** | Prove the separation and measure communication costs |
| **4. Opt-in integration into real ShoopDaLoop** | Drive further development through actual user workflows |
| **5. Close parity gaps, then expand deployment capabilities** | Reach replacement readiness without losing extensibility |

**So I would choose a combination, but with a clear centre of gravity: build the smallest end-to-end slice that retires the composition and compatibility risks.** Architecture documentation should guide that slice; protocol infrastructure should serve it; subsequent vertical layers should be justified by the real application rather than built speculatively.
