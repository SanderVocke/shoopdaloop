# Shared drivers for native, Rust FPGA reference and RTL simulation

## Goal and scope

Make backend selection determine how audio is computed, and driver selection
determine where audio comes from, where it goes, and when processing is requested.
Keep control communication independent of both choices.

Implement the common rendering interface, reusable native drivers, all three dummy
pacing modes, Rust-reference device audio, and a serialized Marlin application
backend. Preserve the shared compiler and the ownership rules in
[LOADING_DESIGN.md](LOADING_DESIGN.md).

The intended matrix is:

| Execution backend | Dummy | CPAL | JACK | WebAudio |
| --- | --- | --- | --- | --- |
| Native Rust | Paced, unpaced, controlled | Yes | Yes | Preserve support through the shared interface |
| Rust FPGA reference | Paced, unpaced, controlled | Yes | Yes | Add behind the reference build feature |
| Marlin RTL simulation | Paced, unpaced, controlled | Yes, through a bounded worker bridge | Yes, through a bounded worker bridge | Unavailable: native simulator |

Native driver availability follows existing platform/build features. Initially
validate Marlin on the platform and Verilator version supported by the current
testbenches. Unsupported platforms and missing features fail explicitly; they do
not select another backend. Device-driver connectivity does not guarantee that a
simulation is fast enough for uninterrupted real-time playback.

This work retains the FPGA application graph subset: Sync plus one mono user
track, up to 16 loops, supported transitions and loop gain, and supported live
graph edits. It does not add general routing, FPGA MIDI/FX processing, physical
FPGA access, backend replacement during a session, or reference/RTL live driver
switching. Preserve existing native live driver switching. No new UI capability
negotiation or special FPGA UI is introduced.

## Current code and gaps

- `shoop_rt_host_native::Prepared` constructs a native engine, and its `Cycle`
  directly owns the native `Render`. Dummy, CPAL and JACK therefore cannot yet
  receive another renderer. Native lifecycle, diagnostics, MIDI cleanup and JACK
  publication also contain native-specific assumptions.
- `shoop_host::fpga_reference` already separates its carrier endpoint from its
  persistent processor/job/media state, but starts its own sleeping worker with
  constant synthetic input. It reports a simulated driver rather than using the
  selected driver's input, clock and output.
- The native dummy driver has `Automatic` pacing and `Controlled` requested-frame
  execution. Public dummy settings expose only rate and buffer size. There is no
  continuously running unpaced mode.
- The reference requires complete fixed quanta and uses fixed-point samples.
  Device callbacks can have different or varying sizes and floating-point samples.
- Marlin has real SP/JP/media execution, shared compiler target lowering, sparse
  upload transactions and fenced switching. Its existing command bridge handles
  singleton switches only. A complete supported-command endpoint, snapshots and
  application host composition must be implemented; they are not already present.
- The browser worklet also constructs the native engine directly. Use its current
  record transport and AudioWorklet lifetime rather than native threads there.
- Historical `PLAN_FPGA_RTL_BACKEND.md` describes retired TP-era interfaces. It is
  background only; this plan uses the current SP/JP loader and protocol.

## Immutable acceptance criteria

1. Backend, driver and UI transport remain separate startup choices in the normal
   host, in-process app, detached launches and Host Manager. Attaching a UI never
   changes the host's choices. Remove the reference-only Dummy restriction.
2. Each supported matrix entry renders the existing supported graph with actual
   driver input/output. Default builds retain their existing optional-dependency
   boundaries. WebAudio reference selection is available in an appropriately
   built worklet; Marlin is explicitly unavailable in a browser.
3. Dummy supports paced, continuous unpaced and explicitly controlled execution
   for every native-host backend. Controlled mode consumes/delivers exactly the
   requested driver frames; incomplete engine quanta remain buffered, with no
   implicit audio quanta. Unpaced mode advances sample time as fast as execution
   permits, rather than changing sample rate or musical timing.
4. Drivers call an engine-independent rendering contract. `carrier::Endpoint`
   remains the control boundary. The reference has no private pacing loop and
   Marlin is touched only by its single owner.
5. Callback-size adaptation preserves samples and frame order for irregular
   sizes, complete FPGA quanta and partial controlled requests. Any introduced
   latency is bounded and correctly reported. No padding may silently advance
   the logical audio timeline.
6. The reference and RTL backends agree on supported command results, graph
   revisions, fence completion, loop state and fixed-point audio in deterministic
   traces. Compare native audio using the existing justified numeric tolerances.
   Sync, cursor and planned-transition execution remain in SP/JP.
7. Marlin command service and quantum execution can be entirely sequential.
   Admission, backpressure, pending switches, paused controlled execution,
   saturated queues, failure and shutdown must not deadlock. Physical callbacks
   never wait for Marlin, compilation, command completion or queue space.
8. Live graph loading preserves retained state and media. Only unused allocations
   are initialized, and old ownership is released only after the real fence.
   Snapshots and successful admission never stand in for completion.
9. Existing native audio/MIDI, JACK publication, driver lifecycle/recovery and
   browser behavior remain covered. Finish with local validation and a PR into
   `feat/backend-replacement`; CI completion and review are not acceptance gates.

## Design rules and decisions

### Rendering, preparation and ownership

- Place a small portable rendering trait beside existing `rt_runtime` audio/MIDI
  block types. It processes borrowed buffers and returns bounded status. Keep
  device APIs, compiler artifacts, queues and UI concepts out of the trait.
- Use explicit preparation/lifecycle hooks for opened sample rate, channel layout,
  maximum callback size, latency, faults and stopping. Native-specific diagnostics
  and MIDI recovery belong in the native implementation, not requirements that
  make every renderer expose a native engine.
- Extract reusable native I/O and pacing into `shoop_audio_drivers`. Keep
  `shoop_rt_host_native` as native composition and preserve public entry points
  where practical. Devices must not instantiate a particular execution engine.
- Extract the reference endpoint/render preparation into a portable host module
  or crate reusable by native startup and the worklet. It owns persistent SP,
  JP and media state, and does no sleeping or device I/O.
- Give production Marlin hosting its own optional crate/module. Share target,
  state codecs and transaction helpers with the testbenches; do not import JSON
  scenario runners, test-only native engines or assertion-driven fixture code
  into the application host. Keep Marlin/Verilator out of default/browser builds.
- Prepare buffers, bridges, models and programs before starting device callbacks.
  Compilation and loading allocations stay off physical audio callbacks. Keep
  existing native no-allocation guarantees; verify the new reference callback path.

### Dummy modes and block adaptation

- Public dummy settings gain a mode: `paced` (default), `unpaced`, `controlled`.
  Preserve a compatibility mapping for internal `Automatic`/`Controlled` APIs if
  useful. Expose the mode in host JSON, normal startup preferences, Host Manager
  and a shared CLI override such as `--dummy-mode`.
- A controlled-driver handle belongs to the host/test harness and is independent
  of the looper command queue. It can request frames while an app operation waits
  for a graph switch. It is not a new musical command or UI transport feature.
- Use the same driver loop for all three modes. Unpaced mode still checks stop
  and yields scheduling opportunities to control work. Silence is normal dummy
  input; deterministic generators/capture sinks are explicit test facilities.
- Use one preallocated adapter for mismatched callback/quantum sizes and sample
  conversion. Split larger requests, retain incomplete input/output fragments,
  and never duplicate, discard or invent logical frames to fill a quantum.
- Causal fixed-quantum processing can require up to a quantum of buffering when
  request sizes do not align. Define and test that latency during preparation.
  Controlled requests smaller than a quantum may fill that latency with silence;
  SP/JP execute only after sufficient real input arrives. Account separately for
  driver frames delivered, renderer frames completed and buffered frames, so the
  requested frame count and the engine's quantum clock are both truthful.
- Keep the legacy simulated status kind only where fixtures need it. Host runs
  report the actual driver and dummy mode; a sleeping reference must not be
  labelled an unpaced clock. Update schemas/protocol versions where necessary.

### Serialized Marlin service and progress rules

1. **One owner:** create, access and drop the model on one native thread. Endpoint
   and driver handles exchange bounded owned records/blocks with it. Neither
   acquires a lock around the simulator. Do not require Marlin objects to be Send.
2. **Two turns:** a command-service turn handles admitted commands, upload work,
   responses and snapshot publication; a quantum turn runs one requested SP/JP
   quantum to completion. Never execute command mutations halfway through a
   quantum. Memory/cache/AXI responder agents needed by that quantum run as part
   of its simulation step loop, without an external poll or callback.
3. **Two clocks:** command turns may clock configuration/control/fence handshakes
   while audio is idle. They do not trigger `run_valid`, advance logical audio
   frames or generate wraps/pulses. Only a driver quantum request does that.
   Endpoint `try_deliver`/`poll` enqueue or drain records; they do not directly
   tick the model or synchronously wait for the owner.
4. **Fair scheduling:** between quanta, service a bounded amount of control work,
   then allow a requested quantum. Long uploads are resumable transactions. A
   command flood cannot starve audio, and unpaced audio cannot starve commands.
   Partial state updates finish before another quantum starts. Use a common wake
   mechanism for command arrival, render demand, output space and stop requests;
   never sleep waiting exclusively on one mailbox.
5. **Pending is not waiting:** a command requiring the next boundary or a fence
   records its continuation and returns to the owner loop. In particular, a
   switch must not wait inside command service for the quantum that service is
   preventing. Subsequent commands retain FIFO/barrier order, but render demand
   can progress independently. There is no implicit quantum in controlled mode.
6. **Credits and drains:** reserve reliable completion capacity before applying a
   group, including an asynchronous switch completion. Full command/reply queues
   cause bounded admission refusal or pending work, not a blocking send. Drain
   hardware events/snapshots into prepared storage while running; never leave a
   hardware ready/valid handshake dependent on an app thread draining a queue.
   Replace/drop superseded telemetry; retain required completions and ownership.
   Reclaim saturation postpones preparation while the current graph can run.
7. **Truthful completion:** distinguish queue admission, command application,
   graph selection and ownership-fence completion. A new-revision snapshot alone
   cannot release the old allocation. Publish coherent snapshots only between
   quanta/command transactions, with their actual frame and revision. Initial
   endpoint metadata/snapshot availability must not require an audio quantum.
8. **Stopped demand:** when controlled execution has no requested frames, service
   control work that can finish without rendering. Leave genuinely boundary-
   dependent operations pending, without spinning or blocking the endpoint.
   Document this behavior and expose the independent driver handle early enough
   that host startup/connect cannot wait for a handle it has not returned yet.
9. **Failure and stop:** use bounded simulation-cycle budgets, periodic checks of
   an out-of-band stop flag, and explicit failed/disconnected completions. A
   control deadline does not cancel a published switch or imply memory is free.
   Stop admission first; quiesce/fence while the owner can still progress, then
   stop/drop the model and complete waiters. Join only on a lifecycle thread,
   never from an audio callback or while holding a queue/model lock. Failed
   fencing retains allocations until the whole runtime is quiesced and torn down.

The existing singleton switch bridge is insufficient for the application path.
Implement the current reference command subset, including grouped plan clearing
followed by a switch, revision/deadline checks, result codes and snapshots.
Preserve existing command-group failure semantics; add no rollback promise.
Use a trusted simulator-side execution command service over existing idle RTL
state/control ports. Runtime writes to active state occur exclusively in command
turns and are distinct from the loader's unused-slot initialization API. Read the
actual RTL state when a command needs it; do not restore an earlier snapshot or
run a shadow Rust SP to produce state/audio. Pure command translation/state codecs
may be shared. A new FPGA hardware command decoder is outside this task.

### Device callbacks and the simulator bridge

- Native/reference renderers normally execute directly on the device callback.
  Marlin uses a prepared input/output block pool and bounded queues to its owner.
  The callback copies input, takes already completed output, signals demand and
  returns. It never waits for simulation or command responses.
- Choose a small fixed bridge depth and report its latency in addition to quantum
  adaptation latency. Identify blocks by frame and runtime generation; preserve
  their order. Define priming output as silence at that reported latency.
- After priming, a required output missing its deadline produces silence, an
  underrun report and a latched stream fault. Queue overflow, stale-generation
  output and owner failure also fault explicitly. Subsequent callbacks remain
  bounded and silent until shutdown/restart. This deliberately simple policy
  avoids silently accumulating latency or pretending slow simulation kept pace.
- Dummy controlled/unpaced mode can wait for an outstanding quantum on its own
  non-real-time driver thread. The owner remains able to run command turns and
  stop checks; no callback or control endpoint is involved in that wait.

## Staged implementation

Execute in order, with stage 7 following the portable reference work in stage 4.
Do not replace an unfinished renderer with native execution to make a pair pass.

### Stage 1 — Branch, baseline and contracts

- [x] Fetch upstream; create `feat/shared-render-drivers` from the latest
      `origin/feat/backend-replacement`. Carry this plan into that branch and
      preserve unrelated work. Record the base SHA and toolchain.
- [x] Read current native lifecycle/driver-switching contracts and characterize
      existing callback splitting, MIDI reset, JACK publication and browser paths.
- [x] Record the supported matrix, renderer preparation/status contract, frame
      accounting, queue capacities and Marlin turn ordering before changing code.
- [x] Verify focused native dummy/CPAL/JACK mocks, reference activation and Marlin
      loading/audio baselines. Record external-service skips and baseline failures.

### Stage 2 — Extract the rendering boundary and native driver machinery

- [x] Add the portable rendering interface and a native implementation; separate
      native diagnostics/cleanup from device-generic buffer handling.
- [x] Extract reusable Dummy/CPAL/JACK ownership, discovery, callback handling and
      status into `shoop_audio_drivers`; pass an engine preparation factory the
      opened device geometry instead of constructing the native engine in drivers.
- [x] Preserve native host/control endpoints, MIDI recovery, dynamic JACK port
      publication, fallback policy and transactional live driver switching.
- [x] Update dependency-boundary rules without weakening separation from UI,
      scripting, application semantics or legacy engines.
- [x] Verify native contract tests, mocked CPAL, dummy, direct JACK I/O, stop/fault
      paths and callback no-allocation checks before connecting another backend.

### Stage 3 — Dummy pacing and quantum/sample adaptation

- [ ] Implement public dummy modes and common startup plumbing, defaulting to
      paced mode. Keep backend and driver independent; strip startup overrides
      when launching an attaching UI. Version affected serialized settings.
- [ ] Add continuous unpaced execution and an independent controlled-frame
      handle, with bounded stop behavior and no command/audio starvation.
- [ ] Implement prepared frame-size and f32/fixed-point adapters with explicit
      latency, partial-fragment accounting and deterministic test input/output.
- [ ] Verify exact requested frame counts, zero demand, long unpaced runs, stop
      under demand, conversion boundaries, and callback sizes below/above/not
      divisible by the engine quantum. Use impulse/ramp fixtures to catch dropped,
      duplicated or reordered samples and incorrectly advertised latency.

### Stage 4 — Rust reference on shared native drivers

- [ ] Split the current reference startup into portable endpoint/render
      preparation and driver composition. Remove the synthetic-input sleeping
      worker; retain persistent SP/JP/media, allocator, loader and reclaim behavior.
- [ ] Implement the renderer and wire Dummy, CPAL and JACK to its actual media
      ports. Apply selected device rate/geometry rather than fixed driver values.
- [ ] Update host, in-process/detached app and Manager factories/validation. Respect
      the independently chosen driver, including saved settings. Do not silently
      substitute Dummy or native execution for an explicitly selected combination.
- [ ] Keep existing graph restrictions, normal unsupported-command refusals and
      automatic graph activation. Preserve current native startup defaults.
- [ ] Verify deterministic record/play/stop, sync plans, gain and live graph
      replacement using all dummy modes and mocked physical drivers. Exercise
      irregular callbacks, retained media/state, reattachment and shutdown through
      the normal application endpoint. Check reference callback allocation behavior.

### Stage 5 — Marlin renderer, command service and ownership

- [ ] Extract reusable RtlTarget, model setup, memory responders, RAM access and
      upload/switch helpers from test-only composition into the optional runtime.
      Prepare/cache the model before readiness; allow cold-build startup time and
      return actionable toolchain/source errors. Replace fixture-wide simulated
      time limits and assertion failures with per-operation budgets and explicit
      faults; healthy sessions must not expire at a fixed simulated timestamp.
- [ ] Implement the single-owner scheduler, nonblocking endpoint handles and
      sequential command/quantum turns specified above. Preserve thread affinity
      at model creation, use and destruction.
- [ ] Implement the reference-supported runtime command groups and coherent
      snapshots from actual RTL state/events. Preserve command ordering, codes,
      target bindings, stale-revision checks and publication/fence semantics.
- [ ] Integrate the shared compiler/allocator and sparse loader without active-
      state initialization. Resume pending switches/fences across service turns;
      keep driver demand independent of their completion.
- [ ] Verify command-only service leaves audio frame/cursor/pulse counts unchanged;
      command–quantum interleavings match the Rust reference. Include clear-plans
      plus switch groups, stale commands, two future plans and retained recording.
- [ ] Verify controlled zero-demand startup, pending switch followed by external
      render demand, response/snapshot backpressure, full request/reclaim queues,
      delayed memory/fence completion and stop during each state. Drive tests with
      explicit steps and finite cycle budgets so failures diagnose a state rather
      than hang indefinitely.

### Stage 6 — RTL device bridge and normal application startup

- [ ] Implement the preallocated simulator bridge and its latency, priming,
      underrun/fault, frame-order and generation rules. All physical callbacks
      remain nonblocking even if simulation deliberately stops making progress.
- [ ] Make `--backend rtl-sim` functional behind an optional Cargo feature in the
      host, app and Manager. Wire all three native drivers and existing in-process,
      socket and shared-memory UI transports; keep unsupported platforms explicit.
- [ ] Keep driver discovery independent of backend availability. Lifecycle controls
      for unsupported reference/RTL live driver replacement refuse normally;
      existing native replacement continues to work.
- [ ] Verify bridge ordering/latency and nonblocking behavior using mock callbacks
      and an intentionally delayed owner. Exercise queue saturation, owner panic,
      failure during upload/run/fence and shutdown without callback/control waits.
- [ ] Run a small real Marlin application scenario through `shoop_host` and the
      normal UI runtime: add supported graph, record, schedule/play/stop, alter
      graph and shut down. Compare samples, states and responses with the reference.

### Stage 7 — WebAudio through the same portable boundary

- [ ] Refactor `shoop_rt_host_worklet` to prepare/render through the portable
      contract while retaining current message framing, MIDI lanes and callback
      lifetime. Preserve native browser audio and processor behavior.
- [ ] Add optional Rust-reference endpoint/renderer selection to worklet startup,
      its build script and browser startup settings. Keep compilation/loading
      preparation out of the WebAudio render callback using existing control-side
      scheduling; use the quantum adapter for worklet block sizes.
- [ ] Account for separate browser/worklet Wasm heaps: transfer precompiled target
      instruction/new-state writes and the switch descriptor through bounded
      internal loader records. Do not pass native pointers/Arc handles across
      heaps or parse an unbounded program image in a render callback. Preserve
      allocation ownership, upload completion and fence ordering on this path.
- [ ] Verify native browser smoke/contract coverage and a limited reference
      record/play/live-load trace. Check output-only/microphone input, rate/block
      geometry, startup errors, disconnect and fixed-point conversion. Reject
      Marlin selection clearly without native simulator dependencies in wasm.

### Stage 8 — End-to-end validation, documentation and PR

- [ ] Run shared contract scenarios across the supported matrix. Use identical
      input samples, command positions and graph revisions; compare before/after
      adapter latency. For RTL/reference use exact fixed-point samples and states;
      document native numeric tolerances. Prove sequential command service is
      sufficient with the simulator owner never reentered.
- [ ] Run the applicable complete local Rust gate once after focused checks pass,
      plus relevant compiler/reference/Marlin suites and browser checks. Do not
      multiply long simulations across every pacing/driver/transport permutation:
      test scheduler/bridge permutations with deterministic mocks and use small
      representative actual RTL runs for the full path.
- [ ] Run formatting, test-attribute policy, dependency audits and warning-denying
      affected builds. Preserve meaningful existing coverage; document baseline
      failures or unavailable real devices rather than silently skipping new paths.
- [ ] Update startup/driver documentation and CI paths/jobs for the new modules,
      modes and tests. Document supported combinations, latency, sequential Marlin
      operation and the consequences of a simulation that misses real-time demand.
- [ ] Commit completed milestones and the final validation record; push the branch
      and open/update a ready PR into `feat/backend-replacement` with scope,
      verification and remaining limitations. Do not wait for CI or review, poll
      for approval, or make a green CI run a completion condition. If a failure is
      already observed, address it only when attributable to this work.
- [ ] Finish with the PR link and local results. Do not merge autonomously unless
      the user separately authorizes that merge.

## Testing budget and execution contract

Use focused behavioral tests at each stage. Cheap mock-based tests cover callback
sizes, pacing, fairness, backpressure and failure interleavings. Reuse compiled
Marlin models; keep actual RTL traces small but include the real media/cache/fence
path. Run existing broad gates once at the final milestone and repeat only for
new failures or subsequent changes that justify them. Hardware audio smoke tests
are additional evidence when facilities exist; mock-based driver correctness and
nonblocking behavior must remain executable without audio devices. A real-time
speed threshold for Marlin is not required.

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
- Writing this plan does not start implementation; implementation starts when requested.
- CI/review waiting is intentionally excluded by the user's instruction. Local
  validation remains required; automatic PR merging is not part of this plan.

## Execution record

- Implementation branch: `feat/shared-render-drivers`, base `819ade43`.
- Toolchain: Rust/Cargo 1.100.0; Verilator 5.032; Linux x86_64.
- Initial native baseline: 13 mocked CPAL tests and 9 dummy tests pass. Six real
  JACK tests require a running JACK server unavailable in this environment;
  five non-server JACK checks pass. These are recorded infrastructure gaps.
- The renderer contract uses portable borrowed audio/MIDI blocks; drivers own
  device timing, while backend factories own preparation and endpoint creation.

- Stage 1/2 validation: Marlin loading baseline 2 passed; existing final-head
  reference activation results retained as baseline. After extraction: shared
  driver library 9 passed (real JACK allowed absent), native library 3 passed,
  CPAL mocks 13 passed, dummy 9 passed. Warning-denying driver/native build,
  formatting, test policy, dependency-policy unit tests and native boundary audit
  passed. Native control endpoints and typed stopped cleanup remain in the facade;
  shared drivers have no production dependency on a concrete execution engine.
