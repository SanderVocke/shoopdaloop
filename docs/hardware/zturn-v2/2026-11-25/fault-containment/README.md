# Z-turn V2 fault containment and recovery — 2026-11-25

## Scope and current state

This follows the [historical ABI-13 partial qualification](../backend-restart/README.md).
The protected bitstream is built from committed `1a71e1e45`; the firmware adds the
**uncommitted opt-in direct-load watchdog**. Application protocol remains 53;
mailbox ABI is now 14. Exact artifact/source digests are in the JSON evidence.
Nothing was installed as a default boot image or pushed to the remote repository.

**Latest board state: recovered by the user's physical RESET, factory Linux
running with no Shoop workload.** Post-reset factory `BOOT.BIN`/`image.ub` hashes
matched; the fresh factory die-temperature check was **48.478°C**. The rearmed
watcher captured the countdown and factory Linux login, but not the earlier
U-Boot banner. Factory VTC version was `6.010`, unlike the custom-image probe's
`5.f5e`. See `result.json` for recovery details and timestamp. No test firmware
was loaded and no failed workload was replayed.

## Passed physical gates

### Independent watchdog and ARM journal

A healthy native run produced 30 health reports with no spurious watchdog reset.
Two more runs collected 30 reports each before controlled CPU freeze or `udf`.
Fresh factory U-Boot banners appeared **10.724 s** and **10.806 s** after the
respective fault markers. These include boot-chain latency, not only timer expiry.

The undefined-instruction UART record passed CRC. The reserved RAM journal read
at intercepted U-Boot matched it exactly; PC **`0x00118dcc`** resolves to
`shoop_watchdog_inject`, `watchdog_fault.S:9`, instruction **`e7f000f0 udf #0`**
in the exact retained ELF. This qualifies one undefined/retention path, not all
aborts or reset modes. Captured CPSR `0x800f019b` has IRQ/asynchronous-abort masks
set but FIQ unmasked despite `cpsid ifa`; this is **not FIQ isolation**.

All 90 pre-fault health reports were at or below **52.539°C**, with no sampled OT.
Supervised probes used a 65°C cutoff and rejected readings older than 3 seconds.
Intentional CPU-fault telemetry loss was followed by bounded reset, not treated as
fresh telemetry. Factory boot files matched afterwards.

### Faulted-owner backend restart

The matched ABI-14/watchdog FPGA pair passed three healthy recording-active
restarts, then one after the 16-loop extra-sync-cycle batch fault:

| Generation | Ready | Fresh attachment | Old TCP retired |
|---|---:|---:|---:|
| 2 | 300 ms | 308 ms | 2021 ms |
| 3 | 302 ms | 310 ms | 2019 ms |
| 4 | 308 ms | 318 ms | 2019 ms |
| 5 — faulted owner | 301 ms | 312 ms | 2017 ms |

UART confirms four service replacements and **no whole-ARM reboot during them**.
The probe verified discarded sessions, new leases/persistence epochs, frame and
health progress, duplicate idempotence and stale/conflicting-operation refusal.
It observed 65 health reports at **52.170–52.908°C**. A final authenticated device
reboot returned to factory Linux; factory hashes matched, temperature 54.015°C.

The retained first fault is **control timeout, cause `8`**, epoch 4, control cycle
**16665**, last accepted tag 55027 / `OP_CORE_CONFIG`, revision 10, frame 618144.
ARM management remained responsive and accepted the failed-owner restart. This
confirms the initiating FPGA fault for this new run, **not the old ABI-13 ARM
hang**. It does not prove that this run exercised the precise FAULT/PUSH race.

**Bulk arming is not fixed:** the live control window still times out. Staged,
whole-validated atomic batches remain separate work.

### Deterministic rejected-MMIO checks

With no host firmware running, a watchdog-backed U-Boot test exercised:

- PUSH while faulted: normal CPU completion, receipt **3 / FAULT**, no response
  pending, original first-fault record unchanged.
- PUSH with retired epoch: receipt **2 / EPOCH**, no response pending or new fault.
- Stale ABORT and recovery request: ignored; live epoch remained 2.

After these checks the harness **disabled its watchdog too early and directly
booted factory Linux with the custom PL still configured**. Linux stalled at the
factory video-timing-controller probe. A serial BREAK + SysRq reset did not
respond. The workflow therefore failed its final-state gate, despite passing the
individual rejection checks. A subsequent physical RESET restored factory Linux;
that separate recovery passed temperature/hash verification without replay. Do
not report the original workflow as passed.

Correct future exit: retain the backstop, issue a full PS reset through the factory
chain, capture U-Boot, then boot factory Linux with its matching PL. Do not repeat
the faulty sequence or replay the workload to recover this stall.

## Offline and implementation evidence

- Warning-denying workspace build and complete Linux suite before commit:
  **3583 passed, 11 skipped**; application/worklet WASM builds passed.
- Actual-C watchdog tests and surrounding task/reset/crash regressions passed;
  native/FPGA/watchdog-fault Cortex-A9 builds passed.
- Pinned **QEMU 10.0.13** passed control, broker, renderer, CPU-freeze and undefined
  cases. Explicit watchdog **poweroff** terminates each isolated VM without replay.
  These prove modeled policy expiry, not hardware reset/factory-chain behavior.
  Earlier harness failures and guest-clock differences are recorded in JSON.
- ABI-14 route: setup **+0.270 ns**, hold **+0.047 ns**, no failing endpoints;
  thermal report and XSA/bitstream identity verified. CTL0 overtemperature-powerdown
  bit remains set. Live XADC thresholds matched nominal 75°C/65°C protection.
- Factory `BOOT.BIN` and `image.ub` were not replaced. Separate candidate files
  were staged; no provisional PS/DDR setup, boot environment or QSPI changes.

No physical lost-ACK, exhaustive repeated-fault/lifetime, permanent external-MMIO
stall, thermal-trip, sustained no-heatsink, physical audio or signed-deployment
qualification is claimed. Raw UART, credentials, nonces, ELFs, profiles and local
bitstreams remain private; public evidence is filtered and audited.
