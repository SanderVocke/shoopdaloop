# Plan: first opt-in new engine backend

[Main plan](PLAN.md) · [Architecture](ARCHITECTURE.md) · [Validation](VALIDATION.md) · [Loop features assessment](LOOP_FEATURES_ASSESSMENT.md)

Scope agreed with the user on 2026-10-30: native and browser, the core looping
feature set without session save/load, prepared capacity pools, and work continuing
on `feat/loop-execution-layer` without a separate branch or pull request. The design
below was agreed afterwards and governs the goals, criteria, rules and stages.

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
5. The layering in the design exists and is used by both environments: one engine,
   one protocol, one adapter, with thin hosts and carriers, ready for later IPC and
   network carriers and for growable capacities.

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

**Out of scope:**
- JACK; IPC and network carriers (the protocol is designed for them);
- capacity growth inside `rt_runtime` and `shoop_loops` (the first milestone ships
  fixed pool segments);
- render-path benchmarking and performance regression testing;
- the loop-library features in the [assessment](LOOP_FEATURES_ASSESSMENT.md) other
  than gain and balance;
- changes to the legacy engine, legacy drivers, `NativeBackend` or `EngineBackend`.

## Acceptance criteria

These must not change without explicit user approval.

- **B1 — opt-in selection.** A restart-required setting and an `AppArgs` override
  choose the legacy or the new engine backend; legacy is the default. The new
  backend never routes an operation to the legacy engine.
- **B2 — platforms.** The new backend runs natively with the dummy and the CPAL and
  midir drivers on Linux, macOS and Windows, and in the browser's AudioWorklet, with
  the same core feature set.
- **B3 — core workflow.** On both platforms a user can start the application, create
  a new session, add regular audio and MIDI tracks, connect them to host ports and
  Master, record, play, stop and clear loops with immediate and synchronized
  transitions, adjust gains, balance, mute and monitoring, watch meters and loop
  progress, and inspect waveform and MIDI details.
- **B4 — explicit capabilities.** The backend reports a capability descriptor; the
  legacy backends report their full feature set. The UI disables or hides every
  unsupported action with an explanation, refusals reach the user, and the
  application rejects documents and script operations that need an unsupported
  capability instead of applying them partially, including composite documents on a
  backend without composites.
- **B5 — one implementation of the semantics.** The engine (including its control
  endpoint), the protocol and the adapter each exist once and are used by every
  environment. Native traffic goes through the protocol via the in-process carrier.
  Hosts and carriers contain no engine or application semantics. Engine-side crates
  do not depend on `shoop_app_api` or other application crates, which a boundary
  check enforces.
- **B6 — carrier-neutral protocol.** The protocol has a handshake with version,
  capabilities and limits; full-state resynchronization; defined disconnect
  behaviour; credit-based lanes; limits as changeable values with a limits-changed
  message; and IDs with generations. Both carriers pass the same protocol tests.
- **B7 — audio never stops for structural changes.** Adding, removing and rerouting
  tracks and loops is activated at an exact frame within normal callbacks without an
  output gap; removed or rerouted paths fade out over a short ramp. Only device,
  sample-rate and buffer-size changes restart audio, as explicit lifecycle
  operations.
- **B8 — prepared, segmented pools.** Pools are made of segments with configurable
  sizes. Exceeding them is refused with a visible message. Inactive slots are not in
  the active graph or in per-callback resolver work.
- **B9 — timing equivalence.** For the core workflow the new backend produces the
  same transition frames, loop lengths and recorded content as the legacy backend,
  shown by shared contract tests that run against both.
- **B10 — callback contract.** The render owner, the control endpoint, the driver
  callbacks and the worklet's render path do not allocate, free, lock or wait; tests
  guard them, and xruns are reported.
- **B11 — legacy unchanged.** The legacy backends' tests and behaviour are unchanged
  apart from reporting capabilities and the shared contract tests.
- **B12 — documentation.** `ARCHITECTURE.md` and `PLAN.md` describe the new layers
  consistently with this design; `VALIDATION.md` records the evidence; user docs say
  how to enable the new backend and what it supports; every new crate has a README.

Render performance is not an acceptance criterion of this milestone.

## Design rules

- **Follow the design above.** Its layers, principles and naming are binding for
  this work; deviations are documented revisions.
- **Respect the architecture.** Selection happens at a lifecycle boundary, no
  operation is silently routed to another engine, unsupported documents are rejected
  whole, and render paths keep the callback contract. Preparation happens before
  rendering or off-render.
- **The application owns the composition.** The engine's node enum wraps the loop
  parts and its own processors (gain, balance, mute, monitoring, meters, summing into
  Master). Loop-specific behaviour stays in `shoop_loops`; the application signal
  chain stays out of it.
- **The control side never blocks on render.** `poll()` assembles the snapshot from
  telemetry and terminal results. IDs are reserved on the control side and confirmed
  through the snapshot; failures come back as `mutation_failures`.
- **Legacy timing semantics.** Map `cycles_delay` onto sync-source pulses
  (`Timing::AfterTicks`). A nearer planned transition supersedes later ones, and a
  loop without a sync source that is not playing transitions immediately, as legacy
  `plan_transition` does.
- **Worklet discipline.** The worklet module has no Wasm imports, as the existing
  worklet does; its engine is prepared before the first render quantum.
- **Minimal loop-library change.** The planned `shoop_loops` changes are gain and
  balance, clearing a loop, and changing boundary smoothing at runtime. Anything else
  the backend needs there is a documented plan revision. *Revised during Stage 5:*
  clearing was added because the core set needs it and the loop library had no way
  to return a loop to empty content; runtime smoothing was added because the
  application changes it as a live global setting, while the loop library fixed it
  when preparing a loop.
- **Do not touch the legacy path** beyond reporting capabilities and exposing the
  shared contract tests.
- **Test as described below**, not exhaustively at every step.
- **Share the machine.** Other users work on the same host, so every build, test,
  Miri and browser command runs at the lowest priority and within a hard memory
  budget of 20% of system memory (about 25 GiB on the current 125 GiB host):
  - run each command as `nice -n 19 ionice -c 3 …`;
  - a wrapper starts the command in its own process group and checks once a second
    the total resident memory of all processes in the session; above the budget it
    stops the command's process group (SIGTERM, then SIGKILL) and reports the run as
    failed, to be repeated with less parallelism;
  - keep parallelism bounded so runs stay well below the budget: at most 8 Cargo
    build jobs and 8 test threads, at most 4 Miri batches at a time, and no heavy
    jobs running concurrently;
  - where the session's launcher can place it in a cgroup with a 20% memory limit,
    that kernel-enforced limit is preferred; the wrapper then remains as an early
    warning;
  - keep build output off the shared `/home` pool: the repository's `target`
    directories are symlinks into the scratch dataset
    (`/scratch/sander/dev/shoopdaloop/target`), and caches and reports go to `/tmp`.
- **Project rules apply.** Follow `AGENTS.md` and `.agents/`. Build through
  `nix develop` with `RUSTFLAGS="-D warnings"`, run `cargo fmt --all` before
  committing, use `#[shoop_test]` and run `check_shoop_test_usage.py` when tests
  change, and add new source files to the tracing coverage inventory.

## Testing strategy

Aim for a well-verified result without spending more time waiting on tests than
developing.

- **Deterministic offline host first.** Engine and adapter behaviour is tested
  through a test host that pulls callbacks synchronously, with no real time and no
  threads. It is fast, reproducible and heap-guarded. Real-time threads are tested
  only in the host stages.
- **Inner loop.** While developing, run only the touched crate's tests, narrowed to
  the relevant test file or filter, in debug, plus `cargo fmt` and a warnings-denied
  `cargo check` of that crate.
- **Stage end.** Run exactly the verification listed for the stage. Use release only
  where a stage changes real-time paths. Do not rerun unrelated suites; legacy suites
  run only when a stage changes crates the legacy backends share (Stages 1 and 2).
- **Workspace-wide runs** (complete `nextest`) only at Stages 1, 7 and 10.
- **Wasm.** `cargo check --target wasm32-unknown-unknown` of the engine-side crates
  at their stages (seconds). Node Wasm tests only for crates a stage changes, at its
  end. The complete Node suite runs only in Stage 10.
- **Browser.** One smoke of the core workflow at the end of Stage 8; complete browser
  runs only in Stage 10. Chrome is manual (CI no longer runs it); Firefox where
  convenient.
- **Miri.** Register cases for new ownership-sensitive code (the codec, the control
  endpoint's materialization, pool handover) in the inventory as they are written,
  which only lists them, and run Miri only in Stage 10, in batches.
- **No performance gating.** Render benchmarks are deferred beyond this milestone.

## Stages

Order: 0 → {1, 2, 3, 4} → 5 → 6 → {7, 8} → 9 → 10. Stages 1–4 are independent.
Stage 6 needs 2, 3 and 5. Stages 7 and 8 need 6; Stage 7 also needs 1.

### Stage 0 — Baseline

- [x] Run the complete workspace `nextest` once and record the counts of the crates
      later stages touch.
- [x] Record the legacy reference for the core workflow on the dummy driver:
      transition frames, loop lengths and recorded content for immediate, N-cycle,
      superseded and unsynchronized transitions.

**Verify:** counts and reference data recorded; every run passed.

### Stage 1 — Capability descriptor and UI gating

- [x] Add a capability descriptor to `Backend`; the legacy backends report their
      full feature set.
- [x] Gate every unsupported action in the UI (disabled with an explanation, or
      hidden): saving and loading, composites, grab, the modes, processor tracks, FX,
      latency, imports, length and timing edits, buses other than Master, driver
      switching.
- [x] Reject documents, session loads and script operations that need an
      unsupported capability, including composites on a backend without them.
- [x] Make refusals and capacity errors from user actions visible to the user.

**Verify:** new `shoop_app` and `shoop_egui` tests against `FakeBackend` with each
capability switched off; `shoop_backend`, `shoop_app` and `shoop_egui` tests; one
workspace `nextest` at the end.

*Done:* the descriptor is `BackendCapabilities` in `shoop_app_api`, re-exported by
`shoop_backend`; refusals reach the user as dismissible notifications in the
snapshot. Workspace `nextest`: 2217 passed, 4 skipped.

### Stage 2 — Shared backend contract tests

- [x] Move the generic contract tests from `shoop_backend`'s private test module into
      a public test-support module.
- [x] Let backends supply their host port names, and skip capabilities a backend
      does not report.
- [x] Add transition-timing contracts from Stage 0's legacy reference.

- [x] *Added:* contracts for the rest of the core set: track removal, track and
      Master controls, mixer routes, clear, loop gain and balance, and New Session.

**Verify:** the contracts pass against `FakeBackend` and `EngineBackend::new_dummy`
(`shoop_backend` tests only).

*Done:* `shoop_backend::contracts` (feature `contracts`) with `ContractProfile`,
`backend_contracts`, the `TimingHarness` trait with its frame conventions, and
`harness_contracts`; the legacy reference harness is `WebAudioTimingHarness`.

### Stage 3 — Loop gain and balance in `shoop_loops`

- [x] Add balance next to the per-loop gain, applied in the audio renderer with the
      legacy stereo factors; update the loop guides.

**Verify:** `shoop_loops` audio and runtime tests in debug and release; its Node Wasm
tests; boundary script.

### Stage 4 — Protocol (`shoop_rt_protocol`)

- [x] Schema modules: strips and mixer, loops, telemetry, bulk; engine-owned driver
      configuration and discovery vocabulary.
- [x] Capability and limit descriptor, limits as changeable values, IDs with
      generations.
- [x] Connection lifecycle: handshake, resynchronization, disconnect, limits-changed.
- [x] Lanes with credits; carrier traits; the in-process carrier; the binary record
      codec.

**Verify:** codec round-trip and malformed-input tests; lane credit and lifecycle
tests run against both carriers; wasm32 check.

### Stage 5 — Engine (`shoop_rt_engine`)

- [x] Composition: node enum over the loop nodes and the strip processors; segmented
      prepared pools (fixed segments in this milestone); inactive slots outside the
      active graph.
- [x] Structural transactions over prepared slots, activated at exact frames with
      fade ramps on removal and rerouting.
- [x] The control endpoint: protocol messages to runtime commands, bounded and
      allocation-free; telemetry publication; chunked bulk reads.
- [x] The render owner: one runtime `process` call per callback, absolute frames,
      MIDI recovery, fault and disconnect handling, stopped cleanup.
- [x] *Added:* a minimal operation in `shoop_loops` that clears a loop's content.
- [x] *Added:* a minimal `shoop_loops` operation that changes a loop's boundary
      smoothing, applied by the engine to current and later loops.
- [x] *Added:* protocol support for the application's mixer and I/O model: Master
      channels connect to host ports like strip ports (`EnginePort`), routes go from
      one strip output to one Master channel, and `Limits` reports Master's channel
      count.
- [x] The deterministic offline test host; a boundary check that engine-side crates
      do not depend on application crates.

**Verify:** engine tests through the offline host with heap guards, in debug and
release: structural edits while rendering produce no gap and ramp out cleanly;
capacity refusals; telemetry; bulk reads; wasm32 check with no imports for engine and
protocol.

*Done:* `prepare(&EngineDescription, DriverStatus)` returns the control endpoint,
the render owner and the host status. Loops without a sync source are synced to
themselves, so delays count their own wraps, as in legacy. A render fault silences
output and ends the connection with `Disconnected(Fault)`. A throwaway module built
from the engine and protocol had no Wasm imports; Stage 8 commits that check.

### Stage 6 — Adapter (`shoop_rt_backend`)

- [ ] `Backend` over a protocol client: ID mapping, snapshot mirror, asynchronous
      confirmation, capability descriptor from the handshake.
- [ ] Transitions with the legacy timing semantics; connections and the mixer model;
      new-session structure replacement; explicit refusal of everything else.

**Verify:** the shared contracts from Stage 2, including timing equivalence, against
adapter, in-process carrier, engine and offline host; `shoop_app` cooperative-runtime
tests of the core workflow on the same stack.

### Stage 7 — Native host and application integration (`shoop_rt_host_native`)

- [ ] The dummy driver on a real-time thread; the CPAL and midir adapter with
      lock-free bridges, xrun counting and a clear refusal of buffers above the
      prepared maximum.
- [ ] The composition root in `shoopdaloop`: a restart-required backend setting, an
      `AppArgs` override, pool size settings, fallback warnings.

**Verify:** host tests through the CPAL mock with heap guards, in release; an
application smoke of the core workflow on the dummy driver; a manual smoke on a real
device on Linux, plus macOS or Windows where available; one workspace `nextest`.

### Stage 8 — Browser host (`shoop_rt_host_worklet`)

- [ ] The raw-Wasm worklet module around the engine and the MessagePort carrier
      server, prepared before its first render quantum.
- [ ] The main-thread half: carrier client wiring and Web MIDI forwarding on its own
      lane.
- [ ] Build integration next to the existing worklet artifact; browser selection
      through the same setting and override.

**Verify:** Node tests of the worklet logic over a simulated MessagePort carrier;
the protocol tests over the record carrier; a check that the worklet module has no
imports; one browser smoke of the core workflow.

### Stage 9 — Documents

- [ ] `ARCHITECTURE.md`: the implementation map, the new layers, the protocol and its
      carriers, the capability model, growable capacities and uninterrupted
      structural changes, consistent with the design here.
- [ ] `PLAN.md`: the workstream row, the authority note and the remaining parity
      gaps, consistent with this plan.
- [ ] User docs (`docs/source/`): enabling the new backend and its supported
      features; READMEs for the new crates; tracing coverage entries.

**Verify:** link check; no document claims support for refused features; the design
here, `ARCHITECTURE.md` and `PLAN.md` read consistently.

### Stage 10 — End-to-end validation

- [ ] Run the CI workflow's Linux and web command set locally, including the new
      crates and the complete Node suite.
- [ ] Run the shared contracts against the legacy and the new backends.
- [ ] Run the core workflow natively on the dummy driver and on a real CPAL device,
      and in the browser (Chrome manually, Firefox).
- [ ] Run the Miri inventory, including the new cases, in batches.
- [ ] Check every acceptance criterion and record the results in `VALIDATION.md` and
      here.

**Verify:** B1–B12 are met, with evidence linked.

## Baseline

Recorded at `137cc837` on 2026-10-30, under the resource rules above (peak session
memory 6.8 GiB).

Complete workspace `nextest` (`shoop_engine/app_backend`,
`SHOOP_ALLOW_MISSING_BACKENDS=1`): 2191 passed, 4 skipped. Per crate: `shoop_backend`
123, `shoop_app` 126, `shoop_egui` 212, `shoopdaloop` 50, `shoop_app_api` 23,
`shoop_worklet_client` 48, `shoop_loops` 159, `rt_runtime` 197.

Legacy timing reference: `EngineBackend` with the physical port model at 1 kHz and
one-frame quanta. The capture input is a ramp of `frame / 1000`, and both tracks'
inputs are connected to it.

| Step | Observed |
| --- | --- |
| Sync loop: immediate Record at frame 3, immediate Play 16 frames later | Length 16, content frames 3–18, plays from frame 19; wraps at 35, 51, 67, … |
| Loop A, synced: Record with delay 1, requested at frame 20 | Snapshot shows next Recording, delay 1; recording starts at 51 (the second wrap) |
| Loop A: Play with delay 2, requested at 51 | Plays from 99 (the third wrap); length 48; logical content frames 51–98 |
| Loop A: Stop with delay 3, then Stop with delay 1, at 99 | The nearer request supersedes: next Stopped, delay 1; stops at 131; still stopped three cycles later |
| Loop B, no sync source, not playing: Record with delay 2 | Applies immediately |

So a delay of `n` applies at the (n+1)-th sync wrap after the request. Legacy stores
one cycle of pre-recorded material before the logical start of a synced recording
(stored content began at frame 35); equivalence compares the logical window from the
start offset over the loop length.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
