# First FPGA runtime adapter: Marlin simulation and queued gain graphs

Status: completed on `feat/fpga-runtime-adapter`.
PR: [#979](https://github.com/SanderVocke/shoopdaloop/pull/979), targeting
`proto/fpga-rust-reference`.

During execution, fetch and merge `origin/proto/fpga-rust-reference` at the start
and between milestones. Push only the working branch; the final PR targets
`proto/fpga-rust-reference`.

## Marlin prerequisite and sequence

Start by embedding the existing RTL in a native Rust program through Marlin,
then build the queue-driven adapter on that simulation interface. The new
demonstrator, adapter and end-to-end runner must build/run without Python or
cocotb. Existing Python/cocotb suites remain available independently; migrating
or deleting them is outside this milestone.

A scratch investigation already drove the existing 746-bit command FIFO with
Marlin 0.21.1 and Verilator 5.032, checking 20 transfers, odd-depth pointer wrap,
stalled reads and VCD output. This is feasibility evidence, not completion of
the integration below. Pin the simulation dependencies and retain reproducible
tests in the repository rather than depending on scratch artifacts.

The Marlin stages first establish a reusable device driver and a standalone gain
demonstrator. The remaining stages then follow the original queue/compiler/
adapter plan, using that same Marlin driver throughout.

## Goal and scope

Build a Rust consumer for the existing `rt_runtime` SPSC interface that compiles
small production graphs at runtime and controls existing RTL through
Marlin/Verilator in the same Rust process. Compare execution with production
Q8.24; no board deployment.

Adapter profile: Q=32, one mono input/output, one or two instances of one whitelisted
gain processor, initial graph installation and quantum-aligned instant gain
changes. Exercise both chained gains and parallel gains summed at the output.
Exclude looping, MIDI, FX, composites, media transfer, ramps, live replacement
and descriptor double buffering. Explicitly reject unsupported operations.

## Immutable acceptance criteria

1. The producer submits ordinary `Install`, `Commit` and scheduled typed gain
   operations through the existing queues and reads existing completions and
   execution receipts. Preserve their supported semantics and object ownership;
   change only necessary construction/access plumbing.
2. The adapter consumes actual installed processor descriptions and the submitted
   production `PreparedPlan`. It generates the persistent SP program, state and
   bindings and existing empty TP topology. Tests provide no precompiled image
   or expected jobs/state to hardware.
3. All output samples match production Q8.24 exactly. Supported command outcomes,
   sequences, epochs and effective/execution frames match at identical boundaries.
   Reject unsupported timing rather than silently rounding it.
4. Gain changes update state without recompilation. Report compilation, image
   size and configuration-transfer measurements separately, without claiming
   board deadlines or larger-graph scalability.
5. Unsupported installs, stale identities, capacity failures and live commits
   are rejected without corrupting the accepted configuration. Queue pressure
   loses neither requests nor terminal results; hardware faults are surfaced.
6. A standalone Rust demonstrator resets/configures the existing processing
   pipeline, executes a loop-free gain workload and checks every output sample
   against production Q8.24. It supports deterministic stalls, bounded waits
   and useful waveform output through Marlin.
7. The complete new build/run/replay path has no Python/cocotb dependency or
   subprocess. Marlin remains an optional native simulation dependency; normal
   CPU and Wasm production builds do not require it or Verilator.

## Design constraints

- Keep queues, Rust objects and retirement bookkeeping on ARM; hardware sees
  values/handles. Preserve cancellation-before-dequeue and credit/collection
  rules. A device FIFO write is not execution completion.
- Extract/reuse the existing production-backed gain processor and shared DSP;
  describe its concrete type/configuration through small accessors. Extract the
  reusable lowering from `rt_backend_testbench/src/rtl.rs`, preserving production
  ordering. Do not create another audio, loop or FPGA execution model.
- Compilation may allocate on an ARM service thread. Leave native sample
  processing unchanged. Prepare/validate an entire candidate before device writes;
  acknowledge activation after loading. Apply due controls before stepping each
  quantum; this simulation makes no ARM service-time guarantee.
- Wrap existing RTL configuration, command and audio ports behind a small Rust
  device-access interface shared by the demonstrator and adapter. Rust owns
  clocking, packing, handshakes, compilation and results; Marlin evaluates RTL.
  Keep the DDR AXI master unchanged. No DRAMsim3/Python memory-service migration
  is required for this gain-only profile.
- Use generated static Marlin bindings for the fixed simulation top. Provide a
  thin wrapper with Rust-driven clock inputs and explicit port widths/parameters;
  avoid the existing test wrapper's behavioral `#5` clock. Supply source/include
  manifests from Rust or checked-in data without running Python generators.
- Define deterministic drive/evaluate/sample phases, reset order, timestamps and
  cycle timeouts. Keep simulator construction/building outside audio callbacks;
  simulation speed is not an audio deadline guarantee. Compile/cache the RTL
  model separately from runtime graph lowering: gain updates must trigger neither.

The native runtime requires declared stable slots before startup. The demonstrator
therefore prepares matching slots on both sides, then queues ordinary `Remove`
and next-generation `Install` before its initial `Commit`. Startup removal is
supported only before activation; this exercises the existing ownership protocol
without adding dynamic slot allocation or live graph replacement. Initial commit
is accepted at frame zero; unsupported later activation is rejected explicitly.

## Stages

### 1. Marlin simulation foundation

- [x] Create an implementation branch from latest `proto/fpga-rust-reference`.
  Add an optional native simulation crate/feature with pinned Marlin and recorded
  Rust/Verilator/C++ tool versions. Keep its dependencies out of native production
  and Wasm runtime paths.
- [x] Build a thin wrapper and Rust source manifest for the existing integrated
  TP/SP/JP pipeline with empty logical TP configuration. Add static bindings, clock/
  reset stepping and a bounded device-access driver for configuration, status
  and raw sample I/O. Preserve the synthesizable processing design.
- [x] Verify 129-bit state and wide TP/control packing, reset and stalled
  ready/valid handshakes. Promote the scratch FIFO check into a reproducible smoke
  test where useful; include odd-depth wrap. Verify waveforms record clock edges
  and data, independent model lifetimes/reset, and cached model creation.

### 2. Standalone Rust/RTL gain demonstrator

- [x] Add a Rust executable/test that loads a loop-free gain graph into the
  existing pipeline through the stage-1 driver and steps Q=32 audio. Initially
  use the current Rust fixture/compiler APIs and production Q8.24 execution;
  no new queue adapter or graph-lowering rewrite is needed for this prerequisite.
- [x] Check every output word for signed inputs with nonzero low bits, silence
  and gain updates. Exercise deterministic configuration/sample stalls and bounded
  timeout/device-error reporting. Reuse production arithmetic rather than writing
  another gain executor for expected results.
- [x] Provide one Cargo command, replayable inputs, optional VCD output and a
  short result report. Confirm build/run uses no Python, including source
  generation and packing. Gate the queue-adapter stages on this demonstrator
  passing; preserve it as a direct device-driver regression.

### 3. Reusable queue plumbing and supported processor

- [x] Factor endpoint/receipt construction just enough to attach an alternative
  consumer while retaining the native `GraphRuntime` path and public commands.
- [x] Extract the gain processor/typed operation and its description. Add the
  adapter's registry, generation checks and supported-operation profile. Verify
  native gain behavior and queue ownership/credits with focused Rust tests.

### 4. Runtime graph lowering

- [x] Extract the existing compiler into reusable Rust code accepting the actual
  ordered plan, supported-node registry and hardware limits. Produce program,
  initial state, bindings and empty TP configuration. Keep oracle fixtures using
  the same lowerer.
- [x] Verify chain, parallel fan-in and silent-output images, including exact
  arithmetic grouping, identities and refusal before device writes.
  Instrument validation/lowering time, instruction/state counts and image bytes.

### 5. Adapter command and result flow

- [x] Implement initial `Install`/`Commit` handling and queued duration-zero gain
  operations. Use existing scheduling/receipt machinery where practical; match
  native admission/delivery/terminal reporting. Keep address mappings stable.
- [x] Test translation with a recording/stalling device double that executes no
  DSP. Cover unsupported requests, stale IDs/epochs, pressure, unread results,
  pre-dequeue cancellation and device errors. Assert gain changes update state
  without compiling/reloading the program.

### 6. Queue-driven Marlin integration

- [x] Connect the adapter to the existing Marlin driver. Load configuration
  generated from queued installs and the actual submitted plan, supply raw
  inputs and return samples/status. Do not supply the stage-2 fixture image to
  this queue-driven path. Test stalled handshakes and surfaced device faults.
- [x] Run the same producer workload through the production Q8.24 runtime and
  the adapter/RTL. Compare every output word and supported receipt/frame. Use
  signed samples with nonzero low bits, chained/parallel graphs and several
  quantum-aligned gain changes. Retain replayable command/input traces.

### 7. End-to-end validation and delivery

- [x] Document one-command differential execution and failure replay. Verify
  unsupported operations/device faults. Assert one graph compilation per accepted
  initial graph, none for gain changes, and no Verilator rebuild when loading
  graph/parameter data into an unchanged model. Record host/tools, separate RTL
  build/load and graph compile/configuration timings, and image sizes.
- [x] Run affected Rust/transport tests, compiler/packing contracts, the standalone
  demonstrator and queue-driven Marlin comparisons with stalls. After lowerer
  extraction, preserve the original looping regression gate: replay one existing
  record/playback case through the direct Marlin driver and functional SV memory,
  comparing audio, state and captured samples with production Q8.24. Port only
  the driver operations needed for that case; this does not add looping support
  to the adapter. Retain the remaining cocotb suites independently, without making
  them a prerequisite of the new runner.
- [x] Run formatting, warning-denying workspace build, test-policy and dependency
  isolation checks, including relevant Wasm builds when available. Verify a fresh
  simulation output directory and document prerequisites/failure replay. Reuse
  unchanged expensive evidence; do not repeat the waived 64-loop benchmark.
- [x] Commit/push milestones, open a PR and address applicable incoming automated
  review feedback. Do not inspect PR CI under the standing user instruction, and
  do not merge autonomously.

## Testing cost and follow-up boundary

Start with narrow Marlin binding/handshake checks and one standalone gain workload
to settle simulator integration before refactoring the queues/compiler. During
adapter work use focused native tests and a device double checking traffic, not
audio. Then add queue-driven RTL comparisons and bounded integration/regression
gates. Reuse compiled models and earlier passing checks unless edits invalidate
their evidence. A complete cocotb migration, multi-clock verification framework
and realistic DDR-service port are separate follow-up work.

Live replacement follows later: coordinate SP program/bindings and TP topology
activation, retaining/migrating mutable state and pending work. This milestone
supplies the compiler and adapter, not that activation mechanism.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Implementation evidence

- `rt_fpga_sim` supplies the optional native static Marlin bindings, fixed wrapper,
  direct gain demo, queued differential runner and original recording replay.
- `rt_fpga_adapter` supplies the shared lowerer and owning queue consumer. Native
  construction reuses the factored queues/scheduler; the shared gain processor
  retains production DSP. Recording/stalling doubles check protocol behavior.
- A reproducible Marlin/Verilator context-lifetime fault required a small local
  bridge patch. Each model now owns its context, traces close before models,
  and independent runtimes pass concurrently. The patch and upgrade boundary are
  documented in `third_party/marlin-verilator/SHOOP_PATCH.md`.
- Final commands, measurements and any environment-related check failures are
  recorded in `src/rust/rt_fpga_sim/VALIDATION.md`.
