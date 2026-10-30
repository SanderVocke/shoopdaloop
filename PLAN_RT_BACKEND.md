# Plan: first opt-in runtime backend

[Main plan](PLAN.md) · [Architecture](ARCHITECTURE.md) · [Validation](VALIDATION.md) · [Loop features assessment](LOOP_FEATURES_ASSESSMENT.md)

Scope agreed with the user on 2026-10-30: native and browser, the core looping
feature set without session save/load, prepared capacity pools, and work continuing
on `feat/loop-execution-layer` without a separate branch or pull request.

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

## Acceptance criteria

These must not change without explicit user approval.

- **B1 — opt-in selection at a lifecycle boundary.** A restart-required setting and
  an `AppArgs` override choose the legacy or the runtime backend; legacy is the
  default. The runtime backend never routes an operation to the legacy engine.
- **B2 — platforms.** The runtime backend runs natively with the dummy and the CPAL
  and midir drivers on Linux, macOS and Windows, and in the browser's AudioWorklet,
  with the same core feature set.
- **B3 — core workflow.** On both platforms a user can start the application,
  create a new session, add regular audio and MIDI tracks, connect them to host
  ports and Master, record, play, stop and clear loops with immediate and
  synchronized transitions, adjust gains, balance, mute and monitoring, watch meters
  and loop progress, and inspect waveform and MIDI details.
- **B4 — explicit capabilities.** The backend reports a capability descriptor; the
  legacy backends report their full feature set. The UI disables or hides every
  unsupported action with an explanation, refusals reach the user, and the
  application rejects documents and scripts that need an unsupported capability
  instead of applying them partially. Composite documents on a backend without
  composites are rejected rather than loaded without their composites.
- **B5 — prepared pools.** Capacities for tracks, loops per track, channels and
  ports are fixed when the backend starts and are configurable. Adding or removing a
  track or loop activates or retires prepared slots without interrupting audio.
  Exceeding a capacity is refused with a visible message.
- **B6 — timing equivalence.** For the core workflow the runtime backend produces
  the same transition frames, loop lengths and recorded content as the legacy
  backend, shown by shared contract tests that run against both.
- **B7 — callback contract.** Driver callbacks, the engine's render path and the
  worklet's render path do not allocate, free, lock or wait; tests guard them, and
  xruns are reported.
- **B8 — legacy unchanged.** The legacy backend's tests and behaviour are unchanged
  apart from reporting its capabilities.
- **B9 — boundaries.** `rt_runtime` and `shoop_loops` keep their dependency
  boundaries; the new crates do not depend on `shoop_engine`.
- **B10 — documentation.** User docs say how to enable the runtime backend and what
  it supports; `ARCHITECTURE.md`, `PLAN.md` and `VALIDATION.md` describe the new
  layer and its evidence; the new crates have READMEs.

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

## Stages

Order: 0 → {1, 2, 3} → 4 → 5 → {6, 7} → 8 → 9 → 10. Stages 1, 2 and 3 are
independent. Stage 8 needs Stage 5's control logic. Stages 6 and 7 need Stage 5.

### Stage 0 — Baseline

- [ ] Record the test counts and results of `shoop_backend`, `shoop_app`,
      `shoop_egui` and `shoopdaloop` (native, and Node Wasm where opted in).
- [ ] Record the legacy core workflow's transition frames and recorded lengths on
      the dummy driver as reference data for B6.

**Verify:** the counts and reference data are recorded, and every run passed.

### Stage 1 — Capability descriptor and UI gating

- [ ] Add a capability descriptor to `Backend`, with a default for the existing
      backends that matches what each one supports.
- [ ] Gate every unsupported action in the UI (disabled with an explanation, or
      hidden): saving and loading, composites, grab, the modes, processor tracks, FX,
      latency, imports, length and timing edits, buses other than Master, and driver
      switching.
- [ ] Reject documents, session loads and script operations that need an
      unsupported capability, including composites on a backend without them.
- [ ] Make refusals and capacity errors from user actions visible to the user, not
      only in the log.

**Verify:** app and UI tests against `FakeBackend` with each capability switched off;
the legacy backends' tests are unchanged.

### Stage 2 — Shared backend contract tests

- [ ] Move the generic contract tests in `shoop_backend`'s private test module
      (backend, loop content, connections, direct tracks, session IO, transitions)
      into a public test-support module that other crates can run against their own
      backend.
- [ ] Let the backend supply its host port names instead of the hard-coded dummy
      names, and let contracts skip capabilities the backend does not report.
- [ ] Add transition-timing contracts (immediate, N-cycle, supersession, no sync
      source) that capture the legacy reference data from Stage 0.

**Verify:** the contracts still pass against `FakeBackend` and
`EngineBackend::new_dummy`.

### Stage 3 — Loop gain and balance in `shoop_loops`

- [ ] Add balance next to the per-loop gain, applied in the audio renderer with the
      legacy stereo factors.
- [ ] Update the loop guides.

**Verify:** loop tests in debug and release with heap guards, Node Wasm tests,
boundary script.

### Stage 4 — Engine composition (`shoop_rt_engine`)

- [ ] Engine description: sample rate, maximum frames per callback (at least the
      largest device buffer), host audio and MIDI slots, and pool capacities with
      defaults.
- [ ] Prepared pools for track slots, loop slots and the Master bus, with the node
      enum wrapping the loop nodes and the application processors.
- [ ] Structural transactions: activating and retiring track and loop slots,
      routing tracks to Master and host slots, and assigning sync sources, through
      the loop library's topology and graph staging.
- [ ] The domain schema: reliable requests and results, latest-value telemetry
      (loop state, peaks, MIDI activity), and chunked bulk reads of loop media
      through exports.
- [ ] The render owner: one runtime `process` call per callback, absolute frame
      count, MIDI recovery output, fault and disconnect handling, stopped cleanup.

**Verify:** native tests in debug and release with heap guards for every render
path, structural edits while rendering, capacity refusals, Wasm build without
imports, Node tests.

### Stage 5 — Native backend core and dummy driver (`shoop_rt_backend`)

- [ ] The typed in-memory carrier between control and render.
- [ ] The `Backend` implementation for the core set:
  - track, loop and port ID mapping onto slots;
  - snapshot assembly from telemetry and results;
  - transitions with the legacy timing semantics;
  - connections and the mixer model;
  - new-session structure replacement;
  - the capability descriptor;
  - explicit refusal of everything else.
- [ ] The dummy driver adapter: a thread that drives the render owner in real
      time without allocating.

**Verify:** the shared contracts from Stage 2 pass against the runtime backend for
every supported capability, including B6 timing equivalence; app-level tests with
the cooperative and threaded runtimes on the dummy driver.

### Stage 6 — CPAL and midir driver adapter

- [ ] Device discovery and configuration through the `shoop_app_api` driver
      vocabulary; host audio slots map onto device channels and host MIDI slots
      onto midir ports.
- [ ] A lock-free bridge from the input callback to the output callback, and
      lock-free MIDI delivery from midir into the render owner; xrun and stream-error
      counting.
- [ ] Refuse device buffers larger than the prepared maximum frames, with a clear
      message.

**Verify:** allocation-guarded tests through the CPAL mock; a manual smoke on a real
device on Linux, plus macOS or Windows where available.

### Stage 7 — Native application integration

- [ ] A restart-required backend setting next to the audio settings and an
      `AppArgs` override; the runtime backend is constructed where `NativeBackend`
      is today, with fallback warnings like the existing ones.
- [ ] Settings for the pool capacities, with defaults.
- [ ] An end-to-end application smoke of the core workflow on the dummy driver,
      modelled on the existing native application smoke.

**Verify:** the smoke passes; the legacy backend remains the default and its tests
pass; the runtime backend selected by setting and by argument starts and reports
its capabilities in the UI.

### Stage 8 — Browser carrier, worklet module and integration (`shoop_rt_worklet`)

- [ ] A bounded record codec for the domain schema's three lanes; prepared
      materialization of requests into runtime owners inside the worklet; buffer
      recycling.
- [ ] The raw-Wasm worklet module wrapping the engine composition, prepared before
      its first render quantum, with the existing stereo device boundary and Web MIDI
      forwarded from the main thread.
- [ ] The main-thread `Backend` implementation over this carrier, sharing Stage 5's
      control logic.
- [ ] Build integration next to the existing worklet artifact, and browser
      selection through the same setting and override.

**Verify:** Node tests of the codec and worklet logic; Wasm without imports; an
AudioWorklet host test of the core workflow in Chrome (manual) and Firefox; the
existing browser smokes still pass on the legacy backend.

### Stage 9 — Documents

- [ ] `ARCHITECTURE.md`: the new layer in the implementation map, the composition,
      the two carriers and the capability model.
- [ ] `PLAN.md`: the workstream row and the remaining parity gaps.
- [ ] `VALIDATION.md`: the evidence.
- [ ] User docs (`docs/source/`): enabling the runtime backend and its supported
      features.
- [ ] READMEs for the three new crates.

**Verify:** links resolve; no document claims support for refused features.

### Stage 10 — End-to-end validation

- [ ] Run the CI workflow's Linux and web command set locally, including the new
      crates.
- [ ] Run the shared contracts against the legacy and the runtime backends.
- [ ] Run the core workflow natively on the dummy driver and on a real CPAL device,
      and in the browser (Chrome manually, Firefox).
- [ ] Measure callback cost against the legacy backend for the same scenario on the
      same machine.
- [ ] Check every acceptance criterion and record the results in `VALIDATION.md` and
      here.

**Verify:** all of B1–B10 are met, with evidence linked.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
