# Z-turn V2 backend restart: partial physical qualification

This directory preserves the historical **ABI-13** attempt and its checkpoint.
A later protected ABI-14/watchdog pair passed failed-owner recovery; see the
[separate follow-up evidence](../fault-containment/README.md) for its results and
latest board/recovery state. Nothing below upgrades this ABI-13 attempt to a pass.

**Overall result: not a complete pass.** Three healthy, recording-active backend
restarts passed. The subsequent 16-loop bulk-arming test lost ARM/management
responsiveness, preventing submission of the failed-owner restart. A safety device
reboot was not acknowledged. Physical RESET was subsequently captured and stopped
at U-Boot for the read-only inspection below. Factory Linux/hash recovery has not
yet been rechecked. Do not automatically replay this workload.

## Tested pair and routed implementation

- Application protocol **53**, mailbox ABI **13**, build `zturn-restart-board`.
- ELF SHA-256: `aac02c8415334b579aca8ca11c16baec1106d2b3dcce76d32e029b6e4f448261`.
- Protected bitstream SHA-256:
  `b3050bd4940f7ccc54cb5300d9e7731064cdd89beda52337cfc66f94377aa1ba`.
- Pinned Vivado 2021.1 SW Build 3247384, `xc7z020clg400-2`, 100 MHz.
- Routed setup slack **+0.201 ns**, hold slack **+0.035 ns**; no routing errors
  or critical warnings. Reports and the frozen FPGA source manifest are included.
- 27,029 LUTs, 18,111 FFs, 83 RAMB36, seven RAMB18, four DSPs, one XADC.
- XSA bitstream identity and CTL0 overtemperature-powerdown bit verified. Live
  XADC thresholds remained nominal **75°C / 65°C**; induced thermal trip untested.

A separate `.bit` and credential-bearing `.elf` were staged on SD. Factory
`BOOT.BIN` and `image.ub` hashes were verified unchanged before/after staging and
before retries. No QSPI, factory boot environment, PS/DDR initialization or clock
configuration was changed. The original failed protocol-52 host was rebooted
through its existing authenticated management path for this authorized test.
Credentials, profiles, nonces, ELFs and full boot/login captures remain private.

## Three healthy restarts passed

The `shoop_host` example `zynq_restart` used real MMIO/IRQ FPGA processing, not a
laptop simulation. It created a recording loop, verified Recording mode and frame
progress, then submitted one management restart operation. Each cycle checked exact
duplicate idempotency, refusal of a conflicting operation carrying the retired
generation, operation/generation identity,
fresh UI lease and persistence epoch, discarded session and continued frame/health
progress after fresh attachment. See the example for the precise assertions.

| Cycle | Ready from request sequence start | Fresh attachment | Old TCP transport closed |
|---|---:|---:|---:|
| 1, generation 2 | 307 ms | 319 ms | 2,019 ms |
| 2, generation 3 | 298 ms | 309 ms | 2,021 ms |
| 3, generation 4 | 294 ms | 305 ms | 2,020 ms |

The old logical service/lease was retired before the fresh attachment. The socket
broker intentionally permits a **two-second TCP close grace**; closure is observed
while polling the new attachment and retaining temperature freshness checks. It is
not part of datapath restart latency. Continuous UART uptime and three
`SHOOP_BACKEND_RESTARTED` markers distinguish these operations from whole-device
reboots. Frame progress was observed for another eleven seconds after each restart.
This exercised recording traffic but did not directly measure outstanding AXI
transaction counts at the instant the drain began.

Across 49 health reports, current temperature was **53.031–53.769°C**, retained
maximum **55.123°C**, no overtemperature indication and no reported xruns. The
65°C cutoff and three-second freshness limit were active, except for a five-second
initial connection acquisition budget. Repeated UART samples returned to Rust
heap usage **3,217,932 bytes**, peak **6,247,156**, with unchanged C free heap
**16,317,128**. This is limited repeated-lifetime evidence, not an exhaustive leak,
physical I²S/audio-output, sustained-load or no-heatsink safety qualification.

Three earlier attempts aborted: one initial connection timeout and two overly
short old-transport closure assertions. Their safety device reboots were
acknowledged and captures retained privately. Investigation also found and fixed
an independent client `Connection::poll` error path which returned Disconnected
without updating client state. A regression test and the host unit suite passed
(**40/40**). The close-grace assertion, not just that state bug, required correction.

## Bulk-arming phase failed more severely

After the three passes, the probe prepared 16 loops and sent one batch of
next-plus-one-sync-cycle recording plans. Its next management Status connection
timed out. The safety full-device reboot attempt also timed out; a later read-only
ping check received zero of two replies. UART diagnostics stopped without an
`SHOOP_FPGA_OWNER_FAILED` marker. Unlike the earlier ABI-12 incident, management
survival was **not** established here. No failed-owner restart was submitted.
The physical initiating fault, CPU exception state and last FPGA register state
were not captured. Do not equate this with the earlier owner-only mailbox fault,
or claim this test demonstrates recovery from it.

### Post-reset saved-PC inspection

At **14:13:08 CET**, the user's physical RESET was captured and autoboot interrupted.
`md.l 0x0050000c 3` read zero for the old image's undefined, prefetch-abort and
data-abort saved PCs. Eight words at its former vector table (`0x00100000`) read
`ffffffff`, unlike the exact preserved ELF. Thus these RAM samples are not a
reliable pre-reset snapshot; no exception PC was recovered. This neither confirms
nor excludes the ARM-abort hypothesis. The factory boot chain may overwrite RAM.

The board was left at U-Boot: no Linux, ELF or workload launch and no memory/storage
writes. Factory hashes after this reset have not been rechecked. The audited small
readout and `saved-abort-inspection.json` are included; the full reset capture stays
private. No further reset was requested or issued for this inspection.

### Additional offline MMIO hazard, not decoded physical first cause

`reproduce-mmio-race.rs` injects FAULT after the driver's request-space check but
before its `REQUEST_PUSH` write, using the same mailbox RTL as this bitstream.
The write receives **AXI SLVERR, BRESP=2**, not a normal `Error::Fault`; no command
executes. The board's register implementation uses volatile MMIO and an
`Infallible` bus error type. A fabric bus error can therefore enter an ARM abort
handler rather than return a recoverable Rust error. This is a plausible route to
whole-ARM unresponsiveness, **not proof** of the observed physical failure.

A follow-up fix should make this asynchronous rejection safe for CPU MMIO, retain
explicit fault status and prohibit command execution. It must test the race at
individual AXI handshakes, including bus stalls, invalid/stale requests and recovery.
Exception/first-fault diagnostics are also needed. This evidence does **not**
implement that fix or the separate staged atomic-batch control-window solution.

The reproducer below characterizes the **frozen ABI-13 source** fingerprinted in
`fpga-source-manifest.json`. The later ABI-14 source intentionally removes this
SLVERR outcome; the historical assertion must fail on fixed RTL. To test the fix,
use `cargo test -p rt_fpga_rtl --features sim,rt-interface --test mailbox_faults`.
The saved output remains evidence of the original hazard, not an ABI-14 result.

Reproduce the historical behavior offline with the matching ABI-13 sources,
from the repository root:

```sh
nix develop --command bash -c '
  cargo build -p rt_fpga_rtl -p rt_fpga_compiler \
    --features rt_fpga_rtl/sim,rt_fpga_rtl/rt-interface
  rustc --edition=2024 \
    docs/hardware/zturn-v2/2026-11-25/backend-restart/reproduce-mmio-race.rs \
    -o work/restart-mmio-race-probe -L dependency=target/debug/deps \
    --extern rt_fpga_rtl=target/debug/librt_fpga_rtl.rlib \
    --extern rt_fpga_compiler=target/debug/librt_fpga_compiler.rlib
  work/restart-mmio-race-probe
'
```

The physical example is destructive and requests one safety device reboot on
failure. It requires the matching private profile and supervised UART/thermal
monitoring; do not run it as a generic connection probe. No clean full-workspace
suite, faulted-owner physical recovery, GUI-click recovery or thermal-trip pass is
claimed. The final warning-denying workspace build, formatting, test-attribute
policy and diff checks passed. QEMU's previously reported version-pin blocker
remains separate.
