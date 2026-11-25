# ABI-15 staged bulk arming — bounded physical pass

A matching protected generated-audio FPGA/FreeRTOS pair passed one supervised
Z-Turn V2 run on 2026-11-25. This exercises the timing fix, not only containment of
the old fault. Factory Linux was restored afterward; no workload remains active.

## Results

| Case | Observed armed plans | Record-and-observe duration | Equal reported capture lengths | Xruns / refusals |
|---|---:|---:|---:|---:|
| 16 loops, extra sync-cycle skip 1 | 33 ms after submit | 3128 ms | 1600 frames for all 16 | 0 / 0 |
| 32 loops, extra sync-cycle skip 1 | 32 ms after submit | 3098 ms | 416 frames for all 32 | 0 / 0 |

For each case the probe recorded the sync loop for approximately one second,
switched it to playback, submitted **one** future-recording group with `skip=1`,
observed the armed plans, then observed all targets recording with equal positive
capture lengths and continuing frame progress. It did not resubmit the group.
The equal-length vectors are client snapshot observations, not an independently
captured hardware start-time trace. The observed arm times include network and
snapshot publication; they are not measurements of the local FPGA commit window.

Each recording-active case was followed by an authenticated backend restart:

| Generation | Ready | Fresh attachment | Old TCP retired |
|---|---:|---:|---:|
| 2 | 309 ms | 320 ms | 2021 ms |
| 3 | 299 ms | 310 ms | 2020 ms |

Both discarded the old session, fenced the old lease and created a fresh persistence
epoch. There was no ARM reboot, owner-failure marker, retained-first-fault log or
ARM-exception marker during the batch/restart test.

The 41 health reports ranged **50.078–50.447°C**, with no sampled overtemperature
alarm. The probe enforced a **65°C cutoff** and rejected telemetry older than three
seconds. Factory pre-staging temperature was 51.923°C; the final factory-Linux
check was **51.308°C**. These are dated measurements, not ongoing monitoring.

## Pair and routed gates

- Mailbox ABI **15**, application protocol **53**, 100 MHz, `xc7z020clg400-2`.
- Pinned Vivado 2021.1; setup WNS **+0.321 ns**, hold WHS **+0.027 ns**, zero failing
  endpoints and routing errors; all user timing constraints met.
- Bitstream: 4,045,674 bytes, SHA-256
  `4f0c995366031c740bf3c3aa25ff2252ebe016047e690841cfbe3406095bb0d1`.
- Credential-bearing firmware stays private; ELF SHA-256
  `a85b428f67a4784f172ae6bbff1d446e44d6983dbbc5fce399980b286c956222`,
  build ID `zturn-batch-dev`, direct-load watchdog enabled, no fault injection.
- Frozen FPGA source snapshot: `e2ff1e882` plus uncommitted batch changes; see the
  source manifest. No changes were made to the frozen hardware inputs during the run.
- Thermal report/XSA identity/CTL0 overtemperature bit verified before programming.
  Live XADC registers retained nominal 75°C shutdown / 65°C release configuration.
- Factory PS/DDR setup and clocks were reused, not replaced by generated PS setup.

A 60-second reset watchdog was armed **before custom PL loading**; the firmware
then installed its existing 30-second startup / 10-second progress-quorum policy.
The final authenticated device reboot was acknowledged and a fresh factory
U-Boot countdown/prompt was captured before factory Linux was entered. The watchdog
was not disabled to enter factory Linux under custom PL. No physical RESET button
was required. Factory `BOOT.BIN` / `image.ub` hashes still matched; separate candidate
FAT files do not alter default boot selection, QSPI or the boot environment.

## Reproduction and limits

The public `src/rust/shoop_host/examples/zynq_batch.rs` contains the supervised
network probe. Its profile, nonce, raw UART, paired ELF and bitstream remain private.
This is a destructive qualification tool, not an automatically replayable test.
The orchestration verifies the pair, factory hashes, clocks, live ABI and thermal
configuration before starting; it captures the final full reset and verifies the
factory return. No new authentication mechanism is introduced.

This is **one bounded run**, not a soak/capacity or standalone-SD qualification.
It does not qualify physical I²S/output correctness, an induced thermal trip,
long recordings, all 32-command mixtures, lost-ACK recovery, arbitrary permanent
MMIO stalls or the historical ABI-13 ARM-hang cause. Immediate-now, single, gain,
mixed and program-switch paths are not converted to staged control by this fix.
The local commit-window counters remain **simulation** measurements, documented in
[the batch contract](../../../../../fpga/host_bridge/control_batch.md).

`result.json`, filtered probe/UART output, routed reports and artifact hashes retain
the evidence. `bitstream-verification.json` is explicitly the pre-programming
checkpoint; `result.json` records the subsequent successful programming and test.
