# FPGA semantic command FIFO: investigation and implementation plan

## Status and execution contract

**Execution has resumed under the renewed explicit full-plan GO.** The earlier
plan-only update did not itself resume implementation. The original Stage 2 GO
and historical area-repair/LUT amendments remain recorded below.
Stages 0–3 are complete, including the reduced-profile/ABI contract freeze.

**Current priority: functional correctness first.** The user has
requested another area-budget relaxation. Kernel/frontend/whole-design LUT/FF
figures are now interim reference targets, not stop/go gates during functional
integration. No new numerical LUT/FF ceilings were supplied or are invented here;
final budgets must be explicitly agreed before hardware qualification, promotion
or overall acceptance. See the functional-first amendment in Stage 2. BRAM/DSP,
profile, semantic, cycle, timing and safety requirements are unchanged.

The latest measured partial frontend includes indexed MMIO, low-range discovery/
retained diagnostics/public recovery/ABORT and qualified audio/watchdog sidebands
on the actual semantic/audio/media pipeline, with the CORE/AUDIO/STORE public
configuration subset. It is **7,943 LUT / 6,943 FF after standard optimization**,
**57 LUT / 57 FF below the previous 8,000/7,000 targets**. Current functional source
also joins public CLEAR_VIEW/bulk-media through the existing private loader/owner
and a complete optional-trace reader through the existing immutable trace slot.
These functional extensions have no new resource/timing measurement. The figures
above predate CLEAR_VIEW/trace/bulk-media and are not current-source fit.
That narrow margin no longer blocks functional integration of remaining
preparation/lifecycle services and adapters. The latest readiness refinement
improves worst unrouted WNS to **−9.754 ns**, still failing; aggregate setup
violations worsen. Production still selects ABI 15; Stages 4–7 remain incomplete. This is not
complete frontend, protected whole-design or routed-timing acceptance.
The measured component and original stop decision are recorded in the Stage 4 resource gate below.
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
   bounds. Final resource usage meets the numerical budgets explicitly agreed
   under Stage 2's functional-first amendment before hardware qualification,
   promotion or overall acceptance. Total quantum timing retains its approved
   ceilings. Deferring interim LUT/FF gates is not final fit acceptance; a smaller
   FSM state count alone is not evidence of success.
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

Original Stage 2 ceilings (report §6): kernel 2,500 LUT/2,500 FF/no DSP; replacement
frontend 6,000 LUT/7,000 FF excluding existing execution/storage; whole design
32,000 LUT/24,000 FF/100 RAMB36-equivalents/at most the existing four DSPs;
command phase 4,096 cycles, snapshot phase 8,192 cycles for at most 128 rows,
total admitted quantum 60,000 cycles at 100 MHz, Q=32/48 kHz; nonnegative routed
setup/hold slack. These were the original hard-stop limits; historical breaches
and stops remain unchanged. The later functional-first amendment below defers
interim LUT/FF gates only, not other binding limits.
This GO does not waive Stage 6's separate physical-test authorization or Stage 7's
no-autonomous-merge rule.

#### User-approved frontend LUT amendment

After area-repair checkpoint `90b5c26b3431d66bde286696bed0b81b80bc6095`, the
user requested “relax the area limit in the plan before we continue.” The proposed
change was **only replacement-frontend LUTs: 6,000 → 8,000**, with every other
limit unchanged. The user explicitly confirmed: **“confirmed, go ahead.”**

The numerical limits after that amendment were:

| Scope | Previously approved numerical reference |
| --- | --- |
| Replacement frontend, excluding existing execution/storage | **8,000 LUT / 7,000 FF** |
| Shared kernel, included in frontend | 2,500 LUT / 2,500 FF / no DSP |
| Protected whole design | 32,000 LUT / 24,000 FF / 100 RAMB36-equivalents / at most the existing four DSPs |
| Command phase | 4,096 cycles/group |
| Snapshot phase | 8,192 cycles / at most 128 rows |
| Total admitted quantum | 60,000 cycles at 100 MHz, Q=32/48 kHz |
| Routed timing | Nonnegative setup and hold slack |

At that amendment, only frontend LUTs changed. The extra allowance did not raise
the whole-design ceiling or guarantee integrated fit. The 6,345-LUT partial
measurement excluded public MMIO and board integration. Historical reports,
original ceilings and failed outcomes remain unchanged; they are not relabeled
as passes by either amendment. Neither amendment supplies new measurements,
waives remaining tests, authorizes physical work or completes a later stage.

#### User-requested functional-first area amendment

The user subsequently requested: **“relax the area budget again. lets go for
functional correctness first. update the plan”**.

This changes the ordering and interim LUT/FF acceptance policy:

- Prioritize complete functional integration and correctness coverage in Stages
  4–5 before further area-only optimization. Do not delay required services,
  matched software or meaningful tests merely to preserve the narrow current
  LUT/FF margin.
- Kernel, replacement-frontend and protected-whole-design **LUT/FF numbers above
  become reference targets, not interim hard-stop ceilings**. Record and report
  overruns honestly; a LUT/FF overrun alone no longer stops functional development.
  Keep source-qualified, same-profile measurements at meaningful milestones,
  without requiring a new synthesis run for every small functional change.
- **No new numerical LUT/FF ceilings are agreed yet.** After the complete
  functional baseline is established, measure the complete protected design,
  present actual area/timing and obtain explicit user agreement on final LUT/FF
  budgets. Meet those budgets before hardware qualification, production promotion
  or overall acceptance, or return to the user if they cannot be met. This is not
  an unlimited final budget or permission to exceed the target device's capacity.
- Retain the shared sequential architecture, full profile/capacities, ownership,
  semantics, fault/recovery protections and all correctness coverage. Do not
  reduce functionality or move live decisions back to ARM to manufacture fit.
- **BRAM/DSP constraints remain binding**, including the kernel's no-DSP/useful-
  scratch restriction and whole-design 100 RAMB36-equivalents/existing four DSPs.
  Command/snapshot/quantum bounds, clock/Q/rate and nonnegative routed setup/hold
  requirements are unchanged. A breach of a still-binding limit returns to the
  user rather than silently weakening it.
- The amendment request was documentation-only and did not resume the earlier
  paused goal. The renewed explicit full-plan GO subsequently resumed software
  implementation under this policy, not deployment, physical testing or acceptance.

Verification: retain the original GO, both user-requested amendments and all
historical measurements. Stage 6 must explicitly freeze final numerical LUT/FF
budgets and verify full-design fit; interim functional success cannot waive that
gate or imply completion of Stages 4–7.

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

**Current priority:** complete the remaining public preparation/lifecycle
services, adapters and hostile lifecycle correctness coverage,
then the matched Stage 5 transports and CPU-control retirement. Do not insert
further area-only milestones ahead of required functionality solely because the
previous LUT/FF targets have little headroom. Keep resource evidence honest;
final optimization/fit acceptance remains a Stage 6 gate under the amendment above.

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

**Actual-RAM/audio component milestone:** generated production module
`fifo_audio_pipeline.sv` connects the banks/consumer/kernel to the actual audio
core's internal loop/state RAM port, selection handshake and SP/JP processing.
Selection has a pre-run boundary hook; local RAM grants are restricted to an idle
whole audio core and lose to processing admission. Pending handoff no longer gates
core rendering. Its strengthened live-loading differential test renders three
sample-accurate quanta while handoff is withheld, preserving ramps/history/plans,
first-frame identity and retained allocator ownership. The old gate's regression
failure remains `stage4-audio-handoff-before-fix`.

The new component test selects through a semantic packet, changes gain with exact
same-quantum samples/original-f32 strobe, arms 32 actual loop records with common
SP stamp 64/capture counters 32, and renders with all four result reservations
unread, a withheld handoff, continuous refused host requests and result reads.
Stale epoch/sequence fence events neither complete nor fault the current operation.
Only an epoch/sequence-matching test-controlled fence, after the core requests it,
releases the selected result. This is not real cache/DMA ownership qualification.
The graph has 32 sync visits plus input/gain/output, not media recording jobs.

Actual-RAM command peak is **2,950 cycles**. Boundary-to-core-done cycles are
**2,138 / 2,174 / 5,129 / 2,180 / 2,233 / 2,180** for that local-service test.
There is no DDR/media/cache/snapshot work in this envelope; these figures do not
prove the 60,000-cycle bound for all admitted graphs or protected-design timing.
Fresh gates pass **180 affected-package plus 25 directed component tests (205
total)**, warnings, formatting, policy/coverage and generators, including the new
semantic generator in `generate_top.py --check`. Evidence is
`work/cmd-fifo-investigation/stage4-audio-integration-milestone.{log,status}`;
`stage4-semantic-audio-fence-identity` retains the cycle output. Final
`stage4-audio-integration-precommit` checks the normal top generator hook,
workspace warnings/formatting/policy and independently counted results/links.

The new component is **not yet selected by the legacy host/media/MMIO top**; that
path still ties off the internal ports and remains ABI 15. Real media/cache/DMA
fence integration and media-store admission, in-flight fault/reset integration,
local observation accumulation, settled snapshots and ABI-16/reference/firmware
work remain. The wrapper drains SP events into an unbackpressured local strobe;
required metadata storage is not yet implemented. No new resource/route, complete
quantum, physical, fresh Linux/Wasm or delivery qualification is claimed. Stage 4
checklist items remain open until full production integration passes.

**Private media-service prerequisites:** inspection found that removing only the
media wrapper's admission gate would move ARM stalls into JP: `media_store`'s
`SAMPLE_DATA` waits for its client's write-valid/read-ready, and the existing wrapper
routes data/completions by current pipeline-busy state. That routing is safe only
while loader and rendering remain serialized. The implementation refinement is
whole-chunk private staging plus an accepted-operation owner, not a larger hold or
an ARM-assisted execution path; accepted goals/ceilings remain unchanged.

`fifo_media_loader.sv` buffers a complete <=32-word upload before acquiring the
store and reserves/copies a complete read result before ARM consumption. Unread
results retain its private bank, not store service. Large preparation transfers
will be lowered into these immutable chunks outside semantic groups. Short
successful transfers and actual memory errors fail closed without replay.
`fifo_media_arbiter.sv` prioritizes execution over new background work, retains the
accepted job/loader/fence owner for data/completion routing, and latches fence
epoch/sequence at actual backend acceptance rather than echoing live pending
inputs. A held fence request cannot reissue after completion; fence errors fault.

Five new unit tests cover every partial 32-word upload prefix, stalled consumers,
closing-cycle data/completion, empty/short/error/reset cases, owner preservation
when processing starts during loader work, unread private replies, and latched
fence identities/no reissue. They use a synthetic backend, **not the real
media/cache/DMA/AXI path or a rendering-progress qualification**. Fresh gates pass
**185 affected-package plus 25 directed tests (210 total)**, workspace warnings,
formatting, policy/coverage and generators. Evidence:
`work/cmd-fifo-investigation/stage4-private-media-milestone.{log,status}` and
`stage4-media-{loader,ownership}-tests`.

At that checkpoint both units remained unconnected pending the semantic media
wrapper; the existing host/media top and ABI 15 were unchanged.

**Semantic media/AXI component milestone:** `generate_semantic_media.py` emits
`fifo_media_audio_pipeline.sv`, joining the consumer/SP/JP to the actual media
store, private loader and operation-owner arbiter. The AXI generator also emits
`fifo_media_audio_pipeline_axi.sv`, with fixed two-region program storage. The
arbiter's latched, epoch/sequence-qualified fence now completes only through the
real sample-cache DMA flush and metadata-cache drain, including AXI responses—not
an ARM/test-controlled handoff callback. Render admission/done no longer require
store idle, loader-command inactivity or ARM response/data consumption. Processing
also wins against new configuration admission. The legacy deployment top is not
yet switched to these components.

A new directed Rust test has three fixtures: word memory, 64-bit AXI/fixed memory,
and an AXI fence-memory failure. Each healthy fixture renders 41 exact-sample
quanta, including every partial 32-word upload prefix, pending real writeback with
responses withheld, unread private replies/read data, simultaneous refused FIFO
traffic/result reads, same-quantum gain, and actual JP playback/capture. It checks
the original 32 words plus 32 JP-recorded samples in the independent backing
memory after fencing, not merely cache readback. The fault fixture retains the
selected-but-unfenced operation, refuses its ACK, exposes no successful result,
blocks further processing and does not replay or advance frame 1152.

This small graph's boundary-to-done costs are **289–499 cycles**, command peak
**91**; these are not worst-case admitted-graph/snapshot/60,000-cycle or routed
resource proofs. The existing 32-target actual-RAM case remains 2,950 command
cycles. Fresh gates pass **185 affected-package + 26 directed tests = 211 selected
Rust tests**, warning-denying workspace build, formatting, policy/coverage and all
generators. Evidence: `stage4-real-media-{jp,axi,fault}-tests` and
`work/cmd-fifo-investigation/stage4-real-media-milestone-final.{log,status}`. Initial
fixtures passed a zero logical READ length and correctly received `InvalidView`;
those failures remain separate from the corrected passing evidence.

Stage 4 remains open: mandatory local observations, settled snapshots, complete
in-flight fault/reset/always-on recovery integration and resource/cycle acceptance
are not done. Host/MMIO/firmware still select the legacy ABI-15 path; ABI-16
selection/reference/software retirement and Stages 5–7 remain pending. No fresh
whole-design route, full Linux/Wasm, physical, push or PR claim is made.

**Local-observation component milestone:** `fifo_observations.sv` adds indexed
original-gain, scheduled-failure, last-pulse and pulse-valid storage for at most
128 physical slots. These are the specified observational fields, not another
execution record, planner or graph registry. Successful gain strobes and the
existing SP pulse/failure event encoding update locally without a reader handshake;
independent fields can update together. Mandatory updates win over configuration
and metadata reads. The metadata read port responds in one cycle when granted.

New-lease initialization uses the specified three-u64 layout, finite original gain
and zero failures/pulse/validity, rejecting nonzero unused bits and out-of-range
addresses without aliasing. The trusted loader must restrict writes to unused
leases. Control reset invalidates responses, trace and fault state without a full
RAM reset; fresh-incarnation members still require initialization before publication.
An independent one-record optional trace holds captured epoch/revision/data while
stalled. Its saturating u64 loss counter cannot fault or backpressure mandatory
updates; scheduled failures saturate at u32 max.

Three standalone Rust tests exercise all 128 slots, simultaneous gain/pulse,
consecutive failures, 512 events with trace storage full, immutable captured trace
identity, same-cycle dequeue/replacement, initialization/range/reset faults and
actual-width 32/64-bit saturation. Counter-edge seeding uses existing simulation
probes, not a production restoration API or reduced-width counter. An optimized-
away probe alias initially prevented one fixture from elaborating; that failure
remains separate from the corrected passing evidence.

Fresh gates pass **188 affected-package + 26 directed = 214 selected Rust tests**,
workspace warnings, formatting, policy/coverage and generators. Evidence:
`work/cmd-fifo-investigation/stage4-observation-milestone-final.{log,status}` and
`stage4-observation-tests{,-probe-alias}`. At this checkpoint the component was
**not yet connected** to actual SP events/gain strobes or `CORE_CONFIG` space 3;
that hookup and the remaining snapshot/recovery/integration gates were still open.

**Actual-observation integration milestone:** the generated semantic audio core
now connects successful gain updates and unbackpressured SP events to retained
observations, exposing a private one-cycle metadata reader and independent optional
trace. Both the real word-memory media wrapper and shared-AXI wrapper carry these
interfaces. The existing configuration port routes space 3 to new-lease metadata;
a single outstanding configuration retains its accepted source until the copied
response, rather than routing by subsequently changed request flags. Configuration
ownership does not gate rendering. Initialization remains trusted unused-lease
work, not continuing-state restoration; the ARM/schema adapter is still Stage 5.

An actual two-loop SP/JP fixture renders eleven exact-sample quanta with valid
frame-zero pulses, one failed empty scheduled Play, blocked configuration/private
readers, a full undrained trace and unread reliable results. An original subnormal
f32 gain survives its zero Q8.24 encoding and remains intact, along with the
failure count, across a primitive selection. The held trace keeps revision 1
while revision 2 runs; loss reaches 33 without a fault or reader service. Extended
real word/AXI media fixtures independently read retained original gain while real
cache/DMA fences and ARM returns remain stalled.

Connecting immediate observation fault back into SP abort exposed an elaborated
combinational feedback loop. The consumer still sees immediate fault suppression;
the media-enabled execution abort uses the existing retained observation-fault
bit instead. No warning was disabled and no extra per-slot state was added. Unit
checks distinguish immediate and latched status. Complete in-flight faults,
non-media abort/recovery/reset and epoch flushing remain separate open work.
The initial unused-helper warning and feedback-loop failure are retained evidence.

Fresh gates pass **188 affected-package + 27 directed = 215 selected Rust tests**,
workspace warnings, formatting, policy/coverage, generators and Python compilation.
Evidence: `work/cmd-fifo-investigation/stage4-observation-hookup-milestone.{log,status}`;
directed logs use `stage4-observation-hookup-{initial,tests,retained-abort}`. Small
fixture boundary-to-done costs are **443–533 cycles**, command peak **91**; the
existing actual-RAM 32-target peak remains **2,950**. These are not all-admitted
60,000-cycle, 8,192-cycle snapshot or integrated resource/route qualification.

Stage 4 remains open for whole-sync settled capture/two snapshot banks and complete
fault/reset/recovery. Stages 5–7 remain. The deployed host still advertises ABI 15;
no fresh full Linux/Wasm, physical, push or PR qualification is claimed.

**Whole-sync settled-gate milestone:** SP now has a hardware capture phase after
all declared sync visits and before any audio instruction is emitted. Its frame
is latched render F, not the already-advanced next quantum counter. Zero-sync and
empty programs also pass this normal processing gate without manufacturing an
extra frame. The semantic component fixes this profile on and rejects work before
the sync prefix; default standalone/legacy interleaving remains unchanged.

During capture, the actual loop/state RAMs grant one-cycle private reads while SP
remains busy. Local writes return error without mutation; loader loop/state writes
cannot change the held view. The semantic wrapper also excludes new configuration
admission during this phase. No ARM-controlled ready input is exposed: until the
snapshot engine is connected, semantic capture releases unconditionally in one
clock. The legacy wrappers tie the unused hooks off with capture disabled.

Three new tests verify all 128 rows over F=0/32, including recording render-length
interpretation, all processed stamps/render cursors versus next cursors, frozen
ramps/no JP writes, 4,096 raw-word read cycles, attempted writes, zero-sync/empty
programs and strict-prefix refusal versus retained standalone interleaving. Initial
fixtures mistakenly counted output-buffer clearing as final samples; the retained
failed log is superseded by exact final-buffer sample checks, while capture still
requires no JP writes at all.

Fresh gates pass **188 affected-package + 30 directed = 218 selected Rust tests**,
workspace warnings, formatting, policy/coverage, generators and Python compilation.
Evidence: `work/cmd-fifo-investigation/stage4-settled-gate-{tests,milestone}.{log,status}`;
`stage4-settled-gate-tests-output-fixture` retains the fixture failure. The release
adds one hardware clock per semantic quantum; actual-RAM command peak stays 2,950.
This is a capture boundary/RAM-service checkpoint, **not** two-bank packet storage,
complete snapshot publication, initial snapshot, loss/sequence/ACK or full 8,192-cycle
capture qualification. Those, complete fault/reset/recovery and Stages 5–7 remain.
No new integrated resource/route, full Linux/Wasm, physical or delivery claim.

**Standalone snapshot packet-engine milestone:** `fifo_snapshots.sv` now supplies
both independently released packet banks and an FPGA-local sequential producer.
It reserves a free bank at capture entry, copies 16 raw words plus all 129 metadata
bits per row, writes the eight-u64 header last and exposes only complete packets.
Each maximum packet is 19,520 bytes; neither packet RAM nor a bus response is held
by ARM readiness. Full banks skip the whole attempt with a saturating u64 loss
count; per-attempt sequence gaps remain visible. Sequence exhaustion disables new
publication without wrapping, faulting or withholding capture release.

The producer captures epoch/F/revision/quantum/phase at entry. Reads have independent
one-cycle copied responses; matching epoch/bank/sequence ACKs release either bank.
Old/duplicate and wrong-authority traffic cannot free a reused slot. Capacity is
sampled conservatively before a same-edge ACK; a held capture request never retries.
Reset/epoch changes invalidate publication without resetting RAM and discard a held
old capture until its source drops it. Internal RAM/metadata errors, a lost capture
gate and the watchdog cannot publish partial packets; actual pipeline faults also
suppress same-edge publication.

Five directed tests cover every word of 128-row packets, continuous partial reads,
full banks, original metadata bits, initial/settled headers, latched identity,
real-width saturation/exhaustion probes, out-of-order releases, simultaneous
read/ACK/reuse, same-edge full-bank ACK, completed/in-flight epoch flush and eight
fault/stall variants. One-cycle synthetic RAM/metadata service takes **4,618 clocks
entry-to-ready / 4,619 including entry and ready handoff**, within 8,192. Watchdog
accounting reserves both turnover clocks; its timeout peak is exactly 8,192.
Small-profile index-width elaboration warnings were fixed with sized indices after
full-width bounds checks, not warning suppression; the failed evidence is retained.

Fresh `stage4-snapshot-milestone.{log,status}` passes **193 affected-package + 30
directed = 223 selected Rust tests**, workspace warnings, formatting, policy/coverage,
generators and Python compilation. Narrow evidence is `stage4-snapshot-tests-final`;
`stage4-snapshot-tests-index-width` retains the failed initial geometry run. At
`e883a1d13` this engine was standalone; the following milestone joins normal capture
to execution. Root initial capture, full recovery and Stages 5–7 remain pending.

**Actual settled-snapshot hookup milestone:** semantic audio now lets the FPGA
snapshot producer own the bounded capture-ready gate and the real loop RAM during
capture. The kernel owns the same port outside capture; capture is read-only.
Snapshot metadata requests outrank diagnostic reads. A one-bit accepted-response
owner preserves a diagnostic read granted immediately before capture rather than
misrouting it when the live mux changes. Quantum is latched at actual render
admission; F comes from SP's settled gate, not the next-frame counter. Private
snapshot read/ACK/status ports propagate through actual word-media/shared-AXI tops.

A new 128-row actual-SP test checks every raw loop word, F=0 processed stamps,
render versus next cursors, recording render length, original subnormal gain bits,
response ownership across gate entry, and exact output. Config writes are not admitted
while the view is held; changing live quantum to 7 does not relabel its captured
Q32 header. Two unread packets stay immutable across continued rendering, sequence
gaps expose full-bank skips and qualified release permits a later complete packet.
Snapshot ACK never manufactures the still-withheld ownership fence.

Actual 128-row capture is **4,619 clocks including entry/handoff**. Seven fixture
quanta take **7,973–12,680 clocks**; these are not an all-admitted workload bound.
The existing local 32-target command peak remains **2,950**. Actual two-loop packets
also preserve valid frame-zero pulses/failure counts and held revision-1 identity
while revision 2 runs. Word/AXI media tests now independently read the new gain and
selected revision from a complete snapshot while real dirty-cache/DMA fencing is
stalled; backing-memory, playback/capture and memory-fault assertions remain.

Fresh `stage4-snapshot-hookup-milestone.{log,status}` passes **193 affected-package +
31 directed = 224 selected Rust tests**, workspace warnings, formatting,
policy/coverage, generators and Python compilation. Narrow evidence is
`stage4-snapshot-hookup-128-tests`; initial `-packets-tests`/`-128-diagnostic` retain
a new fixture's F128 clip overrun. Its longer valid clip keeps all seven cursor
trajectories in range; no assertions or production guards were weakened.

At `734356826`, retained snapshot faults joined immediate consumer suppression
and media-enabled abort; initial capture and complete recovery remained open.

**Root initial-capture milestone:** the same producer now captures an explicitly
marked phase-0 packet once after fresh runtime reset, before processing begins.
It copies empty revision 0 at F=0 after the audio-owned quantum is known. No SP
sync pass, command group or processing frame is manufactured. Startup owns the
same bounded private RAM/metadata service and excludes configuration writes;
completion releases audio without waiting for packet reads or acknowledgement.

This clarifies the existing initial-capture requirement without adding a graph
registry or initialization pass: membership remains ARM bindings, and revision 0
has none. Unused physical rows may retain prior RAM, even valid-looking flags or
old observations; clients must ignore them rather than restore or infer members.
New-slot initialization is still required before publishing installation.

The 128-row idle-component test observes **4,622 startup clocks**, with **4,619**
in the producer including turnover. A pending first audio opportunity, partial
unpublished command and configuration-write pressure cannot advance time or
execute commands during capture. Initial packet/normal F0 packet stay unread while
F32/F64 render with exact silence and full-bank skips. A subsequent idle reset
flushes partial input and snapshot identities but preserves raw RAM; an old-epoch
ACK cannot free the new phase-0 packet. Merely changing epoch without reset now
leaves startup and publication blocked, matching command banks' reset-latched
incarnation. This is not full in-flight, media/AXI drain or always-on recovery.

Fresh `stage4-initial-capture-milestone.{log,status}` passes **193 affected-package +
32 directed = 225 selected Rust tests**, workspace warnings, formatting,
policy/coverage, generators and Python compilation. Narrow final evidence is
`stage4-initial-capture-tests`; `-tests-owner-declaration` retains an initial
duplicate-wire elaboration failure. `-tests-epoch-sequence` retains the discovered
epoch-only startup/bank mismatch; the final test checks refusal until proper reset.
All existing exact-sample, 128-row, full-return, primitive/fence and actual word/AXI
media tests remain green, with normal quantum/command costs unchanged. Initial
publication consumes sequence 1; normal packet expectations advance without reuse.

At `9c021b241`, initial capture was qualified locally; complete execution fault-stop
and reset/drain/first-fault integration still remained open.

**Non-media execution fault-stop milestone:** audio/JP now accept a generic latched
abort independent of media enable. Semantic execution drives it from external
pipeline failure, retained consumer/observation/snapshot faults and reported core
errors. The observation input uses its retained bit, not the immediate event-valid
feedback path. A one-cycle pulse blocks new admission, execution writes, sample
I/O, job/hint issue and new handoff requests immediately and remains terminal until
reset. Local loop/state, scratch/output and smoothing writes are gated; unissued
hint control is flushed without resetting execution RAM or backend/AXI owners.
Legacy media/job-stream wrappers tie the new input low, preserving their behavior.

Seven directed non-media fault points cover state start, loop writeback, settled
capture entry, partial packet copy, sample input, gain streaming and output write.
Same-edge write strobes drop; complete 64/129/32/128-bit persistent RAM fingerprints
stay unchanged, preserving earlier effects without rollback or later writes. In
these inline/local fixtures SP/JP control retires within **two clocks**. A held new
opportunity, configuration write, command flood and fake fence completion cannot
resume it or expose reliable fenced success. Partial snapshots remain unreadable;
already-complete packets remain diagnostic-readable but faulted ACKs do not release
them. Frame/sequence/ready-mask identity stays fixed after the fault.

`stage4-nonmedia-stop-milestone.{log,status}` passes **193 affected-package +
35 directed = 228 selected Rust tests**, including existing standalone core and
legacy media-pipeline regressions, workspace warnings, formatting, policy/coverage,
generators and Python compilation. `stage4-nonmedia-stop-tests` is the original
seven-point narrow run; the final milestone also checks post-fault packet reads/
ACKs. `stage4-nonmedia-stop-legacy` independently passes core/pipeline tests.
Normal 128-row and initial-capture cycles, exact samples and real word/AXI cache,
DMA fence and failure tests remain green; the 32-target command peak stays 2,950.

This is **execution stop, not backend retirement or recovery**. Accepted store,
DMA, loop-response or MIDI ownership still needs its real drain/response or safe
isolation/reset protocol; local idle and no further DSP writes are not its fence.
At `b4b9790cb`, always-on drain/reset/first-fault integration still remained open.

**Actual AXI isolation and epoch-reset milestone:** reuse the existing serial
AXI master rather than add a parallel transaction tracker. A terminal, latched
quiesce cuts command admission and cancels headers not yet presented; active
AW/W/AR are preserved, including W-before-AW. Writes require all real payload
beats and B, with no padding. Reads and B drain independently of upstream read/
completion readiness, without reporting successful callbacks to the discarded
runtime. Wrong IDs, unexpected/early/duplicate responses and mismatched RLAST
retain a protocol fault that prevents claiming quiescence or reusing storage.
Ordinary legal SLVERR/DECERR responses remain distinct from unsafe accounting.

Generated `fifo_recovery_pipeline_axi` now keeps `host_recovery` outside runtime
reset and connects the actual semantic/media/shared-AXI component. Recovery first
stops SP/JP effects and isolates new AXI commands. It requires actual bus retirement,
zero AW/B and AR/RLAST credits and two quiet clocks before incrementing incarnation
once and resetting the entire old runtime. This safely discards internal pending
operations; it does **not** manufacture their program/media fence. Defaults remain
100,000,000 drain cycles and 128 reset cycles. A held old-epoch recovery request
cannot initiate another reset. Unexpected B/R during reset or any unsafe accounting
fails closed; a timed-out transaction completing later cannot restart recovery.
Hardware recovery state 3 means reset complete, not management/backend Ready;
fresh startup/lease/session/first-frame qualification still belongs to Stage 5.

Four new AXI tests cover 608 write-prefix/order cases, 272 read-prefix cases,
32/64-bit buses, unread callbacks, missing bodies, cutoff and protocol failures.
Two actual semantic/shared-AXI tests cover held AW, W and DDR fetch AR, plus idle
unsolicited B, B/R during reset and W timeout. No reset/epoch change occurs during
the three held transactions. After real drain, old fills/queues/selected-but-unfenced
work and snapshots disappear; fresh epoch 2 publishes empty F0/revision-0 without
ARM ACK or replay. Retained physical loop/gain rows remain nonmembers, not restored
state. Old ACKs/requests cannot free or acquire the new incarnation.

`stage4-axi-recovery-milestone.{log,status}` passes **197 affected-package +
49 directed = 246 selected Rust tests**, workspace warnings, formatting, policy/
coverage, generators and Python compilation. The extra directed AXI/media/metadata/
ownership targets contribute 12 tests and also pass independently in
`stage4-axi-recovery-legacy`. Narrow evidence is `stage4-axi-drain-tests` and
`stage4-axi-recovery-{tests,races}`. The initial `-tests-until-api` compilation error
and `-races-write-gap` fixture failure are retained: a gap in WVALID is not WLAST/B
retirement. Normal semantic samples/capture costs and command peak 2,950 remain.

At `82b78edbc`, retained first-fault integration and broader lifecycle/error-ordering
qualification remained open. The following milestone implements retained diagnostics.

**Retained first-fault milestone:** `host_fault_record.sv` factors the existing
mailbox recorder into a small shared leaf. It retains the complete 64-bit group
sequence, revision, frame, cause and counters. A same-epoch fault or ABORT cannot
clobber it; epoch zero/older epochs are ignored, and only a genuine higher-epoch
capture replaces it. Legacy ABI-15 fields remain unchanged, with a zero-extended
legacy tag. The semantic recovery wrapper keeps this leaf outside runtime reset.

The consumer exposes its actual first fault and qualified owner: delayed fence
failure or a blocked selected fence-result writer retains its original group/F,
not a competing command's identity. Wrong epoch/sequence callbacks cannot create
that event. Header refusals cannot inherit a previous packet's opcode. A small
saturating opportunity counter/frame latch records audio-owned F during execution,
not SP's already-advanced next counter; idle faults use the next opportunity F.
Accepted recovery records ABORT/RECOVERY only if no earlier cause exists. Quiesce/
reset-induced aborts cannot relabel the retained epoch-1 cause as epoch 2.

The new leaf test checks every field at full width, same/older-epoch immutability,
zero/non-event epochs and whole-device reset. Actual semantic/media/shared-AXI
fixtures fault a decoded command and settled F0 after SP advances to 32, retain
the original cause through recovery/fresh startup, keep rendering independent of
record reads, and replace it only on a genuine epoch-2 F32 fault. Existing held
AW/W/AR, failed recovery and late-completion tests now check retained records too.
The finalizer race distinguishes retained WaitingFence ownership from an explicit
failed-fence state; neither permits an ACK or a successful result after fault.

`stage4-first-fault-milestone.{log,status}` passes **198 affected-package +
50 directed = 248 selected Rust tests**, workspace warnings, formatting, policy/
coverage, generators and Python compilation. Narrow evidence is
`stage4-first-fault-{tests,identity,header-context,finalizer-context}`. The retained
`-milestone-header-result-order` and `-finalizer-context-bank-state` failures were
fixture assumptions: FIFO result access needs the prior head released, and a
terminal pipeline fault retains the old WaitingFence bank without claiming its
fence failed. The corrected checks preserve ownership/no-success assertions.

Stage 4 remains open for broader hostile lifecycle and public MMIO integration;
internal SP/JP error-ordering parity, including the two baseline differences, is
still required in Stage 6. The ABI-16 contract reserves a read-only sequence-high
word without truncating semantic diagnostics; schema/ARM mapping and Stages 5–7
remain pending. Deployed ABI is still **15**. No fresh whole-design resource/
protected-route, all-admitted quantum, full Linux/Wasm, physical, push/PR or
final-delivery qualification follows from the selected tests.

Verification: RTL kernel/queue/arbitration tests and cycle assertions pass against
Stage 2 bounds, including stalls at every packet word and request/response phase.

**Historical resource gate — breach and stop at `c6ddb7652`:** the broader
fixed-Q32 semantic/media/recovery component was synthesized with Vivado 2021.1,
`xc7z020clg400-2`, 100 MHz, 128 loops, 512 state rows/buffers/views, DDR program
storage and the board's 128 MiB PL arena. The
[resource evidence](docs/fpga/command-fifo-implementation/reports/pipeline-synthesis-9b8cd12c8/results.json)
records packed hierarchical area, not raw LUT primitive counts:

| Measured scope | LUT | FF | Then-approved ceiling |
| --- | ---: | ---: | --- |
| Kernel | 1,824 | 1,940 | 2,500 / 2,500 |
| New frontend/glue, excluding SP/JP, media store and existing AXI/program backend | 61,414 | 22,894 | 6,000 / 7,000 |
| Entire measured component, **before MMIO/audio adapter/PS/DDR/thermal board integration** | 85,056 | 37,785 | Whole design: 32,000 / 24,000 |

The component uses 59 RAMB36 + 7 RAMB18 = **62.5 RAMB36-equivalents** and the
existing four DSPs. Local observations dominate at **55,372 LUT / 17,127 FF**, with
no RAM inferred; Vivado explicitly dissolved its four arrays into registers.
Even excluding that entire block leaves 6,042 frontend/glue LUTs before MMIO, so
an observations-only repair is not yet a fit proof. The kernel is not the area
failure. Synthesis timing is only an estimate: WNS −4.584 ns / WHS +0.183 ns;
no placement/routing or protected-board qualification was attempted.

Synthesis finished with zero errors/critical warnings. The runner subsequently
failed an incorrect hierarchy-accounting path check and returned **1**, with no
complete marker. That failure is preserved, not relabeled a passing run; packed
area above is reconciled from the native hierarchy report. Earlier source-order,
probe-width and macro-guard preparation failures are also retained. Measurement
tooling paths were corrected after the run, without changing or resynthesizing
production RTL. The previous 248-test milestone remains historical/current-RTL
functional evidence, not evidence that this implementation fits.

At `c6ddb7652`, the approved breach rule triggered a **stop and return to the
user**. The subsequently supplied area-repair direction is recorded below; the
stop did not authorize changing any ceiling, profile or semantic requirement. No ceiling increase, profile
reduction, CPU fallback, ABI-16 deployment, physical work or delivery is authorized
by this failed resource gate. Stages 4–7 remain incomplete.

**Area-repair direction:** the user subsequently explicitly requested “try to fix
the area problem.” This authorizes investigating and repairing RAM inference and
frontend overhead within the existing limits, not changing acceptance criteria.
The failed measurement remains historical evidence; repairs require fresh
functional, source/hash, resource and eventually complete integration gates.

**Area-repair milestone:** explicit single-address/single-write observation banks
now infer distributed RAM instead of 16,512 data flip-flops and wide register muxes.
Runtime effects still outrank configuration/reads, independent fields can update
together, and one-cycle copied responses, original f32 bits, saturating counts,
optional trace loss and reset persistence remain unchanged. A fourth standalone
observation test covers cross-row gain/failure updates, read/config contention,
read-before-write initialization and isolation of all three initialization words.

The sequential command packet and single kernel working record now shift only on
accepted responses; writeback reads the fixed head rather than a variable wide
mux. No states, passes, commands, working records or cycles are added. Both frozen
prototype and production kernels still pass 3,970 differential cases per source;
maximum synthetic group/fence service remains 3,783 clocks.

[Fresh evidence](docs/fpga/command-fifo-implementation/reports/area-repair-c6ddb7652/results.json)
uses the same Q32/128-loop/full-storage profile and packed hierarchy accounting:

| Variant | Component LUT / FF | Frontend LUT / FF |
| --- | ---: | ---: |
| Observation RAM repair | 30,322 / 21,268 | 6,643 / 6,379 |
| Plus sequential working records | 30,027 / 21,263 | 6,348 / 6,374 |
| Same checkpoint, standard `opt_design` | 29,923 / 21,235 | **6,345 / 6,374** |

Observations fall from 55,372 LUT / 17,127 FF to **605 LUT / 615 FF**, including
258 LUTRAMs. The kernel is **1,595 LUT / 1,932 FF / no DSP**. Total storage remains
62.5 RAMB36-equivalents and four existing DSPs. Synthesis/optimization complete
without errors/critical warnings; the initial postprocessing attempt's unopened
checkpoint error remains separately recorded, not a resource result.

`stage4-area-repair-milestone.{log,status}` passes **199 affected-package +
50 directed = 249 selected Rust tests**, workspace warnings/formatting,
test-policy/coverage, generators and three offline tooling tests. Real semantic
SP/JP, 128-row/initial snapshots, media/AXI fences, fault-stop and recovery remain
covered. There is no fresh full Linux/Wasm or physical qualification. At this
milestone, the partial frontend exceeded the then-approved 6,000-LUT ceiling by
345 LUT; that historical result is preserved. **Subsequent approved amendment:**
Stage 2 now records an 8,000-LUT frontend ceiling, leaving 1,655 LUT under that
limit before public MMIO/integration. The frontend FF ceiling remains 7,000.
This removes the partial measurement's frontend-LUT overrun under the current
limits, not the remaining integration/acceptance work. Whole-component figures
exclude board integration and are not whole-design acceptance; unrouted timing
still fails, and no route or all-admitted-quantum proof is claimed. Stages 4–7
remain open.

**Indexed-MMIO component milestone:** `fifo_mmio.sv` implements the candidate
ABI-16 indexed register range 0x150–0x32c over AXI-Lite. A separate generated
semantic schema supplies matching SV/Rust constants without changing the deployed
legacy schema/version 15. This is not the complete public endpoint: discovery,
loader/service and recovery/retained-diagnostic registers through 0x14c still need
joining to the real semantic audio/media/recovery wrapper and matching software.
In particular, sequence-high must receive the qualified retained recorder value,
not the consumer's current attempted-group identity.

AW/W are independently captured and cause no backend request until paired. Sticky
per-half incarnation authority prevents a pair crossing runtime reset from
mutating the new runtime. Runtime reset clears selection/cursors/ACK staging,
refuses abandoned local callbacks with receipt 2, and preserves accepted AXI
response ownership/copies; only global reset clears the transport. Packet reads
copy into RDATA, never hold RAM for RREADY, and cannot wrap or advance a newly
selected/rewound cursor. Unavailable reads and expected admission/ACK refusals use
OKAY plus receipt; malformed register accesses still use SLVERR. Concurrent ACK
completion has priority over same-channel read-status receipt updates.

Seven new Rust tests use the real consumer/paired banks and 128-row snapshot
engine with synthetic local RAM: all 400 partial publication prefixes, full
32-command execution while B is held, all 4,880 snapshot words, R-held ACK/reuse,
FIFO/non-head/delayed-fence visibility, select/rewind races, pending and completed
reset responses, both AW/W reset orders, epoch-only refusal, and faulted readable
packets with refused effects. The new ACK/read race first reproduced an incorrect
accepted receipt overwriting an epoch refusal; receipt priority fixes it without
added states/cycles. Earlier invalid fixtures attempted a second read while R was
held; write-only attack helpers preserve the held response and defer receipt reads.
Failed logs remain separate, not passes.

`stage4-semantic-mmio-milestone.{log,status}` passes **206 affected-package +
50 directed = 256 selected Rust tests**, including both 3,970-case kernel oracles,
actual SP/JP/snapshot/media/AXI/recovery regressions, workspace warnings/formatting,
policy/coverage and generators. Three new schema tests and three measurement-tool
tests pass. No profile/ceiling change, complete frontend/resource/route proof,
full Linux/Wasm or physical qualification is implied. The 6,345-LUT measurement
predates this unintegrated MMIO component. Stage 4 remains open for complete
public integration/broader lifecycle qualification; Stages 5–7 remain open.

**Actual indexed-MMIO/audio/recovery hookup milestone:** generated
`fifo_mmio_pipeline_axi.sv` now joins the indexed AXI-Lite range to the actual
semantic SP/JP/RAM, snapshots, media owner, shared AXI and always-on recovery.
The normal generator drift gate includes this wrapper. MMIO receives global reset
for its transport, the real safe runtime reset for local authority, the recovery
controller's runtime incarnation and the qualified retained recorder's full
sequence—not the live consumer diagnostic sequence. Packet controls are private
wires owned by MMIO; preparation/configuration and recovery request inputs remain
private integration interfaces, not a completed low-range public service.

Two new actual-pipeline tests render exact samples with B held at publication,
ACK/reuse a real snapshot and result bank while an old copied R word is held,
execute 32 repeated gain commands at one boundary, and read all 4,880 u32 words
of a coherent 128-row packet. Uploads/readback do not advance frames. Small-fixture
quanta take **4,662 / 4,662 / 45 / 4,962 / 6,641 clocks**, command peak **1,734**,
128-row snapshot peak **4,619**. These are not all-admitted-graph cycle bounds.
A second test seeds actual-width sequence counters through simulation-only probes,
faults the accepted group, and holds a real DDR upload AW during recovery. Reset
and epoch change wait for actual external retirement while MMIO R/B remain held;
fresh local startup needs neither reader. Retained sequence-high stays correct
when live diagnostic sequence becomes zero, until a genuine higher-epoch fault.
Stale admission and snapshot ACK cannot acquire/release epoch-2 ownership.

[Fresh same-profile evidence](docs/fpga/command-fifo-implementation/reports/mmio-hookup-a07b89b06/results.json)
includes the indexed range, unlike the earlier 6,345-LUT checkpoint:

| Variant | Partial component LUT / FF | Replacement frontend LUT / FF |
| --- | ---: | ---: |
| Synthesis | 30,664 / 21,616 | 6,985 / 6,727 |
| Standard `opt_design` | 30,560 / 21,588 | **6,982 / 6,727** |

The indexed MMIO leaf is **638 LUT / 353 FF**; kernel remains **1,595 LUT /
1,932 FF / no DSP**. Component storage remains **62.5 RAMB36-equivalents** and
four existing DSPs. Packed frontend totals include LUTRAM and exclude only the
same four disjoint execution/media/AXI/program backend boundaries, now under the
new wrapper. `--pipeline --mmio` freezes this profile; default consumer Tcl and the
earlier pipeline probe remain unchanged. Source hashes, both native reports,
Tcl/profile/provenance and 15 archive checksums are preserved. Synthesis and
optimization complete with zero errors/critical warnings, runner status 0 and a
complete marker. Unrouted WNS is **−5.481 ns synthesis / −5.017 ns optimized**;
there is no route, protected-board or complete-frontend acceptance.

`stage4-mmio-hookup-milestone.{log,status}` passes **206 affected-package +
52 directed = 258 selected Rust tests**, including both 3,970-case kernel oracles
and the retained real-media/fence/reset/fault regressions, warnings/formatting,
policy/coverage, generators and **11 offline Python tests**. Fresh full Linux
passes **3,584 / 11 skipped** with nextest 0.9.116 and isolated dummy JACK; default
application/worklet and optional `fpga-reference` worklet Wasm builds pass.
These compile/dummy-host gates do not qualify physical audio or browser runtime.

The current partial frontend is below **8,000 LUT / 7,000 FF**, with **1,018 LUT /
273 FF** left for remaining integration; the independent whole-design ceilings
remain binding. The 0x000–0x14c discovery/preparation/service/recovery/diagnostic
range is not yet joined to the new endpoint. ABI-16 ARM/reference adapters,
CPU-control retirement, source-specific external audio/watchdog hookup, the two
baseline fault differentials, complete admitted-service/route/platform acceptance
and delivery remain open. No profile/ceiling change, ABI-16 deployment, physical
authorization, push/PR or completion of Stages 4–7 follows from this milestone.

**Lower discovery/diagnostics/public recovery milestone:** the existing MMIO
transaction owner now optionally decodes discovery/geometry at 0x000–0x03c,
telemetry at 0x104–0x10c and recovery/retained diagnostics at 0x114–0x14c.
The actual candidate enables that subset; the standalone indexed fixture keeps
it disabled. The independent candidate schema mirrors these existing addresses
without importing the retired legacy operations. Production/schema remain ABI 15.

The recovery doorbell now owns private request/epoch wires into the existing
always-on controller, not another AXI owner or recovery machine. A complete,
current-epoch, full-strobe request is accepted only in hardware states 0/3.
Wrong epoch/old AXI halves return OKAY/receipt 2; a busy/failed controller returns
OKAY/receipt 5; malformed strobes return SLVERR without requesting recovery.
Receipt 0 means request accepted, not successful retirement/fence or application
Ready. Hardware state 3 still means reset complete only. Accepted safety receipts
and copied R/B survive runtime reset; incomplete old halves cannot request another
reset in the new epoch. All low fault fields, including `FAULT_TAG`, and high
sequence 0x174 come from the same qualified always-on retained record.

Saturating completed-quantum/peak-cycle telemetry reuses the existing audio-owned
opportunity cycle counter; no second elapsed-time tracker or processing gate was
added. Geometry preserves the existing 512-job admission cap despite larger
backend storage. The candidate defaults to the approved 128 MiB arena and rejects
an unrepresentable/zero u32 discovery extent rather than truncating 4 GiB to zero.
IRQ reflects complete results/snapshots or terminal fault; the audio-xrun input is
a trusted hardware interface, not an ARM mutation API or completed board hookup.

Three actual-path MMIO tests now cover exact rendering/128-row copies and full
retained-record reads, held public-recovery B and fault R across real DDR drain,
low EPOCH R held across reset, both AW-first/W-first old recovery halves, wrong
incarnation/busy/malformed requests, IRQ and audio-owned telemetry. Reads do not
advance F. The new diagnostic assertion initially indexed the ten-field saved
record incorrectly (opcode 6 versus tag 66); corrected test indices preserve all
assertions, with no RTL change. The first concurrent Linux/default-build attempt
used a mismatched host fixture rejecting `test_signal`. Its failed log remains;
a fresh matching host rebuild and sequential full rerun pass without host changes.

[Fresh native evidence](docs/fpga/command-fifo-implementation/reports/low-discovery-efaad8ccd/results.json),
based on `efaad8ccd` plus the captured source changes, has **19 checksummed files**:

| Variant | Partial component LUT / FF | Replacement frontend LUT / FF |
| --- | ---: | ---: |
| Synthesis | 30,988 / 21,682 | 7,309 / 6,793 |
| Standard `opt_design` | 30,884 / 21,654 | **7,306 / 6,793** |

MMIO is **938 LUT / 419 FF**; kernel remains **1,595 LUT / 1,932 FF / no DSP**.
Storage remains **62.5 RAMB36-equivalents**, with four existing DSPs. Runner 0,
complete marker, zero native errors/critical warnings and 217 synthesis warnings
are recorded. Unrouted WNS remains **−5.481 ns synthesis / −5.017 ns optimized**;
there is no place/route or complete frontend/protected whole-design acceptance.
At this checkpoint, partial frontend margin was **694 LUT / 207 FF**, not a whole-design waiver.

`stage4-low-discovery-milestone-v2` passes **206 affected + 53 directed = 259 Rust
tests**, both 3,970-case kernel oracles, warnings/formatting/policy/coverage,
generators and **12 Python tests**. `stage4-low-discovery-linux-v2` passes **3,584 /
11 skipped**, nextest 0.9.116, 404.162 s with isolated dummy JACK; default and
optional-reference Wasm builds pass. Original failed broad/Linux logs are retained.
No physical or browser-runtime qualification follows from these gates.

**Historical subset boundary:** this checkpoint did not map 0x110 ABORT or provide
qualified audio/watchdog inputs. The following milestone adds those paths without
claiming complete service/software/board integration or acceptance.

**Qualified safety/ABORT milestone:** the same MMIO owner now maps 0x110 ABORT.
It requires complete, full-strobe, current-epoch AW/W authority and data equal to
the epoch. Neither half alone halts execution; wrong epoch/old halves refuse with
OKAY/receipt 2, malformed strobes with SLVERR. Receipt 0 means accepted local halt,
not backend retirement, a successful fence, runtime reset or application Ready.
Only explicit recovery requests enter the existing drain/reset controller.

- [x] Implement candidate ABORT and cause-qualified hardware sideband routing on
  the actual pipeline, retaining the existing sticky halt and same-edge masks.
  ABORT/audio/watchdog retain flags **16/2/32**; genuine simultaneous generic
  pipeline faults retain **64**, rather than flattening all events to 64.
  Physical audio/watchdog adapter integration remains separate and incomplete.

The shared semantic wrapper carries a qualified cause word through the existing
fault path. A zero cause retains legacy generic-64 behavior; AXI quiesce/drain
faults preserve their generic qualification. No new halt/recovery controller,
backend owner, elapsed timer, state record or CPU live-state fallback was added.
Global/local reset separation and retained first-fault identity are unchanged.

Four actual-MMIO tests now pass. The new test covers five source combinations,
rendering while an ABORT AW is incomplete, malformed/stale ABORT, terminal local
halt with an actual DDR AW still owned, explicit recovery with copied R/B held,
and genuine higher-epoch replacement while old copied cause RDATA stays immutable.
The previous reset-crossing-half test now covers both ABORT and recovery, in both
AW-first/W-first orders. Its original assertions remain. The first new test run
queried nonexistent `sample_output_valid`; correcting the fixture to the actual
`sample_output_write` retains the same-edge assertion and requires no RTL change.
Failed narrow/backtrace logs remain preserved.

[Fresh native evidence](docs/fpga/command-fifo-implementation/reports/qualified-safety-d6faa5c77/results.json)
freezes the source based on `d6faa5c77` in **25 checksummed files**:

| Variant | Partial component LUT / FF | Replacement frontend LUT / FF |
| --- | ---: | ---: |
| Synthesis | 31,160 / 21,688 | 7,481 / 6,799 |
| Standard `opt_design` | 31,055 / 21,660 | **7,477 / 6,799** |

MMIO is **964 LUT / 419 FF**; the unchanged kernel source maps to **1,763 LUT /
1,936 FF / no DSP**. Storage remains 62.5 RAMB36-equivalents/four existing DSPs.
Runner 0/complete marker pass, with zero errors/critical warnings and 217 synthesis
warnings. **Unrouted WNS worsened from −5.017 to −9.142 ns** (both new variants).
No place/route or complete frontend/protected whole-design acceptance is claimed.
At this checkpoint, partial frontend margin was **523 LUT / 201 FF**. The non-MMIO default
probe changes only by a new cause input tied to zero; tests byte-check all other
probe I/O/geometry and the unchanged consumer Tcl against historical evidence.

`stage4-qualified-safety-milestone` passes **206 affected + 54 directed = 260 Rust
tests**, both 3,970-case kernel oracles, warnings/formatting/policy/coverage,
generators and **12 Python tests**. Full pinned-nextest Linux passes **3,584 /
11 skipped** in **404.838 s** with isolated dummy JACK; default/optional-reference
Wasm builds pass. Native fixture builds and Linux tests ran sequentially with a
fresh matching host binary. These gates are not physical/browser-runtime proof.

Preparation/loader/service/optional-trace MMIO and external audio/watchdog adapter
integration remain open, as do matched ABI-16 software and CPU-control retirement,
full admitted-service/whole-design/route/platform acceptance, both baseline fault
differentials and delivery. Production remains ABI 15. Stages 4–7 remain incomplete;
no physical authorization, push/PR or overall completion follows.

**Reset-owned epoch margin refinement:** before adding buffered loader services,
remove only the duplicate MMIO epoch tracker in the actual wrapper. Its existing
always-on recovery controller already latches runtime epoch and changes it only
with global reset or real safe runtime reset. Static `EPOCH_RESET_COUPLED=1` uses
that authority; standalone MMIO defaults to 0 and retains independent epoch-change
tracking. Never enable the coupled mode for an independently changing epoch source.
No controller, epoch width, startup gate, transport owner or acceptance criterion
was removed or reduced. Runtime reset still poisons every old half/callback and
clears local selection/ACK authority; only global reset drops copied R/B.

The fifth actual-MMIO test changes the boot seed while epoch 1 is live: seed 99
cannot become request authority, prevent a real epoch-1 group or change runtime
epoch. It observes the actual 1→2 transition under reset, with old EPOCH=1 RDATA
and recovery BRESP held through fresh startup. Global reset with seed zero then
proves a later seed change alone cannot release startup; another genuine global
reset latches seed 99. All prior four actual and seven standalone MMIO cases pass,
including generic uncoupled epoch changes, both stale-half orders and callbacks.

[Fresh evidence](docs/fpga/command-fifo-implementation/reports/coupled-epoch-519455096/results.json)
contains **16 checksummed files** based on `519455096`. The public profile probe
is byte-identical to the preceding qualified-safety probe: no I/O, capacity,
geometry, clock, delay or service-budget reduction.

| Variant | Partial component LUT / FF | Replacement frontend LUT / FF |
| --- | ---: | ---: |
| Synthesis | 31,140 / 21,656 | 7,461 / 6,767 |
| Standard `opt_design` | 31,035 / 21,628 | **7,457 / 6,767** |

This saves **20 LUT / 32 FF**, leaving **543 LUT / 233 FF**. MMIO is **944 LUT /
387 FF**; kernel remains 1,763 LUT / 1,936 FF / no DSP. Storage remains 62.5
RAMB36-equivalents and four existing DSPs. Runner 0/complete marker pass, zero
errors/critical warnings, 217 synthesis warnings. Unrouted WNS improves slightly
from −9.142 to **−8.997 ns**, still unclosed and worse than the older −5.017 ns
checkpoint. No route, complete frontend or protected whole-design acceptance.

`stage4-coupled-epoch-milestone` passes **206 affected + 55 directed = 261 Rust
tests**, both 3,970-case kernel oracles, warnings/formatting/policy/coverage,
generators and **12 Python tests**. Full Linux passes **3,584 / 11 skipped** in
**405.239 s**, pinned nextest 0.9.116 and isolated owned dummy JACK; default and
optional-reference Wasm builds pass. Native fixture builds and Linux tests remain
sequential. Physical/browser-runtime qualification was not performed.

Preparation/loader/service/optional-trace MMIO is still absent: this refinement
frees measured margin for its storage, not a claim that it is implemented. All
remaining integration/software/timing/platform/delivery gates above remain open.
Production remains ABI 15; Stages 4–7 remain incomplete. No new physical permission,
push/PR, deployment or whole-goal completion follows.

**Configuration-only public-service milestone:** the same MMIO owner now accepts
CORE_CONFIG 1, AUDIO_CONFIG 2 and STORE_CONFIG 3 through the retained request/
response addresses. A single private 16-u32 distributed RAM reserves the paired
request/reply before backend effects. Epoch, full opcode and all twelve payload
words are staged in order, then a nonzero tag publishes once. Incomplete or
out-of-order fields cannot issue a configuration request; writing epoch restarts
an unpublished fill. Full-width/reserved checks precede truncation and grant.
One cursor, one selected RAM read/write and one 160-bit working register preserve
RAM inference rather than adding field masks, a dispatcher or another AXI owner.

- [x] Join candidate configuration service 1/2/3 to the actual SP/JP/store ports,
  with complete private publication, copied replies and public lifecycle tests.
  This is a subset milestone, not completion of the Stage 4 checklist above.

Backend replies are captured without ARM readiness and copied privately over five
clocks. An unread reply reserves this lane only, never audio or semantic result/
snapshot capacity. STATUS bits 0/1 expose request space/response pending; IRQ adds
a complete service reply. Matching POP releases the lane. Accepted RDATA remains
stable across POP/reuse; the existing read channel briefly arbitrates the shared
RAM on POP, without a new transport FSM or processing gate. Runtime reset flushes
local service authority; old AXI halves stay poisoned and copied R/B stay owned
by global transport. Fault/uncertain replies are not cancellation or a fence.

The actual wrapper now owns all **16 configuration inputs** privately through
MMIO; corresponding backend outputs remain observable. The profile probe differs
from the epoch checkpoint only by those 16 declarations and 16 instance bindings.
Every other byte/parameter is checked unchanged: no capacity, clock, delay or
capability reduction. Ordered staging matches the inspected existing submit
ordering, but that client is still ABI 15; matched ABI-16 software and trusted
unused-lease APIs remain Stage 5. No continuing-state initializer or CPU semantic
fallback is introduced. STORE_CONFIG is metadata, **not** bulk media preparation.
Other legitimate preparation/media/lifecycle/trace operations remain required;
this subset's refusals do not remove them from the approved final scope.

Three standalone codec, seven indexed and seven actual-MMIO tests pass. New real
paths cover every unpublished configuration prefix with audio/gain nonmutation,
32-bit scratch and full 160-bit metadata readback, held publication B, unread
replies, held R across POP/reuse, continued rendering, and both stale service-half
orders across actual held-DDR retirement/reset. Existing full-width snapshots,
real fences, retained faults and boot-seed qualification remain covered. Initial
new fixtures incorrectly read write-only audio history-invalidation space 3;
using scratch space 2 fixes them, while explicit space-3 refusal/zero data and
continued rendering remain tested. The failed log is retained; no RTL workaround
or weakened assertion was needed.

[Native evidence](docs/fpga/command-fifo-implementation/reports/config-mmio-606ce161b/results.json)
contains **40 checksummed files** based on `606ce161b`, including four preserved
standalone codec variants, the final integrated source/profile and native reports.
Standalone optimized LUT/FF costs were 548/89, 522/701, 360/221 and 315/175;
these were preflight experiments, not arithmetic proof of integrated fit.

| Variant | Partial component LUT / FF | Replacement frontend LUT / FF |
| --- | ---: | ---: |
| Synthesis | 31,674 / 21,832 | 7,995 / 6,943 |
| Standard `opt_design` | 31,568 / 21,804 | **7,990 / 6,943** |

Packed totals include LUTRAM and exclude only the same four disjoint execution/
media/AXI/program backends. MMIO is **1,030 LUT / 388 FF**, service **315 LUT /
175 FF** including 24 LUTRAM, kernel **1,763 LUT / 1,936 FF / no DSP**. Component
storage remains 62.5 RAMB36-equivalents/four existing DSPs. Runner 0/complete marker
pass, zero errors/critical warnings, 217 synthesis warnings. **Unrouted WNS worsens
from −8.997 ns to −12.580 ns synthesis / −12.116 ns optimized.** No placement/route,
complete frontend or protected whole-design acceptance follows. Only **10 LUT /
57 FF** remain after optimization (5 LUT / 57 FF at synthesis); remaining integration
must still fit, or an actual breach returns to the user under the unchanged rule.

`stage4-config-mmio-milestone` passes **209 affected + 57 directed = 266 Rust tests**,
both 3,970-case kernel oracles, warnings/formatting/policy/coverage, generators and
**12 Python tests**. Full Linux passes **3,584 / 11 skipped** in **405.917 s** with
pinned nextest 0.9.116/owned isolated dummy JACK; default and optional-reference
Wasm builds pass. Fixture builds and Linux ran sequentially. No physical or
browser-runtime qualification was performed. Complete preparation/media/lifecycle/
trace service, external adapters, matched software/CPU-control retirement, admitted
service/whole-design/routed timing/platform qualification, baseline fault parity
and delivery remain open. Production remains ABI 15; Stages 4–7 are incomplete.
No physical authorization, push/PR, deployment or whole-goal completion follows.

**Reply-data mux sharing — area-only refinement:** move only the existing
CORE/AUDIO/STORE response-data selection inside the codec's existing capture/shift
cone. Its single 160-bit working register, phase/cycle count, data widths,
handshakes, response copying and same-edge fault masks are unchanged. Raw
129/32/160-bit replies replace the already-multiplexed internal input; no second
record, owner or controller is added. Standalone tests now drive distinct replies
on all three sources, retaining exact unused/error-data masking and halt/reset
assertions. All seven indexed and seven actual-MMIO cases still pass.

[Fresh evidence](docs/fpga/command-fifo-implementation/reports/reply-mux-b21d0925a/results.json)
contains **16 checksummed files** based on `b21d0925a`. The public probe and complete
profile metadata are identical to the configuration checkpoint: no I/O, geometry,
capacity, clock, delay or acceptance change. All 65 native sources match current
measured files; the same four disjoint backend exclusions include packed LUTRAM.

| Variant | Partial component LUT / FF | Replacement frontend LUT / FF |
| --- | ---: | ---: |
| Synthesis | 31,627 / 21,832 | 7,948 / 6,943 |
| Standard `opt_design` | 31,521 / 21,804 | **7,943 / 6,943** |

This saves **47 LUT / no FF**, leaving **57 LUT / 57 FF** after optimization.
Root glue loses 127 LUT while the codec gains 80 LUT (now 395 LUT / 175 FF), so
larger preflight saving hypotheses were not measured results. MMIO remains
1,030 LUT / 388 FF, kernel 1,763 LUT / 1,936 FF / no DSP; storage remains 62.5
RAMB36-equivalents/four existing DSPs. Runner 0/complete marker pass, zero errors/
critical warnings, 217 synthesis warnings. Unrouted WNS is **−12.579 ns synthesis /
−12.115 ns optimized**, effectively unchanged from −12.116 ns, not timing closure.
The measured 40-level ABORT/fault/service-ready path still needs repair without
timing exceptions or weakened same-edge suppression. No route or complete
frontend/protected whole-design acceptance follows.

`stage4-config-reply-mux-milestone` passes **209 affected + 57 directed = 266 Rust
tests**, both 3,970-case kernel oracles, warnings/formatting/policy/coverage,
generators and **12 Python tests**. Full Linux passes **3,584 / 11 skipped** in
**403.944 s**, pinned nextest 0.9.116/owned isolated dummy JACK; default/optional-
reference Wasm builds pass. Builds/tests ran sequentially, not physical or browser-
runtime qualification. Only configuration 1/2/3 is implemented; complete services,
adapters, matched software/retirement, admitted-envelope/whole-design/route/
platform acceptance, baseline fault parity and delivery remain open. Production
remains ABI 15; Stages 4–7 remain incomplete. No physical authorization, push/PR,
deployment or overall completion follows from this area-only refinement.

**Valid-independent readiness refinement:** remove requester `cfg_valid` only
from the dual-region background-write branches of SP/audio-core `cfg_ready`.
Actual acceptance remains `valid && ready`, so
`valid && (idle || valid && eligible) == valid && (idle || eligible)`.
Ready may be high without valid; it cannot create a request/effect. Actual child
valid/grants, local/processing priority, single-region protection, capture and
same-edge halt masks remain unchanged. No phase, register, width or accepted-
transaction cycle is added or removed.

A new regression covers **eight configurations**: standalone SP/audio core ×
one/two program regions × local/DDR program storage. Held capture checks all four
internal grant probes, full 129/64-bit RAM fingerprints, no backend traffic/
callback/frame advance under valid 0, then actual valid-1 acceptance, original
four-beat DDR payload/response and continued completion. Single-region and
capture/port exclusions remain. Before-fix-v1/v2 were fixture-width errors;
corrected before-fix-v3 fails genuine readiness 0 versus 1. The first surrounding
run exposed a test driver's single-region-only busy/ready assertion. An explicit
dual-region loading driver retains run/job exclusions, payload protocol and queue
bounds; default/single-region guards stay unchanged. No production safety gate
or meaningful effect assertion was removed.

[Fresh evidence](docs/fpga/command-fifo-implementation/reports/config-ready-b1c37e8fd/results.json)
freezes `b1c37e8fd`-based source in **18 checksummed files**. All 65 native source
hashes match; public profile bytes, complete metadata and timing Tcl are identical
to the preceding checkpoint. Packed totals include LUTRAM and the same four
disjoint backend exclusions.

| Variant | Partial component LUT / FF | Replacement frontend LUT / FF |
| --- | ---: | ---: |
| Synthesis | 31,624 / 21,832 | 7,948 / 6,943 |
| Standard `opt_design` | 31,518 / 21,804 | **7,943 / 6,943** |

The three saved LUTs lie in **excluded execution**, not frontend margin: headroom
stays **57 LUT / 57 FF**. MMIO 1,030/388, codec 395/175, kernel 1,763/1,936/no DSP,
62.5 RAMB36-equivalents/four existing DSPs remain unchanged. Runner 0/complete
marker pass, zero errors/critical warnings, 217 synthesis warnings.

Unrouted WNS is **−10.218 ns synthesis / −9.754 ns optimized**, a 2.361 ns
worst-slack improvement, **not timing closure**. Optimized aggregate TNS worsens
to −58,656.453 ns with 12,143 failing setup endpoints. The worst path is now
35 logic levels / 19.204 ns estimated data delay from MMIO ABORT qualification
through faults/service valid to real SP state-RAM write enable. No timing
exception, delayed mask, route or complete frontend/protected whole acceptance.

`stage4-config-ready-milestone` passes **209 affected + 58 directed = 267 Rust
tests**, both 3,970-case kernel oracles, warnings/formatting/policy/coverage,
generators and **12 Python tests**. Full Linux passes **3,584 / 11 skipped** in
**400.699 s**, pinned nextest 0.9.116/owned isolated dummy JACK; default/optional-
reference Wasm builds pass. Builds/tests remain sequential, not physical/browser-
runtime qualification. All remaining services/adapters/software/CPU retirement,
admitted-envelope/whole-design/route/platform/fault-parity/delivery gates remain.
Production stays ABI 15; Stages 4–7 remain incomplete. No physical authorization,
push/PR/deployment or whole-goal completion follows from this refinement.

**Public CLEAR_VIEW functional milestone:** extend the same private service slot
with opcode 4, a full u64 view in payload words 0–1 and ten reserved-zero words.
Complete publication and full-width validation precede a request to the existing
private media loader/accepted-operation owner, using its real CLEAR opcode 2.
No second dispatcher, backend owner, working record, processing gate or live-state
planner is added. The existing 160-bit working record carries the view, and the
same response slot captures completion without ARM readiness. All twelve response
words are zero, including stale unused backend data. CLEAR is not a program fence,
cancellation, backend retirement or permission to clear a continuing lease.

- [x] Join candidate CLEAR_VIEW 4 to the real private loader/owner with complete
  publication, full-width/reserved checks, locally consumed completion and public
  actual-pipeline tests. This is another subset, not Stage 4 completion.

The wrapper now privatizes seven media command/response-ready controls in addition
to the existing sixteen configuration controls. Sample-data interfaces remain
integration seams pending the complete public bulk service; their 32-word private
staging, all other media operations and the full approved profile remain required.
The profile drift check permits exactly those seven additional declarations and
bindings to disappear, preserving every other probe byte/geometry. No fit or final
capability acceptance is inferred from this still-incomplete endpoint.

Four codec, seven indexed and eight actual-MMIO cases pass. The new actual case
clears an unused empty range-view, checks all fourteen unpublished prefixes with
silent audio progressing, held publication B, copied R across POP/reuse, backend
reply release without ARM consumption, full-u64 nonaliasing, reserved-field refusal
and no RPC-driven frame advance. The codec additionally tests all ten reserved
words and eighteen halt positions through validation/issue/wait with late replies,
no replay and reset flushing. Nonempty clear/fence and full hostile-lifecycle
qualification are not established by the empty-view fixture.

`stage4-clear-view-milestone` passes **210 affected + 59 directed = 269 Rust tests**,
both 3,970-case kernel oracles, warnings/formatting/policy/coverage, generators and
**12 Python tests**. Full Linux passes **3,584 / 11 skipped** in **405.368 s**, pinned
nextest 0.9.116/owned isolated dummy JACK; default and optional-reference Wasm
builds pass. All builds/tests ran sequentially. Source-qualified functional
[evidence](docs/fpga/command-fifo-implementation/reports/clear-view-8d60ec301/results.json)
records no new synthesis/route or cycle-envelope acceptance. The prior 7,943/6,943
and −9.754 ns figures remain pre-change measurements, not measured CLEAR_VIEW fit.

Complete bulk preparation/media/lifecycle/trace services, external adapters,
matched software/CPU-control retirement, admitted-envelope/whole-design/route/
platform/fault-parity gates and delivery remain open. Production stays ABI 15;
Stages 4–7 remain incomplete. No physical authorization, push/PR/deployment or
whole-goal completion follows from this functional subset.

**Public optional-trace functional milestone:** the existing MMIO owner now
exposes status/loss/receipt at 0x480–0x48c and an eleven-u32 record at 0x490–0x4b8:
captured epoch, u64 producing revision and the unchanged 256-bit SP event. No
second trace record, dispatcher, backend owner or publication counter is added.
A four-bit read cursor requires the complete ordered prefix before the final AR
can consume the existing immutable source slot. RDATA is copied globally before
source reuse and never waits for ARM RREADY. Header reads can rewind the same held
record; wrong-order/repeated final reads cannot consume a replacement.

- [x] Join the optional public trace reader/loss counter to the actual immutable
  source, retaining unbackpressured mandatory observations and nonfatal overflow.
  This return-stream subset does not complete Stage 4 or matching software.

Runtime reset/incarnation change discards only local partial-read authority,
not accepted global R/B copies. Epoch mismatch refuses consumption; faulted final
reads preserve diagnostic data but refuse source release. Full/stalled readers
only increase the existing saturating u64 loss count. Trace polling/consumption
cannot advance F, free semantic/snapshot reservations, establish a backend fence
or become application Ready; optional trace adds no IRQ dependency. Matching
software must serialize complete record reads and check incarnation/callback
ownership. The actual wrapper alone enables the interface and privately owns
trace-ready; default standalone MMIO leaves the new range unmapped.

The new standalone case covers disabled/range/alignment/order/rewind, full-width
fields/loss, copied R across replacement, all twelve reset-prefix positions,
stale final reads, epoch mismatch and faulted diagnostic retention. The new actual
SP/JP/media/AXI case renders exact samples at each partial prefix and with R held,
checks continuing mandatory pulse F/validity and rising losses, reads a replacement
coherently and preserves copied trace R/public recovery B through real safe reset.

`stage4-trace-milestone` passes **211 affected + 60 directed = 271 Rust tests**,
both 3,970-case kernel oracles, warnings/formatting/policy/coverage, generators and
**12 Python tests**. Eight indexed and nine actual-MMIO cases pass. Full Linux
passes **3,584 / 11 skipped** in **405.651 s**, pinned nextest 0.9.116/owned isolated
dummy JACK; default and optional-reference Wasm builds pass. Builds/tests remain
serialized. [Functional evidence](docs/fpga/command-fifo-implementation/reports/trace-mmio-abff6a9af/results.json)
records no new synthesis/route, current-source fit or admitted-cycle acceptance.
The strict profile guard permits only the extra trace-ready declaration/binding
to disappear; all remaining probe bytes/geometry are unchanged.

Bulk preparation/media/lifecycle services, external adapters, matched software/
CPU-control retirement, full admitted-envelope/whole-design/route/platform/fault-
parity gates and delivery remain open. ABI 15 stays selected; Stages 4–7 remain
incomplete. No physical authorization, push/PR/deployment or whole-goal completion
follows from this functional subset.

**Public bulk-media functional milestone:** candidate service 17 now carries
backend opcode 0–7, full u64 view/start/length, u32 count and four reserved-zero
words in the existing twelve-word payload. Retired CPU opcodes remain refused.
The same single working record grows **160→224 bits** for the complete descriptor;
configuration/CLEAR wire formats, 16-u32 slot RAM, seven phases and five-clock
reply copying stay unchanged. No second dispatcher, operation owner, payload bank
or publication counter is added. Full opcode/reserved/count checks precede grant;
APPEND/READ count is at most 32. Existing backend geometry checks and the Stage 5
trusted unused-lease API retain ownership responsibility.

- [x] Join all public preparation media operations 0–7 through service 17 and
  tag-qualified transfers through the existing private 32-word bank. This does
  not complete lifecycle/adapters, matched software or Stage 4 acceptance.

The aperture at 0x400–0x410 provides status, tag authorization, write/read data
and receipt. One authorization bit compares the existing reserved slot tag, not a
new tag record. An APPEND can occupy private fill after its validated descriptor,
but cannot acquire the store before its whole body is local. READ becomes visible
only after complete successful backend work; accepted AR copies globally and
consumes locally, never on RREADY. Unread data retains only this preparation lane;
POP refuses 7 while data remains. Retired error-prefix data is discarded locally,
never published as success; validation refusal cannot drain another result.
Generic replies contain backend view/empty/read count and zero padding; errors
remain all-zero data/uncertainty. Runtime reset/old halves and same-edge halt reuse
existing authority/masks. A loader FENCE cannot complete the consumer's separately
qualified program-handoff fence. Service/data/polls/POP never advance F.

Four new codec cases cover all eight full-width descriptors, tag/data gates,
malformed/reserved/alias/count refusal, error-prefix discard, nineteen halt
positions and a refused media tag not redirecting an unrelated live configuration
address. Two new actual loader/store/cache/DMA/64-bit-AXI cases cover all fourteen
unpublished header and thirty-two partial body prefixes with exact silent quanta,
held publication/data B, unread replies/data, malformed-access nonconsumption,
zero counts, all backend operations, a real stalled dirty fence and all 32 words in
independent backing memory. Copied final R survives POP/reuse with different new
source data. Nonempty CLEAR_VIEW is now tested. Both old AW/W-half orders retain
copied bulk R while actual DDR retirement precedes epoch 1→2/reset; stale data
cannot acquire the fresh incarnation. These are not authored-JP/all-admitted or
physical qualification; the existing actual JP/playback/capture cases also pass.

`stage4-bulk-milestone` passes **215 affected + 62 directed = 277 Rust tests**,
both 3,970-case kernel oracles, warnings/formatting/policy/coverage/generators and
**12 Python tests**. Eight codec/eight indexed/eleven actual-MMIO cases pass.
Full Linux passes **3,584 / 11 skipped**, **405.549 s**, pinned nextest 0.9.116 and
owned isolated dummy JACK; default and optional-reference Wasm builds pass.
Builds/tests remain serialized. The strict profile guard permits only three more
sample-data input declarations/bindings to become private: 26 configuration/media
inputs plus trace-ready, with all other probe bytes/geometry unchanged.
[Functional evidence](docs/fpga/command-fifo-implementation/reports/bulk-media-69cb80a00/results.json)
records no new synthesis/route, current-source fit or admitted-cycle acceptance.

Lifecycle (including ENABLE_AUDIO 9), external audio/watchdog adapters and complete
hostile lifecycle coverage, matched software/CPU-control retirement, full envelope/
whole-design/route/platform/fault-parity gates and delivery remain open. Production
remains ABI 15; Stages 4–7 remain incomplete. No new physical authorization,
push/PR/deployment or whole-goal completion follows.

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
- [ ] After the complete functional baseline, measure the protected full design,
  present the resource/timing tradeoffs and obtain explicit user agreement on
  final numerical kernel/frontend/whole-design LUT/FF budgets. Do not substitute
  the former interim targets, an invented increase or device capacity alone for
  this acceptance decision.
- [ ] Optimize and synthesize/route the matching full design to meet those final
  budgets and the unchanged BRAM/DSP, routed-timing and total-quantum requirements.
  Compare net resources/timing/cycles with the recorded baseline. Do not call
  kernel-only synthesis or network observation a system timing proof.
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
