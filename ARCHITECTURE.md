# Architecture overview

## Purpose and scope

This document describes part of the architecture intended to support eventual
integration of the new engine into ShoopDaLoop at feature parity. It is **not a
claim that feature parity or production integration has been achieved**.

For now, the architecture covers only:

- The generic real-time runtime, `rt_runtime`.
- The loop-execution layer, `shoop_loop_engine`.
- A proposed modular, transport-independent control protocol and its adapters.
- How another processing component, such as an audio plugin host, could participate.

**The rest of the stack is not yet described here.** In particular, the complete
session/application layer, feature-parity mapping, device integration, persistence,
undo, scripting, UI integration and migration strategy require further design.
References to a future application composition point do not specify those layers.

## Stack and communication structure

```text
UI / control-side clients
    |
    | Loop client       Other component clients
    |                   e.g. audio plugin-host client
    v
Application-selected domain protocols
    |
Common protocol infrastructure
framing / routing / correlation / flow control / lifecycle
    |
Transport adapter
in-process | browser messages/shared memory | IPC | network
    |
Engine-side domain adapters
    |
    +-- loop control API --> loop planner + playback/capture processors
    |
    +-- plugin-host API --> hypothetical plugin-host processor
    |
    v
rt_runtime
local command/ownership transport / scheduling / signal graph / resources
    |
Host-supplied audio/MIDI callback
```

This is a responsibility diagram, not a prescribed thread layout. Native hosting
can place preparation and protocol handling on non-RT threads. An AudioWorklet
must perform its bounded receiving, command adaptation and rendering work on its
rendering thread. Heavy application work remains outside that thread.

## Generic RT runtime

`rt_runtime` supplies the reusable execution machinery:

- Prepared audio/MIDI signal graphs and processor execution.
- Absolute sample-frame scheduling and an optional interval planner.
- Bounded local command delivery, results and ownership returns.
- Prepared media resources, immutable leases and incremental transfers.
- Admission, recovery and reclamation mechanisms.

It does not define loops, tracks, sessions, plugin policy, devices or browser
bindings. Hosts provide callbacks and processors; the runtime creates no internal
worker threads. Its RT contract depends on conforming host and processor code and
does not guarantee arbitrary workloads meet audio deadlines.

Its local transport moves typed Rust values within a compatible memory/ownership
environment. It is **not** an inter-process or inter-Wasm serialization protocol.

## Loop-execution layer

`shoop_loop_engine` adds musical semantics on top of the runtime:

- A resolver/planner owning authoritative loop and composite timing.
- Separate audio/MIDI playback and capture processors following interval decisions.
- Coordinated recording lifecycle, media ownership and MIDI restoration.
- A control facade for transitions, media selection, coherent queries and exports.

Musical relationships and composite programs are distinct from the audio/MIDI
signal graph. Playback and capture for one logical loop can be separated by other
processors, for example `loop playback -> plugin processing -> loop capture`.

The coordinated loop engine owns its underlying runtime. Its existing facade can
host custom effects, but this does not mean independently typed control APIs for
arbitrary components are already fully composable through it.

## Other processing components

An audio plugin host is an example of a component that need not belong to the
loop layer or depend on its musical semantics. It could supply:

- A processor implementation and its preparation/lifecycle APIs.
- A lightweight protocol schema for parameters, state transfers and outcomes.
- A client and engine-side adapter mapping that schema to the implementation.

The application selects the supported components and connects their processors
in the signal graph. Neither `rt_runtime` nor the loop library should depend on
all possible DSP modules. Selecting a protocol domain does not load executable
code or permit arbitrary processor construction during rendering.

A general local composition interface for independent processor operations and
planner coordination remains to be designed. The wire protocol must not conceal
that execution-side requirement.

## Proposed protocol architecture

Use one common communication framework with **modular domain protocols**:

- Common infrastructure handles delivery, identities, versions, credits and results.
- Each domain defines its own portable requests, results and bounded decoding.
- Application composition selects supported domains and registers their adapters.
- Carrier-specific adapters handle in-process, browser, IPC or network delivery.

A fixed application build may compose domains with ordinary enums or static
routing tables. Dynamic discovery or universal serialization of Rust types is not
required. A native in-process adapter may retain typed ownership transfer rather
than serializing every operation.

Reliable requests/results, disposable telemetry and bulk data share conventions
but need not share one queue or one capacity pool. Atomic/shared state mirrors are
an optional telemetry path, not a replacement for reliable operation outcomes.

The detailed proposal, including deployment and failure boundaries, is
[PROPOSAL_TRANSPORT.md](PROPOSAL_TRANSPORT.md).

## Implementation status and architectural continuity

The runtime and loop layer have implementations with explicitly scoped APIs and
validation. Browser command codecs and bridges exist in independent test hosts;
they are not a complete supported production integration. The loop core also has
a portable topology-data codec, not a complete inter-instance control protocol.

The common modular protocol framework and hypothetical plugin-host adapter above
are **proposed**, not delivered components. IPC/network hosting is a supported
design direction, not an implemented capability claimed by this document.

This direction extends the shared native/browser service idea in
[PROPOSAL_RT_ENGINE.md](PROPOSAL_RT_ENGINE.md). Runtime execution is described in
[DESIGN_RT_ENGINE.md](DESIGN_RT_ENGINE.md); loop responsibilities are described in
[PROPOSAL_LOOP_LAYER.md](PROPOSAL_LOOP_LAYER.md). This overview does not supersede
the governing [inter-loop semantics](INTER_LOOP_PROCESSING.md) or existing
implementation-plan acceptance criteria.
