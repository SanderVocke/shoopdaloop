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

Stage 3 is **not complete**: the versioned FIFO, capability and explicit
quiescence/ownership-lifecycle contract still needs freezing and corresponding
loading/architecture documentation. The ABI-15 CPU control-window/batch bridge is
still present pending Stages 4–5. No production semantic FIFO, fresh integrated
resource/timing measurement, protected route, physical qualification, push or PR
is claimed by this milestone.
