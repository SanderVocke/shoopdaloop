# FPGA boundary milestone 1: ARM-owned request slots

Status: implementation pushed on `feat/fpga-arm-request-slots`; final expanded
local regression stopped at the user's explicit request. CI is unreviewed by instruction.
Parent roadmap: [PLAN_LONGTERM_FPGA_BOUNDARY.md](PLAN_LONGTERM_FPGA_BOUNDARY.md).
Integration target: `proto/fpga-rust-reference`.
Implementation branch: `feat/fpga-arm-request-slots`.
Investigated baseline: `5b7dd0a5b242884f86862991dc7b456a5b091df7`.

## Goal and scope

Establish a real, testable boundary between ARM request management and FPGA
transition execution. A reusable Rust service owns request allocation, duplicate
sequence detection and original command metadata. FPGA installs an action into
an explicitly named slot and continues to resolve its musical behavior.

Retain the current shared request pool and allocation policy, 640-bit command
payload, 832-bit response, packed loop/request/event records, pending lists,
terminal retention and processing phases. Keep source-first settlement,
self-sync ordering, repeat waits, arbitration and stop cancellation in RTL.
Keep existing cancel, collect, readback, fault and reset behavior.

This milestone covers the existing primitive TP operations and supported quanta,
including the application's empty-content clear extension. It does not expand
the public mono/application profiles or introduce composites, new musical modes,
a physical ARM transport or a finalized register map.

Compacting records, moving pending-list membership, removing mandatory fabric
fences, isolating held response backpressure, removing application settlement
pauses and qualifying physical deadlines belong to later roadmap milestones.
The service-pause tests here finish outstanding transfers before pausing ARM.

## Immutable acceptance criteria

1. High-level submission through the Rust service preserves current admission
   errors, shared capacity, lowest reusable-slot allocation, generation retirement
   and duplicate-sequence retention through collection. Raw indexed installation
   has its own documented misuse errors; it is not the public request API.
2. Production Rust and service-plus-RTL agree on supported loop states, ordered
   events, pending actions, outcomes and requested/effective/terminal frames at
   identical logical delivery boundaries. Compare integrated Q8.24 audio and
   captured/exported samples exactly, retaining the existing float-accuracy gate.
3. The candidate synthesized TP has no free-slot search or whole-table duplicate
   scan. It accepts a caller-selected slot/token, guards that address and identity,
   and initializes execution state itself. Its real-time resolver and lists stay
   in hardware, and storage capacities/layouts remain unchanged.
4. One reusable Rust service runs above the simulation transports. The original
   cocotb host bridge and its upstream Marlin replacement use that same owner.
   Python preparation does not implement another allocator, duplicate registry
   or response owner. Marlin execution needs no Python subprocess. Production
   Rust remains an independent oracle.
5. After installation acknowledgements are consumed, ARM service can stop for
   many quanta while the harness advances audio without invoking that service.
   Resident absolute and source-timed actions, wraps, repeat waits, sync chains,
   supersession and stop cancellation still behave correctly. Uncollected
   terminal records survive and consume their normal credits.
6. Existing application/mono queue behavior, supported-operation whitelists,
   receipt ownership and failure propagation remain intact. All current TP
   submission producers use the new installation contract; generated demos and
   board workload preparation continue to work.
7. Publish reproducible matched baseline/candidate command and processing cycles,
   source/tool/capacity manifests, and TP resource/timing measurements. No minimum
   LUT/BRAM saving is promised for this layout-preserving milestone. Missing
   measurement tools remain an explicit incomplete gate, not an inferred result.

## Design rules and boundary contract

- Preserve the existing application/runtime queues and public request/reply
  types. Add a service module under `src/rust/rt_fpga_adapter`; keep simulator,
  Python and oracle types out of it. Prepare bounded metadata from device limits.
- Use a single owner and at most one outstanding device command initially. Keep
  command exchange separate from the test harness's audio progression. Functional
  input/output movement and read-only observers may run during a service pause;
  request mutation, status polling for management, collection and hidden service
  calls may not.
- Reserve a slot before installation; publish acceptance only after its device
  acknowledgement. A definite refusal rolls back the reservation. A timeout or
  uncertain transfer faults the service; do not reuse a possibly installed slot.
- Retain request/sequence ownership until successful terminal collection.
  Cancelling a request does not free its slot. Collecting a pending request does
  not free it either. Mirror the device's checked generation transition, including
  retirement at zero, and preserve the existing collection/receipt cuts.
- Reset/reload invalidates the service's old associations. Reinitialize only with
  a coordinated device reset; old responses or commands must not enter the new
  instance. Do not add an unsafe host-only recovery path.
- ARM validates request metadata and duplicate sequences in the existing error
  precedence. FPGA retains bounds, configured/idle/fault guards, slot/token
  checks, target/source identity and relationship checks, and execution-dependent
  validation. A raw installation bypassing the service is a low-level trusted
  caller, not a second public command admission path.
- Keep `max(requested_frame, device_frame)` rounding and overflow checks in FPGA
  for now. Rust obtains authoritative effective and terminal frames from device
  records; it must not estimate them from a stale host clock.
- Reuse opcode 4 as indexed `INSTALL`: index supplies the slot and the existing
  request-generation header field supplies a nonzero reuse token. Preserve port
  widths and the successful slot/generation response. Specify exact bit packing
  and errors before changing callers. Sanitize result, remaining-tick and list
  fields exactly as the current submission path does.
- Hardware checks that the addressed row is empty and its stored generation
  matches before mutation. Retain Prepare initialization and Collect generation
  advancement as guards. Removing allocation does not remove identity protection.
- The existing Rust command codec accepts fewer operations than standalone TP
  tests. Factor a complete codec for already-supported primitive RTL actions,
  including positioned play/seek/retrigger, without broadening application
  whitelists. Do not use expected resolver output to prepare device actions.
- Keep the old design as a frozen baseline, rather than retaining a synthesized
  legacy allocator beside the new path. Temporary host/test compatibility must
  disappear at cutover. Regenerate and invalidate affected prepared streams.

## Relevant code and evidence

- `fpga/transition_processor/rtl/transition_processor.sv`: `S_SUBMIT_SCAN` /
  `S_SUBMIT_CHECK`, indexed RAM helpers, generation initialization, list insertion,
  cancellation and collection.
- `src/rust/rt_fpga_adapter/src/{protocol,looping,engine}.rs`: current packing,
  adapter request ownership and submission/read/cancel/collect call sites.
- `src/rust/rt_fpga_sim/src/simulator.rs`, `rtl/marlin_pipeline.sv` and
  `tests/{marlin,mono_rtl}.rs`, plus
  `src/rust/shoop_rt_host_fpga_sim/tests/app_rtl.rs`: native RTL transport and comparisons.
  The default Marlin top has four loop slots; application adapters admit one.
  Exercise a short sync chain directly without expanding the application profile.
- `src/rust/rt_fpga_testbench/src/{drivers/transition,suites/transition,
  suites/production}.rs`: parameterized transition and full-pipeline comparisons.
  Upstream `2bdd10c9` replaced the original cocotb implementation during this work.
- `src/rust/rt_backend_testbench/src/{main,transition_fixture/mod}.rs`: production
  oracle entry points and the location for a separate service transport bridge.
- `fpga/audio_io/prepare_demo.py`, `fpga/zedboard/prepare.py` and their consumers:
  prepared command producers using the current submission opcode.
- `fpga/transition_processor/{measure.py,check.sh}` and
  `measurements/ACYCLIC_SYNC.md`: matched TP measurement facilities and historical
  evidence. The recorded complete core still misses 100 MHz; do not confuse
  standalone TP timing with complete-core closure.

## Stages and verification

Complete stages in order. Each stage's relevant checks gate the next; make the
wire-interface cutover coherent across all callers.

### 1. Freeze the baseline and installation contract

- [x] Create the implementation branch from the latest fetched
  `proto/fpga-rust-reference`, preserving local work. Record the exact base revision,
  source hashes, capacities and tools; refresh investigations if the base changed.
- [x] Record submission/validation precedence, slot reuse, terminal retention,
  reset behavior and the new INSTALL encoding/error table. Inventory every direct
  submission caller, including tests, generators and checked-in prepared assets.
- [x] Run representative baseline transition and Marlin cases. Save separate
  admission, processing and observation cycle counts under `work/`, including
  shallow/deep request pools, retained terminals and duplicate/full refusals.
- [x] Verify the common corpus covers the acceptance criteria and identify only
  missing behavioral cases. Preserve the baseline independently of candidate
  changes for matched synthesis and replay.

### 2. Implement the reusable Rust request service

- [x] Add the bounded slot/sequence registry, original request metadata, codecs,
  reserve/acknowledge/refuse transitions, cancellation and collection handling.
  Preserve the public error precedence and reuse policy recorded in stage 1.
- [x] Separate preparing a device transfer from consuming its reply. Provide a
  synchronous transport wrapper for Marlin and a small stateful JSONL bridge in
  `rt_backend_testbench` for cocotb to drive those same service transitions.
  Keep this bridge separate from production oracle execution.
- [x] Verify with transport-only doubles: empty/full pools, duplicate pending and
  retained-terminal sequences, cancellation without release, pending collection,
  repeated/stale operations, generation retirement, definite refusal rollback,
  uncertain acknowledgements and coordinated reset. Doubles perform no resolution
  or audio processing. Keep the existing RTL path working at this stage.

### 3. Cut over RTL and every installation producer

- [x] Replace allocation/duplicate scanning with an indexed row read and guards.
  Feed the selected slot/token into existing rounding and insertion. Preserve
  real-time arbitration, all list operations, terminal records and packed widths.
- [x] Connect the Rust service to mono/application adapters and direct Marlin
  scenarios. Route cocotb's high-level requests through the shared Rust bridge;
  retain raw INSTALL tests for malformed low-level traffic.
- [x] Update prepared audio/ZedBoard producers through shared Rust preparation
  helpers. Their finite streams may reserve distinct slots without reuse where
  they never collect; check total credits without predicting musical outcomes.
  Regenerate checked-in assets and check wrapper/source manifests.
- [x] Verify raw out-of-range, occupied, zero/stale-token and non-idle installs,
  failed-install non-mutation, retained terminals, reset and held ready/valid
  responses. Reclassify old raw SUBMIT duplicate/full checks as service-level
  tests; preserve end-to-end rejection coverage.
- [x] Pass a first Rust-service/Marlin comparison through TP/SP/JP, including a
  record/play/stop cycle, slot reuse and exact audio/state/outcome comparisons.
  Confirm all callers migrated and the candidate contains no legacy allocator.

### 4. Prove semantics under independent service schedules

- [x] Add deterministic, independently driven ARM-service and audio progression
  entry points. Preinstall actions, consume acknowledgements, then disable all
  service calls for at least 1,024 quanta. Resume and collect exact results.
- [x] Cover future absolute actions, gated AfterTicks countdowns, self-sync,
  external-source chains, repeat waiting, competing eligible actions, failed
  higher-priority actions, stop masking and cancellation of older future actions.
  Keep all required actions resident before pausing service.
- [x] Compare audio throughout the pause, then authoritative state and retained
  terminal records after resumption. Use nonfunctional observation hooks if
  intermediate events are needed; do not secretly service queries/collection.
- [x] Separately test slow service before admission, same-frame arrival/cancel
  ordering, delayed collection, capacity exhaustion and stale reuse tokens.
  Match oracle delivery cuts explicitly; service delays must not be hidden by
  adjusting expected outcome frames after a mismatch.
- [x] Run the full directed/seeded TP corpus across all five existing capacities
  and the production pipeline corpus with stalls/faults. Use a short multi-loop
  Marlin scenario and existing queue-driven mono/application replays. This stage
  does not claim held-response isolation or continuous fabric autonomy.

### 5. Measure the boundary change

- [x] Replay matched baseline/candidate workloads at identical capacities and
  logical cuts. Report INSTALL, CANCEL, COLLECT, PROCESS and observation cycles
  separately, plus Rust service wall time, metadata size and transfer bytes.
  Include different pool depths and long per-loop lists: insertion is still
  variable work even after removing the allocation scan.
- [x] Use `measure.py` on frozen baseline/candidate RTL with identical Vivado,
  harness, part, parameters, thread count and constraints. At minimum use the
  existing integrated 64-loop/272-request/400-event, 100 MHz TP context and retain
  LUT/FF/BRAM, setup/hold and source/tool manifests. Run lint/generic synthesis
  across supported capacities, and check the integrated core for regressions.
- [x] Commit a concise report and machine-readable summaries. Report increases or
  unchanged BRAM honestly; do not shrink capacities or change constraints to
  manufacture a saving. Record and resolve measurement blockers before declaring
  the milestone complete. Existing core timing limitations remain visible.

### 6. Complete end-to-end validation and documentation

- [ ] Run the full native Rust suite once at this milestone, the complete relevant
  FPGA regression set, and bounded regular-host Marlin application replay. Compare public
  receipts, query cuts, audio, captured media, exports and fault propagation.
- [x] Check regenerated AXI/audio tops, prepared demo assets, ZedBoard workload
  preparation and continuous-audio regression. The existing fence-driven fabric
  path must still work, although removing its fences is deferred.
- [x] Update TP/adapter/simulator documentation with the host-versus-wire contract,
  opcode encoding, service ownership, replay commands, measured results and the
  remaining autonomy dependencies. Check every immutable acceptance criterion
  against linked evidence and update the parent roadmap.
- [x] Integrate the new `7e855f70` target-branch head, which arrived during this
  investigation. Preserve the shared owner across the regular host's FPGA
  backend, migrate application replay commands to `shoop_rt_host_fpga_sim`, and
  rerun affected host/application, pipeline and workspace checks. The isolated
  TP RTL baseline is unchanged by these upstream commits. Also integrate
  `2bdd10c9`'s full Marlin migration, moving the shared owner and raw guard tests
  into its native driver and prepared producer without retaining cocotb runners.

### 7. Publish, complete CI and address review

- [x] Commit completed stages or coherent milestones, push only the implementation
  branch, and create a PR targeting `proto/fpga-rust-reference`. Describe the
  final boundary, preserved semantics, deferred work and measurement evidence.
- [x] Ensure FPGA workflow path filters cover `rt_fpga_adapter`, `rt_fpga_sim`,
  the bridge and the plans. Add bounded service and app replay coverage where the
  existing workflow only runs `--features sim`. Archive actionable failure traces.
- [x] Publish the branch so configured CI starts automatically. The user
  explicitly instructed “Once pushed, don't check ci” on 2026-11-13, superseding
  this stage's CI inspection/wait gate. Do not claim final CI success or review
  approval. Address actionable review feedback and retain local validation.
  The PR has no submitted reviews or review threads at the final inspection.
- [x] Leave the PR ready for review/merge. Do not merge autonomously.

## Test-cost policy and principal commands

Use focused service tests and a small Marlin case while editing. Run all
TP capacities and the broader pipeline corpus after the cutover stabilizes;
run routed measurements and the complete native suite at the final milestone.
Repeat broad checks only for changes or failures that justify them. Follow
`.agents/info/build.md` for the selected environment; do not install another
toolchain over CI/Nix provisioning.

Principal existing gates, supplemented by the new service/bridge tests:

```sh
cargo test --locked -p rt_fpga_adapter
cargo test --locked -p rt_backend_testbench
cargo test --locked -p rt_fpga_sim --features sim -- --test-threads=1
cargo test --locked -p shoop_rt_host_fpga_sim --features sim --tests -- --test-threads=1
python fpga/transition_processor/simulate.py --config all --random-cases 20
cargo test --locked -p rt_fpga_testbench --features sim
cargo run --locked -p rt_fpga_testbench --features sim -- production --directed --random-cases 30 --stalls
cargo run --locked -p rt_fpga_testbench --features sim -- production-faults
python fpga/axi_memory/generate_tops.py --check
python fpga/audio_io/generate_top.py --check
python fpga/audio_io/generate_demo.py --check
python fpga/audio_io/prepare_demo.py --check
python fpga/zedboard/generate.py --check
```

Before committing Rust changes, run `cargo fmt --all` and the warning-denying
workspace build. If Rust tests change, run
`python3 scripts/check_shoop_test_usage.py`. At stage 6 use the complete native
suite in `.agents/info/test.md` and the existing FPGA workflow commands for
stalls, memory models, continuous audio and synthesis. Preserve exact commands,
seeds, tool versions and outcomes with the evidence.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Evidence index

- Admission/lifecycle and error precedence: [installation contract](fpga/transition_processor/INSTALL.md)
  and [shared owner tests](src/rust/rt_fpga_adapter/src/transition_service/tests.rs).
- The wire cutover, unchanged record capacities and real-time resolver:
  [RTL](fpga/transition_processor/rtl/transition_processor.sv).
- One Rust owner above both transports: [service](src/rust/rt_fpga_adapter/src/transition_service.rs),
  [JSONL bridge](src/rust/rt_backend_testbench/src/transition_service.rs) and
  [native wire driver](src/rust/rt_fpga_testbench/src/drivers/transition.rs).
  Python prepared-input tools use the [bridge client](scripts/rt_backend_testbench/__init__.py).
- Independent service/audio scheduling, retained results, captured media and
  authoritative late admission: [Marlin boundary tests](src/rust/rt_fpga_sim/tests/transition_boundary.rs).
- Public application/mono ownership and behavior: existing queue-driven Marlin
  replays, full application-feature suite, common production/RTL comparisons and
  regenerated prepared audio/board workloads.
- Matched baseline/candidate cycles, resource/timing results, source/tool
  manifests and full validation accounting: [measurement report](fpga/transition_processor/measurements/ARM_REQUEST_SLOTS.md)
  and [machine-readable evidence](fpga/transition_processor/measurements/arm-request-slots.json).
- Publication and CI: [PR #990](https://github.com/SanderVocke/shoopdaloop/pull/990),
  targeting `proto/fpga-rust-reference`; detailed local logs/checkpoints are under
  `work/fpga-boundary/`.

The complete native sweep found two baseline app-protocol golden vectors still
expecting version 34 while `VERSION` already equals 36. Correcting only those
bytes makes all nine codec tests pass. That intermediate sweep had 3,252
passes and two failures; the later clean full sweep supersedes it. Prepared-demo recovery also failed at the
frozen baseline: fault injection preceded epoch reset completion. Waiting for
the fresh boot before injecting preserves all fault-stage, recovery-count and
held-response drain assertions, and passes for both baseline and candidate.

The initial indexed-install implementation copied slot/generation into redundant
registers. A controlled cleanup reuses the latched command identity, reducing
Vivado synthesis from 12,075 to 11,287 LUTs and from 7,943 to 7,886 flip-flops.
The original baseline uses 11,208 LUTs and 7,979 flip-flops. Preserving old binary
state values alone instead increases synthesis to 12,206 LUTs and is not adopted.
None of these trials changes capacities, record layouts, constraints or semantics.

The integration refresh preserves the new host profile and protocol version 37,
including the upstream codec fixtures. Application replays now run from
`shoop_rt_host_fpga_sim --features sim`; the workflow's regular-host job covers
the full relocated suite. The new complete native Marlin workflow also runs
shared-owner/kernel checks after the component matrix.
The repeated-runtime kernel test exposed a redundant Marlin global tracing
initializer blocked in `VerilatedContext::calcUnusedSigs`. Each generated model
already enables tracing on its owned context. Removing the global initialization
makes both mono RTL tests pass in 0.73 seconds; the complete kernel suite and
relocated bounded replay also pass. The warning-denying workspace build, regular
host/carrier tests, normal application workflow and 45 pipeline cases pass after
integration. A subsequent serialized full native sweep passes all 3,261 tests
(11 skipped). A detached-host stderr failure was fixed by disabling internal
logger diagnostics on a closed diagnostic pipe; the original shutdown regression
also passes ten consecutive stress repetitions.

Upstream `2bdd10c9` then moved the complete FPGA suite to Rust/Marlin. The merge
preserves that migration: the new transition wire driver and native prepared-ROM
producer now use the shared request owner, and raw INSTALL guard coverage moved
to the new suite. The command measurement frontend also uses Marlin, with an
explicit frozen-legacy measurement option. All five transition capacities pass
20 seeded cases after this integration; R=11 command measurements reproduce
every historical cycle count. Fresh routed candidate measurements reproduce
11,310 LUTs, 8,299 flip-flops, 29 BRAM and +0.103/+0.006 ns setup/hold slack.
The complete new component matrix was started locally, then stopped on the
user's explicit “Push and stop, no more testing” instruction. Audio, audio I/O,
AXI and both ownership-campaign groups passed before interruption; the broader
matrix is not claimed complete. CI inspection after pushing is explicitly
waived by the user; no final CI result is claimed.


The expanded native matrix exposed a separate exported-scope teardown hang.
Verilator's scope destructor consults its thread's current context, so the bridge
must select the model's owned context before deleting that model. The fix also
bumps the compiled-model cache discriminator. A focused overlapping-model
regression, the complete 32/64-bit AXI matrix over all three memory backends,
the kernel suite and both independent-service tests pass with this fix.

Final handoff: upstream conflicts are resolved through `2bdd10c9`, and the
exported-scope context fix is pushed in `b30f7f21`. The remaining expanded local
regression was stopped, no further tests were started, and no CI inspection was
performed after the user instructed it to stop. No user input is pending.
