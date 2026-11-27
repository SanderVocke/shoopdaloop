# Semantic command FIFO implementation milestones

The authoritative scope, approval and acceptance criteria are in
[`FPGA_CMD_FIFO.md`](../../../FPGA_CMD_FIFO.md). The isolated kernel measurements
remain historical evidence in the [investigation report](../command-fifo-investigation/README.md).

**Current gate: Stage 4 integration, not complete fit.** Stages 0–3 are complete;
Stages 4–7 are not. The user explicitly approved a frontend ceiling of **8,000 LUT /
7,000 FF**; all other limits remain unchanged. The indexed range is now joined to
the actual semantic/audio/media/recovery candidate. Its measured partial frontend
is **6,982 LUT / 6,727 FF after standard optimization**, leaving **1,018 LUT /
273 FF** before the remaining low-range service/public and board integration.
Unrouted WNS remains **−5.017 ns**; production still selects ABI 15. The original
stop and repair evidence remain historical records;
the amendment is recorded in [`FPGA_CMD_FIFO.md`](../../../FPGA_CMD_FIFO.md#user-approved-frontend-lut-amendment).

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

At `e883a1d13` the producer was standalone. The following milestone connects
normal capture; root initial-capture admission and complete recovery remain open.
The synthetic result alone was not integrated 8,192/60,000-cycle or resource/
protected-route qualification. ABI/MMIO deployment remains 15.

### Actual settled-snapshot integration

The semantic generator now instantiates the producer. It owns capture-ready and
actual loop-RAM reads while the entire sync prefix is settled; the kernel owns the
same port outside capture. The capture side is read-only. No ARM-controlled ready
input reaches this gate. Snapshot metadata requests outrank diagnostic requests;
a registered accepted-reader tag routes the next-cycle response by its owner,
not the current mux selector. A diagnostic grant just before capture therefore
still completes correctly, while internal metadata cannot leak as its response.
Configuration admission remains excluded during capture. Snapshot packets expose
independent read/ACK/status ports through real word-media and shared-AXI components.

Quantum is latched at render admission, and header F is SP's latched render frame.
The 128-row fixture changes live quantum to 7 during capture but still receives a
Q32 packet and exact 32-frame output. All 128 stamps match F=0 before any JP writes;
render/next cursors and recording render-length projection are distinct. Every raw
word is compared with actual loop RAM. Attempted loader writes during capture
cannot alter it. Pre-gate diagnostic response ownership is explicitly checked.

The next packet retains original f32 bits 1 after a successful subnormal gain maps
to Q8.24 zero. Two unread full packets remain immutable while subsequent quanta
render, with per-attempt sequence gaps and skipped-publication counts. A qualified
ACK permits a later complete F192 packet; duplicate/wrong-epoch ACKs cannot free
it. An older packet remains exact. The first program's ownership completion is
still withheld throughout: snapshot publication/ACK is not a handoff fence.

Actual 128-row capture is **4,619 clocks**, including entry and ready handoff.
Seven directed fixture quanta take **[12644, 12680, 7973, 7973, 7973, 7973, 12590]**
clocks with exact samples. This qualifies this connected local-RAM capture case,
not all-admitted 60,000-cycle, service-envelope or resource/timing acceptance.
The separate actual-RAM 32-target command peak remains 2,950. Its six quanta now
cost **[3301, 3337, 5131, 2182, 2235, 2182]** with capture/full-bank skips included.

The existing two-loop observation test also verifies packet pulse validity at
frame zero, cumulative scheduled failure, subnormal bits and immutable producing
revision across primitive selection. Existing word/AXI media fixtures free one
snapshot bank and read the new 0.5 original gain/selected revision while actual
dirty-cache/DMA fencing and an ARM return remain stalled. Rendering, exact
playback/capture, independently checked backing memory and real AXI fence-error
assertions remain; healthy 41-quantum fixtures take 291–501 clocks each. Selected
revision in a complete snapshot still does not expose reliable fenced success.

Snapshot's retained fault joins immediate consumer suppression and media-enabled
abort. This does not qualify complete non-media/in-flight abort or always-on
recovery/AXI-drain/first-fault integration. Root initial capture is still tied off
(normal packets are phase 1); the standalone phase-0 test is not root admission.

`stage4-snapshot-hookup-milestone.{log,status}` passes **224 selected Rust tests =
193 affected-package + 31 directed**, workspace warnings, formatting, policy/
coverage, generators and Python compilation. `stage4-snapshot-hookup-128-tests`
is passing narrow evidence. The initial `-packets-tests`/`-128-diagnostic` failures
retain an F128 fixture clip overrun: extending its clip keeps the seven distinct
cursor trajectories valid without changing production guards or assertions.

At `734356826` root initial capture and complete fault/reset/recovery remained
open. The next milestone adds initial capture; ABI/MMIO deployment stays 15.

### Root initial capture without processing time

A once-per-reset startup owner now uses the same snapshot producer and private
RAM/metadata service before any processing opportunity. With a known audio-owned
quantum and a reset-latched nonzero incarnation, it publishes phase 0, F=0,
revision 0. It never asks SP to run or consumes a semantic group to produce this
packet. Configuration cannot mutate its held view. Startup release waits only for
bounded hardware capture and local gate cleanup, never ARM reads or ACKs.

The initial revision has no graph members. Physical rows can retain old data,
valid-looking flags and prior observations: ARM bindings determine membership,
not these raw fields. Ignore unused rows before interpreting observations. There
is no blanket RAM clearing, FPGA allocator or snapshot restoration. The trusted
loader must initialize each new lease before publishing an installation.

`initial_capture_has_no_frame_or_reader_dependency_and_reset_preserves_only_raw_rows`
uses all 128 physical rows. While audio geometry is not yet supplied, a pending
first opportunity cannot advance frame 0. After Q32 is known, startup takes
**4,622 clocks**; the producer reports **4,619**, including its entry and ready
handoff. Throughout copying, frame/revision/group-attempt counters remain 0,
no normal settled gate or JP output appears, and configuration is not admitted.
A partial unpublished group never becomes executable. No snapshot reader or ACK
is supplied before startup release.

Three exact-silence quanta at F0/32/64 take **[4662, 45, 45] clocks** with the
initial packet left unread. The first settled packet is independent phase 1/F0;
subsequent full-bank opportunities advance sequence/loss without delaying audio.
The immutable initial packet remains F0/revision 0 while processing reaches F96.
Initial publication consumes sequence 1; normal captures start at 2. Existing
fixture expectations advance accordingly, without resetting or reusing identity.

The idle reset portion deliberately retains initialized-but-unpublished raw loop
and gain data. Epoch 0 flushes control and blocks startup. Changing epoch to 2
without reset cannot fabricate readiness/publication while command-bank identity
is still unlatched. Reset then latches epoch 2; another **4,622-clock** initial
capture copies retained raw data as nonmembers of revision 0. Old epoch-1 ACKs
cannot release its bank even with the same sequence 1. A matching release does
not process time; a subsequent exact F0 quantum costs 4,662 clocks. This is an
idle local-component reset test, not qualification of in-flight media/AXI drain,
always-on recovery, retained first faults or safe ownership reuse.

`stage4-initial-capture-tests.{log,status}` passes the final directed test.
`-tests-owner-declaration` retains the duplicate auto/manual wire declaration
failure; `-tests-epoch-sequence` retains the epoch-only startup/bank mismatch that
led to reset-qualified startup. Earlier `-tests-pre-epoch-guard`/`-audio-tests`
passes predate that final guard. Final `stage4-initial-capture-milestone` passes
**225 selected Rust tests = 193 affected-package + 32 directed**, workspace
warnings, formatting, policy/coverage, generators and Python compilation. Existing
normal 128-row, original metadata, full readers, exact samples, primitive/fence,
word/AXI backing-memory and memory-fault tests still pass; their cycle costs are
unchanged.

At `9c021b241`, complete execution fault-stop and always-on recovery remained open.
The next milestone adds generic execution stop; ABI/MMIO deployment stays 15.

### Generic latched execution fault-stop

Audio and JP now have a generic `abort_run` input independent of media enable.
Each latches the stop until reset. Semantic execution drives it from pipeline
failure, retained consumer/observation/snapshot faults and reported core errors.
The observation source is its retained abort bit: this does not reintroduce an
immediate-fault/event-valid combinational loop. The current legacy media and
job-stream wrappers tie the new input low; standalone default error sequencing
is not silently changed.

On assertion, new run/switch admission, SP writes/emission, sample reads/output,
scratch/output RAM writes, smoothing commit and new job/media/hint issue stop
before the clock edge. The serial SP/JP path cannot consume more queued work.
Unissued read-hint control is flushed, not the RAM holding execution or media.
A pulse remains terminal, so dropping the fault input cannot resume work or
accept another run. New handoff requests are suppressed; already-accepted store/
cache/DMA operations retain their backend ownership. SP still waits for an
accepted external loop response rather than fabricating retirement. An active
local run reports error 8/stage 3 after SP/JP control stops; abort while idle
blocks admission without inventing a completed processing frame.

`nonmedia_fault_stop_suppresses_same_edge_writes_and_never_rolls_back_or_resumes`
uses actual execution RAM, SP, JP and snapshots. Seven injection points are state
start, loop writeback, settled gate entry, partial snapshot copy, input read,
gain streaming and a nonzero output write. Observation-only probes fingerprint
all 16 loop words, sixteen 129-bit state records, 128 scratch words, four 128-bit
smoothing histories and validity. Fault assertion masks the same-edge writes;
the exact pre-edge data remains after a 100-clock traffic attack, without
rollback or post-fault mutation. No production restore or injection API is added.

In these inline/local, non-media fixtures local control retires within **two
clocks**. Already-advanced frame stays 32; earlier state/loop stops stay at 0.
The attempt is not replayed. A held run opportunity, writes, commands and fake
fence success cannot restart it, advance frame/sequence or publish fenced success.
A partial packet stays unreadable even if its RAM contains an older header;
complete F0/revision-1 packets remain immutable diagnostic reads, with ACK
receipt 3 preserving the faulted bank. This is not a memory-fence assertion.

`stage4-nonmedia-stop-tests.{log,status}` passes the original seven-point test;
final `stage4-nonmedia-stop-milestone` also includes the post-fault packet/ACK
assertions. `stage4-nonmedia-stop-legacy` separately passes existing core and
legacy pipeline regressions. The final gate passes **228 selected Rust tests =
193 affected-package + 35 directed**, workspace warnings, formatting, policy/
coverage, generators and Python compilation. Normal 128-row, initial, exact
samples, local 32-target commands and actual word/AXI media/fence/failure tests
remain green with unchanged cycles (command peak 2,950, producer peak 4,619).

Local SP/JP idle does **not** retire an accepted store operation, DMA, loop-read
or external MIDI owner. Their actual response/drain or safe isolation/reset is
still required; neither RAM nor backend/AXI controllers are reset by fault-stop.
At `b4b9790cb`, always-on drain/reset/first-fault integration remained open. The
following milestone adds actual AXI isolation and epoch-qualified runtime reset,
not a successful completion for discarded internal operations.

### Actual AXI isolation, safe drain and fresh runtime epoch

The shared AXI transport already has one serial burst owner. Its master now has
terminal `quiesce`, `quiescent` and retained `drain_fault` interfaces. Quiesce cuts
new command acceptance and cancels CHECK-stage commands with no presented header.
It does not withdraw AW, W or AR already exposed, including independent W-before-AW.
Once quiescing, the master stays isolated until reset even if the input drops.

An active write still requires every original data beat and actual B response.
Missing payload cannot be padded, skipped or called complete. Active reads drain
all actual beats through RLAST even with upstream read-ready low; B likewise drains
without upstream completion-ready. Read/completion callbacks are suppressed during
isolation: no abandoned client receives fabricated success. DMA/cache client state
may remain internally busy; safe reset discards it only after external AXI ownership
is actually retired. Neither local SP idle nor an internal operation's busy flag is
the bus-retirement proof. This is a runtime-retirement/reset protocol, **not** a
successful primitive-program or media fence.

Unexpected/early/duplicate B, unexpected R, wrong IDs and wrong RLAST retain a
protocol/accounting fault. It blocks new commands and forbids quiescent even if
raw inputs subsequently disappear. Legal error responses remain normal error
responses, not unsafe bus accounting. Legacy word/chunk/media wrappers tie quiesce
low; semantic AXI exposes it and drives execution fault-stop from quiesce/protocol
fault. The existing AXI/media/metadata/ownership tests still pass.

The new generated `fifo_recovery_pipeline_axi` wraps real semantic/media/shared
AXI with the existing always-on recovery controller outside runtime reset. A
trusted request must match current nonzero epoch. Quiesce stops SP/JP effects while
AXI continues draining. Zero actual AW/B and AR/RLAST credits, master quiescence
and two quiet clocks precede the sole epoch increment and runtime reset. Defaults
remain 100,000,000 timeout clocks and 128 reset clocks. Reset flushes all old
controller/client identities; raw RAM is not cleared or treated as a continuing
lease. Unexpected B/R during reset are checked outside the reset domain and fail
closed. Unsafe accounting or timeout stays Failed, even after late bus retirement;
there is no automatic retry or successful fence. A held old request cannot match
the new incarnation. The legacy host/MMIO component still selects ABI 15.

Recovery state 3 is **hardware reset complete**, not application/backend Ready.
Stage 5 must still require fresh hardware startup, lease/session and first frame,
and preserve lost-ACK status queries rather than replay. At `82b78edbc`, the wrapper
still lacked the always-on first-fault record and ABI-16 MMIO/ARM adapter. The next
milestone adds the record, not deployment.

`axi_drain.rs` adds four directed tests. Across both 32- and 64-bit buses they check
608 write prefix/AW-W order cases and 272 read-prefix cases, zero consumer readiness,
terminal pulses, unpresented-header cancellation, missing payload and protocol
errors. No alternate transaction controller, shadow payload or production injection
API was added.

`recovery_drains_real_aw_w_and_ddr_fetches_before_flushing_old_runtime_identities`
uses actual 64-bit shared AXI, program upload/prefetch, SP/JP, execution RAM and
packet banks. Held AW, W and DDR fetch AR each prevent reset/epoch change under
run/command pressure. On release, actual bus completion precedes epoch 2/reset;
old fills, queued work and selected-but-unfenced work are not replayed. A fresh
empty revision-0/F0 packet appears without reading/ACKing the old initial packet.
Raw marker `0xfedc_ba98_7654_3210`, valid-looking loop flags and original 0.5 gain
persist only as nonmembers. Stale requests/ACKs cannot acquire/release epoch-2 banks.
The first fresh quantum remains empty at revision 0, rather than executing old work.

`recovery_timeout_protocol_fault_and_unsolicited_reset_responses_never_report_ready`
checks idle unsolicited B, B/R during reset and an accepted AW with stalled W.
The fixture uses 4,096 timeout/4 reset clocks, not enlarged production budgets.
A late WLAST/B can make AXI quiescent after timeout, but recovery remains Failed,
epoch unchanged, run-ready low and frame 0. A reset-edge protocol fault freezes the
already-incremented epoch without ever reaching hardware Ready or permitting reuse.

`stage4-axi-recovery-milestone.{log,status}` passes **246 selected Rust tests =
197 affected-package + 49 directed**, workspace warnings, formatting, policy/coverage,
generators and Python compilation. `stage4-axi-recovery-legacy` independently passes
12 AXI/media/metadata/ownership tests. Narrow logs are `stage4-axi-drain-tests` and
`stage4-axi-recovery-{tests,races}`. Failed `-tests-until-api` (unit-returning
`Sim::until`) and `-races-write-gap` remain separate; the latter fixture now waits
for actual B readiness after all payload beats, not a temporary WVALID gap.

Normal exact samples and command/capture cycles remain unchanged, including the
2,950-cycle actual 32-target command peak and 4,619-cycle 128-row producer peak.
At this AXI-drain checkpoint, retained first-fault wiring and broader hostile
lifecycle/internal SP/JP error parity remained open. The deployed ABI stayed **15**;
no whole-design, platform or delivery pass followed from those selected tests.

## Retained first-fault diagnostics outside runtime reset

`host_fault_record.sv` is the small shared recorder used by the existing mailbox
and new semantic recovery wrapper. It atomically stores the complete 64-bit group
sequence, revision/F and legacy cause/counter/detail fields. Same-epoch ABORT and
later faults cannot overwrite the first record. Epoch zero/older events cannot
replace a newer record; epoch advance without a fault does nothing. Only global
reset clears it, not child runtime reset. The legacy mailbox zero-extends its
32-bit tag, preserving ABI-15 register meanings and read-race checks.

The consumer reports a qualified first-fault event rather than requiring a reader
to infer its owner from current selection. A delayed fence error or blocked
selected fence-result writer names the pending group's original frame/sequence,
even while a later F32 command finishes. Stale epoch/sequence callbacks do not
fault or relabel it. A header-refused group has no decoded opcode; it cannot copy
one left in the previous command packet. No new controller, transaction tracker,
per-loop execution copy, restoration API or ARM-ready signal was introduced.

Semantic audio latches F at the actual opportunity handshake and counts active
local clocks with u32 saturation. With no group, a fault uses active audio F, not
SP's next-frame counter; an idle fault uses the next opportunity. Counters are
context, not a successful-render latency bound. A fence-owned event has zero
command counter rather than borrowing a competing group's duration. The real
media wrapper adds its memory cause. The always-on parent captures actual faults
or accepted recovery (`ABORT|RECOVERY`), suppressing reset/quiesce-generated aborts
that would otherwise mislabel the new epoch. The existing first cause wins.

`first_fault.rs` checks every retained field at full 32/64-bit width, including
nonzero sequence high bits, same/older-epoch immutability, zero epochs, non-event
epoch changes and whole-device reset. `semantic_groups.rs` extends the existing
fault/finalizer race with owner/frame/opcode checks, stale-callback rejection, a
pipeline fault during a selected result write and previous-opcode exclusion on
header refusal. Both WaitingFence retention and explicit failed-fence retention
remain unreleasable, with no successful result after a fault.

`first_fault_keeps_audio_owned_frame_group_context_and_original_cause_through_reset`
uses the actual semantic/media/shared-AXI recovery component. Command and settled
capture faults record epoch-1 F0/cause 64 even after SP reaches 32; recovery, fresh
initial capture, stale submissions and a fresh quantum do not erase/relabel that
record or require a reader. A genuine epoch-2 idle fault records F32 and replaces
it. Existing real held-AW/W/DDR-fetch and recovery failure fixtures also assert
that drain/reset, unsolicited reset responses and late completions preserve the
original record rather than manufacturing an epoch-2 cause.

Final `stage4-first-fault-milestone.{log,status}` passes **248 selected Rust tests =
198 affected-package + 50 directed**, warning-denying workspace build, formatting,
test-policy/coverage, generators and Python compilation. Narrow logs are
`stage4-first-fault-{tests,identity,header-context,finalizer-context}`. Earlier
passing milestones before header/finalizer attribution remain separately named.
`-milestone-header-result-order` retains the fixture's non-head result-read error;
`-finalizer-context-bank-state` retains its incorrect assumption that a pipeline
fault must convert an old WaitingFence bank into an explicit failed-fence state.
Corrected checks retain FIFO release, terminal ownership and no-success assertions.
The finalizer narrow run uses both `sim,rt-interface`; an earlier `sim`-only filter
ran zero cases and is not independent narrow evidence.

The [ABI-16 contract](../../../fpga/host_bridge/semantic_fifo.md) reserves the full
sequence-high diagnostic register; the schema/MMIO/ARM implementation still selects
ABI **15**. This milestone is not public deployment, a whole-design resource/route
or all-admitted quantum proof, fresh full Linux/Wasm, physical or delivery
qualification. Broader lifecycle, the two production-base reference differences
and Stages 5–7 remain. Normal exact samples, 2,950-cycle actual 32-target command
peak, 4,619-cycle capture and 4,622-cycle initial startup remain unchanged.

## Resource gate: measured breach and implementation stop

The broader [Q32 component evidence](reports/pipeline-synthesis-9b8cd12c8/results.json)
is a **failure of the approved resource gate**, not acceptance or deployment.
`measure_semantic.py --pipeline` freezes the production semantic/media/recovery
RTL, a generated fixed-audio profile, exact source hashes and tool configuration.
The profile uses 128 loops, 512 state rows/buffers/views, 1,024 instructions/jobs,
DDR program storage, 32-frame chunks, 124 MiB sample slots and the board's
128 MiB PL arena. Vivado 2021.1 synthesized it out of context for
`xc7z020clg400-2` at 100 MHz. It excludes public MMIO, the audio adapter, PS/DDR,
thermal protection and protected whole-board integration.

| Synthesized scope | Packed LUT | FF | RAMB36-equivalent | DSP |
| --- | ---: | ---: | ---: | ---: |
| Entire component before the excluded integration | 85,056 | 37,785 | 62.5 | 4 |
| Existing SP/JP and execution RAM | 10,626 | 5,945 | 34 | 4 |
| Existing media store | 11,388 | 8,024 | 10 | 0 |
| Existing AXI endpoint and DDR program mux | 1,628 | 922 | 0 | 0 |
| Remaining new frontend/glue before MMIO | **61,414** | **22,894** | 18.5 | 0 |
| Kernel, included in frontend | 1,824 | 1,940 | 0 | 0 |
| Local observations, included in frontend | **55,372** | **17,127** | **0** | 0 |

The kernel satisfies its 2,500-LUT/2,500-FF/no-DSP ceiling. The frontend exceeds
6,000 LUT / 7,000 FF, and this incomplete component already exceeds the protected
whole-design 32,000-LUT / 24,000-FF ceiling. Observations' four arrays were
explicitly dissolved into registers, not RAM. That is the dominant measured
mapping failure, not a proven complete root-cause analysis or permission to
change observation semantics. Even treating the entire observations block as
zero area would leave 6,042 frontend/glue LUTs before MMIO. A RAM-oriented repair
alone is not yet demonstrated sufficient.

Native packed hierarchical utilization supplies the table; raw primitive LUT
counts are different and are not used as packed area. Synthesis completed with
zero errors/critical warnings. The runner then failed a mistaken expected media
instance path and returned **1**, without a complete marker. Its raw partition
output was therefore not used; the packed table reconciles the actual media,
execution, AXI and program-mux hierarchy. Tooling paths were corrected afterward,
without modifying/resynthesizing production RTL or relabeling this run successful.
The archive preserves original Tcl, generated profile, frozen Python tooling,
provenance, native reports, the array-mapping/synthesis outcome and checksums.
Preparation failures in local `vivado-pipeline-9b8cd12c8-v1/v2/v3` retain native
package-order, unresolved `BUFFERS`, and guarded SP-macro width failures; none
produced usable synthesized area. Version 4 produced the area above.

Synthesis-only timing estimates are WNS **−4.584 ns**, TNS −5,906.748 ns with
6,166 / 100,611 failing setup endpoints, and WHS +0.183 ns. No routing was
attempted and no routed slack, cycle-envelope, board or final-delivery claim
follows. The latest 248-test result remains functional evidence for unchanged
production RTL, not fit proof; no fresh full Linux/Wasm or physical test ran.

`stage4-resource-stop-precommit.{log,status}` passes **three offline Python
tooling tests**, Python compilation, register/top/AXI generator checks, all ten
archive checksums, documentation links and whitespace checks. It verifies that
current production RTL matches every measured RTL hash, the corrected tooling
still emits the byte-identical measured profile, the original consumer-only Tcl
is unchanged, and packed frontend accounting reconciles with native hierarchy.
These are record/tooling checks, not additional Rust functional or fit tests.

**Implementation was stopped at `c6ddb7652` pending user direction**, as required
for an actual ceiling breach. The user subsequently requested “try to fix the
area problem,” authorizing the bounded area repair recorded below. That direction
does not change any numerical limit, profile or semantic requirement. No profile reduction, raised ceiling, CPU fallback, ABI-16
selection, board access, push/PR or delivery is implied. The deployed interface
remains ABI 15 and Stages 4–7 remain incomplete.

## Area repair: real RAM ports and sequential working records

[New evidence](reports/area-repair-c6ddb7652/results.json) preserves the original
failed measurement and compares three subsequent variants with the same
Q32/128-loop/512-state-buffer-view/full-media profile. This is a **partial repair,
not a resource acceptance pass**.

Vivado identified multiple write ports in each observation array. Runtime updates
and trusted initialization were mutually exclusive in the protocol, but separate
indexed assignments prevented RAM inference. Each field now has one explicitly
selected address and one write assignment. Required updates still outrank reads
and configuration; gain and SP updates can affect different banks/rows together.
No RAM reset, continuing-state restoration, extra response cycle or reader gating
was added. All original f32, pulse-valid/frame, saturation and optional-trace
semantics remain. The added test exercises all 128 rows with cross-row gain/failure
updates, denied configuration, accepted diagnostic reads, read-before-write
initialization and preservation of the other fields between initialization words.

The complete 12-word command packet shifts only on command-read responses. The
kernel's single 16-word record shifts only on successful read/write responses,
with a fixed-head write-data output. This replaces variable wide register muxes;
it does not add a planner, duplicated record, state or cycle. Frozen prototype
source/evidence remain unchanged.

| Phase | Component LUT / FF | Frontend LUT / FF | Observation LUT / FF |
| --- | ---: | ---: | ---: |
| Original failed synthesis | 85,056 / 37,785 | 61,414 / 22,894 | 55,372 / 17,127 |
| Explicit observation RAM ports | 30,322 / 21,268 | 6,643 / 6,379 | 605 / 615 |
| Plus sequential working records | 30,027 / 21,263 | 6,348 / 6,374 | 605 / 615 |
| Same checkpoint, standard `opt_design` | 29,923 / 21,235 | **6,345 / 6,374** | 605 / 615 |

The 16,512 observation data FFs become **258 LUTRAMs**, included in the frontend
LUT total rather than excluded from accounting. The kernel is **1,595 LUT /
1,932 FF / no DSP**, within its ceiling. Component storage stays at 59 RAMB36 +
7 RAMB18 = 62.5 equivalents with four existing DSPs. No capacity/profile reduction
or limit increase was used. Native packed hierarchy, not raw LUT primitive counts,
reconciles frontend versus existing execution/media/AXI/program backend in every
variant. Source hashes, changed RTL snapshots, the identical generated profile,
Tcl/provenance, native reports and 23 checksums accompany the results.

`stage4-area-repair-milestone.{log,status}` passes **249 selected Rust tests =
199 affected-package + 50 directed**, warning-denying workspace build, formatting,
test-policy/coverage, generators and the three offline measurement-tool tests.
The two kernel tests each retain **3,970 differential cases** and their original
73/65/35/99-cycle successful paths. All nine group tests pass; maximum synthetic
iteration/fence arbitration stays **3,783 clocks**. Actual SP/JP, initial/128-row
snapshots, local observations, word/AXI media fences, stop/reset and retained
first-fault fixtures are included. No tests/oracle assertions were weakened.

Local narrow logs are `stage4-area-observation-ports-tests` and
`stage4-area-streaming-record-tests`. Synthesis runs are
`vivado-area-{observation-ports,streaming-record}-v1`; both complete successfully
with zero errors/critical warnings. `vivado-area-streaming-record-opt-v2` opens
the exact synthesized checkpoint and runs standard optimization, without placement
or routing. Its initial `-opt-v1` used `read_checkpoint` without opening a design;
that failure remains separate and supplied no usable optimization result.

**Historical area outcome:** at this milestone the partial frontend was **345 LUT
over the then-approved 6,000**. The archived results and original ceilings remain
unchanged; the repair did not retroactively pass that gate.

**Subsequent user-approved amendment:** after `90b5c26b3`, the user confirmed
raising only the frontend LUT limit to **8,000**. Frontend FF remains 7,000;
whole-design 32,000 LUT / 24,000 FF, RAM/DSP, cycle, profile and routed-timing
requirements are unchanged. The partial frontend's 6,345 LUT now leaves **1,655 LUT
before public MMIO/integration**; no further reduction is required merely to clear
the old 6,000-LUT ceiling. This is not complete frontend or whole-design acceptance.

**Remaining:** the partial component being below 32,000 LUT / 24,000 FF does not
prove the protected whole design fits. Unrouted estimates still fail setup
(streaming synthesis WNS −5.475 ns; default optimization −5.015 ns), so no routed
timing acceptance follows. Public MMIO, full integration/quantum bounds, the two
baseline fault-path differences, fresh full Linux/Wasm/platform gates, physical
authorization and delivery remain open. ABI deployment remains 15. The amendment
is documentation-only and supplies no new simulation, synthesis or route result.

## Candidate indexed AXI-Lite MMIO component (`a07b89b06`)

[`fifo_mmio.sv`](../../../fpga/host_bridge/rtl/fifo_mmio.sv) now implements the
ABI-16 indexed range **0x150–0x32c**: discovery of semantic capabilities/limits,
full-width live counters, retained fault sequence high, command filling/publication,
FIFO reliable-result reads/releases and independently selected snapshot reads/ACKs.
The matching candidate map is generated from
[`semantic_registers.json`](../../../fpga/host_bridge/semantic_registers.json)
to SV and Rust, separately from the unchanged deployed ABI-15 register schema.
Stream data registers each occupy one u32 word, not a legacy 12-word window.
Three Python tests check generated definitions, invalid maps/constants/receipts
and the unchanged legacy version.

**This is a component, not the complete public endpoint or an ABI promotion.**
The 0x000–0x14c discovery/geometry, loader/service, recovery and retained-record
range is not yet joined to it. The actual semantic audio/media/recovery wrapper,
matching ARM/reference adapter and Stage 5 CPU-control retirement remain pending.
The sequence-high input must be the qualified always-on retained recorder's full
sequence, not the consumer's live attempted-group diagnostic. No default native,
browser, deployed FPGA or audio pacing path selects this component yet.

Transport/ownership properties:

- Capture AW and W independently. Neither half alone issues a backend request;
  accepted payload/address/strobes cannot be relabeled by later bus inputs.
- Record per-half current-incarnation authority and invalidate it across runtime
  reset/epoch changes. Pairing an old half with a new one completes safely with
  epoch receipt 2 and no new-runtime mutation. Epoch change alone cannot make the
  reset-latched command banks accept publication.
- Keep global AXI reset separate from runtime reset. Runtime reset clears private
  selection/cursors/ACK staging and refuses pending old callbacks, but does not
  drop or rewrite an already copied RDATA/BRESP. It does not manufacture successful
  execution, memory retirement or an ownership fence.
- Use one independently outstanding read and write. Packet data copies into the
  response register; RREADY/BREADY never hold RAM, execution or snapshot capacity.
  ACK/reuse and rendering continue while the old response remains unread.
- Do not advance failed/out-of-range reads, wrap a cursor, truncate invalid bank
  selections, or let a previously accepted read advance a newly selected/rewound
  cursor. Unavailable data returns zero/receipt 5, not a copied arbitrary RAM word.
- Preserve OKAY plus receipts for semantic full/epoch/fault/old/unavailable/
  incomplete refusals, including partial packet-data strobes. Malformed register
  accesses retain SLVERR. ACK completion outranks a simultaneous read-status
  update of the same receipt, so an epoch refusal cannot become accepted.

[`semantic_mmio.rs`](../../../src/rust/rt_fpga_rtl/tests/semantic_mmio.rs) has
**seven passing tests**, using actual paired banks/consumer/kernel/snapshot engine
and synthetic local RAM. They check:

1. All **400 packet-word prefixes**, incomplete/partial-strobe invisibility,
   32-command execution while the publish B response is held, complete results,
   cursor exhaustion, discovery/counters and malformed versus semantic refusals.
2. Both AW/W reset orders; already copied R/B held across epoch change/reset;
   private staging reset and epoch-only refusal without processing time advance.
3. Reset between accepted packet/fill request and copied response: no dropped AXI
   owner or new-incarnation effect; abandoned callbacks return epoch refusals.
4. FIFO/non-head visibility behind a selected-but-unfenced switch, select/rewind
   races, qualified synthetic fence completion, ACK/reuse and three later groups
   rendering while old RDATA remains held. This fixture's fence is not real DMA.
5. Faulted diagnostic reads remain available while new admission and result/
   snapshot ACKs refuse with fault receipts and no architectural AXI error.
6. Concurrent result and snapshot data reads cannot hide refused ACK receipts;
   successful ACKs can free storage while the accepted read completes its copy.
7. Both snapshot banks/full-bank skip, held initial-header read across ACK/reuse,
   stale/wrong-epoch ACKs, all **4,880 u32 words / 128 rows** and no read-cursor alias.

`stage4-semantic-mmio-milestone.{log,status}` passes **256 selected Rust tests =
206 affected-package + 50 directed**. This includes both 3,970-case kernel oracles,
all nine group cases and existing actual SP/JP, 128-row/initial snapshots, real
media/AXI fences and stop/recovery/retained-fault regressions. Warning-denying
workspace build, formatting, policy/coverage and AXI/host/register generators pass,
as do **six offline Python tests** (three schema, three measurement). There is no
fresh full Linux/Wasm, whole-design route or physical qualification.

Retained failed evidence is explicit:

- `stage4-semantic-mmio-tests-v1` has two invalid fixtures attempting another AR
  while intentionally holding RVALID. Write-only attack helpers defer receipt
  reads and keep both the original copied-response and backpressure assertions.
- `stage4-semantic-mmio-tests-v2` passes the original four cases, before adding
  further race/reset/fault tests; it is not the final seven-test gate.
- `stage4-semantic-mmio-ack-read-race-v1` reproduces a **real receipt bug**: accepted
  read status 0 overwrote epoch-refused ACK status 2 on the same clock. ACK-response
  priority fixes it without extra states or cycles; the regression remains.
- `stage4-semantic-mmio-tests-v3` passes six directed cases. The complete milestone
  includes the seventh faulted-MMIO case and the broader 256-test matrix.

Narrow reproduction:

```sh
nix develop --command bash -c '
  export SHOOP_FPGA_SOURCE_ROOT="$PWD"
  RUSTFLAGS="-D warnings" cargo test -p rt_fpga_rtl --features sim \
    --test semantic_mmio -- --test-threads=1
'
nix develop --command python3 fpga/host_bridge/test_semantic_registers.py
nix develop --command python3 fpga/host_bridge/generate_registers.py --check
```

**At this component-only checkpoint:** actual audio/media/recovery hookup and a
fresh cost measurement including MMIO remained open. The following milestone
records those steps; the complete low-range service/public endpoint and matching
software still remain incomplete.

## Actual indexed-MMIO/audio/recovery hookup and fresh partial resources

`generate_semantic_mmio.py` generates `rtl/fifo_mmio_pipeline_axi.sv`, also covered
by the normal `generate_top.py --check`. The wrapper joins indexed AXI-Lite to the
actual semantic SP/JP/RAM, observation/snapshot producer, private media owner,
shared AXI and always-on recovery. It introduces no controller or shadow owner:
MMIO drives the existing private packet/ACK/read wires directly.

Critical domain bindings are explicit:

- Transport reset is global reset, not runtime reset; held RDATA/BRESP survive
  safe runtime reset until ARM consumes them.
- Local authority receives the actual recovery-owned runtime reset/incarnation,
  not the initial seed epoch or a synthetic host pulse.
- `FAULT_SEQUENCE_HIGH` receives `first_fault_sequence` from the qualified retained
  recorder, not the consumer's live attempted group.
- Private preparation/configuration and recovery-request inputs are still external
  integration ports. They are **not** a complete 0x000–0x14c public service map.
  The production native/browser/deployed FPGA/audio-pacing path is not promoted.

### Actual-path verification

Two new tests in `tests/support/semantic_mmio_audio.rs`, included by the existing
`semantic_audio` target, use the real 64-bit AXI/media/recovery pipeline and DDR
program storage, with 128 snapshot rows:

- ACK/reuse of a snapshot bank while its old copied sequence word remains held in
  RDATA; three exact silent quanta continue, both banks fill, and the next whole
  opportunity skips without faulting. Readers/preparation do not advance time.
- Initial selection and sample-accurate input/gain/output rendering while publish
  B remains held; successful reliable completion comes through the actual
  JP/cache/metadata/DMA fence, not a test-controlled callback.
- ACK/reuse of the reliable bank with an old sequence word held in RDATA; a complete
  32-command repeated-target gain group executes at one boundary, with all 32
  original-f32 strobes, exact half-gain samples and copied-result immutability.
- Every one of **4,880 u32 words** in a coherent actual 128-row snapshot is read
  through indexed MMIO; original gain, epoch/F/revision/sequence/skip identity and
  cursor exhaustion remain checked.
- Actual-width counter probes seed a high u64 group identity without adding a
  production restoration API. A genuine fault retains it. A held real DDR upload
  AW prevents reset/epoch advance; actual external retirement then permits reset
  and fresh local capture even with ARM R/B held. Retained sequence-high remains
  correct after live diagnostic sequence becomes zero, until a genuine newer-epoch
  fault. Stale admission/ACK cannot acquire or release fresh ownership.

Small-fixture quantum costs are **[4662, 4662, 45, 4962, 6641] clocks**, command
peak **1734**, snapshot peak **4619**. These are not all-admitted service/60,000-cycle
bounds. The older dirty-writeback, unread-loader, failed-fence, 32-target, static
composite, snapshot, stop/reset and qualified-fault assertions remain in the fresh
broader gate.

### Fresh fixed-profile measurement

[Native reports, frozen inputs and results](reports/mmio-hookup-a07b89b06/results.json)
use the same approved Q32/128-loop/512-state-buffer-view profile, 124 MiB sample
arena, Vivado 2021.1, `xc7z020clg400-2`, 100 MHz and 1 ns I/O delays. This working-tree
snapshot is based on `a07b89b06`; its complete source manifest and new generated
wrapper are preserved, with **15 checksummed files**. The verified existing catalog
is mounted read-only; no tool download or board access occurred.

| Variant | Partial component LUT / FF | Frontend including indexed MMIO LUT / FF |
| --- | ---: | ---: |
| Synthesis | 30,664 / 21,616 | 6,985 / 6,727 |
| Standard `opt_design` | 30,560 / 21,588 | **6,982 / 6,727** |

The MMIO leaf is **638 LUT / 353 FF**. Kernel remains **1595 LUT / 1932 FF / no
DSP**; component storage remains **62.5 RAMB36-equivalents** and four existing
DSPs. Packed LUT totals include LUTRAM. Only the same four disjoint existing
execution/media/AXI/program subtrees are excluded, now beneath `dut/pipeline`;
raw primitive partitions are not packed LUT area. Synthesis primitive totals and
post-optimization partitions are labeled separately.

Synthesis and optimization complete with **zero errors/critical warnings**,
runner status **0** and a complete marker. Synthesis reports 217 other warnings.
Unrouted WNS is **−5.481 ns synthesis / −5.017 ns optimized**, WHS **+0.183 ns**.
No place/route or protected-board result is supplied. The remaining **1018 LUT /
273 FF** frontend margin does not increase or prove the independent whole-design
budget. The endpoint still lacks the low-range service and matching software;
this is **not** complete frontend acceptance.

The new opt-in `--pipeline --mmio` freezes this composition, loads the semantic
package first, performs standard optimization and checks the nested accounting
boundaries. Without that option the historical consumer Tcl/default pipeline probe
remain byte-identical. Offline regressions cover both modes and reject `--mmio`
without `--pipeline` before creating output.

### Milestone gates and reproduction

- **258 selected Rust tests = 206 affected-package + 52 directed**, including both
  3,970-case kernel oracles and the two new actual-MMIO tests.
- **11 offline Python tests**, generator drift, coverage/test policy, formatting and
  warning-denying workspace build pass.
- Fresh full Linux: **3584 passed / 11 skipped**, nextest **0.9.116**, **406.205 s**,
  isolated 48 kHz/128-frame dummy JACK and `SHOOP_ALLOW_MISSING_BACKENDS=1`.
- Default application/worklet Wasm and optional `fpga-reference` worklet Wasm builds
  pass. These are compile/dummy-host checks, not browser-runtime or physical-output
  qualification. The two known standalone FPGA fault differentials remain Stage 6
  work, not erased by these passing feature selections.

Local logs/status: `stage4-mmio-hookup-{tests-v1,tooling-v1,milestone,measure-v1,export,linux,wasm}`
under `work/cmd-fifo-investigation/`. Both new directed cases and the new synthesis
run passed on their first attempt; no interrupted/failed run is relabeled.

```sh
nix develop --command bash -c '
  export SHOOP_FPGA_SOURCE_ROOT="$PWD"
  RUSTFLAGS="-D warnings" cargo test -p rt_fpga_testbench --features sim \
    --test semantic_audio mmio_audio -- --test-threads=1
'
nix develop --command python3 fpga/host_bridge/test_semantic_mmio.py
nix develop --command python3 fpga/host_bridge/test_measure_semantic.py
nix develop --command python3 fpga/host_bridge/measure_semantic.py \
  work/cmd-fifo-investigation/vivado-mmio-hookup-rerun --pipeline --mmio --run \
  --catalog work/vivado-zynq/catalog-tree/tools/Xilinx/Vivado/2021.1/data/ip
```

Use a fresh output directory; historical reports/ceilings/outcomes remain unchanged.
**Remaining:** complete 0x000–0x14c discovery/preparation/service/recovery/diagnostic
MMIO, source-specific external audio/watchdog hookup, matching ARM/reference
transports and CPU-control retirement, full admitted-service/lifecycle/route/board
acceptance, the two baseline fault differences, and Stage 7 delivery/audit. Stages
4–7 remain open; ABI 15 stays selected. No physical authorization, push/PR or
whole-plan completion follows from this checkpoint.
