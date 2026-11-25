# Direct-load host watchdog

`build.py --board zturn-v2 --app host --watchdog ...` explicitly opts a
UART/U-Boot-loaded host into the Cadence system watchdog. Ordinary direct-load
builds remain unchanged. This option cannot be combined with the signed SD
handoff mode, whose existing loader/runtime watchdog is separate.

The watchdog is armed during platform initialization, before exception-handler
installation: **30 seconds** before service readiness, **10 seconds** afterwards.
After readiness the tick hook feeds it at most once per 100 ms and only after
both control and socket-broker progress, plus renderer progress when required.
The hardware timer does not depend on tick interrupts continuing to execute.

A returned/stopped FPGA owner can retire the renderer requirement while management
remains live. A new owner gets at most **25 seconds** for its first progress;
thereafter render progress is mandatory. Neither grace nor stopped rendering
exempts control or broker progress. Clean host shutdown disables the watchdog;
backend restart does not disable it.

The firmware uses the board profile's existing CPU-1x watchdog frequency and
changes no clock/PLL/DDR configuration. It clears only the factory FSBL's PL-warm
flags in SLCR reboot status, preserving other bits and the previous SLCR lock
state. This allows factory PL initialization on a later reset. It changes no
boot environment, default boot files, QSPI, deployment state or storage.
A direct-load failure returns through the factory boot chain, **not** an automatic
reload/replay of the candidate. A different boot policy needs separate qualification.

**Return through full reset, not a direct `boot` under the custom PL.** Factory
Linux's device tree expects its own FPGA peripherals. A later U-Boot MMIO test
passed its rejection checks but disabled the watchdog before directly entering
factory Linux; boot stalled at the video driver and serial SysRq could not recover
it. Keep the backstop armed through the reset/restore transition. See the
[physical evidence and recovery state](../../docs/hardware/zturn-v2/2026-11-25/fault-containment/README.md).

The tracked `factory_return.return_to_factory` helper enforces SD boot mode, an
armed reset watchdog and cleared PL-warm flags before issuing exactly one `reset`.
It authorizes factory Linux only after a fresh countdown and subsequent U-Boot
prompt; stale prompts, missing boot banners, changed boot mode and timeouts are
covered by `python3 platforms/zynq7000/tests/factory_return.py`. It neither disables
the backstop nor retries reset or boots a candidate on failure. The local MMIO
harness uses this helper. The correction is offline-tested, not a replay of the
failed board workflow.

## Qualification-only faults

`--watchdog-test-fault control|broker|render|freeze|undefined` requires
`--watchdog`. It emits a distinct test ELF and manifest marker and fires once the
host has been ready for 32 seconds. The first three suspend their respective
task; `freeze` requests interrupt masking and waits forever; `undefined` executes
an ARM `udf`. There is no remote injection command or new authentication surface.
Normal firmware is checked for leaked test markers. Do not stage these as default
boot images or use them as production firmware.

## Validation boundary

The actual C policy is covered by `tests/watchdog.py`: startup expiry, healthy
quorum, each missing task, feed pacing, retirement and bounded re-entry grace,
clock wrap, explicit shutdown, SLCR preservation and all fault hooks.

On Z-turn V2, a healthy native run survived 30 seconds without spurious reset.
A controlled CPU freeze and a controlled undefined instruction both returned to
fresh factory U-Boot, respectively **10.724 s** and **10.806 s** after their UART
fault markers. These include boot-chain latency, not just watchdog expiry. The
undefined journal passed CRC and matched UART/RAM; its PC resolved to the injected
`udf`. The factory `BOOT.BIN`/`image.ub` hashes remained unchanged. Ninety health
reports stayed at or below 52.539°C with no sampled OT; the final factory-Linux
reading was approximately 53.646°C. Supervised probes enforced 65°C/3 s freshness
before intentional failure. Missing telemetry during the deliberate CPU fault
was not accepted as a fresh temperature reading.

This does not qualify pre-ready physical expiry, each suspended-task case on
silicon, permanently stalled external MMIO, every reset mode, FIQ isolation,
thermal trip, no-heatsink sustained operation, signed deployment or FPGA audio
correctness. A matching protected ABI-14/watchdog FPGA pair subsequently passed
three healthy restarts and one faulted-owner restart without ARM reset; the
retained first cause was control timeout at cycle 16665. This does not fix the
bulk-arming timing defect.
