# Finish the ARM–FPGA boundary roadmap

Status: execution in progress; baseline frozen, focused resource review complete,
current TP retained. M4–M6 and physical qualification remain open.
Execution evidence: [boundary completion](fpga/verification/BOUNDARY_COMPLETION.md).
Parent roadmap: [PLAN_LONGTERM_FPGA_BOUNDARY.md](PLAN_LONGTERM_FPGA_BOUNDARY.md).
Prerequisite: [milestones 2–3](PLAN_FPGA_COMPACT_ACTIONS_AND_MEMBERSHIP.md),
[PR #993](https://github.com/SanderVocke/shoopdaloop/pull/993).
Planning/initial execution branch: `plan/fpga-boundary-completion`.
Stack base: `plan/fpga-compact-actions-membership` at
`60beabb53e1f4a5051db9baa2e3f69815f61ad18`.
Eventual integration target: `proto/fpga-rust-reference`.

This plan completes the remaining roadmap: a focused TP resource review and
opportunistic optimization, autonomous progression (M4), application integration
(M5), and system/physical qualification (M6). It includes the missing physical ARM
control and audio driver work needed for qualification. Writing this plan does
not execute those stages.
The user's instruction not to inspect CI after pushing overrides the planning
template's default CI-to-green loop. No autonomous merging is authorized.

## Goals and scope

ARM owns command validation/construction, admission bookkeeping, membership
preparation, public replies, queries and non-real-time exports. FPGA owns musical
time, admitted actions, dynamic validation, natural wraps, repeat waits, sync
propagation, arbitration, stop cancellation, sample processing and required media
operations at transition cuts. An admitted action never needs an ARM continuation.

Finish with an engine that runs from fabric/audio clocks while ARM command and
response service is absent. An empty control queue means ordinary progression.
Control traffic has bounded access to device resources and cannot determine when
audio runs. Application service consumes authoritative frame-stamped results
later, without a per-quantum settlement rendezvous.

Pursue clear-cut opportunities to reduce TP LUT usage where the measured benefit
justifies complexity, risk and verification cost. Investigate the M3 increase
before choosing another protocol and record the resource tradeoffs. LUT reduction
is welcome but is not required for roadmap completion or progression to M4.
An open-ended area search must not hold up autonomy/integration. The final system
must close timing. Existing musical behavior, supported operation profiles and
shared capacities remain. Composites/new modes, unrelated application rewrites,
other board ports and production boot/security redesign are outside this scope.

## Starting evidence and implementation

[The M2–3 report](fpga/transition_processor/measurements/COMPACT_ACTIONS_AND_MEMBERSHIP.md)
contains matched sources, harnesses, tool versions, raw reports and replay patches.

| Metric | Matched M1 | M2 | Latest M2–3 |
| --- | ---: | ---: | ---: |
| TP routed LUTs, L64/R272/E400 | 11,320 | 11,356 | 13,312 |
| TP flip-flops | 8,148 | 7,898 | 8,813 |
| TP BRAM36 equivalents | 29 | 25.5 | 28.5 |
| Dense PROCESS | 5,465 | 5,465 | 4,380 cycles |
| TP setup/hold, 100 MHz | +0.010/+0.047 | +0.030/+0.006 | +0.105/+0.007 ns |

Full dense admission now takes 276 transfers / 1,654 elapsed fabric clocks in
the one-loop workload. Retained terminals raise the steady 64-loop chain from
1,033 to 1,609 cycles. The separate L64/R128/E256 core experiment reduces LUTs
from 30,011 to 29,419 but worsens setup from -1.353 to -1.845 ns; both miss
100 MHz. Smaller raw records and removed responsibilities have not established
TP LUT savings. Banks/guards are candidate costs, not a completed attribution.

The final combined sweep ran 3,342 tests: 3,340 passed, two failed, 11 skipped.
Both failures pass targeted corrected reruns (an interrupted model cache and a
direct continuous setup path missing prepared membership). Keep that distinction;
do not relabel the initial sweep as wholly green or rerun it merely to erase it.

Important entry points:

- `fpga/transition_processor/rtl/transition_processor.sv`: bank selection,
  `staging_seen`/`active_seen`, publication/version guards, wide writeback and
  resolver FSM. [COMPACT.md](fpga/transition_processor/COMPACT.md) is ABI 3.
- `fpga/audio_io/rtl/{audio_control,audio_loop_pipeline,audio_io}.sv`: control
  fences, run gating, clock crossings, bank ownership, deadlines and recovery.
- `fpga/loop_pipeline/rtl/loop_pipeline.sv`: `HOST_SETTLE_WAIT`/`BOUNDARY`,
  execution priority, PROCESS/FINISH and state/job/media sequencing.
- `src/rust/rt_fpga_adapter/src/{transition_service,engine,looping,loop_media}.rs`:
  shared ownership; `Adapter::process` services commands; `Core::quantum` sends
  opcode 16, reads state/pending requests and observes media before rendering.
- `src/rust/shoop_rt_host_fpga_sim/src/{worker,lib}.rs` and
  `src/rust/rt_fpga_sim/src/simulator.rs`: model ownership, software-driven frame
  advancement, transport and application execution.
- `src/rust/rt_fpga_testbench/src/{prepared,command_measurements}.rs`, drivers,
  suites and tests: direct/prepared producers, independent production comparisons
  and continuous/deployed tests. Update Python preparation through the Rust owner.
- `fpga/{transition_processor/measure.py,synthesis/run.py,zedboard/}` and
  `platforms/zynq7000/`: matched physical experiments and actual ARM deployment.

The current ZedBoard benchmark keeps both CPUs halted, generates its input and
has no expected-output comparison. Its old hardware digest is not an audio
correctness proof. The ARM platform currently uses dummy audio; GP0 control is
disabled in the FPGA benchmark build. Real ARM/PL integration and codec I/O are
work in this plan, not existing capabilities to assume.

## Immutable acceptance criteria

1. **Behavior:** at identical device admission cuts, match production Rust for
   loop state, ordered events, pending countdowns, identities, requested/effective/
   terminal frames, outcomes and reasons. Preserve exact Q8.24 digital audio,
   captured/exported samples and media ownership, plus independent float bounds.
   Preserve every current supported quantum/profile and operation whitelist.
2. **Ownership:** preserve shared pool sizes, public queue/receipt semantics,
   error precedence, lowest-slot reuse, duplicate retention through collection,
   cancellation without release, generation retirement and coordinated reset.
   Unknown transport outcomes quarantine ownership. No counter/version wrap
   aliases an old action, observation, bank or media reference.
3. **Autonomy:** after setup/admission, continuous fabric I/O runs for at least
   1,024 quanta with zero ARM fences, settlement, command/membership updates,
   completion draining or required observation/media service. Cover future AT
   actions, source/self-sync, repeat waits, chains, supersession, failed winners
   and stop cancellation. Retained terminals and stale membership need no cleanup.
4. **Isolation:** held replies, full result/telemetry storage, partial updates,
   command floods and a stopped service worker cannot stall runnable audio.
   Bound control arbitration, transfer size and work admitted between quanta.
   Preserve existing legitimate media-capacity exhaustion and fault/silence rules.
5. **Visibility:** a control batch has a specified atomic boundary and admission
   point. Incomplete batches leave committed state usable. FPGA computes actual
   effective frames and reports late/refused work; ARM predicts no pulse time.
   Concurrent observations are coherent and frame/epoch stamped. Accepted query,
   capture/export and ownership results cannot be silently overwritten.
6. **Callback separation:** the ARM audio/deadline path performs no command
   compilation, membership construction, completion polling, synchronous MMIO
   waits, settlement rendezvous or media housekeeping. It uses bounded prepared
   transport where required. Required capture allocation/pinning/reference work
   remains device-side. Slow ARM observation delivery changes delivery time only.
7. **Resource accounting and timing:** publish matched LUT/FF/BRAM/storage/cycle/
   transfer costs throughout, including final integration and timing work. Seek
   clear-cut LUT savings; #993's 13,312 and M1's 11,320 are comparison references,
   not acceptance thresholds. No decrease or saving percentage is required.
   Any claimed saving must be distinguishable from observed run variation.
   The design must fit the selected device, preserve standalone TP timing and
   achieve nonnegative setup/hold for the complete core and selected board
   deployment. Do not lower clocks/capacities or hide paths to pass.
8. **Physical qualification:** a real ARM runs the shared service against the
   final FPGA and physical audio path on the selected board. Verify output,
   timing, reset, cache/DMA coherence and service/memory interference under the
   frozen workload envelope. Simulated clocks, QEMU, a successful bitstream or
   an unverified digest cannot satisfy this criterion.
9. **One maintained implementation:** all transports/producers use the shared
   Rust owner and final versioned contract. Remove obsolete production fence/
   settle protocols and experimental representation selectors after migration.
   Diagnostic stepping and archived evidence must not become hidden prerequisites
   or a second resolver. Public functionality is not reduced to achieve area.

These criteria are completion gates. Inability to meet correctness, safety,
device capacity, deadline or hardware requirements requires a recorded blocker/redesign,
not a checked completion box. Absence of LUT savings alone is not a blocker.

## Design rules and boundary decisions

- Treat the shared ARM producer as trusted for global construction properties.
  Inventory device checks individually: required safety/identity/atomicity checks,
  trusted-producer validation, or redundant state. Prove a lifetime/bound before
  moving or deleting a check. Malformed raw input must remain bounded and unable
  to access out of range, corrupt live storage or alias a reused identity.
- Attribute LUT cost using hierarchy, mux/register/RAM inference, critical paths
  and controlled variants. Separate masks/bank selection/guards from resolver
  datapaths and FSM encoding/mapping. Sequential old work can cost cycles without
  much logic; fewer RTL states alone is not evidence of lower area.
- Keep staging off live execution state. Consider simpler ordered writes, RAM
  validity/version representations and less duplicated metadata when attribution
  warrants them. Use the current linked layout as the control; reopen the layout
  choice only with evidence. Retain one final representation and checked versions.
- Specify atomic action/membership publication and multi-control batch visibility
  together. A synchronous ARM exchange may involve bounded transfers; audio never
  waits for the next transfer or acknowledgement consumer. Do not remove a fence
  wire while retaining its dependency elsewhere in the driver or scheduler.
- Define early/on-time/late publication, cancellation ordering, batching order,
  producer restart and reset. Preserve valid public delivery semantics; do not
  turn ordinary ARM delay into a device-wide fault or apply a partial batch.
  Distinguish public queue acceptance from device admission. Every supported
  future control that requires an exact execution cut, including signal/parameter
  changes, must be resident or have an explicit admission deadline; it cannot
  rely on ARM waking at that future frame. Preserve public completion semantics.
- Separate optional latest telemetry from accepted queries/results requiring
  retention. Reserve bounded capacity before acceptance; backpressure new service
  work without backpressuring audio. Optional loss needs a detectable policy.
- Audit every device operation currently issued by `loop_media::Media::observe`.
  Capture cuts, old-content retention, retakes and required reference changes must
  occur at the correct device frame. Moving their calls to a later ARM thread is
  insufficient. Bound retained snapshots/pins and preserve exhaustion behavior.
- Audit `snapshot_parameters` and supported signal-program updates as well:
  compilation can move to ARM service, while any required continuity of mutable
  DSP/ramp state must be established at the device's actual activation cut.
  Publishing an old ARM snapshot back over advancing state is unsafe.
- Keep a single owner for each Marlin model/context. Independent service/audio
  progression may use the existing clock scheduler and queues; do not introduce
  concurrent unsafe model calls or count faster host simulation as ARM evidence.
- Reuse Zynq boot/BSP/service infrastructure, with an explicit PS/PL memory map,
  cache maintenance, ordering and ownership protocol. The benchmark's DDR arena
  cannot be assumed safe when live ARM firmware also occupies DDR.

## Ordered implementation stages

Begin with a focused stage-1 review. Stage-2 implementation is conditional on
finding worthwhile changes; retaining the current design with recorded evidence
is an acceptable outcome. LUT savings are not a prerequisite for M4. M4 gates M5;
coherent device observations and media cuts gate application migration.
Timing/resource checks recur at each architectural checkpoint; final hardware
qualification follows functional and timing closure.

### 0. Establish the stack, baselines and qualification contract

- [x] Fetch #993 and create this planning branch at the recorded head.
- [x] At execution, refresh the parent and preserve the stack; freeze current
  RTL/service/driver sources, profiles and evidence. Reuse valid M1/M2/#993
  measurements, and rerun only invalidated/missing comparisons.
- [x] Inventory control, observation and media service calls in every production,
  mono, application, prepared and board path; classify their actual deadline role.
- [ ] Freeze a profile matrix: supported kernel quanta; TP 64/272/400; existing
  complete-core 64/128/256; 8×8/64-loop workloads; continuous 16-channel tests;
  and a physical reference profile. Use the existing ZedBoard as the initial
  hardware reference, recording exact silicon/codec/pins/toolchain and supported
  physical channel count. Keep the Zybo-package OOC experiment separate.
- [ ] Set concrete sample rate, latency, admission lead time, service budgets,
  memory contention envelope, run lengths and required deadline margin before
  implementation. Start from Q32/48 kHz/100 MHz; derive deadlines from actual
  ingress/publication edges, including CDC/DDR costs. Record physical sample
  formats and codec quantization separately from internal Q8.24 behavior.

Verification/gate: reproducible inputs and observable frame/event/sample/media
results; each eventual acceptance claim has a test or physical measurement.
Hardware access is not established by this planning session. Identify the board,
debug access and audio fixture early; keep independent work moving if absent.

### 1. Investigate TP LUT costs and assess clear-cut optimizations

Start with existing reports and a small set of attribution experiments. Expand
the investigation only when the evidence exposes a promising opportunity.

- [x] Reuse #993's mapped result after verifying frozen RTL and harness hashes
  (no invalidated input required a new baseline run). Build a field/check/
  state ownership and resource ledger, including both banks, masks, read muxes,
  version/generation comparisons, writeback and resolver datapaths.
- [x] Use controlled variants to distinguish actual logic cost from encoding,
  fanout and synthesis mapping. Do not infer the cause solely from source size
  or the 13,312 versus 11,320 difference. Disabled guards are diagnostic only.
- [x] Prototype only alternatives supported by attribution on the real resolver.
  Compare ARM construction, full transfer/ack cost, stale visits, storage,
  LUT/FF/BRAM and timing. Include dense/shared-pool, empty, one-loop, mixed-class,
  full terminal, reused-slot, 8×8 and 64-chain cases.
- [x] Decide which changes, if any, justify their complexity and risk. Retain the
  current protocol if no worthwhile candidate emerges. Publish the evidence and,
  for proposed changes, the check/lifetime proof and visibility/admission cuts.
  Account for later autonomy/observation needs and their resource costs.

Verification/gate: owner tests and small independent RTL comparisons for candidates
actually pursued; same-geometry synthesis, then route promising candidates.
Record the decision and its evidence, including an unchanged-design outcome.
Keep the investigation focused; lack of savings does not delay M4. Do not carry
an unsafe shortcut or make an unsupported causal claim.

### 2. Apply worthwhile simplifications and record the resource checkpoint

Implement the following changes only if stage 1 identifies a worthwhile candidate.
Otherwise record the decision to retain the current design and reuse its valid
baseline evidence; no protocol rewrite or ABI cutover is required for this stage.
Mark conditional implementation/verification items not applicable with that decision.

Execution decision: retain #993. The encoding-only candidate increased synthesized
LUTs by 121; no production protocol or RTL changes are selected. Frozen baseline
and candidate evidence are linked in the execution report. The first three
conditional implementation items below and candidate-only verification are
**not applicable**. Existing #993 costs and correctness/timing evidence remain the
resource checkpoint. No M4–M6 acceptance claim follows from this decision.

- [x] (N/A: retain #993) Implement the chosen owner/publication contract, with bounded staging,
  generation-safe reuse, checked versions and definite/uncertain failure handling.
  Remove redundant masks/checks/state only where stage 1 supplies the proof.
- [x] (N/A: retain #993) Migrate every transport, guard, prepared stream and generated artifact in
  one ABI cutover. Preserve full numeric ranges, acceptance/error precedence,
  original-command reconstruction and independently runnable execution.
- [x] (N/A: retain #993) Remove the discarded prototypes and obsolete production paths. Test partial
  writes, stale tokens, cancellation/terminal/collection races, held replies,
  counter exhaustion and coordinated reset at each transfer phase.
- [x] (retained #993 checkpoint) Publish a working checkpoint against M1/M2/#993: whole admission elapsed
  clocks/bytes/ARM preparation, PROCESS/FINISH, stale traversal, total storage,
  routed TP resources and complete-core timing impact.

Verification/gate for implemented changes: focused owner/ABI tests; all five TP
capacities with at least 20 seeds; exact production/app replay and 1,024-quanta
service-pause variants; lint/generic mapping and matched TP route. Preserve TP
timing and report all resource tradeoffs. Repeat paired physical runs only when
a claimed marginal saving needs a variation check. Record the changed or retained
design before changing cadence or observation machinery; LUT reduction is not a gate.

### 3. Make fabric progression independent of per-quantum fences (M4)

- [ ] Specify and implement bounded control access around autonomous PROCESS/
  state/job/audio/FINISH progression. Input availability and device resources
  drive execution. An empty, stalled or unfinished control stream cannot stop it.
- [ ] Publish complete batches at explicit cuts, preserving within-batch ordering
  and authoritative effective-frame reporting. Bound the largest non-preemptible
  command and total control work per quantum, including configuration/media traffic.
  Include supported processor/parameter updates in this audit, not just TP actions.
- [ ] Remove normal fence production/consumption and stale-sequence assumptions
  from audio control, coordinator, drivers, prepared ROMs, demo and board runner.
  Version incompatible inputs; keep diagnostic stepping explicitly separate.
- [ ] Preserve input/output bank ownership, fixed latency, prefill, overrun,
  watchdog, sticky fault/silence and quiescent reset/drain behavior.

Verification/gate: continuous I/O with no control producer for 1,024+ quanta,
held replies, full credits, incomplete publication, command floods and adversarial
arrival at every boundary. Check exact samples/state/events/results against Rust,
deadline counters, bus stability and fault recovery in instant/fixed/DRAMsim3
memory. Instrument the harness to reject hidden fences/settlement/service calls.

### 4. Make observations and media cuts autonomous (M5 prerequisite)

- [ ] Define coherent frame/epoch/version-stamped observations for state, outcomes,
  capture starts, content changes and query barriers. Define bounded ownership,
  reserved result capacity and how ARM resumes after missed optional telemetry.
- [ ] Implement device-side capture/retake/reference/pin work currently relying
  on ARM inspection between settlement and sample execution. Keep every required
  continuation inside the device and account for its worst-case resources/work.
- [ ] Retain exact data for accepted queries/exports and preserve receipt order.
  Delayed ARM delivery must not substitute a later loop state, revision or media
  view for the accepted operation's actual cut. Release only on validated ack.
- [ ] Remove normal host SETTLE/opcode-16 rendezvous and dependent mutation/read
  assumptions after replacement observations work. Preserve one coordinated
  reset epoch across in-flight actions, snapshots, replies and media ownership.

Verification/gate: stop observation/collection/export service for 1,024+ quanta
while recording, playing, wrapping, retaking and replacing content. Resume exact
historical queries/results/media; fill all observation credits; hold/reorder
transport delivery where allowed; test reset and faults at cut/ack/reuse races.
Extend the near-capacity two-take test; do not replace it with short recordings.

### 5. Move application command/response service off the deadline path (M5)

- [ ] Split admission/construction, transport, response reconstruction, queries,
  waveform/export work and cleanup from audio/deadline execution. Reuse public
  queues, scheduler identities, receipts and the shared Rust owner.
- [ ] Replace `Adapter::process` / `Core::quantum` service/settlement coupling and
  worker-driven command dependencies with bounded handoff and asynchronous device
  observations. Audit mono and application paths, graph/parameter updates,
  monitoring, clear, stop replacement, read budgets and media retirement.
- [ ] Keep queue/exhaustion and fault/reset behavior explicit. The service worker
  may lag or stop; it cannot hold a device/model mutex needed by the deadline
  path or require that path to poll to complete a previously admitted action.
- [ ] Migrate application/simulator transports and benchmarks coherently. Add
  instrumentation that detects allocations, waits and service work in the
  deadline path, and timestamps effects separately from observation delivery.

Verification/gate: exact application/mono comparisons, all current controls,
queries/exports, variable callback sizes and startup/recovery. Pause the real
service worker while the independent audio driver continues. Assert bounded
handoff/ownership and no mandatory callbacks into command service. Native model
wall time is diagnostic; ARM scheduling bounds require stage 7 measurements.

### 6. Integrate real ARM/audio transport and close timing (M6 preparation)

- [ ] Implement the physical ARM control/status transport and firmware driver
  for the shared service (MMIO/mailbox or equivalent chosen from measured needs).
  Specify register/descriptor layout, ABI/epoch handshake, ordering, backpressure,
  reset/drain, cache maintenance and sample/media arena ownership. Bound bursts
  and shared-memory traffic; host polling cannot gate PL cadence.
- [ ] Reuse existing Zynq boot/BSP/application infrastructure and PS7 initialization.
  Add the real FPGA/audio backend and required board codec/serial-audio/clock/CDC
  path. Wire actual application queues to the same service; generated-workload
  JTAG runs and dummy-audio firmware alone do not satisfy integration.
- [ ] Close 100 MHz setup/hold for the full core, then full board with ARM bridge,
  I/O, clocks and reset logic included. Profile the known sample-memory DMA paths;
  pipeline/simplify with handshake and deadline proofs. Include added cycles and
  final area in the checkpoint comparisons.
- [ ] Build reproducible firmware/bitstream/deployment manifests. Check all clock
  domains, CDC and valid constraints, routing, DRC and reset behavior. Keep debug
  overhead and historical clg400 experiments separate from the selected board.

Verification/gate: targeted protocol/cache/DMA/CDC tests and equivalent simulation
of the actual transport/I/O wrapper; full-core and board routes have nonnegative
setup/hold and no unexplained paths or critical DRC defects. Report the resource
effect of timing work, including changes to any earlier savings. Do not use
false/multicycle paths, lower fabric clocks, smaller pools or a reduced workload
to conceal a timing miss.

### 7. Qualify ARM independence, interference and physical audio (M6)

- [ ] Load the exact final bitstream/firmware on the recorded board. Verify PS DDR,
  ARM/PL ownership, cache coherency, actual sample clocks, analog/digital routing,
  latency and reset. Capture expected and actual PCM; replace the old digest-only
  benchmark's correctness gap with attributable sample/result/media evidence.
- [ ] With resident future/tick actions, pause command, membership, observation
  and completion service for 1,024+ quanta while physical audio continues.
  Resume and verify identities, frames, state and retained media. Keep unrelated
  platform watchdog support explicit rather than hiding audio service inside it.
- [ ] Measure release-build ARM service CPU/time/memory, callback/handoff worst
  work and complete audio processing/publication times separately. Exercise idle,
  sparse, dense, stale, full-credit and retained-media workloads, 8×8/64 chains,
  command floods, cache/DMA/DDR contention and concurrent application activity.
- [ ] Demonstrate the stage-0 deadline/latency envelope and margin, zero unintended
  underruns/overruns, long-run stability, correct explicit overload behavior,
  fault-to-silence and coordinated recovery. Record worst cases, not just averages.

Verification/gate: source/tool/board/silicon/clock/profile hashes, raw timings and
sample/media/outcome captures are reproducible and satisfy criteria 1–8. QEMU and
DRAMsim3 supplement this evidence. If physical access or instrumentation is
missing, finish independent work, state the exact dependency and leave physical
qualification and the overall roadmap incomplete until evidence exists.

### 8. Final end-to-end validation and handoff

- [ ] Run the full native suite once at the final behavioral milestone, plus the
  complete relevant Marlin/component/simulator/application/continuous/demo/board
  matrix. Include stalls, faults, all memory models, long sessions, alternate
  geometries, 64-loop loads, near-capacity retained media and independent float
  bounds. Reuse one combined feature build or serialize workspace builds with
  native tests so host executable aliases remain consistent.
- [ ] Check generated wrappers, prepared ROM parity, coverage, dependency boundaries,
  formatting, warning-denying builds and test-attribute policy. Investigate each
  failure; rerun affected cases after repair and broaden only when justified.
  Preserve initial failures, interrupted runs and skips in the evidence.
- [ ] Publish matched M1/M2/#993/resource-checkpoint/final measurements with identical
  capacities, parts and constraints within each comparison. Include whole public
  command/observation traffic, host preparation, stale work, all storage/banks,
  routed area/timing and physical deadline evidence. Recheck final area after
  timing replication and board integration, stating which savings persist and
  which costs increase. Final LUT reduction is not required for completion.
- [ ] Update contracts, adapter/testbench/board docs, this plan and the long-term
  roadmap against an explicit acceptance-evidence map. Mark M4/M5/M6 complete
  only when their gates pass, with scope and physical limitations stated.
- [ ] Commit coherent milestones and push this stack's branch. Reuse this PR for
  implementation or create explicit checkpoint children; rewrite PR scope as it
  changes. While #993 is open, retain its branch as the base. After parent merges,
  rebase/retarget onto the integration result without duplicating parent changes.
- [ ] Let configured CI run normally. Do not inspect, poll or rerun CI after
  pushing unless the user changes that instruction. Inspect available automated
  review feedback, address actionable items with affected local verification and
  repeat for new feedback. No submitted review means no approval; do not wait
  indefinitely for an absent reviewer or merge any PR autonomously.

Gate: every immutable criterion links to actual evidence. Publish software-only
progress honestly if hardware is blocked; do not call that the finished roadmap.

## Verification cost and representative entry points

Use the selected project environment. Begin each change with owner/transport
doubles and one small real-RTL case; mocks never resolve musical decisions. Broaden
to all capacities and directed workloads at ABI/cadence/ownership cutovers. Route
only measured viable area candidates, then each architectural checkpoint. Run the
expensive full native/application/media sweep at the final behavioral milestone.
Repeat narrower tests for fixes; repeat broad checks only for changed assumptions,
failures or unresolved concerns. Hardware gates have their own staged smoke,
correctness, stress and endurance runs; their duration is fixed in stage 0.

Adapt the existing entry points to the new contracts without dropping coverage:

```sh
cargo test --locked -p rt_fpga_adapter -p rt_backend_testbench
cargo run --locked -p rt_fpga_testbench --features sim -- transition --config small --random-cases 2
cargo run --locked -p rt_fpga_testbench --features sim -- transition --config all --random-cases 20
cargo test --locked -p rt_fpga_sim --features sim --test transition_boundary -- --test-threads=1
cargo run --locked -p rt_fpga_testbench --features sim -- production --directed --random-cases 30 --stalls
cargo test --locked -p rt_fpga_testbench --features sim
cargo test --locked -p shoop_rt_host_fpga_sim --features sim --tests -- --test-threads=1
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --locked --workspace \
  --features shoop_engine/app_backend,shoop_backend/native-fx,rt_fpga_testbench/sim,shoop_rt_host_fpga_sim/sim \
  --profile ci --test-threads 2
```

Use `measure_commands.py`, `measure.py --context shared`, `fpga/synthesis/run.py`,
the generated/coverage/float checks and the board build/load/control frontends
documented in their READMEs. Preserve Vivado 2021.1 and matched harness hashes for
historical comparisons; pin a supported board toolchain and rebuild both sides
if a tool/harness change invalidates a comparison. Reuse the documented public
Vivado payload/catalog/runtime instead of assuming a fresh full installation.

Before Rust commits run `cargo fmt --all` and a workspace build with
`RUSTFLAGS="-D warnings"`, enabling simulator/application features for changed
feature-gated code. Changed Rust tests require
`python3 scripts/check_shoop_test_usage.py`. Board qualification must build and
execute the ARM target, beyond host-only compilation. No CI or review pass may
be inferred from publishing a branch.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
