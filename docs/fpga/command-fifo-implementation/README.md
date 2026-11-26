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

The boundary consumer, actual RAM arbitration, MMIO ABI-16 schema/bridge, settled
snapshot collector, local event effects and firmware/reference integration are
still pending. Kernel RAM errors must be promoted to real consumer faults,
including read errors; an ordinary refusal result alone is not sufficient.
Current generated hardware remains ABI 15 with the legacy CPU bridge. No full
Linux/Wasm rerun, fresh integrated resource/timing measurement, board activity,
production semantic FIFO acceptance or delivery is claimed by this milestone.
