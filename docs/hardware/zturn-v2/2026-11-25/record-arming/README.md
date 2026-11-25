# Sixteen-loop recording-arm failure investigation

## Physical observation

The user confirmed the Waypipe UI was working against `zturn-reboot-fpga`, then
reported failure while starting recording on 16 loops, before the recording
boundary. The UART reported `SHOOP_FPGA_OWNER_FAILED FPGA mailbox Fault` between
approximately 92.5 and 102.5 seconds of firmware uptime. ARM memory diagnostics
continued. Initial authenticated management status was `ready=false, busy=true,
closed=false`, and the existing UI connection was still receiving data.

At 10:46:47 CET, the UI process was no longer running, but management still
responded with `ready=false, busy=false, closed=false`. Its exit cause is unknown;
the UI log contains only the initial dirty-tree and EGL warnings. No reset,
restart, firmware replacement, controller takeover or competing data probe was
performed during this investigation.

## Timing defect reproduced offline

The generated synthesis wrapper for the deployed protected bitstream explicitly
instantiates `CORE_CLOCK_HZ=100000000` and `CONTROL_CYCLES=16666`: a **166.66 µs**
control-window deadline. The four host-bridge RTL sources used here are byte-for-
byte identical to their saved bitstream-build source snapshot (digests in
`result.json`). No Vivado build or board programming was needed.

The command path is:

1. `shoop_app_protocol/src/immediate.rs` builds recording transitions for the
   selected loops, including a scheduled first step when synchronization is on.
2. The service maps these to backend commands. `shoop_rt_backend/src/immediate.rs`
   submits the complete immediate command group as one RT batch.
3. `shoop_rt_host_reference/src/fpga/owner.rs::apply` opens one `ControlWindow`,
   processes every command serially, then releases it.
4. `rt_fpga_rtl/src/host/control/window.rs` captures each loop, reads its 16 state
   words through three mailbox reads, computes its plan and writes changed words
   through additional mailbox exchanges. Each exchange itself uses many AXI
   register accesses.
5. `host_dispatch.sv` hard-faults when that held window expires, disables audio,
   and prevents further commands. The owner reports failure and issues ABORT.

The standalone probe exercises the actual RTL and `ControlWindow` implementation
with a 17-loop graph (one sync source plus 16 recording targets). Its smaller
geometry has 64 state values rather than the physical image's 512. Audio remains
disabled and the quantum frame stays zero in **every** case: no recording boundary
or sample-write load is involved. Assertions verify all 20 expected outcomes.

Selected results at the deployed deadline:

| Scheduled skip | Loops | Added delay per register access | Result |
|---|---:|---:|---|
| 0 — next sync boundary | 1 | None | Pass, 12.60 µs |
| 0 | 16 | None | Pass, 150.00 µs; 3,094 register accesses |
| 0 | 16 | 10 ns | Fault before completion |
| 0 | 16 | 100 ns | Fault after 6 complete loop edits |
| 1 — one additional sync cycle | 15 | None | Pass, 166.64 µs |
| 1 | 16 | None | Fault before completion |

The added delays are **synthetic**, not measurements of the ARM or physical AXI
bus. The zero-added-delay cases omit ARM instructions, synchronization barriers,
scheduling and interrupt work. Even the default next-boundary case leaves very
little room for them. Scheduling an additional cycle changes another state word,
which requires more mailbox traffic.

As a diagnostic control, increasing only the simulated deadline to 33,332 cycles
lets the otherwise identical zero-delay 16-loop skip-1 arm succeed at 177.52 µs.
However, even that doubled limit fails with 200 ns of added access delay. This
is not a proposed hardware fix: an audio quantum is only 666.67 µs at 48 kHz and
32 frames, and simply extending the pause could move the failure into audio
starvation/deadline handling.

## Confidence and remaining limits

This confirms a **bulk-command/control-window timing defect** that strongly
explains the reported physical failure. It is not a decoded first-fault cause
from the physical board. ABI 12 exposes one combined FAULT bit for control-window
expiry, audio/memory faults, event overflow, abort and other processing failures;
the running firmware exposes no arbitrary register-read diagnostic. After an
owner failure, its explicit ABORT further obscures the initiating cause. No JTAG
interface is available. Resetting now would discard the existing failed state,
not recover that missing history.

A useful next diagnostic revision would latch the first fault cause before ABORT
and log the current owner operation/group size plus control-window elapsed time.
A robust fix should bound or stage the bulk work outside the live pause, then
commit it with the required common-frame/quantized semantics. Splitting groups
naively could change their timing or leave partial updates; extending or disabling
the watchdog alone is not a sound fix.

## Reproduction

This is an offline RTL investigation program, not a physical-board test. It does
not connect to the MYIR or contain credentials or boot nonces. From the repository
root:

```sh
nix develop --command bash -c '
  cargo build -p rt_fpga_rtl -p rt_fpga_compiler -p rt_fpga_reference \
    -p shoop_rt_protocol --features rt_fpga_rtl/sim,rt_fpga_rtl/rt-interface
  mkdir -p work/record-arming-investigation
  rustc --edition=2024 \
    docs/hardware/zturn-v2/2026-11-25/record-arming/reproduce.rs \
    -o work/record-arming-investigation/reproduce \
    -L dependency=target/debug/deps \
    --extern rt_fpga_rtl=target/debug/librt_fpga_rtl.rlib \
    --extern rt_fpga_compiler=target/debug/librt_fpga_compiler.rlib \
    --extern rt_fpga_reference=target/debug/librt_fpga_reference.rlib \
    --extern shoop_rt_protocol=target/debug/libshoop_rt_protocol.rlib
  work/record-arming-investigation/reproduce
'
```

`simulation-results.txt` records all cases. `result.json` separates physical
observations from simulation and records provenance/limitations. The UART extract
starts after ELF launch so it does not contain the private launch nonce. Full
captures, credential profiles and firmware binaries remain private. No production
code or deployed image was changed by this investigation.
