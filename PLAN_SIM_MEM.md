# Native SystemVerilog memory for direct simulation

Status: in progress on `proto/fpga-native-sim-memory`; stages 1–4 verified; CI/review delivery pending.
Investigated at `54c616cf` on `proto/fpga-rust-reference`.

## Goal and scope

Make direct functional simulation use a simple, non-synthesizable SV AXI memory
instead of a Python AXI responder. Keep Cocotb for fixtures, DUT control and Rust
comparisons. DRAMsim3 through the existing Cocotb/ctypes path is the realistic
memory simulation. Direct mode only needs to execute correctly.

Use the existing CLI names to limit churn:

| Mode | Memory implementation | Purpose |
| --- | --- | --- |
| `instant` (default; "direct") | Native SV AXI RAM | Functional simulation |
| `dramsim3` | Existing Python AXI slave + native DRAMsim3 | Realistic memory timing |
| `fixed`, `legacy` | Existing implementations | Artificial-delay/protocol diagnostics |

This work does not port DRAMsim3, add DPI-C, replace Cocotb, change synthesizable
RTL or Rust semantics, or attempt to reproduce DDR behavior in SV. It does not
require matching the old Python responder's cycles, random sequences, latency
statistics or report internals. No minimum simulation speedup is required.

## Immutable acceptance criteria

1. `--memory-model instant` uses SV-owned storage and SV-driven AXI responses in
   the endpoint/adapter, chunk, media, audio and connected-loop runners. Ordinary
   direct runs require neither DRAMsim3 nor its native library.
2. Direct memory services transactions from the simulation clock without a
   Python memory-service coroutine or per-cycle `drive/sample/step` calls.
   Cocotb may still drive clocks, stimulate the DUT and observe execution.
3. Supported direct functional cases match Rust for statuses, samples, state,
   media contents and ordering. Both AXI widths, chunk bursts and word transfers
   work. Setup images and snapshots reflect the actual SV backing store.
4. AXI handshakes, byte strobes, stalled-response stability and coordinated reset
   are correct. Reset cancels protocol state and retains backing samples;
   explicit fixture loading replaces samples when requested.
5. `dramsim3` retains its current functional, fault, timing, physical-drain and
   statistics behavior. Its end-to-end Rust comparisons continue to pass.
6. Reports identify direct simulation as functional. DDR service/utilization
   fields are absent or explicitly unavailable there. Realistic memory timing
   conclusions use DRAMsim3; old instant-mode cycle counts are not acceptance
   targets. Existing substantive fault/protocol coverage remains exercised on
   the Python backend when it is outside the direct model's scope.

## Design rules

- Keep all DUT memory traffic through the existing AXI RTL. Instantiate the SV
  slave in simulation wrappers; keep it out of production tops and synthesis
  source lists. Support the current Verilator flow, including manually driven
  clocks and the connected testbench's free-running clock.
- Use a bounded word/byte array sized for the fixture arena, not the full DDR
  device. Implement the AXI subset exercised here: 32-/64-bit data, fixed ID,
  aligned INCR transfers of up to 16 beats, narrow byte lanes and WSTRB. Check
  bounds and unsupported shapes instead of silently wrapping addresses.
- One outstanding transaction is sufficient. A saved write address and a small
  beat buffer can handle independent AW/W arrival, including W before AW.
  Reads may return one beat per clock; writes complete after their accepted
  payload. Hold R/B responses until accepted. Simple deterministic timing is
  sufficient; no banks, refresh, native-block scheduling or AXI reordering.
- Use bulk image import/export at setup and fenced snapshots, for example
  `$readmemh`/`$writememh` with a small request/acknowledgement interface. Allow
  these operations only during reset or quiescence. Do not mirror live samples
  into Python each clock or introduce a transparent mutable-list proxy.
- Introduce explicit async load/readback helpers where drivers currently share
  and mutate `memory`/`words` lists. The Python backend may implement those
  helpers with its existing lists. Audit direct accesses so comparisons cannot
  accidentally inspect a stale Python copy.
- Keep controls and counters minimal: busy, read/write counts, and any small
  pause/error controls needed by retained direct tests. Detailed scripted fault
  injection and randomized memory timing can stay on the Python backend. Make
  test selection explicit; do not silently ignore requested faults or disable
  assertions to obtain passes. Consumer backpressure remains supported.
- Keep Python for result formatting. Direct reports may omit transaction
  histories, latency distributions and native-memory statistics. Read basic
  counters at checkpoints rather than maintaining them through Python each
  clock. Preserve realistic-mode reports and probe behavior.

## Implementation stages

Stages are sequential. Each stage depends on the preceding stage's verification.

### 1. Establish the implementation branch and baseline

- [x] Fetch the latest FPGA branch and check local changes/instructions. Create
  a dedicated implementation branch from `proto/fpga-rust-reference`.
- [x] Audit runner selection, generated tops and storage/probe access in
  `fpga/axi_memory/sim/{options,word_service}.py`, endpoint tests,
  `chunk_driver.py`, `media_driver.py` and `loop_memory.py`.
- [x] Record supported direct tests versus Python-specific fault/timing tests.
  Define the small image/control interface before changing callers.
- [x] Verify one small current direct case and one DRAMsim3 record/playback case;
  record a simulation wall-time baseline with rebuild time separated out.

### 2. Build the standalone functional SV memory

- [x] Add a simulation-only module under `fpga/axi_memory/tests/` implementing
  the bounded RAM, AXI subset, reset and bulk image interface above.
- [x] Add focused tests for both widths: image round-trip, reads/writes, strobes,
  upper lanes, single/multi-beat bursts, AW/W ordering, R/B backpressure and
  reset during a transfer. Verify retained contents and reject invalid bounds.
- [x] Verify warning-clean Verilator compilation and those small tests. Keep
  elaborate fault scenarios on the existing Python model.

### 3. Integrate direct mode and migrate image ownership

- [x] Add direct simulation wrappers, extending `generate_tops.py` where useful,
  for endpoint/adapters and integrated tops. Select them only for `instant`.
  Ensure exactly one component drives AXI slave signals.
- [x] Add a small Python administration adapter for image exchange, occasional
  controls and counters. It must not emulate transactions or service clocks.
- [x] Update drivers' initialization, reset/reload, fences and snapshots to use
  explicit image helpers. Remove direct-mode Python AXI servicing from manual
  tick functions and the free-running `LoopMemory.serve` path. Retain existing
  test observation hooks independently of memory servicing.
- [x] Update mode metadata, capability-based test selection and reports; keep
  `dramsim3`, `fixed` and `legacy` routing intact. Avoid a second backend flag.
- [x] Verify a small Rust comparison after each integration boundary: chunk,
  media, audio, then connected loop. Include a minimum-size chunk and dirty
  writeback followed by fence/readback. Rerun a realistic case after modifying
  shared driver/image code. Check generated wrappers for drift.

### 4. Validate end to end and document the result

- [x] Run the direct functional FPGA suites over normal, small/odd and minimum
  applicable capacities. Cover 32-/64-bit buses, burst and word paths,
  recording/playback, shared ranges, wrapping, cache forwarding and slot reuse.
  Reuse existing Rust fixtures instead of inventing another expected model.
- [x] Run the Python-specific AXI/fault tests on DRAMsim3, including reset and
  failures during processing, plus connected record/playback Rust comparisons.
  Keep unsupported direct cases explicitly routed to these checks.
- [x] Run the same four-quantum 64-loop fixture in direct and realistic modes.
  Compare each to Rust; do not require their cycle counts to match. Confirm no
  Python AXI responder is active in direct mode and no native DDR dependency is
  loaded. Record host simulation time separately from simulated cycles/time.
- [x] Verify wrapper consistency and that synthesis manifests exclude the new
  model. No repeated full device synthesis is needed for testbench-only changes.
- [x] Update simulation documentation with commands, functional/realistic mode
  distinctions, supported direct controls and the observed wall-time comparison.

### 5. Deliver and validate CI/review

- [ ] Push the implementation branch and open a PR targeting the FPGA branch,
  or update an explicitly designated existing PR rather than creating a duplicate.
- [ ] Ensure CI covers a native direct smoke and a realistic record/playback
  smoke with the pinned DRAMsim3 dependency; add focused coverage if missing.
  Check relevant CI results and fix failures until required checks are green.
- [ ] Check for automated review feedback, address actionable findings, and
  repeat review/CI checks until the automated reviewer approves, if present.
- [ ] Leave the PR ready for review. Do not autonomously merge it.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Execution notes

Stage 1: upstream remains `54c616cf`. Small-profile direct and DRAMsim3 suites
pass all three tests (directed record/playback, preparation/fault/reset); test
execution excluding build is 6.38 s and 6.31 s respectively. The initial runs
were concurrent, so these are approximate host-time baselines.

The image interface will use a fixed per-test-directory `sim-memory.hex` file
and a clocked load/dump request/acknowledgement. Simple pause, response-pause,
channel-hold and next-transaction-error controls stay available in direct mode;
Python-only mid-beat injection/native timing statistics stay on the realistic
or diagnostic backend. Direct service uses no random memory delays.

Stage 2: standalone native RAM passes warning-clean Verilator builds and the
functional/protocol suite at 32 and 64 bits. Tests cover partial byte writes,
all supported beat sizes, 1/2/16 beats, independent AW/W, held R/B, local bounds
errors, injected failures, image round-trip and reset preserving samples. The
Python image-administration helper was developed with these tests; it performs
no bus scheduling. Full driver integration remains stage 3.

Stage 3: all ten endpoint/adapter tests pass in native mode at both widths;
minimum-size chunk, standalone media, media/audio and connected-loop smoke
suites pass against Rust. The small DRAMsim3 loop suite also passes after the
shared driver changes. Generated simulation variants preserve DUT hierarchy
and retain read-only AXI probes; production RTL files are unchanged.

Native counters are relative to each Python administration instance, while SV
reset preserves backing storage. Snapshot checks now explicitly read that
storage. The short-job active-arithmetic fault-placement test is explicitly
Python-only because it depends on responder latency; ordinary direct faults,
reset and playback fault propagation remain checked.

Stage 4: native direct endpoint, chunk (both widths and all capacities), media,
audio (including single-buffer multi-chunk), and connected suites pass. Direct
connected coverage is 102 cases / 808 quanta. Realistic endpoint, audio (37 /
93), and connected (39 / 254) suites also pass. The short-job scripted arithmetic
fault check is restricted to >=16 frames: DDR burst drain can legitimately
outlast shorter jobs. Both gain/mix fault placements remain checked in the
full-size realistic profile; ordinary faults remain checked in all profiles.

Four 64-loop quanta match Rust in native direct, Python zero-delay diagnostic,
and realistic modes. Host simulation times excluding compilation are 21.19 s,
33.33 s and 36.05 s in one local observation; these are not guaranteed speedups.
Direct mode works with a missing DDR library and has no per-cycle memory
responder API. Reports label backend/purpose and omit direct-mode DDR counters.
Generated wrappers pass drift checks; synthesis lists do not include the model.
A focused GitHub Actions workflow now covers native and realistic smokes.
