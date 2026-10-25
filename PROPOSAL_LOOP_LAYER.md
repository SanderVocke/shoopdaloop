# Proposal: a portable loop-execution layer

## Recommendation

Build a small, portable loop-execution layer next, rather than replacing the entire ShoopDaLoop backend at once. A working name is `shoop_loop_engine`.

Its purpose is:

> Execute prepared audio/MIDI loops, with precisely defined timing, recording, playback, and transition semantics, on top of `rt_runtime`.

This is the first layer that understands what a loop means. It should not depend on the GUI, session files, audio devices, browser messaging, or plugin hosting.

This document proposes an architectural direction, not an approved implementation plan. The generic runtime's proposal and design are retained in `PROPOSAL_RT_ENGINE.md` and `DESIGN_RT_ENGINE.md`.

## Overall layering

| Layer | Responsibility |
| --- | --- |
| Application/session policy | Tracks, user actions, composite-loop arrangements, undo, persistence, scripting |
| Loop execution — build next | Loop positions, modes, coordinated audio/MIDI playback and recording, exact transition behavior |
| `rt_runtime` | Bounded execution, graph routing, scheduled delivery, resources, ownership, overload, and retirement |

Host adapters sit alongside these layers, connecting the engine to native audio/MIDI facilities or an AudioWorklet. They must not contain a second implementation of loop semantics.

The division of responsibility is:

- Session policy decides **what should happen**.
- Loop execution defines **exactly what that means for samples, MIDI, and recorded content**.
- The runtime supplies **the bounded machinery that executes it**.

## Responsibilities of the next layer

### 1. A small loop state machine

Separate loop mechanics from buffer processing:

- Stopped, playing, recording, and eventually replacement modes.
- Loop position, length, and cycle count.
- Start, stop, seek, and transition behavior.
- The distinction between absolute engine frames and positions within a loop.
- Explicit ordering when recording ends, playback starts, a loop wraps, or content changes at the same frame.

These mechanics should be deterministic enough to test without rendering audio.

The existing `BasicLoop` / `AudioMidiLoop` separation provides a useful starting idea, but the contracts should be rederived rather than those implementations ported wholesale. Their queues, state mirrors, and backend dependencies should not be carried into the new layer.

A useful internal interface would conceptually produce:

> For this span, play this region, capture these inputs, then perform this boundary transition.

It need not allocate a list of spans. It can produce and consume spans incrementally under explicit bounds.

### 2. Audio and MIDI channel execution

Channel execution implements media-specific behavior.

**Audio:**

- Read regions from immutable content revisions.
- Wrap playback.
- Apply gain and defined boundary smoothing.
- Capture input into prepared storage.
- Select new content at an explicit boundary.

**MIDI:**

- Read timestamped events and rebase their offsets.
- Track relevant note/controller state.
- Handle start, stop, wrap, and seek correctly.
- Define note cleanup and state restoration.
- Respect bounded event capacity and recovery behavior.

MIDI deserves its own implementation, not an audio-channel abstraction with a different sample type. MIDI discontinuities are state transitions, not merely changes in position.

### 3. Coordinated execution of a loop's channels

An audio/MIDI loop should have one authoritative position and transition decision. Its channels must not independently calculate when recording ends or playback wraps.

The starting point should be one logical loop processor owning the loop clock and channel execution state, with multiple audio/MIDI ports exposed to the runtime graph. This gives coordinated transitions a manageable implementation boundary without requiring shared mutable state between graph nodes.

Channel-processing code should remain separable internally. Later dry/wet routing or effects may justify separate graph nodes for parts of a loop. The logical ownership boundary should not permanently dictate every physical routing boundary.

## Responsibilities that stay above this layer

Keep the following out of the first implementation:

- Track and scene management.
- Composite-loop editing and arrangement policy.
- GUI selection, button behavior, and keyboard/MIDI bindings.
- Session serialization and file loading.
- Undo history.
- Plugin discovery and process management.
- Native/browser transport details.

For example, the upper layer might request:

> Launch these loops together at the next boundary of this synchronization source.

The execution layer makes that request precise and coherent. It does not decide which loops the user selected or how an arrangement expands into such requests.

This separation does **not** mean all timing decisions belong on the control thread. If a boundary depends on live recording or playback state, a bounded render-side coordinator must resolve it. Round-tripping through the UI thread would undermine sample accuracy.

## Expected extensions to the generic runtime

The runtime is a tested foundation, but it is not yet a complete looper substrate. Concrete use cases in this next layer should drive the necessary extensions.

### Typed processor/domain operations

The current scheduled processor interface handles parameters and MIDI. Operations such as finishing recording and starting playback should not be encoded as magic parameter numbers and `f32` values.

Provide a bounded, typed operation path with explicit admission and execution outcomes. The runtime should supply generic delivery and ownership mechanisms; the loop layer should define the operations' musical meaning.

### Exact-frame recording lifecycle and coordinated transitions

Resource capture currently starts and stops through block-boundary commands. Sample-exact recording transitions, and coherent changes across several channels, need a carefully designed preparation/activation boundary.

Preparation should establish the required capacity and ownership before execution promises are made. The precise contract for coordinated activation and failure must be defined before implementation.

### Efficient positioning within media

Resource cursors currently support sequential reads. Arbitrary audio seeking and MIDI event/state lookup must not become whole-resource scans in a callback.

Prepared indexing or bounded positioning primitives will be needed. Their memory and work costs should be explicit.

### Coherent domain state publication

The UI will need loop mode, position, pending transitions, content revision, and capture status. Publish that information through bounded mechanisms, not locks around live processors.

The guiding rule is:

> Add generic mechanisms to `rt_runtime`; keep loop semantics in the loop layer.

Do not implement a second scheduler, resource owner, or retirement system above the existing runtime. Necessary extensions should be validated through concrete loop scenarios rather than treating the initial runtime API as either untouchable or a reason to build parallel infrastructure.

## Performance and complexity principles

Architectural choices matter more than the number of crates:

- Use one runtime graph, not one runtime per loop.
- Process spans of samples instead of introducing per-sample abstraction or command overhead.
- Reuse runtime resources and leases rather than creating a second competing media store.
- Keep structural preparation off the callback and retain processor state across graph edits.
- Budget playback reads, recording writes, MIDI events, and loop-boundary work explicitly.
- Define coordinated failure behavior. A channel exhausting recording capacity must not silently produce a supposedly complete but inconsistent multichannel take.

Initially, implement one crate with clearly separated modules. Do not immediately create separate crates for clocks, channels, transport, recording, and synchronization. Split crates when there is a useful independent boundary, not merely a different noun.

## First milestone: a narrow vertical slice

Demonstrate:

> One logical loop with stereo audio and a MIDI channel can record, finish recording at an exact frame, start playback without an unintended silent block, wrap repeatedly, stop cleanly, and export a coherent snapshot.

Verify this with:

- Different callback sizes.
- Loop lengths not aligned to callback boundaries.
- Very short loops.
- Events exactly on recording and wrap boundaries.
- MIDI notes spanning a boundary.
- Capture exhaustion and stalled consumers.
- The same command journal in native offline execution and a real AudioWorklet.

Then add a second loop and prove synchronized transitions before moving on to tracks, composites, effects, or UI integration.

## Summary

Build the loop-execution kernel next. It is the smallest layer that adds ShoopDaLoop's essential behavior while testing whether the runtime's timing, resource, and ownership abstractions are genuinely suitable for the application.
