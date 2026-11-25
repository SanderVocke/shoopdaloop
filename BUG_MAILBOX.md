# FPGA mailbox failure during simultaneous recording

## ABI-14 physical follow-up

After commit `1a71e1e45`, a matched protected ABI-14 pair with an opt-in hardware
watchdog passed three healthy backend restarts and one faulted-owner restart.
The 16-loop batch still faulted, but ARM management stayed responsive. Retained
first cause was control timeout (`8`), control cycle 16665; restart generation 5
reached Ready in 301 ms. This confirms that new incident's FPGA cause, not the
old ABI-13 ARM-hang cause. The staged atomic-batch timing fix remains unimplemented.

Controlled CPU freeze/undefined runs qualified watchdog reset through factory
U-Boot; one CRC-valid undefined journal survived. Deterministic faulted/stale PUSH
checks returned explicit rejections without CPU abort. Their harness then
incorrectly disabled its backstop and booted factory Linux under custom PL,
stalling at a factory video-driver probe. The user's subsequent physical RESET
restored factory Linux; factory file hashes matched and die temperature was
48.478°C at the fresh check. No test firmware was loaded or workload replayed.
The original MMIO workflow still failed its return-path gate. See the
[follow-up evidence and latest recovery state](docs/hardware/zturn-v2/2026-11-25/fault-containment/README.md).

## Bug report

On 2026-11-25, the MYIR Z-Turn V2 physical FPGA backend failed when the user
started recording on 16 selected loops. Failure occurred before the recording
boundary. UART reported:

```text
SHOOP_FPGA_OWNER_FAILED FPGA mailbox Fault
```

The ARM/control task and authenticated management endpoint remained responsive.
Initial status was `ready=false, busy=true, closed=false`; the UI TCP connection
was still receiving data. The UI subsequently exited for an undetermined reason;
management continued responding with `ready=false, busy=false, closed=false`.
This is not evidence of a whole-board or Ethernet failure.

Affected build: `zturn-reboot-fpga`, application protocol 52, mailbox ABI 12.
Firmware SHA-256: `0f185a20f0f592dbb71d1b7994561b4e8efe3fdb8a244dfa04645e1d5d76c4c4`.
Protected bitstream SHA-256:
`0ee4640b2739ad17587195c6a2dfb77613c41fbe6e22ca63650097f2b5228ba3`.

## Investigation

The deployed synthesis wrapper sets a 100 MHz core clock and `CONTROL_CYCLES=16666`,
a 166.66 microsecond control-window deadline. The owner opens one window for an
entire immediate command group. It captures each loop, reads its 16 state words,
calculates the transition plan on the ARM and writes changed words back through
multiple mailbox exchanges. Each exchange requires many AXI register accesses.
Audio processing is paused while the window is held.

The actual RTL and `ControlWindow` implementation reproduce a timing defect:

| Case | Offline result |
|---|---|
| One next-boundary recording arm, no added access delay | Pass, 12.60 us |
| Sixteen next-boundary arms, no added access delay | Pass, 150.00 us / 3,094 accesses |
| Sixteen next-boundary arms, 10 ns added per access | Fault before completion |
| Sixteen arms delayed one additional sync cycle, no added delay | Fault before completion |

All 20 expected diagnostic outcomes were verified. Audio remained disabled and the
quantum frame stayed zero. The added delays are synthetic, not measured ARM
latencies; the idealized cases omit CPU instructions, memory barriers, interrupts
and scheduling. The host-bridge sources match the saved deployed bitstream source
snapshot byte for byte.

This confirms a bulk-control-window timing defect and strongly explains the
physical report, but does not decode the physical initiating fault. ABI 12 exposes
only a combined FAULT bit. The owner subsequently sends ABORT, further obscuring
first cause. The running firmware exposes no arbitrary register-read diagnostic.

Evidence and standalone reproduction:
[record-arming investigation](docs/hardware/zturn-v2/2026-11-25/record-arming/README.md).

## Proposed timing fix — separate from backend restart

Stage a bounded batch of compact control intents while audio continues. Validate
identities, graph revision, timing and resources before mutation. Commit the batch
locally in the FPGA at one agreed processing boundary, within a measured bounded
budget, without ARM round trips during the live pause. All selected loops must
retain the same effective frame/sync-boundary semantics. Reject invalid or
unserviceable batches without partial changes.

Do not simply lengthen/disable the watchdog or split a group into independent
updates: the former risks audio starvation and the latter changes timing or leaves
partial results. A 32-frame audio quantum at 48 kHz is only 666.67 us.

Add retained first-fault cause, operation/batch identity and elapsed-cycle
information before issuing ABORT. Include bulk arming near sync boundaries,
invalid-group atomicity, admission limits and worst-case timing in regression tests.

## Backend restart — implementation priority

Provide an opt-in, destructive management operation, independent of session/UI
lease readiness. Reuse existing management authentication unchanged. It must work
when the FPGA owner has failed while the ARM supervisor remains responsive.

### Contract

- Restart the FPGA processing runtime, its owner task, service and queues, not the
  ARM, FreeRTOS scheduler, Ethernet/lwIP, DDR, factory clocks or boot chain.
- Do not reconfigure the bitstream or modify SD, QSPI or factory boot files.
- Discard the active backend session and unsaved recordings in the initial
  implementation; restoration is a separate feature.
- Keep management, health sampling and recovery-operation status alive outside
  the replaceable runtime. Acknowledge acceptance separately from completion.
- Use operation identities and queryable status so a lost reply does not cause a
  second destructive restart. Serialize restart, shutdown, deployment and reboot.
- Capture diagnostics, invalidate old data sessions/leases, stop new FPGA work,
  mask/clear IRQ delivery and quiesce outstanding memory transactions.
- Reset the processing datapath only after a safe AXI drain/fence. Never reset or
  gate the thermal guard/XADC protection.
- Do not free or reuse FPGA-accessible memory until the fence/reset is confirmed.
- Recreate owner/service objects and queues, advance runtime/mailbox identity and
  reject stale work. Require initialization and frame progress before readiness.
- If quiescence or safe task termination cannot be established, fail closed with
  an explicit full-reboot-required result. Do not force-delete arbitrary tasks,
  silently escalate to a full reboot or continue with uncertain memory ownership.

The existing FCLK reset reaches the core, audio and GP interconnect, but a blind
pulse is not a safe general recovery mechanism with AXI transactions in flight.
Use an independent recovery/quiescence controller or an equivalent proven fence
which remains accessible while the datapath is reset. Existing task stop/done
flags and static deadlines must become per-runtime state.

### UI

Expose **Restart backend…** in Host recovery controls and the failed/disconnected
view, plus Host Manager where applicable. Dispatch through management rather than
the failed data channel. Advertise capability and keep unsupported hosts disabled.
Confirm that audio stops and the current session/unsaved recordings are discarded.
Show operation progress/errors; keep the UI open and reconnect only to the fresh,
ready runtime. Expose full device reboot separately, never as silent fallback.

### Validation and deployment boundary

Test healthy and failed-owner restart, conflicting requests, duplicate/lost replies,
stale generations, active AXI traffic, stuck drain/worker, repeated restart resource
usage, preserved thermal state, UI confirmation/reconnection, unsupported hosts and
management availability throughout recovery. Run targeted protocol/service/platform/
RTL/UI tests and warning-denying builds. Physical qualification requires a matching
client/firmware/bitstream and remains a separate step; offline tests must not be
reported as a successful live recovery.

## Implemented restart path

- Application protocol **53** adds capability/status queries and an idempotent,
  generation-checked restart command on the existing management carrier. The
  supervisor retains operation state when the service is replaced. Desktop,
  native-renderer and scripted-register fixture hosts do not advertise restart.
- Mailbox ABI **13** adds `RECOVERY_ID` (`0x114`), `RECOVERY_STATUS` (`0x118`)
  and epoch-checked `RECOVERY_REQUEST` (`0x11c`). The always-on controller stops
  new execution/maintenance, waits for datapath idle and all accepted AXI reads
  and writes to finish, then asserts a bounded processing/audio reset. It retains
  the management aperture and advances the mailbox epoch. Drain timeout, invalid
  bus accounting or traffic during reset cannot report a successful fence.
- The ARM supervisor drops old data connections, cooperatively stops/reaps the
  owner task, obtains the hardware fence, disposes of the old owner, creates new
  queues/backend/service, and requires startup plus frame progress. Failed stop,
  drain or initialization retains uncertain resources, requests cooperative
  quarantine of any new worker, and reports **full device reboot required**.
  It never force-deletes a worker that has not returned or silently reboots.
- Task stop/done/deadline state and IRQ pending state are refreshed for each
  owner lifetime. Render-watchdog admission is suspended for stopped/failed
  workers and restored for a newly started worker; existing whole-system safety
  watchdogs are not removed. Thermal/XADC configuration is never reset. Control-
  task temperature reads continue during recovery; the diagnostic data stream
  resumes only after the fresh UI attachment.
- The main UI offers **Host → Restart backend…**, also available through its
  disconnected-host controls. It confirms loss of the session and unsaved
  recordings, shows progress/errors and creates a fresh client on completion;
  old queued commands and file callbacks are not replayed. A lost acknowledgment
  triggers only status queries, not another destructive command. A newer ready
  generation permits reconnection. Host Manager profile/credential integration
  is not extended in this first implementation.
- CLI: `shoopdaloop --restart-backend --host-profile PATH` submits once and
  polls completion, without automatically restarting or rebooting again.

Offline checks cover protocol/service/client/UI behavior, ordered supervisor
steps and failures, repeated actual mailbox/RTL recovery and stale epochs, AXI
read/write drain and timeout, unexpected traffic during reset, and 32 mocked
FreeRTOS task lifetimes. An ARM firmware build is not a hardware qualification.
The local QEMU smoke attempt is blocked by an emulator pin mismatch (required
10.0.13, installed 10.1.5); its version check has not been bypassed.

A protected ABI-13 bitstream and matching protocol-53 firmware/client/profile were
built and deployed for the authorized board test below. Three healthy restarts
passed; failed-owner recovery is not physically qualified. Initial boot failures
before the management supervisor is established still need the existing boot
recovery path.
The staged-batch timing fix and retained physical first-fault diagnostics above
are deliberately **not implemented** here; repeating bulk arming can still fault.

### Offline validation results

- Service integration suite: **110/110** passed, including duplicate/conflicting
  requests, fresh attachment, operation retention and persistence-epoch fencing
  after project replacement.
- Egui suite: **272/272** passed, including capability gating with a lost data link.
  Targeted desktop recovery-controller, CLI-role and management-client tests passed;
  the desktop-host transport test confirms restart is unsupported without stopping
  the host or acquiring its UI lease.
- RTL mailbox/recovery targets: **8/8** passed; the integration case recovers an
  actual expired control window followed by repeated abort faults and rejects old
  epochs. Controller tests cover outstanding reads/writes, stuck drain, bad bus
  accounting and traffic during reset.
- Protocol, connection and supervisor tests passed. The mocked C task test covers
  **32** lifetimes plus task-creation failure and a non-destructive stop timeout.
  Board-policy, temperature/reset C tests, generated-file checks and all seven
  thermal-policy tests passed.
- `RUSTFLAGS="-D warnings" cargo build --workspace`, browser-target compilation,
  formatting/test-attribute checks, and native/FPGA/fixture ARM firmware builds
  passed. ARM builds retain the existing unstable-NEON and vendor RWX-link warnings.
- No clean full-workspace test-suite pass is claimed. QEMU remains blocked by the
  pinned-version mismatch described above.

### Physical test: healthy restarts pass, bulk-fault recovery blocked

On 2026-11-25, three recording-active restarts passed on the Z-turn V2 using the
protected ABI-13/protocol-53 pair. Ready was observed in **307 / 298 / 294 ms**;
fresh clients attached with new leases/persistence epochs and discarded sessions.
Frame/health progress continued for eleven seconds after each. UART uptime
continued across all three operations. The old logical connection is retired
immediately, but its TCP transport has the broker's existing two-second close
grace; observed closure was **2019 / 2021 / 2020 ms** from restart-sequence start.

The subsequent 16-loop, extra-sync-cycle arming batch lost management, ICMP and
periodic UART diagnostics. There was no owner-failure UART marker. The safety
whole-device reboot was not acknowledged; physical RESET was requested. No
failed-owner restart was submitted. This is a more severe outcome than the
original ABI-12 owner-only failure and does not qualify recovery from that fault.

An additional offline reproducer confirms an MMIO race: FAULT between the
request-space read and `REQUEST_PUSH` produces **AXI SLVERR**, not a normal
mailbox fault return. The physical register bus uses volatile MMIO with an
`Infallible` error type, so this can enter an ARM abort handler. It is a plausible
explanation, not decoded physical first-cause proof. Neither safe asynchronous
rejection nor the retained diagnostics below was present in that physical image.
The staged timing fix also remains pending.

[Checksummed physical results, routed reports and MMIO-race reproducer](docs/hardware/zturn-v2/2026-11-25/backend-restart/README.md).
Temperatures were **53.031–53.769°C**, peak **55.123°C**, with zero reported xruns
before loss of telemetry. Factory boot hashes were unchanged at staging; the
post-failure factory recheck awaits a later factory Linux boot. Physical RESET was
captured at 14:13:08 CET and stopped at U-Boot: all three saved PCs were zero and
the old vector words differed from the preserved ELF. No abort PC was recovered;
these RAM reads cannot prove or exclude the hypothesis. Three short restart cycles
are not sustained-load, thermal-trip or whole-system-failure qualification.

### Fault-containment source follow-up (not deployed)

The next source milestone uses **mailbox ABI 14**, retaining application protocol
53. Valid aligned/full-strobe mailbox register accesses no longer produce AXI
SLVERR for normal fault, queue, stale-epoch or acknowledgement rejection. PUSH
has an explicit submission-result register; the driver checks it and revalidates
epoch. Incorrect acknowledgements do nothing, and driver acknowledgements check
retirement after writing. Malformed/unmapped accesses can still produce bus
errors. No stale/rejected request is permitted to execute.

The dispatcher latches its first cause mask and accepted mailbox tag/op, execution
and control-window cycles, phase/error details, graph revision and frame. The
always-on mailbox retains that record across datapath reset, without letting
later ABORT obscure it. A later runtime's first fault replaces it; intentional
recovery ABORT is annotated. This is bounded mailbox context, not full application
batch tracing. The owner logs the record before its failure ABORT; no new
management diagnostic API has been added.

ARM firmware now installs explicit undefined/prefetch/data-abort handlers. A
64-byte checksummed journal in a reserved end-of-platform page is cache-flushed
and sealed before bounded PS-UART output, then the CPU halts with interrupts
masked. It does not allocate, acquire locks, dereference task names, read PL MMIO
or resume arbitrary faulting instructions. See [journal layout and limitations](platforms/zynq7000/crash.md).
Warm-reset/FSBL preservation is **not guaranteed**, and a hardware-stalled access
can still prevent recording. Hardware-watchdog activation/qualification remains
a separate next step; no new automatic device reset is introduced here.

Focused simulations cover fault/epoch races throughout staging and AXI PUSH,
backpressure, acknowledgement retirement, rejection without execution, retained
records, simultaneous causes, watchdog/pipeline/control faults and recovery.
Mocked actual C handlers cover all three exceptions, blocked UART, recursion,
torn publication, record-bit corruption and independent CRC32 verification.
ARM builds and simulation are not synthesis/timing closure, physical exception
retention or failed-owner recovery qualification. No new bitstream has been
built/programmed for this milestone. The authorized physical RESET/U-Boot
inspection above is complete; the board remains at U-Boot without a workload
relaunch. The staged atomic-batch timing fix remains unimplemented.

Milestone validation: **14/14** focused RTL tests, the actual Marlin mailbox
endpoint test and **2/2** reference-rendering tests passed. All **12** mocked C
exception scenarios passed, including independent CRC32 verification. Native,
FPGA and fixture Cortex-A9 firmware builds passed with the journal at
`0x07fff000`; platform/temperature/reset/task regressions and seven thermal-policy
tests passed. Warning-denying workspace build, WASM compilation, generators,
formatting and test-attribute policy passed. The subsequent pre-commit Linux
workspace gate ran with nextest 0.9.116 and an isolated JACK2 server:
**3583/3583 passed, 11 skipped**. Application and worklet WASM builds also passed.
This does not establish physical ABI-14 qualification or replace the blocked QEMU
and hardware failure-path gates.
