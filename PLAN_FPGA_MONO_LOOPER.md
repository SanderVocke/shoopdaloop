# Queue-driven mono looper through Marlin

Status: complete. [PR #981](https://github.com/SanderVocke/shoopdaloop/pull/981)
targets `proto/fpga-rust-reference` from `feat/fpga-mono-looper`.

## Goal and scope

Extend the merged runtime adapter to operate one complete primitive mono loop
through the existing production transport, comparing it with production Q8.24.
Use Q=32, 32-sample chunks, initially empty media and a fixed initial graph:

```text
Input -> Capture
Playback -> Gain -> Output
```

Support record, stop, play from position zero, playback wraps, instant external
gain changes, state queries, and export/read/release of completed recordings.
Transitions and gains use quantum-aligned absolute timing. Bound recordings and
retained exports to an explicitly declared device capacity. Include re-recording
and an export retained across re-recording.

Exclude MIDI entirely. Also exclude composites, synchronization between loops,
overdub/monitor-previous, seek, smoothing, ramps, imported sessions, live graph or
topology replacement, and board/DRAMsim3 integration. Keep loop playback gain at
unity and use the separate gain processor. Unsupported requests receive explicit
errors. This remains a service-thread simulation adapter, with no ARM deadline
claim or GUI integration requirement.

## Current implementation and constraints

- `rt_fpga_adapter` already consumes owning runtime queues, validates production
  plans and lowers gain graphs through its shared compiler. Its current aliases
  use `NoPlanner`; planner operations and resource requests are rejected.
- The shared compiler already lowers logical capture/playback instructions.
  `rt_backend_testbench/src/rtl.rs` still assembles loop topology and media image
  metadata from scenarios; extract reusable parts instead of creating a second
  graph compiler.
- `rt_fpga_sim` already drives TP/SP/JP and the functional AXI memory in Rust.
  Its direct recording test checks loop state, output and captured samples, but
  sends TP commands directly and reads media outside the runtime transport.
- Production `LoopPlanner::prepare`, `Binding`, `LoopNode` and `LoopClient`
  provide the startup model and command families. Assigned loop nodes are
  protected by `admit_processor_change`; the gain demo's startup
  `Remove` -> next-generation `Install` sequence cannot be copied onto them.
- Production media access uses planner `Export`, `Cursor` and `Release`, followed
  by runtime resource `Read` requests. Preserve that distinction. The existing
  fixture's final off-render media inspection alone does not satisfy this goal.

## Immutable acceptance criteria

1. A common producer prepares equivalent production loop bindings/topology and
   whitelisted processors, then submits the initial graph commit, supported loop
   operations, gain operations, queries and media reads through ordinary runtime
   queues. Loop startup uses production preparation and preserves bound node
   generations. No precompiled image or expected state/jobs are hardware inputs.
2. The adapter generates the SP program, view/state bindings and nonempty TP
   configuration from the actual production plan and startup descriptions.
   Those remain installed across record/stop/play, wraps and re-recording: one
   compilation/activation per accepted initial graph, none for mode or gain changes.
3. Every output sample, exported recording sample, supported logical loop state,
   revision, event and receipt matches production Q8.24 at the same observation
   cuts. Check requested/effective/delivery/terminal frames and graph/topology
   epochs. Opaque media/export/snapshot handles may differ numerically; compare
   them through a checked logical correspondence and verify their lifetimes.
4. Exported completed media is immutable while retained, including across another
   recording. Cursor reads preserve production encoding, position, partial-read
   and EOF behavior. Release permits reclamation; stale handles cannot access
   reused storage. Owning read buffers return through existing completions.
5. Queue pressure, unread results and cancellation lose no terminal results or
   owners. Invalid identities, unsupported operations/timing and capacity failures
   are explicit. Device faults surface without a successful activation or domain
   completion being fabricated. Compare native/RTL behavior within common admitted
   limits; document bounded-device exhaustion separately from elastic CPU storage.
6. The complete new build/run/replay path remains Rust + Marlin/Verilator without
   Python/cocotb. Existing gain and direct-loop regressions remain passing; normal
   CPU/Wasm application paths remain independent of simulation dependencies.

## Design rules

- Keep ownership queues and host handle/retirement bookkeeping on ARM. Reuse the
  current command/reply families and production scheduler/receipt machinery;
  introduce only necessary accessors, preparation records and consumer plumbing.
- Reuse the production-backed loop/gain processor wrapper and DSP. Add small
  immutable descriptions for hardware-supported node kinds and bindings. Keep
  the adapter independent of the testbench crate.
- TP owns musical state transitions, SP generates jobs, and JP/memory execute
  capture/playback. The adapter must not run a software loop resolver, planner
  processing cycle or sample executor to manufacture hardware behavior/results.
  Production preparation and metadata/identifier helpers are reusable off-render.
- Preserve production node/connection arithmetic order. Keep the existing
  coordinator's TP -> SP/JP -> finish phases. Establish query/export cuts from
  native behavior before choosing when to read hardware state.
- Separate transport admission, runtime delivery, TP request admission and domain
  completion. Map runtime sequences to TP request identities and retain receipt
  ownership until the actual terminal outcome; a successful port write is not
  transition completion.
- Reuse existing views, chunk tables, allocator, cache, slice/remove/reclaim and
  AXI ports. Add device methods for TP operations, observation and media streams.
  Change RTL only for demonstrated interface/semantic gaps; do not redesign memory.
- Validate the complete initial configuration before device writes and acknowledge
  it only after successful loading. Keep simulation waits bounded and preserve
  model/context/trace ownership and cached builds.

## Staged implementation

### 1. Establish production contracts and loop startup

- [x] Create a new working branch from latest `proto/fpga-rust-reference`; fetch
  and merge upstream at the start and between milestones. Push only that working
  branch. Read the current adapter and production loop preparation paths.
- [x] Define the supported operation/selection/timing and capacity profile using
  existing types. Factor immutable startup descriptions and loop-node accessors
  as needed; preserve native binding/generation checks and startup ownership.
- [x] Extend/reuse the common producer and production Q8.24 oracle to exercise
  queued initial commit, record/stop/play, state queries, gains, completed-media
  export/cursor/read/release and re-recording. Record exact observation cuts and
  admission/delivery/domain outcomes before implementing translation.
- [x] Verify this contract with focused native tests, including empty playback,
  future aligned actions, same-boundary ordering and retained export behavior.
  Keep larger native/RTL runs for the later integration gate.

### 2. Compile and configure the loop-aware pipeline

- [x] Extend the adapter's closed processor set and runtime command types to the
  production loop planner operation/reply family. Accept prepared loop nodes at
  startup and keep queued gain-install coverage intact.
- [x] Extract/share initial TP topology and media-binding assembly with existing
  fixtures. Lower the actual ordered plan plus checked production topology and
  bindings into persistent SP state/program and TP configuration. Validate the
  three-node graph and device capacities before loading.
- [x] Extend `Device` and the existing Marlin driver with the required typed TP,
  state and media operations, reusing current packing and ports. Retain separate
  C++ build/load and graph compile/configuration measurements.
- [x] Verify compiler/binding contracts and configuration traffic with a recording
  device, then reuse the direct recording regression to check the physical ports.

### 3. Translate queued transitions and observations

- [x] Implement supported `LoopClient` transition/state commands and instant gain
  delivery using existing scheduling semantics. Correlate TP requests, cancellation,
  terminal domain outcomes and runtime receipts; preserve lane/credit rules.
- [x] Publish production-shaped state/query results and events at the cuts fixed
  in stage 1. Track graph/topology epochs and logical media revisions without
  executing a CPU loop-state machine on the hardware path.
- [x] Test through a recording/stalling device: no early acknowledgements, stale
  loop/planner identities, future actions, same-boundary actions, ticket and domain
  cancellation, unread results, pressure, unsupported timing and injected faults.
  Assert that transitions/gains never reload the program or reset the timeline.

### 4. Bridge completed-media ownership and reads

- [x] Map supported planner `Export`, mono `Cursor`, `Release` and resource `Read`
  onto checked host handles and FPGA views. Support completed `Latest`/`Playback`
  selection; reject recording-selection and unrelated resource operations.
- [x] Retain an immutable view using existing media-store operations. Return
  metadata/cursors and queued read-buffer replies using production codecs and
  ownership rules. Define cursor use after release from native evidence.
- [x] Verify partial reads/EOF, stale generations, refused requests, export-table
  exhaustion and returned buffers with focused protocol tests. Retain an export
  across re-recording, release it, and verify chunk/reference reclamation and safe
  reuse without promising unbounded FPGA storage or general resource emulation.

### 5. Run queue-driven native/RTL looping comparisons

- [x] Extend the existing Marlin differential runner to use the stage-1 producer
  and actual queued plans/startup descriptions. Observe state and retrieve media
  through supported queued operations, rather than substituting debug-only reads.
- [x] Add replayable traces covering recording several quanta, stop, play through
  multiple wraps, signed/low-bit gain changes, stopped silence, and a second
  recording of a different length. Read both the retained first export and the
  new recording, then release them and exercise storage reuse.
- [x] Compare raw samples, logical state/revisions/events, public media metadata,
  read bytes and supported completions/receipts at identical boundaries. Include
  deterministic stalls and small queue limits. Report the first mismatch with
  quantum/operation context and a reproducible VCD command.
- [x] Assert one initial compilation/activation, stable bindings and no rebuilds
  for transitions/gains. Preserve existing gain-chain/fan-in and direct-loop tests.

### 6. Final validation and delivery

- [x] Document the supported protocol subset, startup path, handle correspondence,
  observation cuts, capacity/fault behavior and one-command run/replay. Record
  graph/image/configuration measurements separately from simulator construction.
- [x] Run affected native loop/runtime/transport and media tests, adapter/device
  tests and the Marlin differential/regression suite. Run applicable independent
  RTL checks only if synthesizable RTL changed. Reuse cached models and unchanged
  expensive evidence; do not repeat the waived 64-loop benchmark.
- [x] Run formatting, warning-denying builds, the Rust test-attribute policy and
  relevant CPU/Wasm dependency/build checks. Broaden native testing once at the
  final milestone; keep host test binaries stable during execution and isolate
  failed cases before diagnosing regressions. Do not overlap feature-changing
  workspace builds with host integration tests.
- [x] Commit/push milestones and open a PR onto `proto/fpga-rust-reference`. Check
  incoming automated review and address applicable feedback until approval or no
  actionable findings remain. Under the standing user instruction, do not inspect
  PR CI; use the recorded local gates. Do not merge autonomously.

## Testing cost

Stages are sequential: establish native semantics first, then compiler/device
traffic, queued transition behavior and media ownership, then full differential
execution. Use focused tests and devices without DSP during translation work.
Reuse the current Marlin model; expand integration runs when behavior changes,
and perform broader checks at delivery. This keeps intermediate feedback quick
while retaining exact end-to-end checks for every supported looping operation.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Implementation notes

The native contract probe confirms that State/Export queries complete at the
settled pre-audio boundary. The integrated RTL previously exposed host access
only after finishing the quantum. An explicit settle/pause command on the
existing coordinator port now permits queries before the normal run resumes. This keeps
TP state authoritative and preserves native observation cuts without predicting
loop state in Rust or introducing a second sample-processing path.

Native contract tests cover pre-capture query cuts, immutable retained media,
empty playback and future domain cancellation. The first queue-driven RTL case
matches 12 quanta of audio, state, events, transitions and export metadata through
two recordings. The generation-checked host media registry opts into physical view reuse,
preserving bounded view capacity as recordings/exports are reclaimed; raw-ID
clients retain append-only view identities by default. Directed and seeded
queue replays compare playback/latest ownership separately, partial byte reads,
EOF, stale handles, retained exports and many recordings in an eight-chunk arena.


Final local gates and replay measurements are recorded in
`src/rust/rt_fpga_sim/MONO_VALIDATION.md`. All eight deadline failures from the
3,222-test native run passed in one isolated retry. All work is pushed to
`feat/fpga-mono-looper` and [PR #981](https://github.com/SanderVocke/shoopdaloop/pull/981)
targets `proto/fpga-rust-reference`. Review submissions, inline comments and
conversation comments were checked; no actionable findings were present at
delivery. No PR CI was inspected and no merge was attempted.
