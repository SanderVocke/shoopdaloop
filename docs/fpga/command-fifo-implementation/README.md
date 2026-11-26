# Semantic command FIFO implementation milestones

The authoritative scope, approval and acceptance criteria are in
[`FPGA_CMD_FIFO.md`](../../../FPGA_CMD_FIFO.md). The isolated kernel measurements
remain historical evidence in the [investigation report](../command-fifo-investigation/README.md).

## Stage 3: Rust/profile removal and regression milestone (`8972960cc`)

**This is not completion of Stage 3 or implementation of the production FIFO.**
The commit introducing this record removes compiler/reference/ARM/Marlin
composite live-switch preparation, future-cut retries and restart migration.
Compiler admission refuses replacements when either current or candidate layout
contains composites, before reserving physical resources. Reference execution,
hardware `ProgramSession` and Marlin switching independently refuse such edits.
Initial static installation and primitive-only replacement remain supported.
Stopping a composite is not an ownership handoff or permission to replace it.

The reduced-profile tests cover active/candidate composites, introduction/removal,
child edits and otherwise unrelated edits while a composite is installed.
They preserve recorded playback and runtime controls after refusal. Retained
behavior includes authored recording, nested monitoring, shared nested one-shot
re-entry, regular/explicit transport and seek, and primitive recording/playback
across fenced program changes. Native/default-browser implementation and tests
are not retired; the optional worklet `fpga-reference` shares the reduced profile.
Pre-retirement source provenance is recorded in the
[native feature-parity ledger](../../native_session_recovery/feature-parity.md#preserved-evidence-and-retirement-record).

### Verification

All commands ran through `nix develop --command …` on Linux.

- Warning-denying workspace build, formatting, test-attribute policy and host
  register/top generator checks pass.
- All five affected packages pass with their simulation/RT-interface/RTL features
  enabled: **165 Rust tests**, including the kernel test's **3,970 differential
  cases**. The generated cases are included in that test, not 3,970 additional
  Rust tests.
- Full Linux suite: **3,583 passed / 11 skipped**, **395.151 s** reported runtime,
  with nextest **0.9.116**, CI profile and an isolated 48 kHz/128-frame dummy JACK
  server. `SHOOP_ALLOW_MISSING_BACKENDS=1` was enabled; this is not qualification
  of physical host audio devices.
- Default application/worklet Wasm builds and the worklet's optional
  `fpga-reference` Wasm build pass. These are compile checks, not browser runtime
  or physical-output qualification.

Affected-package reproduction:

```sh
nix develop --command cargo test \
  -p rt_fpga_compiler -p rt_fpga_reference -p rt_fpga_rtl \
  -p shoop_rt_host_reference -p shoop_rt_host_marlin \
  --features rt_fpga_reference/rt-interface,rt_fpga_rtl/sim,rt_fpga_rtl/rt-interface,shoop_rt_host_reference/rtl,shoop_rt_host_marlin/sim \
  -- --test-threads=4
```

The complete workspace and Wasm commands follow `.agents/info/test.md`; use its
nextest 0.9.116 requirement. The reused local runner binary's SHA256 is
`d8d3f176f6f9fa2b21edc8a899c99876275d00dd91235a845980ef5ffe35ac58`.
Local verification logs/status files are under `work/cmd-fifo-investigation/`:
`stage3-rust-milestone`, `stage3-linux`, and `stage3-wasm`.

### Interruptions and remaining limits

The first affected-package run exposed a trace-test harness that discarded an
early switch completion while waiting for snapshot configuration. It now retains
that response, waits with a deadline and verifies both switch and control
completions; it does not require an artificial response ordering.

An initial full Linux run with nextest 0.9.143 timed out after 1,200 seconds in
`native_dummy_supports_dynamic_multichannel_and_zero_bus_states`. Host debugger
attachment was refused by ptrace policy; no privilege change was attempted.
The orphaned test and the run's dummy JACK server were cleaned up. Twenty isolated
runs with documented nextest 0.9.116 passed, followed by the complete passing run.
The interrupted log is retained as `stage3-linux-timeout.log`. This does **not**
establish that the runner version caused the hang or erase the interrupted run.
No unrelated native implementation change was made.

At this Rust/profile milestone, RTL preview/handoff controllers, ports and packed
outgoing-span metadata still needed removal; the next section records that work.
The FIFO ABI, capability/lifecycle documentation and production consumer/transport
remained unfinished. There was no new integrated resource/timing measurement,
protected route result, physical test, push or PR.

## Stage 3: RTL/controller and packed-metadata removal milestone

The SP, pipeline and all intervening media/MIDI/AXI/host wrappers no longer expose
or implement the future-switch, conditional-preview, paired-source restart or
outgoing-span handoff paths. The Marlin adapter's former zero tie is removed with
the port. Ordinary static-composite wrap, pulse, iteration/seek, scheduled controls
and primitive replacement with its real memory-completion fence remain.

Compiler `PreviousSpan` and previous/outgoing-span fields, counts, validation and
RTL-target encodings are removed. Composite children require actual current
spans; an empty schedule cannot stand in for a retiring parent. The SP rejects
obsolete child instruction flags and parent-header metadata instead of retaining
a hidden interpretation for them.

The old switch-guard payload fields are reserved and must be zero. Hardware-mailbox
tests exercise **63 individual reserved bits**, including every bit of the retired
guard, and require `RESULT_INVALID`. A normal switch then succeeds at the original
frame/revision. This prevents a legacy guarded request from silently becoming an
immediate replacement. Valid retained-profile encodings keep those bits zero.
This milestone does not introduce or deploy the replacement semantic FIFO ABI.

### Fresh verification

All commands again ran through `nix develop --command …` on Linux.

- **166 Rust tests pass** across the same five affected packages and feature
  matrix shown above. This includes all public static/shared/nested composite,
  primitive replacement, memory-fence, fault and ownership tests, the new empty
  child rejection, hardware-mailbox reserved-field cases and the kernel test's
  **3,970 differential cases**. Changed RTL models were freshly compiled.
- Full Linux suite with pinned nextest **0.9.116**: **3,584 passed / 11 skipped**,
  **401.681 s** reported runtime. The CI profile, isolated dummy JACK server and
  missing-backend allowance are unchanged; this is not physical audio evidence.
- Default application/worklet and optional `fpga-reference` Wasm builds pass;
  these remain compile checks, not browser-runtime qualification.
- Warning-denying workspace build, formatting, test-policy and generated AXI/host
  wrapper/register checks pass. A scoped production RTL/Rust/adapter source audit
  finds no remaining retired controller/type/field names.

Local evidence:

- `work/cmd-fifo-investigation/stage3-rtl-removal.{sh,log,status}`
- `work/cmd-fifo-investigation/rtl-milestone/run-linux.sh`
- `work/cmd-fifo-investigation/rtl-milestone/stage3-{linux,wasm}.{log,status}`
- `work/cmd-fifo-investigation/rtl-milestone/stage3-jack.log`

All three status files are zero. The earlier Rust/profile logs and interrupted
run remain separate historical evidence; the new pass does not establish the
cause of the earlier timeout.

### Remaining limits

At this RTL-removal milestone Stage 3 was **not complete**: ABI, capability and
lifecycle documentation still needed freezing. The following milestone closes
that work, not production integration.

## Stage 3 complete: reduced profile and ABI-16 contract freeze

The [ABI contract](../../../fpga/host_bridge/semantic_fifo.md) specifies four
complete command/result bank pairs, 32-command groups, fixed-cutoff one-group
service, actual-attempt header/deadline checks, per-command results, FIFO reliable
completion visibility, delayed real fencing, independent coherent snapshots and
reset/identity fencing. Local original-gain/failure/pulse observations remove the
need for an ARM response/event reader to maintain snapshot metadata. Their
new-lease initialization remains a loader operation, never continuing-state
restoration. The semantic target requires a complete sync prefix before capture
and audio work, distinct from standalone instruction interleaving.

The source-change implementation rule is refined with explicit evidence: 128
kernel clears at 65 cycles each cannot fit either the 32-command group or
4,096-cycle phase. An execution-owned first-SP-visit source comparison preserves
the prepared reference's existing invalidation behavior. Implementation and
integrated ordering/bounds tests remain future work; no ceiling or acceptance
criterion was weakened.

Architecture, loading, opcode/manual and crate usage docs now describe initial
static composites, primitive-only live replacement, early refusal when active or
candidate contains composites, and the explicit quiesced/new-incarnation lifecycle.
Capability comments distinguish supported-profile replacement and cancellation
of unpublished preparation. CPU-native/default-browser behavior remains separate.
**ABI 16 is specified, not implemented or deployed.** The generated schema and
legacy bridge remain ABI 15 pending Stages 4–5.

### Fresh gates

`work/cmd-fifo-investigation/stage3-contract-verified.{log,status}` records:

- **166 affected-package Rust tests**, with the same full simulation/RT-interface
  feature matrix above, including the **3,970-case** kernel differential.
- **12 directed component tests**: eight graph tests, two target-lowering tests
  and two timing tests. This is **178 Rust tests** in this invocation, not 178
  plus the generated differential cases.
- Warning-denying workspace build, formatting, test-policy, historical coverage
  guard and AXI/host generator checks pass. Retired-name and modified-Markdown
  local-link audits pass separately.

No fresh complete Linux/Wasm run is claimed for this documentation/test-harness
checkpoint; the production removal code retains the preceding milestone's full
Linux/Wasm results. No production semantic FIFO, new integrated resource/timing
measurement, physical test, push or PR is claimed.

### Broader component audit and unresolved baseline differences

DRAMsim3 was built through the documented pinned builder. Component runs select
`SHOOP_FPGA_SOURCE_ROOT` explicitly and use the Z-turn x32 DRAM configuration,
except the native timing unit whose fixed x16 geometry/accounting now explicitly
selects its x16 fixture rather than accidentally inheriting an environment override.

Six failing cases reproduced at untouched production base `ff6c09551`:

- Three graph assertions counted inline schedule rows as executable loop visits.
  Corrected checks retain separate exact instruction-capacity accounting.
- A graph job comparison assumed a total order for independent work. Normalized
  comparisons retain exact payloads, producer-before-consumer/single-writer
  checks and shared-media access order. Negative cases cover unsafe reordering,
  duplicate writes, capture starts, histories, ports, changed payloads and loss.
- A target-capacity fixture omitted a media binding and assumed streaming RTL
  FIFO depth was a total-job batch limit. The repaired test isolates bindings,
  retains the logical batch-budget refusal and tests actual streaming behavior.
- The geometry-specific timing case expected x16 values while inheriting x32.

An early comparison reused a Cargo target across worktrees and an intermediate
run resolved the baseline's compile-time source root. It is not current-RTL
verification. Relevant artifacts were cleaned, current libraries rebuilt and
current runs explicitly select the current root. Later baseline comparisons use
an isolated Cargo target directory. The pinned native DRAM library is shared only
as a generated dependency; baseline source is not edited.

The broad invocation hit the tool's **1,200 s** limit during `pipeline`, after its
completed targets passed. No orphan remained. A subsequent pipeline attempt found
the interrupted model cache without its shared library; preserving/rebuilding that
specific cache produced a passing pipeline run. Bounded follow-ups completed the
remaining targets. Across the 75 component cases, **73 pass and two fail**; this
is not a claim of an uninterrupted or fully passing package invocation.

Both remaining failures reproduce at the production base with the same inputs:
`rust_state_matches_rtl_all_opcodes_partial_failures_and_record_inputs` differs on
preflight queue refusal versus partial SP effects;
`rust_jobs_match_rtl_samples_buffers_ramps_smoothing_and_failures` differs on
streamed job tails retained after JP failure. Their strict assertions remain
unchanged. Resolution is explicit Stage 6 work, without dropping fault coverage.

Additional local evidence under `work/cmd-fifo-investigation/`:

- `stage3-testbench`, `stage3-testbench-with-dramsim`, `stage3-dramsim-build`
- `stage3-pre-removal-graph-compiler`, `stage3-component-graphs`,
  `stage3-component-graphs-current`
- `stage3-components-final` (interrupted), `stage3-component-pipeline` (cache
  failure), `stage3-component-pipeline-rebuilt` (pass)
- `stage3-component-tail` (reference failures), `stage3-component-focused-final`
  (remaining target/timing fixture failures)
- `stage3-pre-removal-reference`, `stage3-pre-removal-targets-timing`
- `stage3-contract-gates`, `stage3-contract-final-gates` (intermediate fixture
  repair failures), `stage3-contract-verified` (final directed/affected pass)

Logs/statuses are retained separately; later passing cases do not erase failures,
prove a cause for unrelated historical timeouts or satisfy the final all-green gate.

## Stage 4 in progress: standalone kernel and packet banks

The production-source `fpga/host_bridge/rtl/fifo_edit_kernel.sv` starts byte-identical
to the frozen prototype (SHA256
`bacf1224ab4cbaaa9f3f4bc3c033dac8b4d9e7c23222a0e58421d43483ada68f`).
The prototype remains frozen. The common harness now runs **two Rust tests**, each
with **3,970 differential cases**, against their independently selected sources.
It writes separate `kernel-cycles.json` and `production-kernel-cycles.json` reports
under `work/cmd-fifo-investigation/`. The production source is not yet on the audio
path. Work-cycle results remain 73 transition, 65 clear/repeat, 35 gain and 99
iteration/seek, excluding packet decode/turnover. No new synthesis is claimed.

`fifo_group_banks.sv` provides four paired input/result banks with BRAM packet
storage, sequential fill/completeness checks, immutable admitted data, exact
publication sequences, independent execution/release cursors and reserved reply
capacity. Only the oldest complete reliable result is visible or releasable.
A waiting synthetic fence retains its bank without blocking later ordinary
execution. Epoch/sequence-qualified completions and ACKs prevent freeing a new
occupant. Result production writes codes then header; incomplete completion fails
closed. No packet RAM reset or full-record shadow banks are introduced.

Four standalone bank tests cover partial publication at every word of a full
32-command packet, partial strobes, malformed/incomplete/gapped publication,
unread/full result banks, payload immutability, FIFO result visibility, duplicate
and stale acknowledgements, delayed fence identity/reset ABA, same-clock
publication/take ordering and incomplete-result failure. These are component
signals and synthetic completions, not real DMA-fence or continuous-audio evidence.
Sequence-exhaustion and additional hostile races remain integration-validation work.

`work/cmd-fifo-investigation/stage4-component-milestone.{log,status}` records **six
passing Rust tests** (the generated cases are inside the two kernel tests), a
warning-denying workspace build, formatting, test-policy and generator checks.
Earlier narrow logs are `stage4-kernel-bootstrap` and `stage4-bank-tests`.

At this initial checkpoint the boundary consumer, actual RAM arbitration,
MMIO ABI-16 schema/bridge, snapshots, local effects and firmware/reference
integration were still pending. The next milestone below connects the standalone
consumer, not the production audio/MMIO path.

### Standalone boundary consumer

`fifo_group_consumer.sv` connects the bank store and shared kernel. Its explicit
processing-opportunity handshake latches a publication cutoff and actual attempt
F, consumes at most one group, preflights all switch positions before executing
anything, and releases a render handshake without ARM acknowledgement. Header
stale/deadline/placement priority and per-command refusals remain separate; later
commands observe preceding writes. State-independent value checks precede target
lookup. No scheduler, graph lookup or per-command full-row shadows are introduced.

Selection updates immutable run parameters together. The selected group's final
result word is withheld until an epoch/sequence-qualified synthetic fence. Other
groups run while it waits; the reliable result head cannot be bypassed. A small
arbitrator lets the earlier fence finish while a later group writes its result.
Headers capture attempt F/revisions before any later group. Gain update outputs
retain original finite f32 bits rather than reconstructing them from fixed point.
Local read/write and fence errors halt/fail closed and retain ownership, including
an error delivered at selection itself; they are not safe `CannotApply` replies.

Eight consumer tests cover 36 mixed repeated-target oracle scenarios; all 400
partial-fill positions with independent processing opportunities; cutoff,
inclusive deadline and header priority; full/unread results; profile refusals;
delayed fence/reset identities; value-before-binding refusals; original gain bits;
RAM/zero-latency fence faults; and concurrent fence/result writes.

| Component scenario | Boundary-to-render cycles at one-cycle local RAM service |
| --- | ---: |
| 16-target arming, skip 0 or 1 | 1,494 |
| 32-target arming, skip 0 or 1 | 2,950 |
| 32 iteration/seek commands | 3,782 |
| Same maximal group with a preceding fence finalizer competing for result RAM | 3,783 |

These include packet decode, turnover and result production, not only sums of
kernel invocations. The peak counter matches measured handshake elapsed cycles.
Both maxima are below 4,096, but this is **not** a proof of SP/JP, DDR, continuous
audio, real DMA fencing or the 60,000-cycle total quantum envelope.

Fresh `stage4-consumer-milestone.{log,status}` passes **179 Rust tests** across the
five affected simulation/RT-interface packages, including two kernel tests with
3,970 cases each, four bank tests and eight consumer tests. Workspace warnings,
formatting, test-policy, coverage and generators pass. Narrow evidence is
`stage4-consumer-{tests,races}` and `production-{group,fence-race}-cycles.json` under
`work/cmd-fifo-investigation/`. `stage4-consumer-tests-width-failure` retains the
initial 4-bit/8-bit comparison warning and consequent missing-model failures;
explicit width extension fixes it. It is not passing evidence or a timing breach.

Actual audio/SP/JP, real JP/cache/DMA fencing, loader/RAM arbitration, settled
snapshot/local SP observation storage, ABI-16 MMIO and firmware/reference
integration remain pending. The probe's configuration access is test-only, not a
production control fallback. Generated hardware remains ABI 15 with the legacy
CPU bridge. No fresh Linux/Wasm, integrated resource/route, physical qualification,
production semantic FIFO acceptance or delivery is claimed by this checkpoint.

### Initial partial consumer synthesis — negative timing retained

The offline Vivado 2021.1/3247384 measurement at source `367a2b69d` targets
`xc7z020clg400-2`, 100 MHz, 1 ns I/O delays. It includes only banks, consumer and
kernel, not the remaining MMIO/snapshot/observation/arbitration frontend or existing
execution/persistent storage. [Results/provenance and checksums](reports/consumer-synthesis-367a2b69d/results.json)
freeze the source/runner/Tcl hashes and native reports.

| Synthesis hierarchy | Packed LUTs | FFs | RAMB36 | RAMB18 | DSP |
| --- | ---: | ---: | ---: | ---: | ---: |
| Partial frontend total | 4,015 | 3,891 | 2 | 1 | 0 |
| Consumer control only | 1,176 | 1,323 | 0 | 0 | 0 |
| Paired banks | 1,009 | 623 | 2 | 1 | 0 |
| Kernel leaf | 1,830 | 1,945 | 0 | 0 | 0 |

Use packed utilization, not the 4,929 raw LUT-primitive count. There are 1,518
observable port bits. The kernel remains within its 2,500 LUT/FF ceilings; the
partial total leaves 1,985 LUTs / 3,109 FFs against the *complete* frontend ceiling,
but absent blocks have not been counted and no net whole-design saving is claimed.

**The unrouted timing estimate fails:** WNS **−2.074 ns**, TNS −237.136 ns,
743/11,084 setup endpoints failing; estimated WHS +0.246 ns. The worst path is
pending-group to bank state/completion/fault control, 24 logic levels. This is
unplaced/unrouted without `HD.CLK_SRC`, not protected whole-design routed closure.
This identified the control-path timing issue addressed by the next milestone;
it was not a pass, Fmax claim or reason to relax routed setup/hold requirements.

Local `vivado-consumer-v2` has status 0 and `complete.txt=passed`; this means the
measurement completed, **not** that timing acceptance passed. `v1` stopped at the
launcher guard before synthesis: an incorrect pin from compacted notes was
replaced by the approved archive's actual launcher SHA
`2924389be0c4297f3e2c4d267e22904a89962575497d0f5fd7eb15dc959e5505`.
The failed startup remains local evidence; no historical archive was changed.
The tool image is pinned/offline with `--pull=never` and no board access.

Reproduce into a fresh directory with:

```sh
nix develop --command python3 fpga/host_bridge/measure_semantic.py \
  work/semantic-consumer-measurement-NEW --run
```

`--route` optionally performs OOC implementation; no route was performed for this
checkpoint. A component route would still not replace the matching protected
whole-design route/total-quantum gate.

### Result-arbitration timing fix and partial OOC route

Address/data arbitration now depends only on the selected local finalizer, not
live fault-validity gating. Live qualified faults still immediately suppress RAM,
render, selection and result effects. This removes the handoff-match → indexed
result-address decode → write-ready → completion dependency without adding a
register, service cycle, controller or shadow row. The same constraints improve
synthesis-estimated WNS from −2.074 to **+0.585 ns**, 24 to 14 logic levels, for
26 additional synthesis LUTs and unchanged FFs. The original negative evidence
above is retained rather than overwritten.

The ninth consumer test exercises a matching fault and stale epoch/sequence faults
at simultaneous finalizer/main-writer arbitration. Matching faults retain both
banks and prevent rendering/result visibility; stale faults do not interfere with
legitimate completion. All **180 affected-package tests** pass, including nine
group/four bank/two 3,970-case kernel tests. Workspace warning-denying build,
formatting, policy/coverage and generators pass. Evidence:
`work/cmd-fifo-investigation/stage4-timing-{arbitration-tests,fault-races,milestone}`.
All prior cycle values, including the 3,783-cycle concurrent-fence maximum, remain
unchanged; the RAM/fence/audio scope has not expanded.

[Native OOC reports and provenance](reports/consumer-route-2cd453c8/results.json)
freeze consumer SHA256 `2cd453c84440c35b225411d64c92390bcf0c56090ac80967450ddd535921dea7`:

| Partial frontend | Packed LUTs | FFs | RAMB36 | RAMB18 | DSP | WNS ns | WHS ns |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Synthesis | 4,041 | 3,891 | 2 | 1 | 0 | +0.585 (estimated) | +0.246 (estimated) |
| OOC route | 4,016 | 3,891 | 2 | 1 | 0 | +0.178 | +0.159 |

The routed kernel leaf is 1,822 LUTs / 1,945 FFs / no BRAM or DSP. There are zero
failing setup/hold endpoints (11,084 endpoints), failed/unrouted/partially routed
nets, critical warnings or errors. Only expected OOC `ZPS7-1` PS7-required DRC
warning remains. Missing `HD.CLK_SRC`, unanchored OOC ports and absent full-system
loads/clock skew prevent interpreting this as board-level timing closure. No
whole-design resource saving or 60,000-cycle total quantum bound is established.
Remaining frontend budget is only apparent headroom: 1,984 LUTs / 3,109 FFs, with
MMIO/snapshot/observations/arbitration still unmeasured. Stage 4 remains open.

`vivado-consumer-v3` synthesized the same RTL; `v4` routed with identical numbers
but emitted a missing-IP-catalog critical warning during DRC reporting. `v5`
mounts the existing verified Vivado 2021.1 catalog read-only and has no such
warning. Catalog PS7 component SHA is validated and recorded; no download or
board access occurred. Reproduce this component route in a fresh directory:

```sh
nix develop --command python3 fpga/host_bridge/measure_semantic.py \
  work/semantic-consumer-route-NEW --run --route \
  --catalog work/vivado-zynq/catalog-tree/tools/Xilinx/Vivado/2021.1/data/ip
```

Generated firmware/RTL still advertise ABI 15. Actual processing/audio, real
JP/cache/DMA fences, MMIO, local observations/snapshots and remaining Stages 4–7
are not delivered by this component milestone. No new full Linux/Wasm, protected
whole-design route, physical qualification, push/PR or final-head CI is claimed.

### Execution-owned SP source-change invalidation

The production SP compares each inline sync source against the source-valid,
physical slot and generation already stored in its packed loop record. It clears
old-source pulse-plan validity before the plan scan, then writes the new source
identity with ordinary SP writeback. A validated external source supplies its
current generation; self and no-source cases use the corresponding canonical
identity. The same operand on later visits does not clear newly armed plans.
There is no new registry, RAM, per-loop controller, SP phase or ARM patch sequence.
Legacy non-inline instructions keep their existing source validation.

Source-bound pulse plans are distinct from frame-absolute plans. The latter
survive source changes, matching the prepared reference's `clear_mode_plan`.
An initial implementation incorrectly used the general primitive clear rule;
`live_regions_preserve_ramps_history_plans_and_frames_with_concurrent_uploads`
caught loss of its scheduled seek and changed samples. The SP now invalidates
only pulse plans. That failed run remains
`stage4-source-milestone-absolute-plan-failure.{log,status}`, not a baseline issue
or passing evidence. The production kernel/general clear operation is unchanged.

Two added sync tests cover **18** combinations of source introduction/removal,
external/self/unchanged operands, stale cached external/self generations, either
or both frame-absolute slots, newly armed plans on the next visit, and all **128**
slots changing no-source to self. Ready pulse plans and natural wraps check that
invalidation precedes scheduled execution; payloads remain unchanged. The broad
case takes **9,991 simulated SP cycles** with local loop RAM, Q32 and all 128 loops
wrapping, no audio work and an always-ready event sink. This is an observed SP
workload, **not** command-phase/snapshot timing or an integrated quantum bound.
It adds no command records; real command/selection/SP ordering remains pending.

Fresh `work/cmd-fifo-investigation/stage4-source-milestone.{log,status}` passes:

- **180 affected-package tests**, including both independently sourced kernel
  differentials, banks/consumer races and public static/primitive-switch tests.
- **22 directed component tests**: graph compiler 8, loading 4, SP sync 8,
  standalone state 1 and smoothing 1. Total **202 Rust tests** in this gate.
- Warning-denying workspace build, formatting, test-policy, historical coverage
  guard and AXI/host register/top generator checks.

Narrow logs are `stage4-source-{tests,pre-scan,absolute-plans}`. The first fixture
run is retained separately as `stage4-source-tests-fixtures`: it omitted the small
model's loop depth and supplied fewer than the reference's four event slots per
loop. Those configuration failures preceded the passing strengthened tests.

Reproduce the directed portion through Nix with the current RTL root and pinned
DRAMsim3 library built as documented in the testbench README:

```sh
nix develop --command bash -c '
  export SHOOP_FPGA_SOURCE_ROOT="$PWD"
  export SHOOP_DRAMSIM3_CONFIG="$PWD/fpga/axi_memory/configs/zturn-v2-ddr3-1066-x32.ini"
  RUSTFLAGS="-D warnings" cargo test -p rt_fpga_testbench --features sim \
    --test sync --test graph_compiler --test loading --test state --test smoothing \
    -- --nocapture --test-threads=2
'
```

Generated hardware remains ABI 15; the consumer is still not connected to the
actual audio/SP/JP and memory fence chain. Loader arbitration, independent
snapshots/local effects, reference/firmware transport changes and all final gates
remain open. There is no fresh resource/route, complete Linux/Wasm, physical,
CI/review or delivery claim for this milestone.

### Internal SP RAM service independent of program-loader ownership

The SP now has a separate internal loop/state access port into its existing RAMs.
Accepted reads/writes receive a one-cycle response independent of the DDR-program
loader's busy and response signals. Internal requests take priority over new
loader requests; an SP run request or active execution takes priority over this
port. Range checks prevent invalid writes, writes return zero data, and reset
invalidates pending responses without resetting RAM or introducing a shadow copy.
This is not an ARM-accessible live-state interface. The current audio wrapper ties
it off until the semantic consumer/capture controller is connected.

`execution_ram_has_one_cycle_service_and_priority_even_with_a_stalled_program_loader`
executes 512 accesses across local-program and DDR-program models. It checks both
RAMs, all 129 state bits, invalid-address refusal/nonmutation, held ordinary
configuration requests, an indefinitely unaccepted program upload, processing
request priority and reset. It checks one-cycle response timing explicitly.

Fresh `work/cmd-fifo-investigation/stage4-local-ram-milestone.{log,status}` passes
**203 Rust tests**: the unchanged 180 affected-package matrix plus 23 directed
component tests (the preceding 22 and the new internal-port test). Warning-denying
workspace build, formatting, policy/coverage and AXI/host generators pass. Narrow
`stage4-local-ram-tests` passes; earlier `-api-failure` and `-settle-failure` logs
retain a wide-bit test setter type error and a missing combinational evaluation
before a priority assertion. Neither is passing evidence or a resource breach.

The existing SP run-admission gates still depend on loader activity, and audio
handoff/media-store readiness still gates rendering. Those production paths have
not yet been replaced. No consumer/SP/audio integration, held-upload audio-progress
proof, ABI-16 deployment, new resource/route or total-quantum bound is claimed by
this internal-port milestone. Snapshots/local effects and Stages 5–7 remain open.

### Dual-region SP admission independent of program-upload ownership

With two program regions, SP run admission no longer waits for program-loader busy
or its response pulse. The prefetch controller preserves an in-flight configuration
transfer across an SP restart, retaining its payload and separately reporting its
result. The next active-program fetch can still require actual DDR service; this
change does not turn an indefinitely stalled memory system into bounded successful
rendering. Single-region configurations retain their existing upload/admission
protection. The one-cycle local-config response condition is unchanged.

The new regression first fails at the original admission gate; its log/status
remain `work/cmd-fifo-investigation/stage4-admission-before-fix`. The fixed test
holds a DDR upload request unaccepted, explicitly runs three empty SP quanta and
checks frames 32/64/96, then completes the upload and verifies all three payload
words plus padding. A fourth quantum starts on the configuration-response cycle.
The same test verifies single-region admission remains blocked through that cycle,
then resumes normally. Upload/request/response activity alone does not advance
frames. These are empty SP runs, not audio/sample or nonempty-fetch qualification.

Fresh `stage4-admission-milestone.{log,status}` passes **204 Rust tests**: the same
180 affected-package tests plus 24 directed components (local RAM/admission 2,
graph compiler 8, loading 4, sync 8, state 1, smoothing 1). Workspace warnings,
formatting, policy/coverage and AXI/host generators pass. Narrow evidence is
`stage4-admission-tests`; all logs are under `work/cmd-fifo-investigation/`.

The semantic consumer is still not connected to the new internal RAM port.
Audio handoff/media-store readiness gates, settled snapshots/local observations,
ABI-16 MMIO/reference/firmware integration and final acceptance remain unfinished.
There is no fresh resource/route, full Linux/Wasm, physical or delivery claim.

### Actual-RAM/audio component connection

`fpga/host_bridge/generate_semantic.py` produces `rtl/fifo_audio_pipeline.sv`,
joining the production banks/consumer/kernel to the actual audio core, SP loop/state
RAM and JP. `generate_top.py --check` now checks it too. This wrapper always uses
two program regions/live selection and enforces at most 128 physical loop slots;
there is no alternate CPU semantic controller in it. The legacy host/media top
has not switched to this module and still advertises ABI 15.

The audio core exposes its internal RAM port only while the whole core is idle,
with processing admission taking priority. Selection can use a distinct pre-run
boundary hook without advancing processing time. Pending handoff does not gate
ordinary core rendering. The existing live-loading differential test now renders
three sample-accurate quanta while its completion is withheld, retaining exact
ramps/history/absolute plans, first-frame identity and unreleased allocator state.
The original admission assertion failure is retained in
`stage4-audio-handoff-before-fix`; `stage4-audio-handoff-tests` passes.

The semantic wrapper requires epoch/sequence-qualified fence events. Successful
events are accepted only once the core requests its handoff; mismatched events do
not acknowledge the core or consumer, while qualified errors remain faults.
Selection refusals drain the core response independently of the reliable reader.
SP events are accepted locally without an ARM-ready input and exported as strobes;
mandatory metadata accumulation and settled snapshots are still pending.

`production_consumer_edits_execution_ram_and_renders_while_results_and_handoff_wait`
executes the new module with actual SP/JP and RAM, not the earlier array probe:

- Initial selection is a semantic group, not a direct switch/config restoration.
- A gain group changes the same quantum's exact samples and emits original f32
  metadata. A 32-command group arms actual loop rows; all have Recording mode,
  capture counter 32 and common SP frame stamp 64 after the quantum.
- Four reserved results remain unread behind a withheld first fence while later
  ordinary groups and empty opportunities render. Continuous refused host requests
  and result reads do not prevent another complete quantum.
- A wrong-sequence success and a stale-epoch failure cannot release or fault the
  current handoff; rendering continues through the stale failure. Matching test
  completion later reveals the original frame-0 result, then ordered later replies.
- Uploads, publication, result reads and acknowledgements do not advance the frame.

The graph is 32 sync visits followed by input/gain/output; it has **no media
recording jobs, cache/DMA/DDR or snapshot work**. Its arm command phase is **2,950
cycles** with the real local port. Six boundary-to-core-done costs are **2,138,
2,174, 5,129, 2,180, 2,233 and 2,180 cycles**. These are a limited local-service
workload, not the all-admitted-graph 60,000-cycle acceptance or physical audio.
Fence completion is test-controlled, not real memory-ownership qualification.

`stage4-semantic-audio-tests` records the initial connection;
`stage4-semantic-audio-fence-identity` adds qualified/stale fence evidence and cycle
output. Fresh `stage4-audio-integration-milestone.{log,status}` passes **205 Rust
tests = 180 affected-package + 25 directed** (graph 8, loading 4, local RAM 2,
semantic audio 1, smoothing 1, state 1, sync 8), plus workspace warnings, formatting,
policy/coverage and generators. Final `stage4-audio-integration-precommit` verifies
the normal top generator hook, workspace warnings/formatting/policy and independently
counts results/checks local Markdown links. Logs are under `work/cmd-fifo-investigation/`.

Real media/cache/DMA fence hookup, media-store admission independence, in-flight
fault/reset integration, observation storage/snapshots and ABI-16/reference/firmware
integration remain open. No fresh complete resource/route, full Linux/Wasm,
physical, push or PR evidence is claimed.

### Private media-service staging and ownership prerequisites

Source inspection exposed two dependencies that cannot be fixed by deleting a
single admission condition. `media_store.sv`'s `SAMPLE_DATA` phase waits for client
write-valid/read-ready; an ARM-stalled upload/download could therefore retain the
shared store after SP admission. `media_audio_pipeline.sv` routes data/done by
current pipeline-busy state, which would misroute a background operation if a new
quantum were allowed to start during it. Neither dependency is acceptable in the
final path. Whole-chunk staging and an explicit accepted-operation owner address
them without changing the approved semantics or ceilings.

`fifo_media_loader.sv` has one private <=32-word bidirectional chunk bank and a
reserved response. APPEND acquires no store service until all words are buffered;
READ copies all returned words before waiting for ARM readback/ACK. Acknowledging
the response early cannot release unread data. Header/body remain immutable while
committed. Invalid chunk sizes refuse before backend mutation; empty reads are
valid; short successful transfers and memory faults latch a fault without replay.
Safe reset invalidates cursors/results without resetting the data RAM. Large user
transfers still require state-independent chunk lowering in the Stage 5 adapter;
this private granularity is not a new public capability restriction.

`fifo_media_arbiter.sv` prioritizes execution and a pending fence over new background
store admission. Once accepted, the job/loader/fence owner controls data and done
routing regardless of later processing-busy changes. It captures fence epoch and
sequence on the actual backend request handshake, preserves them until completion
and does not reissue while the original request stays asserted. Fence failures
remain faults, not reliable success or automatic retry. The final wrapper must
bind this backend to the actual sample-cache/metadata-cache/AXI fence chain.

Five new Rust tests verify all private upload prefixes, stalled/full private
readback, closing-cycle data plus completion, invalid/empty/short/fault/reset
behavior, retained loader ownership while new processing requests arrive, later
job service with loader results unread, and latched fence identity/no duplicate
issue. The backend in these tests is **synthetic**: they do not prove real memory
ownership or actual rendering while media work is stalled.

`stage4-media-loader-tests` records three staging tests;
`stage4-media-ownership-tests` passes all five. Fresh
`stage4-private-media-milestone.{log,status}` passes **210 selected Rust tests =
185 affected-package + 25 directed**, warning-denying workspace build, formatting,
policy/coverage and generators. Logs are under `work/cmd-fifo-investigation/`.

At that checkpoint the units were not connected to a semantic media wrapper or
the legacy host top. The following milestone connects the media component; it does
not switch the deployed host/MMIO path.

### Real media/cache/AXI component connection

`fpga/host_bridge/generate_semantic_media.py` derives the production
`fifo_media_audio_pipeline.sv` wiring from the audio/media interfaces. It combines
the actual semantic consumer/SP/JP, private loader bank, owner arbiter and media
store. `fpga/axi_memory/generate_tops.py` adds
`fifo_media_audio_pipeline_axi.sv`, including the fixed two-region program arena
and existing shared AXI machinery. Normal host/AXI generator drift checks cover
both new outputs; the old generated tops remain unchanged.

The real fence path is now connected: drain prior JP/read-hint ownership, issue
media-store FENCE, flush sample-cache DMA, drain each metadata-cache bank, and wait
for the actual memory/AXI completion. Completion carries the tuple captured at
backend acceptance. Core selection, SP idle, later graph revisions and ARM
readiness cannot manufacture it. Store request/data/completion routing uses the
latched operation owner even when a new quantum starts during loader work.

Rendering's ready/busy/done no longer depend on store busy, ARM loader command
valid, private result ACK or read consumption. New configuration accesses lose to
processing admission; audio/store configuration waits outside the processing
window. Inactive SP program configuration retains its independent dual-region
service. Mandatory local event/gain/failure storage and snapshots are still absent.

`real_media_fences_and_private_loader_streams_do_not_own_audio_admission` in
`tests/semantic_audio.rs` runs one word-memory fixture and a 64-bit AXI fixture with
fixed-latency memory/random ready-valid stalls, plus an AXI error fixture. Healthy
fixtures each check **41 quanta**:

- All 32 partial upload prefixes render without acquiring the actual store.
- Complete uploads no longer need ARM data; unread replies retain only a private
  bank. New processing can start during the accepted loader transaction.
- A real dirty sample-cache writeback response is withheld across rendering and a
  later gain group. The selected switch remains pending, reliable visibility stays
  blocked, and continuous refused FIFO traffic/result reads do not block admission.
- Releasing memory responses completes the real sample/metadata fence; ordered
  reliable replies retain the original selection frame and identity.
- Buffered READ data survives rendering and an early response ACK without holding
  backend service or releasing unread payload.
- Actual JP playback returns the original ramp-scaled samples; actual JP capture
  appends 32 new samples. A subsequent primitive switch/fence persists both sets in
  the independent backing memory (64 exact words), followed by exact suffix readback.

The AXI failure fixture injects a simulated memory error after the held writeback
response. It checks persistent memory fault, no successful fenced result, refused
ACK, no further run admission/replay and unchanged frame **1152**. This is not yet
first-fault MMIO/always-on recovery or in-flight reset/ABA qualification.

The small graph measures **289–499 boundary-to-done cycles**, command peak **91**;
word and AXI healthy traces match. The earlier 32-target actual-RAM test still
measures command peak **2950**. None establishes all-admitted-graph 60,000-cycle,
snapshot 8,192-cycle or complete frontend/whole-design resource acceptance.

Fresh `stage4-real-media-milestone-final.{log,status}` passes **211 selected Rust tests =
185 affected-package + 26 directed**, workspace `-D warnings`, formatting,
policy/coverage and generators/Python checks. `stage4-real-media-{jp,axi,fault}-tests`
retain the directed traces, and `stage4-real-media-tests-{initial-fault,read-length}`
retain invalid READ-length fixture failures; zero logical length legitimately
returns `InvalidView`. All logs remain under `work/cmd-fifo-investigation/`.

The legacy host/media/MMIO deployment still uses ABI 15. New components are not
advertised as ABI 16; software/reference/firmware integration and CPU-control
retirement remain Stage 5 work. Local observations/settled snapshots, complete
fault/reset/recovery integration, final cycle/resources/protected routing and
Stages 6–7 remain open. No fresh full Linux/Wasm, physical or delivery claim is made.

### Standalone local observations and nonfatal optional trace

`fpga/host_bridge/rtl/fifo_observations.sv` implements the specified small per-slot
observational fields in indexed distributed RAM: original f32 gain, saturating
u32 scheduled-failure count, u64 pulse frame and validity. It consumes the existing
SP encoding (kind 2 pulse; kind 3/outcome 3 scheduled failure) and successful gain
strobes without a ready handshake. Independent gain/pulse/failure fields can
update together; this is not a duplicate execution record, source of planning or
new generation/graph registry.

Mandatory updates take priority over the one-cycle private metadata reader and
loader configuration. Initializers match the three-u64 snapshot order and reject
nonfinite gain, nonzero failure/pulse/validity or unused data bits and out-of-range
addresses. Full-width checks prevent truncation aliasing. The trusted adapter—not
a new FPGA allocator—must restrict these writes to unused leases. Reset invalidates
stream/control state without resetting the RAM; every member of a fresh incarnation
must be initialized before publication, not restored from observations.

A separate one-record optional trace captures epoch/revision alongside the event.
The held tuple cannot be relabelled by later revision/incarnation inputs. Full trace
storage drops optional records with a saturating u64 loss count and never blocks
local effects or generates an audio fault. Same-cycle consumption/replacement is
supported. Invalid required source addresses/nonfinite gain strobes fail closed;
legitimate trace overflow and rejected loader initializers do not.

`rt_fpga_rtl/tests/observations.rs` has three tests: all 128 rows with concurrent
updates/blocked loaders/readers and 512 undrained events; real-width u32/u64
saturation; and invalid initialization, address aliases, reset and source faults.
Saturation fixtures use the existing simulation probe facility for near-limit
values, without adding production counter-restoration ports or smaller counters.
The initial optimized-away trace-loss probe alias failure is retained in
`stage4-observation-tests-probe-alias`; the corrected probe targets the real register.

Fresh `stage4-observation-milestone-final.{log,status}` passes **214 selected Rust
tests = 188 affected-package + 26 directed**, warning-denying workspace build,
formatting, policy/coverage and generators. Local logs are under
`work/cmd-fifo-investigation/`.

At this checkpoint the module was not connected to the consumer/SP or CORE_CONFIG
space 3. The following milestone adds that connection; it does not change the
ABI-15 deployment or establish fresh integrated resource or physical qualification.

### Connected actual SP/gain observations and new-lease configuration

The generated `fifo_audio_pipeline` now connects actual SP events and successful
consumer gain strobes to retained observations. Required accumulation has no ARM
or trace-ready dependency. The private metadata reader, optional trace record and
loss/identity fields propagate through the real word-memory and AXI media wrappers.

Configuration space 3 (`cfg_state && cfg_loop`) routes to the three-u64 initializer.
One outstanding configuration latches its response owner; subsequent request flags
cannot redirect a copied metadata response into the program/state path. This small
configuration reservation does not participate in processing admission. The real
trusted ARM unused-lease API/schema selection still belongs to Stage 5; these
component interfaces are not advertised as deployed ABI 16.

`actual_sp_observations_survive_full_trace_readers_and_primitive_selection` uses
real SP/JP and execution RAM, not injected event strobes. Eleven quanta verify:

- Valid frame-zero and continuing self/external pulses; one failed empty scheduled
  Play accumulates once and is not lost when the trace is full.
- Original subnormal f32 gain bits survive a zero fixed-point encoding and affect
  samples in the command's quantum.
- Configuration/private-reader pressure, unread results and delayed first selection
  fencing do not stop processing or advance its frame independently.
- Continuing gain/failure observations survive primitive selection without
  reinitialization. A held revision-1 trace stays labelled 1 while revision 2 runs;
  optional losses reach 33 without an audio fault.
- Configuration responses retain the accepted source when live request flags change.

The real word/AXI fixtures also read original gain from the connected metadata
port while actual dirty-cache/DMA fences and private ARM responses are stalled.
Their backing-memory/playback/capture/fence-failure assertions remain intact.

The first media connection exposed immediate observation fault feeding back through
SP abort into event-valid qualification (`UNOPTFLAT`). Immediate fault still gates
the semantic consumer; media-enabled SP abort now uses the already-retained fault
bit. No warning suppression or new per-slot shadow was introduced. Unit tests check
immediate versus latched status. Complete in-flight/non-media abort, recovery and
reset/epoch integration remain open, not qualified by this wiring repair.

`stage4-observation-hookup-milestone.{log,status}` passes **215 selected Rust tests
= 188 affected-package + 27 directed**, warning-denying workspace build, formatting,
policy/coverage, generators and Python compilation. Directed logs are
`stage4-observation-hookup-{initial,tests,retained-abort}`; initial unused-helper and
feedback failures remain separate. Local evidence is under
`work/cmd-fifo-investigation/`.

Small-graph boundary-to-done costs are 443–533 cycles, command peak 91; the existing
32-target actual-RAM peak stays 2,950. These do not establish the all-admitted total
quantum, snapshot or resource/route bounds. Whole-sync settled capture and two
snapshot banks, complete recovery/reset, ABI-16 host/firmware/reference integration,
full platform/delivery gates and independent final audit still remain. No new full
Linux/Wasm, physical or delivery qualification is claimed.

### Whole-sync settled capture boundary

A parameter-gated SP capture phase now separates the complete sync prefix from
all audio work. The semantic component fixes it enabled; standalone/legacy defaults
preserve their existing independent-work interleaver. In the strict profile, work
encountered before all declared sync visits is a program fault before emitting that
work. Composite schedule rows remain data belonging to their sync visit, not extra
scheduler passes.

The capture frame is latched render F, not the next counter already advanced to
F+Q. Every loop has its processed stamp/render cursor settled before the gate opens.
Zero-sync and empty programs also reach the gate, with no independent frame advance
from reads or polling. This is a normal processing gate, not the still-pending
explicit initial-snapshot publication.

Actual execution RAM grants private one-cycle reads during this phase while SP
stays busy. Writes through that port return an error without mutation. Loader
loop/state writes cannot modify the view; semantic configuration admission is
also excluded. SP resumes before generating audio jobs only after the internal
capture service releases. At this checkpoint that service releases in one clock;
the semantic component does not expose an ARM-controlled capture-ready input.

`settled_capture.rs` has three directed tests: all 128 records over two frames,
zero-sync/empty programs, and strict-prefix refusal versus legacy interleaving.
They check raw render/stamp/next-cursor distinctions, recording render-length
projection, frozen ramp state, no JP writes while held, read-only access under
loader pressure and exact final samples. The 128-row raw read phase is 4,096 clocks;
metadata/header/packet writes are not yet included. The original failed fixture
counted clear-output writes as final samples; that negative log is retained and
corrected expectations keep the strict no-JP-write capture assertion.

`stage4-settled-gate-milestone.{log,status}` passes **218 selected Rust tests = 188
affected-package + 30 directed**, workspace warnings, formatting, policy/coverage,
generators and Python compilation. Directed evidence is
`stage4-settled-gate-tests`; `stage4-settled-gate-tests-output-fixture` is the retained
failure. Semantic quanta gain one hardware clock; the actual-RAM command peak stays
2,950. No fresh resource/route, all-admitted quantum or full platform claim follows.

At the `58b14e85c` gate checkpoint, packet storage/publication and acknowledgements
were still absent. The standalone producer below now supplies them; actual gate
hookup, root initial capture, integrated qualification, complete recovery and
Stages 5–7 remain open.

### Standalone complete-snapshot producer and banks

`fpga/host_bridge/rtl/fifo_snapshots.sv` has two immutable packet banks and one
sequential producer, no scheduler or graph registry. It supports 1–128 physical
rows: 16 raw u64 loop words followed by original gain/failure count, pulse frame
and validity. The eight-u64 header is written last, with magic word zero last of
all; only subsequent complete publication makes a bank readable. The maximum is
19,520 bytes per packet / 39,040 bytes total payload storage, not a measured BRAM
count. RAM is not reset. Header epoch/F/revision/quantum/initial-or-settled phase
are latched before copying; changes to live inputs cannot relabel a packet.

A full pair skips the complete attempt immediately. Sequence advances per enabled
capture opportunity even when capacity causes a skip, cumulative capacity loss saturates at
u64 max and later headers expose gaps. Sequence exhaustion disables further
attempts/publication instead of wrapping or holding the capture gate. No reader
service is needed to finish a capture or release back to execution.

One-cycle indexed u32 reads copy the requested half-word, independently of writer
progress or subsequent bank release/reuse. An ACK must match incarnation, bank and
sequence; either complete bank may be released. Duplicate/old ACKs are idempotently
refused, wrong-epoch/future/not-ready ACKs do not free storage. Capacity is sampled
before a simultaneous ACK, so a full pair still skips that opportunity. A held
capture request does not turn the newly freed slot into an automatic retry.

Reset/epoch change invalidates both banks and prevents an already-held old capture
from restarting in the new incarnation. In-flight RAM/metadata response errors,
a lost capture-valid gate or deadline cannot expose a partial packet. The retained
internal fault blocks further capture until reset. Actual pipeline fault input
also preempts publication on the would-be commit edge. Old complete packets may
remain readable for diagnosis; faulted ACKs do not free them. These are component
properties, not qualification of the complete always-on recovery/AXI drain path.

`snapshots.rs` has five directed Rust tests:

- Every word of both full 128-row packets, continuously denied partial reads,
  initial/settled phases, captured identity and exact original metadata bits.
- Full-bank loss, immutable earlier headers, skipped sequence gaps, delayed and
  out-of-order ACKs, reused-bank and wrong-authority refusals.
- Actual u64 counter saturation/exhaustion using existing simulation force probes,
  not reduced production widths or restoration ports.
- Memory/metadata errors, missing request/response service, lost capture gate and
  same-edge pipeline fault: eight failure/stall fixtures without partial success.
- Completed and in-flight epoch invalidation, copied response across simultaneous
  read/ACK/reuse, and conservative same-edge full-bank ACK with no held-request replay.

For 128 rows and one-cycle synthetic private RAM/metadata service, entry-to-ready
is **4,618 clocks**; peak **4,619 includes entry and the ready handoff**. The fixed
8,192 watchdog budget reserves both turnover clocks, and stall fixtures report
exactly 8,192 at timeout. The initial small-profile elaboration failed on oversized
RAM array indices; sized indices retain independent full-width bounds checks.
No warning suppression, alias acceptance or budget increase was used.

`stage4-snapshot-milestone.{log,status}` passes **223 selected Rust tests = 193
affected-package + 30 directed**, workspace warnings, formatting, policy/coverage,
generators and Python compilation. `stage4-snapshot-tests-final` is passing
narrow evidence; `stage4-snapshot-tests-index-width` retains the initial failure.

The module is not yet instantiated by semantic audio/media/AXI. Its normal capture
must next own the settled gate and private loop/metadata readers without exposing
ARM-controlled readiness; root initial-capture admission also remains pending.
The one-cycle synthetic result does not qualify integrated 8,192/60,000-cycle or
resource/protected-route ceilings. ABI/MMIO deployment remains 15; complete
fault/reset/recovery and Stages 5–7 remain open. No new full Linux/Wasm, physical,
push/PR or delivery qualification is claimed.
