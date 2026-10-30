# Design: first opt-in new engine backend

[Main plan](PLAN.md) · [Architecture](ARCHITECTURE.md) · [Validation](VALIDATION.md) · [Loop features assessment](LOOP_FEATURES_ASSESSMENT.md)

Scope agreed with the user on 2026-10-30: native and browser, the core looping
feature set without session save/load, prepared capacity pools, and work continuing
on `feat/loop-execution-layer` without a separate branch or pull request.

> **Design in progress.** This is a design document; concrete steps, acceptance
> criteria and verification will be written once the design is final. The design
> below was agreed after the problem, goals, scope and design rules further down
> were written. Before turning this into a plan, audit them, `ARCHITECTURE.md` and
> `PLAN.md` for consistency with it. Known differences: the crate layout and names,
> the protocol layer, native traffic going through the protocol, and render
> performance benchmarking, which is deferred beyond this milestone.

## Design

### Layers

Two transport layers separate the application from the render path. Only the outer
one varies by environment.

```text
Application side                              Engine side (same process, worklet, other process or machine)
shoop_egui / shoop_app
  │ Backend trait (tracks, buses, sessions, UI snapshot)
shoop_rt_backend (adapter: app vocabulary ⇄ engine protocol, state mirror)
  │ protocol client
  ═══ T2: carrier (in-process call │ MessagePort │ later IPC │ later network) ═══╗
                                                                                 ║ carrier server
                                  shoop_rt_engine control endpoint (protocol → runtime commands, telemetry, bulk reads)
                                          │ T1: rt_runtime control/completion queues (same memory, lock-free, owning)
                                  shoop_rt_engine render owner (one runtime process call per callback)
                                          │
                                  host: drivers (dummy, CPAL) │ AudioWorkletProcessor │ later a process main
```

| Layer | Crate | Responsibility |
| --- | --- | --- |
| Runtime | `rt_runtime` | Generic graph, scheduling, resources and the T1 queues |
| Loop parts | `shoop_loops` | Musical timing, media and capture |
| Engine | `shoop_rt_engine` | The application's real-time composition: node enum, prepared pools, the signal chain (gain, balance, mute, monitoring, meters, Master), structural transactions, the control endpoint and the render owner. All engine semantics, compiled once for every environment |
| Protocol | `shoop_rt_protocol` | Versioned plain-data schema, capability and limit descriptor, connection lifecycle, three lanes (reliable requests and results, latest-value telemetry, chunked bulk), carrier traits and codecs. No semantics |
| Hosts | `shoop_rt_host_native`, `shoop_rt_host_worklet` (later a process host) | Own the engine, pump audio and messages, open devices. No semantics |
| Adapter | `shoop_rt_backend` | The `Backend` trait in terms of the protocol: ID mapping, snapshot mirror, capability gating, session documents |

The composition root (the native `shoopdaloop` binary or the browser bootstrap)
chooses a host and a carrier and hands the adapter a protocol client. The adapter
does not know which environment it talks to.

### Principles

- **One implementation of the semantics.** The engine, the protocol and the adapter
  exist once. Hosts and carriers differ per environment and contain no semantics.
  Native traffic also goes through the protocol: its carrier passes typed protocol
  values by direct call, without serialization, so there is no second control path.
- **Bounded control endpoint.** The engine's control endpoint is bounded and
  allocation-free in every environment, because in the worklet it runs on the audio
  thread between render quanta. The same code runs on the application thread
  natively, in the worklet, and on a carrier thread in a separate process.
  Operations that move data, such as waveform reads, are chunked.
- **Vocabulary boundaries.** Application concepts end at the adapter; the engine
  speaks only engine terms (strips, loop slots, routes, operations, telemetry).
  Engine-side crates do not depend on `shoop_app_api` or other application crates;
  driver configuration and discovery are engine-owned protocol data that the adapter
  maps. Musical authority stays in the `shoop_loops` resolver.
- **Carrier-neutral protocol.** The protocol defines from the start:
  - a connection handshake with version, capabilities and pool limits;
  - correlation IDs and credit-based flow control per lane;
  - full-state resynchronization for a (re)connecting adapter;
  - defined disconnect behaviour.

  IPC and network carriers are then additive. Devices are opened by the host where
  the engine runs, so no audio streams over a carrier; synchronized commands are in
  musical time, so carrier latency only delays immediate actions.
- **Asynchronous everywhere.** The adapter mirrors engine state and confirms changes
  through the snapshot, natively as in the browser, so native and remote behave
  alike.
- **Environment bridges belong to hosts.** Web MIDI exists only on the browser's
  main thread; the browser host's main-thread half forwards timestamped MIDI into
  the worklet on its own lane.
- **Prepared pools cost nothing while inactive.** Inactive slots are not in the
  active graph or in the resolver's per-callback work.
- **Capacities are per epoch and growable where the host allows.** The render path
  is bounded at every instant, not fixed for the engine's lifetime. Growth follows
  the existing pattern: prepare larger structures off-render, hand them over through
  T1 and activate them at a boundary, returning the displaced owners for release
  off-render.
  - Engine pools are made of segments; adding tracks or loops beyond the current
    segments prepares another segment.
  - The protocol treats limits as current values that can change, with a
    limits-changed message, not as handshake constants.
  - IDs are slots with generations, never indices bounded by an initial capacity.
  - Hosts declare whether they can grow. Native hosts, including a separate process
    or machine, can; the browser worklet is bounded by a preallocated maximum.
  - The first milestone may ship with fixed segments; growth in the engine and
    adapter is then additive. Growing `rt_runtime`'s executor tables and resource
    store and `shoop_loops`' ledger and resolver state is later work inside those
    crates.
- **Structural changes never interrupt audio.** Graph, topology, node-pair and
  capacity changes are staged off-render and activated at an exact frame within
  normal callbacks. Removed or rerouted signal paths fade out over a short prepared
  ramp first, so changes do not click. Changing the audio device, sample rate or
  buffer size is the only operation that restarts audio, and it is an explicit
  lifecycle operation, never a side effect of editing tracks or loops.
- **Render performance is not gated in this milestone.** Benchmarking the render path
  against the legacy backend is deferred to later work.

This is the first concrete instance of the
[proposed modular protocol](ARCHITECTURE.md#proposed-modular-protocol). Its domain
schemas (loops, strips and mixer, telemetry, bulk) start as modules of one crate and
are split only when a component needs its own schema without the engine.

### Naming

- **Runtime** means `rt_runtime` only.
- **Engine** is the application's real-time composition.
- **Host** is environment glue; the worklet is one host.
- **Backend** is the application adapter.

The feature is the "new engine backend".

## Problem

The application talks to its engine through the `Backend` trait
(`src/rust/shoop_backend/src/lib.rs`, 57 methods). Every implementation runs on the
legacy engine: `NativeBackend` natively and `EngineBackend` inside the browser's
AudioWorklet. The new stack — `rt_runtime`, the `shoop_loops` parts and the patterns
proven by `shoop_engine_service` — has never driven the real application. The
service is a one-loop, stereo, 64-frame demonstrator, the legacy drivers own the
legacy engine and allocate in their callbacks, and the application has no way to
describe or present a backend that supports only part of the feature set.

## Goals

1. A new `Backend` implementation on the new stack that the user can select instead
   of the legacy backend, natively and in the browser.
2. It supports the core looping workflow live and integrates with the existing UI,
   scripting and settings without broken or silently ignored controls.
3. Everything it does not support is declared, presented as unavailable and refused
   explicitly; documents that need it are rejected, never partially applied.
4. The legacy backend stays the default and keeps its behaviour.
5. The work establishes the reusable pieces later parity work builds on: a
   capability descriptor, shared backend contract tests, an application engine
   composition with prepared pools, real-time-safe driver adapters, and a native and
   a browser carrier between control and render.

## Scope

**Supported (the core set):**

| Area | Supported behaviour |
| --- | --- |
| Tracks | Create and remove regular (direct) tracks with 1 to the pool's maximum audio channels per direction and optional MIDI in/out; the sync track; input gain, balance and monitoring; output gain, balance and mute; regular default playback mode; on-screen-piano MIDI injection |
| Loops | Add loops to tracks from prepared slots; sync source; stopped/playing/recording transitions, immediate or after N sync cycles with the legacy supersession rules; clear; loop gain and balance; boundary smoothing |
| Views | Loop mode, next mode, transition delay, length and position; track and loop peaks; MIDI activity; waveform and MIDI detail reads |
| I/O | Audio and MIDI connections between track ports and host ports on each driver; the Master bus with its gain, balance and mute; mixer routes from tracks to Master; the global FX control port, accepted and ignored |
| Session | New session: replacing the session with a structure-only document of the core set |
| Status | Sample rate, buffer size, driver state, xruns, callback load |

**Native drivers:** dummy and CPAL with midir, on Linux, macOS and Windows.
**Browser:** the AudioWorklet build, with the existing microphone, output and
Web MIDI permission flows.

**Refused and presented as unavailable:** saving and loading sessions with content,
composite loops, grab, replacing and both dry/wet modes, processor tracks (the
processor catalogs are empty), track and bus FX, latency compensation and take
alignment, loop content import, length and timing edits, buses other than Master,
and runtime driver switching (restart with other settings instead).

**Out of scope:** JACK, IPC or network carriers, general protocol infrastructure
beyond what these two carriers need, the loop-library features in the
[assessment](LOOP_FEATURES_ASSESSMENT.md) other than gain and balance, and changes
to the legacy engine, legacy drivers or `EngineBackend`.

## Design rules

- **Respect the architecture.** Selection happens at a lifecycle boundary, no
  operation is silently routed to another engine, unsupported documents are rejected
  whole, and render paths keep the callback contract. Preparation happens before
  rendering or off-render; render paths only use prepared, finite capacities.
- **Three new crates.**
  - `shoop_rt_engine`: the application engine composition and its domain schema.
    It is platform-neutral and compiles to raw Wasm without imports.
  - `shoop_rt_backend`: the control side and the `Backend` implementation, with the
    native driver adapters and the browser client.
  - `shoop_rt_worklet`: the raw-Wasm AudioWorklet module.

  Driver vocabulary comes from `shoop_app_api`, not the legacy engine.
- **One composition, two carriers.** The same engine composition renders natively
  and in the worklet. Control reaches it through a typed in-memory carrier natively
  and a bounded record carrier over `MessagePort` in the browser. Both carry the same
  domain schema in three lanes with separate capacity semantics: reliable requests
  and results, latest-value telemetry, and chunked bulk reads. This is the minimal
  case of the [proposed modular protocol](ARCHITECTURE.md#proposed-modular-protocol),
  not a general framework.
- **The application owns the composition.** The engine owns one `ResourceStore` and
  one `GraphRuntime` whose node enum wraps the loop parts and the application's own
  processors: input and output gain, balance, mute and monitoring, metering, and
  summing into Master. Loop-specific behaviour stays in `shoop_loops`; the
  application signal chain stays out of it.
- **Prepared pools.**
  - Track slots: host port slots, channel processors and meters.
  - Loop slots: node-pair templates, capture groups and media rows.
  - The Master bus.

  All are prepared at start-up. Structural edits use the loop library's topology
  and graph staging over these templates.
- **The control side never blocks on render.** `poll()` assembles the snapshot from
  telemetry and terminal results. IDs are assigned on the control side when a slot is
  reserved and confirmed through the snapshot; failures come back as
  `mutation_failures`.
- **Legacy timing semantics.** Map `cycles_delay` onto sync-source pulses
  (`Timing::AfterTicks`). A nearer planned transition supersedes later ones, and a
  loop without a sync source that is not playing transitions immediately, as legacy
  `plan_transition` does.
- **Worklet discipline.** The worklet module has no Wasm imports, as the existing
  worklet does. Its engine is prepared before the first render quantum, and messages
  are applied to prepared owners within bounded per-quantum work.
- **Minimal loop-library change.** The only planned `shoop_loops` change is gain
  and balance. Anything else the backend needs there is a documented plan revision.
- **Do not touch the legacy path.** No changes to the legacy engine, its drivers,
  `NativeBackend` or `EngineBackend` beyond reporting capabilities and exposing the
  shared contract tests.
- **Project rules apply.** Follow `AGENTS.md` and `.agents/`. Build through
  `nix develop` with `RUSTFLAGS="-D warnings"`, run `cargo fmt --all` before
  committing, use `#[shoop_test]` and run `check_shoop_test_usage.py` when tests
  change, and add new source files to the tracing coverage inventory.
