# FPGA semantic command FIFO: investigation and implementation plan

## Status and execution contract

**Full execution authorized by the user's explicit Stage 2 GO.** Stages 0–3 are
complete, including the reduced-profile/ABI contract freeze. Stage 4 is in progress.
See [the investigation report](docs/fpga/command-fifo-investigation/README.md)
and [baseline](docs/fpga/command-fifo-investigation/baseline.md).
Branch: `investigate/fpga-command-fifo`, production base `ff6c09551e9dee2cd3a12d37ccad19fa844b09aa`.
Physical testing still requires the separate authorization and protections in Stage 6.

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
- Do not interpret silence, successful prototype tests, or an open PR as a go.

## Goals and scope

Restore the intended CPU/FPGA boundary: ARM publishes complete **semantic command
groups**; FPGA consumes them against execution-owned state at processing
boundaries. Rendering never waits for another ARM state read, patch, release,
response acknowledgement, or snapshot read.

Remove the **FPGA-oriented composite live-switching implementation**, rather than
porting its complexity into the new controller. Regress that feature deliberately
and document its refusal behavior. Reconsider it separately after the basic
communication model is established.

Retain:

- Immediate and two-step loop transitions, clear-plans, gain, repeat-sync, and
  static-composite iteration/seek and transport semantics.
- Existing SP-owned pulse/countdown/scheduled execution and static composite
  schedules. This is not another full-loop scheduler or revival of the retired TP.
- Initial program installation, ordinary primitive-only live program replacement,
  persistent-state ownership, and truthful memory-fenced handoff completions.
- ARM compilation/allocation, logical-to-physical binding, parameter encoding,
  inactive-program uploads, and initialization of unused allocations only.
- Existing fault diagnostics, safe recovery, watchdog and thermal protections.

For graphs involving composites, support initial installation and replacement
only after explicit quiescence and ownership fencing. Conservatively refuse live
replacement when either active or candidate graph contains composites, including
edits to their children. Do not silently stop playback to make an edit succeed.
An alternative reduced capability boundary requires user approval at the gate.
Native CPU/browser composite functionality and their regression coverage are not
part of this removal. Do not expand the FPGA profile's other feature support.

## Immutable acceptance criteria

1. A complete accepted group reaches an FPGA-owned semantic consumer. ARM does
   not read mutable execution state to resolve it or upload replacement state.
2. Partial/unpublished groups are invisible. Groups execute in FIFO order, each
   wholly at one processing boundary with no SP sync pass between commands.
   Preserve header refusal and per-command result semantics; do not silently add
   whole-group rollback or split groups across boundaries.
3. Stalled ARM command delivery, reply/snapshot readers, interrupts, or inactive
   uploads do not stall or fault otherwise healthy rendering. Reliable completion
   capacity is reserved before command acceptance; exhaustion limits admission.
   Snapshot/optional trace loss is observable but does not backpressure execution.
4. Supported single, mixed and repeated-target groups agree with the Rust semantic
   oracle, including 16/32-loop synchronized arming, skip 0/1, two plan slots,
   invalid targets/values, self-sync, capture finalization and counter overflow.
5. Composite live-switch code is removed from FPGA execution, preparation and
   adapters, not retained behind a disabled capability. Static composite execution
   and ordinary primitive-only switching remain correct. Unsupported edits refuse
   without active-state mutation, replay, or ambiguous ownership.
6. Runtime incarnation, program revision, target generation and publication
   sequence fencing prevent stale traffic or torn snapshots. Program selection
   never substitutes for the JP/cache/DMA ownership-handoff fence.
7. No running-audio CPU-driven control window or state-patch fallback remains in
   the FPGA host path. Loader-only writes cannot block scheduled processing.
8. Hardware command service, arbitration and publication have explicit finite
   bounds. Resource usage and total quantum timing meet the numerical ceilings
   approved at Stage 2; a smaller FSM state count alone is not evidence of success.
9. Native/browser behavior outside the explicitly reduced FPGA profile is
   preserved. Matching software/RTL capabilities, documentation and ABI rejection
   make the reduced support truthful end to end.

## Design rules

- Start with one shared sequential edit kernel, indexed RAM access and shared
  arithmetic. Reuse it across commands/loops; do not replicate controllers or
  full-record shadow banks per loop or per command. Validate each command before
  its live writes; uncertain mid-write hardware failures fail closed.
- Do not synthesize a large combinational translation of Rust structs, duplicate
  the SP scheduler, add graph interpretation, or introduce a soft CPU by default.
  Evaluate a small ROM-sequenced alternative only if measurements justify it.
- Physical address binding and state-independent conversion may remain on ARM;
  effective-frame selection, live-state checks and transition resolution may not.
- Use complete-group queues in both directions for commands/results and a separate
  bounded complete-snapshot stream. Reserve reliable replies independently of
  telemetry. Keep a fixed publication cutoff; specify any whole-group deferral
  policy explicitly and align the reference, rather than accidentally changing it.
- Consume execution-relevant sync events locally. Replace the current dependency
  on ARM draining the fault-on-overflow event FIFO; optional traces may be lossy.
- Preserve the trust/ownership model in `LOADING_DESIGN.md`: no FPGA allocator,
  graph registry, continuing-state initialization or snapshot-based restoration.
- Keep snapshot frame/revision and capture phase coherent with the reference.
  ARM may format copied observations but must not reconstruct execution decisions.
- Use the repository development environment and tool pins. Reuse existing Vivado
  2021.1 tooling; target the measured Z-turn part/profile, not an unrelated board.
- No new authentication mechanisms, automatic workload replay, or unrelated
  cleanup. Keep credentials, raw board logs and generated private artifacts out
  of public evidence. Preserve historical qualification records as historical.

## Ordered stages

### Stage 0 — Establish the baseline and investigation branch

- [x] When execution is authorized, verify the worktree/base and create a dedicated
  investigation branch from the then-current backend branch; record its SHA.
- [x] Record instruction/tool provenance, current passing tests, resource reports,
  queue/profile capacities and the existing CPU-dependent paths. Distinguish
  historical routed measurements from new same-configuration comparisons.
- [x] Inventory removal dependencies in compiler/loading, Rust reference, ARM
  adapter, RTL wrappers/SP, ABI, fixtures, capabilities and docs. Read the native
  feature-parity ledger before changing or retiring coverage.

Verification: reproducible baseline and dependency map; no production behavior or
board state changed. Depends on authorization to begin work.

### Stage 1 — Investigate semantic complexity and actual FPGA usage

- [x] Trace `owner::command_lane/apply/edit_loop`, `ControlWindow`, the batch path,
  `host/control.rs`, and `state/sync.rs`. Separate transport/binding overhead from
  required state edits and removed composite live-switch responsibilities.
- [x] List exact state fields, branches, loop bounds, arithmetic widths and refusal
  conditions for every retained command. Identify overlap with SP operations and
  the cost/risk of sharing versus a separate bounded kernel.
- [x] Build an **isolated offline prototype**, not a production frontend: one local
  state-edit kernel covering retained commands, with a differential Rust harness.
  Include primitive/composite transport, iteration multiplication/overflow, two
  slots, repeated targets and refusal-without-mutation cases.
- [x] Synthesize the prototype with realistic RAM interfaces and constraints.
  Report LUT/FF/BRAM/DSP, storage organization, inferred arithmetic, critical
  paths, measured worst-case cycles and scaling with command/loop capacity.
  Check that synthesis has not optimized away the exercised state/result ports.
- [x] Account separately for FIFO/reply credits, coherent snapshots, arbitration
  and loader/fence integration. Measure where practical; label estimates and
  synthesis-only timing honestly. Compare replacement cost **net of retired
  patch/snapshot/mailbox machinery**, with no unmeasured savings presented as fact.
- [x] Produce a concise investigation report linked here: proposed reduced
  capability matrix, candidate ABI/queue layout, group-service policy, total
  quantum budget, numerical resource/latency ceilings, uncertainties and a
  recommendation. Include the cost of removing composite live switching.

Verification: deterministic differential tests pass; synthesis/cycle results and
reproduction commands are recorded. This stage may contain prototype/test changes
but must not replace the production path, remove its features or load the board.

### Stage 2 — Mandatory user go/no-go (hard stop)

- [x] Present the investigation report, measured costs versus estimates, remaining
  integration risks, reduced composite capabilities and proposed numeric ceilings.
- [x] Request the user's go/no-go on the report's proposed scope, policy and ceilings.
- [x] Record the user's decision, report/source SHA, approved resource/timing
  ceilings, capacity/service policy and capability scope. No approval is inferred.
- [x] **Stop here until an explicit go is received.** On no-go, leave production
  behavior untouched, retain the evidence and list alternatives without proceeding.
  If a redesign is requested, repeat the relevant investigation before another gate.

Decision: **GO — explicitly supplied by the user:** “the plan in FPGA_CMD_FIFO.md
is fully executed. I give the GO for all stages herein.” Confirmed goal:
`mwwpv7oy-mi4ch5`.

Approval binds to investigation commit `da2df1eeef556682adf1acb1a2c64fcd50da9996`
and `results.json` SHA256 `a04445254a6c013d2e248300cbf83313f4ceb17753d17da57ac5d1b22e4dca6b`.
Approved scope: the report's reduced FPGA composite profile, retained static
composites/primitive switching, four complete banks of up to 32 commands,
one complete group per quantum from a fixed publication cutoff, reserved replies
and independently nonblocking snapshots. Native/browser support remains unchanged.

Approved ceilings (report §6): kernel 2,500 LUT/2,500 FF/no DSP; replacement
frontend 6,000 LUT/7,000 FF excluding existing execution/storage; whole design
32,000 LUT/24,000 FF/100 RAMB36-equivalents/at most the existing four DSPs;
command phase 4,096 cycles, snapshot phase 8,192 cycles for at most 128 rows,
total admitted quantum 60,000 cycles at 100 MHz, Q=32/48 kHz; nonnegative routed
setup/hold slack. Any breach returns to the user rather than weakening limits.
This GO does not waive Stage 6's separate physical-test authorization or Stage 7's
no-autonomous-merge rule.

Verification: an explicit recorded user decision. All subsequent stages depend on
that go; any later ceiling breach requires returning to the user, not silently
increasing limits or reducing acceptance criteria.

### Stage 3 — Remove composite live switching and freeze the contract

- [x] Delete FPGA-oriented composite cut/ancestor readiness, conditional preview,
  paired-restart/ending handoff and retry-at-future-boundary machinery across its
  actual dependencies. Preserve static-composite runtime behavior and primitive
  switching; do not indiscriminately delete similarly named SP wrap logic.
- [x] Add early capability/admission refusals for excluded graph replacements,
  including child edits and composite introduction/removal. Preserve state and
  allocation ownership on refusal; quiesced replacement remains explicit.
- [x] Replace obsolete live-switch acceptance tests with reduced-profile refusal
  tests while retaining native/browser and static-composite behavioral evidence.
- [x] Specify the versioned hardware FIFO ABI, complete publication, physical
  targets, header/deadline checks, result codes, reply reservation, reset fencing,
  snapshot capture phase and bounded service policy. Update the loading and
  architecture docs to distinguish retained switching from the removed feature.

Verification: focused compiler/reference/RTL composite and loading tests pass;
unsupported live edits are rejected through public admission and execution paths.
No dormant composite live-switch fallback remains.

Rust/profile milestone: compiler cut/ancestor/restart planning and the dependent
reference, ARM and Marlin guarded-switch/retry paths have been removed. Admission
refuses active/candidate-composite replacements before allocation; reference
execution, hardware sessions and the Marlin adapter add independent guards.
Public tests cover introduction/removal, child/program/bus edits, unchanged
recorded playback, stopped-but-not-quiesced refusal, retained static controls,
nested monitoring, shared one-shot re-entry and primitive-only live switching.
Retirement provenance is recorded in the native feature-parity ledger.

Verification: all five affected packages pass with simulation/RT-interface/RTL
features enabled: **165 tests**, including the **3,970-case kernel differential**.
The full Linux suite passes with nextest 0.9.116: **3,583 passed / 11 skipped**.
Default application/worklet Wasm builds and the optional worklet `fpga-reference`
build pass. Warning-denying workspace build, formatting, test-policy and host
register/top generator checks pass. An initial broader test exposed a test
harness dropping an early switch completion while waiting for snapshot metadata;
the harness now retains that response and asserts both correlated completions.
An initial Linux run using nextest 0.9.143 timed out in a legacy native dummy
test; 20 isolated reruns and the complete documented-runner rerun pass. The first
run remains interrupted evidence, not a pass or proof of the timeout's cause.
See [the milestone record](docs/fpga/command-fifo-implementation/README.md).
Local logs: `work/cmd-fifo-investigation/stage3-{rust-milestone,linux,wasm}.log`.

RTL-removal milestone: SP/pipeline preview, paired-source, ending-handoff and
future-switch controllers and their wrapper ports are now removed, along with
outgoing-span types, counts and packed metadata. Ordinary static wrap/pulse,
iteration/seek and scheduling remain. Former switch-guard fields now refuse as
reserved payload bits rather than silently selecting an immediate switch; 63
single-bit cases exercise that behavior. Empty child schedules cannot encode
retirement rows, and obsolete SP header flags are invalid. Scoped source audit
finds no remaining retired implementation names in production RTL/Rust/adapters.

Fresh verification: **166 affected-package tests** (including the 3,970-case
kernel differential), full Linux **3,584 passed / 11 skipped**, both default Wasm
builds and the optional worklet `fpga-reference` Wasm build pass. Warning-denying
workspace build, formatting, test-policy and AXI/host generator checks pass.
Local logs: `work/cmd-fifo-investigation/stage3-rtl-removal.log` and
`work/cmd-fifo-investigation/rtl-milestone/stage3-{linux,wasm}.log`.

**Stage 3 complete — contract freeze:** [hardware ABI 16](fpga/host_bridge/semantic_fifo.md)
specifies complete sequential publication, four paired input/result banks,
32-command groups, epoch/sequence-qualified acknowledgements, physical binding,
header/per-command results, FIFO reliable-result visibility, real switch fencing
and independent settled snapshots. Small local gain/failure/pulse observations
preserve metadata without ARM event/result draining. The semantic target emits a
complete sync prefix before audio; standalone component interleaving is separate.
Capability descriptions, explicit ownership lifecycle, architecture/loading docs,
manual/opcodes and crate usage now distinguish the reduced profile and current
ABI-15 workaround from the pending ABI-16 implementation.

The documented source-change refinement preserves existing plan invalidation
with an execution-owned SP operand comparison. Expanding 128 source changes into
measured 65-cycle kernel clears would exceed both 32 commands and 4,096 cycles.
This changes implementation ordering, not scope, group semantics or ceilings.
Implementation/bounds still need Stages 4–6 verification.

Fresh contract gates pass **166 affected-package tests plus 12 directed component
tests**, including the 3,970-case kernel differential; warning-denying workspace
build, formatting, test-policy, coverage, generators, local Markdown links and
retired-name audit pass. Local evidence:
`work/cmd-fifo-investigation/stage3-contract-verified.{log,status}`.

Broader component validation reproduced six assertion/configuration failures at
production base `ff6c09551`. Repairs distinguish executable visits from schedule
rows, retain exact job payloads and dependencies/media-access ordering, isolate
complete media bindings and streaming-versus-batch queue limits, and pin a
geometry-specific DRAM test to its intended x16 fixture. Other component DRAM
runs use the Z-turn x32 configuration. Negative tests guard the job normalization.
The 1,200 s aggregate run is interrupted evidence, not a pass; completed targets
and bounded follow-ups cover **73 passing / 2 failing** component cases. The two
remaining standalone reference fault-path differences also reproduce at the
production base in an isolated target directory. They remain explicit Stage 6
validation work, not waived gates or regressions attributed to this removal.
The interrupted model cache was rebuilt before the passing pipeline follow-up.
Current runs explicitly select the current RTL root; see the milestone record
for failed/interrupted logs and source-root provenance.

The old CPU control window/batch bridge remains until Stages 4–5 replace it.
Stage 3 completion does not claim a production FIFO, fresh integrated resources,
timing/physical qualification, final-head CI or delivery.

### Stage 4 — Implement the FPGA-owned consumer and return streams

- [ ] Turn the validated edit kernel into the production local controller. Execute
  each complete group in order against actual boundary state, then release to SP
  autonomously. Preserve command-local refusal and duplicate-target ordering.
- [ ] Implement bounded ARM-to-FPGA packet storage with single complete publication
  and immutable published payloads. Expected full/stale/fault refusals retain safe
  MMIO receipt behavior, rather than generating architectural CPU aborts.
- [ ] Implement reserved reliable group-result and handoff completion capacity,
  independently bounded complete-snapshot publication and observable loss counts.
  No return-stream ready signal may make rendering wait for ARM.
- [ ] Move required event accumulation local; remove external event-drain overflow
  as an audio fault source. Keep actual pipeline faults and retained diagnostics.
- [ ] Arbitrate state/program ports and schedule the bounded command phase with
  audio priority. Partial AXI transactions, unread replies and pending loader
  writes cannot hold a processing boundary. Keep selection/fencing truthful.
- [ ] Verify runtime reset flushes unpublished/published packets, replies and
  snapshots without reusing old identities, while always-on recovery survives.

In progress — standalone component milestone: the measured kernel now has a
production source at `fpga/host_bridge/rtl/fifo_edit_kernel.sv`; its initial bytes
match the frozen investigation source. A shared differential harness runs both
sources independently: **2 Rust tests, each containing 3,970 cases**, with separate
cycle reports. Successful work remains 73/65/35/99 kernel cycles for transition,
clear/repeat, gain and iteration; this excludes frontend/group turnover.

`fifo_group_banks.sv` adds four paired input/result banks, sequential complete
publication, immutable admitted payloads, FIFO execution/result-release cursors,
reserved reliable capacity, qualified acknowledgements and synthetic fence/reset
identity handling. Its **4 standalone tests pass**, including all 400 words of a
32-command packet, partial strobes, refusals, full unread results, later execution
while an earlier fence waits, out-of-order result access/ACK refusal, epoch/sequence
ABA, same-edge publication eligibility and incomplete-result fail-closed behavior.
The result producer writes command codes followed by the header; complete result
visibility is checked separately from execution progress.

The initial **6-test component gate**, warning-denying workspace build, formatting,
test-policy and generators pass (`work/cmd-fifo-investigation/stage4-component-milestone`).
That checkpoint did not connect the units to a boundary consumer or the audio path.

**Standalone boundary-consumer milestone:** `fifo_group_consumer.sv` now connects
packet banks to the shared kernel, accepts one group per explicit processing
opportunity, latches the cutoff/actual attempt frame, preflights header switch
placement before any edits, preserves command-local refusal and repeated-target
ordering, and publishes reserved results without waiting for a reader. A selected
switch's final result word waits for a qualified synthetic fence while ordinary
groups and render handshakes continue. Gain observations retain the original
finite f32 bits. Read/write/fence errors fail closed, including a zero-latency
selection/fence-error race, rather than becoming ordinary semantic refusals.

Eight consumer tests pass: 36 mixed repeated-target oracle scenarios, all 400
partial packet-word positions with independently driven boundaries, cutoff and
actual-frame deadline/header priority, full/unread results, static-composite
switch refusal, pending-fence progress and reset identities, value-validation
priority, gain metadata, actual RAM faults and concurrent result/fence writes.
With one-cycle granted local RAM service, full boundary-to-render costs including
decode/turnover are **1,494 cycles for 16-target arming**, **2,950 for 32 targets**
(skip 0/1), **3,782 for 32 iteration/seek commands**, and **3,783** when an earlier
fence finalizer arbitrates with that maximal group, below the approved 4,096.
These are component render handshakes, not SP/JP/audio or DDR timing results.

The fresh five-package simulation/RT-interface matrix passes **179 Rust tests**,
including both 3,970-case kernel tests, four bank tests and eight consumer tests;
workspace warnings, formatting, test-policy, coverage and generators also pass.
Evidence: `work/cmd-fifo-investigation/stage4-consumer-{tests,races,milestone}` and
`production-{group,fence-race}-cycles.json`. An initial width-warning elaboration
failure is retained separately, not counted as a pass.

Initial partial synthesis at source `367a2b69d` measured 4,015 packed LUTs /
3,891 FFs and a **failing unrouted WNS −2.074 ns**, 743 setup endpoints, on a
24-level pending-group/completion/fault path. The [original reports](docs/fpga/command-fifo-implementation/reports/consumer-synthesis-367a2b69d/results.json)
remain unchanged. `vivado-consumer-v1` had stopped before synthesis on an incorrect
launcher pin from compacted notes; `v2` used the approved archive's actual pin.

**Result-arbitration timing milestone:** separating registered-state address/data
selection from live-fault validity gating shortens that estimated path to 14
levels / WNS +0.585 ns, without added registers, latency or weakened fault
suppression. The new ninth consumer test injects qualified and stale errors while
both the fence finalizer and ordinary result writer compete. The fresh affected
matrix passes **180 Rust tests**; warning-denying workspace build, formatting,
policy/coverage and generators pass (`stage4-timing-milestone`). All measured
boundary-to-render cycle costs remain unchanged, including the **3,783** maximum.

The same RTL (consumer SHA256 `2cd453c84440c35b225411d64c92390bcf0c56090ac80967450ddd535921dea7`)
now has an [OOC route](docs/fpga/command-fifo-implementation/reports/consumer-route-2cd453c8/results.json):
**4,016 packed LUTs / 3,891 FFs / 2 RAMB36 + 1 RAMB18 / no DSP**, kernel leaf
**1,822 LUTs / 1,945 FFs**, **WNS +0.178 ns / WHS +0.159 ns**, no failing
endpoints, failed/unrouted nets, critical warnings or errors. `vivado-consumer-v5`
uses the existing verified IP catalog read-only; `v4`'s catalog-setup critical
warning remains separately recorded. The expected OOC PS7-required DRC warning
remains. Unanchored ports/missing `HD.CLK_SRC` and absent remaining frontend/SP/JP
blocks mean this is **not** the protected whole-design gate, a 60,000-cycle quantum
proof or a guarantee that the remaining frontend fits. No limits change or board
work is implied.

**SP source-change milestone:** the production SP now compares the inline source
operand and validated source generation with the existing packed-record fields,
invalidates old-source **pulse** plans before scanning, and stores the new identity
through its ordinary writeback. Frame-absolute plans survive, matching the
prepared reference's `clear_mode_plan`; no new registry, RAM, loop scheduler,
SP phase or expanded command group is introduced. Two directed tests cover 18
source/slot/generation/plan-kind cases, unchanged and re-armed plans, plus all 128
slots changing from no source to self with simultaneous natural wraps. The latter
uses **9,991 simulated SP cycles**, not command/snapshot cycles or a complete
quantum bound. The initial broader run caught an incorrect all-plan clear that
lost a scheduled seek during live loading; pulse-only invalidation fixes it and
the failed run is retained, not labelled a baseline issue.

Fresh gates pass **180 affected-package plus 22 directed component Rust tests
(202 total)**, including static graphs, live loading/DDR fences, SP sync/state and
smoothing. Workspace warnings, formatting, policy/coverage and generators pass.
Evidence: `work/cmd-fifo-investigation/stage4-source-milestone.{log,status}`;
initial fixture failures and the absolute-plan regression remain separate logs.
No new resource/route, full Linux/Wasm or physical result is claimed.

**Internal SP RAM-port milestone:** the SP now exposes a separate internal local
loop/state RAM port with one-cycle responses and priority over new loader requests.
An in-flight DDR-program upload no longer owns this port. A directed test performs
512 accesses across local/DDR-program configurations, including full-width state,
loop words, invalid-address nonmutation, held loader requests, processing priority
and reset. Fresh gates pass **180 affected-package plus 23 directed component tests
(203 total)**, workspace warnings, formatting, policy/coverage and generators;
`work/cmd-fifo-investigation/stage4-local-ram-milestone.{log,status}` records them.
This is an internal interface, not ARM live-control access. The existing audio
wrapper ties it off pending connection; old SP admission/program-loader gating and
audio fence gating still require replacement. It does not prove audio progress
under a held upload, full frontend resources or an integrated cycle envelope.

**Dual-region SP admission milestone:** a pending program upload and its response
pulse no longer own SP run admission when two program regions are configured.
Single-region upload protection and bounded local-config response handling remain.
The new regression first failed at the old `run_ready` gate, then passes: three
explicit empty SP quanta advance frames 0→96 while the upload remains unaccepted;
upload completion preserves every payload word and admits the next quantum on its
response cycle. Single-region admission stays blocked until that response retires.
Uploads/polling alone do not advance frames. This isolates SP admission only, not
nonempty instruction fetch, audio samples or a total-quantum DDR bound.

Fresh gates pass **180 affected-package plus 24 directed component tests
(204 total)**, workspace warnings, formatting, policy/coverage and generators;
`work/cmd-fifo-investigation/stage4-admission-milestone.{log,status}` records them.
The before-fix failure is retained as `stage4-admission-before-fix`. Consumer/RAM
connection, audio/media-store/fence gating and the remaining Stage 4 work stay open.

Actual audio/SP/JP and JP/cache/DMA fencing, loader/RAM arbitration, MMIO ABI 16,
local SP observations and settled snapshots are **not yet connected**. Generated
hardware remains ABI 15. No integrated resource/total-quantum, physical, fresh
Linux/Wasm or delivery qualification is claimed; Stage 4 checklist items remain
open until their production integration passes.

Verification: RTL kernel/queue/arbitration tests and cycle assertions pass against
Stage 2 bounds, including stalls at every packet word and request/response phase.

### Stage 5 — Integrate ARM/reference transports and retire the old bridge

- [ ] Replace ARM semantic planning with state-independent binding/encoding and
  nonblocking queue pumping. Deliver FPGA results to the existing response API and
  copied observations to the existing snapshot API, preserving correlation/order.
- [ ] Align the Rust reference's group cutoff/service semantics and reduced
  composite capability profile; keep processing pacing independent of polling.
- [ ] Route initial installation and primitive-only switches through the ordered
  consumer and real memory fence. Large preparation uploads stay outside it.
- [ ] Remove running-audio `ControlWindow`, CPU state read/modify/write, staged-patch
  operations and associated ABI/fixture fallbacks once their replacements pass.
  Retain legitimate loader/diagnostic access without exposing a live-control path.
- [ ] Update generators, pairing/capability checks, ARM firmware, probes and docs.
  Reject mismatched bitstream/firmware pairs. Recheck watchdog progress accounting:
  host-health policy is separate from FIFO readiness and reader progress.

Verification: reference/RTL/host integration exercises the real new FIFO path;
code-path audit finds no CPU-dependent control fallback, including mixed/single
commands and program switching. ARM firmware and fixtures build successfully.

### Stage 6 — End-to-end validation and resource/timing acceptance

- [ ] Run continuous-audio differential tests through the full AXI/host path for
  single/mixed/repeated-target commands, 16/32-loop arms, self-sync, composite
  runtime controls, stale revisions, deadline misses, refusals and exhaustion.
- [ ] Freeze ARM at partial publication, after acceptance and during readback.
  Fill completion/snapshot/event streams; flood commands and inactive uploads.
  Prove accepted work completes and frames/samples progress without ARM service;
  finite capacities affect admission/publication only.
- [ ] Exercise primitive switches during playback/recording, delayed JP/cache/DMA
  fences, uncertain outcomes, disconnect and reset. Assert no early reclamation,
  stale replay, mixed-revision snapshot or unsupported composite live switch.
- [ ] Run warning-denying workspace builds, formatting, test-policy/generator
  checks, the full Linux suite, applicable Wasm builds, and Zynq platform recovery
  regressions. Rust tests use `#[shoop_test]`; run builds/tests through Nix locally.
- [ ] Resolve the two standalone `rt_fpga_testbench` reference fault-path
  differentials reproduced at `ff6c09551` (preflight queue refusal versus partial
  SP effects, and streamed jobs remaining after JP failure). Preserve meaningful
  fault/partial-effect/fail-closed coverage; do not hide them by dropping tests.
- [ ] Synthesize/route the matching full design and compare net resources, routed
  timing and worst-case total quantum cycles with the approved baseline/ceilings.
  Do not call kernel-only synthesis or network observation a system timing proof.
- [ ] Report offline results first. Perform any physical test **only with separate
  user authorization** and a protected matching pair: supervised temperature/age
  guard, watchdog, factory PS/DDR/clocks/files preserved, no destructive replay,
  full reset/factory-PL restoration before factory Linux. Distinguish intentional
  watchdog resets from transport dependence; preserve bounded qualification limits.

Verification: recorded command/response/state/sample traces, hostile-stall results,
resource/timing comparison and full-suite results satisfy every acceptance criterion.
Physical qualification, if not authorized, remains explicitly pending rather than
being inferred from simulation. No soak, physical-output or standalone-SD claims.

### Stage 7 — Review and delivery

- [ ] Push tested milestones on the dedicated branch and create a PR against the
  appropriate backend integration branch; make the intentional FPGA composite
  regression and Stage 2 approval visible in the PR.
- [ ] Inspect CI at the exact head SHA, diagnose failures using job logs/artifacts,
  and work to make required checks green. Review automated feedback repeatedly;
  address relevant findings and request re-review until approval, or report an
  unavailable/blocking reviewer explicitly rather than claiming approval.
- [ ] Publish sanitized evidence, reconcile this checklist and report remaining
  qualification limits. Return to the user if required scope/budgets cannot hold.
- [ ] Do not autonomously merge the PR.

Verification: final-head CI/review status, reproducible evidence and explicit
remaining limitations. Investigation PR activity never bypasses the Stage 2 gate.

## Testing cadence

Use narrow semantic differential and RTL tests during investigation/iteration;
measure the kernel early before paying for full integration or repeated routing.
At each behavioral milestone run the relevant package/platform suites. Run the
complete Linux suite at integration/final milestones, plus separately enabled RTL
simulation suites that the default workspace run does not cover. Follow
`.agents/info/test.md` for exact workspace/Wasm gates and use CI for other host
platforms. Documentation-only planning requires no behavioral test run.

## Primary starting points

- `LOADING_DESIGN.md` and `FPGA_ARCHITECTURE.md`: ownership and architecture contract.
- `src/rust/shoop_rt_host_reference/src/fpga/owner.rs`: current ARM command consumer.
- `src/rust/rt_fpga_rtl/src/host/control{.rs,/window.rs,/batch.rs,/session.rs}`:
  state translation, control holds, staged patches and loading.
- `src/rust/rt_fpga_reference/src/{rt_interface.rs,state/sync.rs}`: semantic oracle.
- `src/rust/rt_fpga_compiler/src/loading.rs`: allocation and composite-switch scope.
- `src/rust/rt_runtime/src/{immediate.rs,snapshot.rs}` and
  `src/rust/shoop_rt_transport/src/rt.rs`: complete groups, credits and snapshots.
- `fpga/host_bridge/rtl/{host_mailbox.sv,host_dispatch.sv,host_pipeline_axi.sv}` and
  `fpga/state_processor/rtl/{state_processor.sv,sp_engine.sv}`: integration/arbitration.
- `docs/hardware/zturn-v2/2026-11-25/bulk-arming/`: historical ABI-15 baseline,
  not evidence that the intended semantic FIFO architecture is implemented.
