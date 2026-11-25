# Zynq-7000 firmware

[Board profiles](boards/README.md) select the BSP, memory budgets and boot chain.
`--board qemu` preserves the reference below; `--board zedboard` targets 512 MiB
DDR with reduced budgets. Physical ZedBoard qualification is the final, pending
stage. Both profiles currently use dummy audio.

For physical ZedBoard bring-up, see [JTAG development without Vivado](boards/jtag-development.md).
It distinguishes direct-load development (no SD) from the signed SD/network
workflow. The direct-load commands in this README otherwise exercise QEMU;
they do not initialize a physical board's PS/DDR.

Target: one Cortex-A9, 1 GiB PS DDR, FreeRTOS 10.6.1 and AMD lwIP 2.2.0.
The firmware runs the portable Shoop session service, backend and RT engine with
stereo 48 kHz dummy audio, 256-frame quanta and one dummy MIDI input/output.
Clients use the existing authenticated TCP data/management protocol. Session files,
codecs and scripts remain on the client machine. The
[SD boot and network deployment guide](deployment/README.md) covers generic
signed firmware, device credentials provisioned on SD, A/B updates and recovery.
The direct-load development mode below compiles its explicit test credential.

Grab and Replacing use the common engine implementation and negotiated capabilities.
The shipped engine policy keeps histories disabled to retain its eight-track,
thirty-two-loop resource shape. Enable them with an explicit configuration:

```sh
python3 platforms/zynq7000/build.py --app host --fetch --credential-file work/zynq/host.key --engine-config platforms/zynq7000/input-history.json
python3 platforms/zynq7000/integration.py --input --engine-config platforms/zynq7000/input-history.json
python3 platforms/zynq7000/integration.py --board zedboard --input --engine-config platforms/zynq7000/input-history.json
```

The example configures three tracks/twenty loops, thirty-second history, 2,048
shared audio pages, 512 MIDI pages, 8,192 MIDI records and two writers. Page size
is 4,096 bytes; history density can evict MIDI before the nominal horizon. Board
recording/media budgets and physical arena layouts remain unchanged. A policy
file can override `tracks`, `loops` and individual `input_history` fields; omitted
fields retain the documented defaults in `engine_policy.py`. The resolved policy
is recorded in each image manifest. The same option applies to USB and deployment
builds. Actual memory and exact-media acceptance remain required for a new policy.

Direct Cargo builds select the same settings through `SHOOP_ENGINE_TRACKS`,
`SHOOP_ENGINE_LOOPS` and `SHOOP_INPUT_HISTORY_{SECONDS,AUDIO_TAKE_PAGES,MIDI_PAGES,
MIDI_RECORDS,WRITERS}`. Availability follows these settings, independently of the
compilation target. Policy changes take effect on the next firmware build/boot.

## Reproducible inputs

`pins.json` records the tested tool versions and immutable AMD source commit.
The CI workflow pins its Debian image by digest and its Arm compiler/newlib/QEMU
packages by version. Install Python 3, Git, patch, those packages and the pinned
Rust toolchain with `armv7a-none-eabihf`. The build
rejects a different compiler/source revision; the runner rejects a different QEMU.
No Vitis installation, FPGA synthesis, TAP device or KVM is required. Native
client/UI probes also need the Linux build dependencies listed in
[the firmware workflow](../../.github/workflows/zynq_freertos.yml).

From the repository root:

```sh
python3 platforms/zynq7000/build.py --fetch
python3 platforms/zynq7000/qemu.py
python3 platforms/zynq7000/integration.py
python3 platforms/zynq7000/integration.py --ui
python3 platforms/zynq7000/integration.py --recovery
python3 platforms/zynq7000/integration.py --fixture
```

`suite.py --fetch` builds and runs all three gates, including an intentional echo
content mismatch that must fail and leave diagnostics. `integration.py` tests
production rendering, authentication, fresh attachment and confirmed shutdown,
then restarts the same ELF at the same endpoints and refuses old authority.
`--ui` paints the actual native application and exercises settings, editing,
record/play, UI-only close/reconnect and client-side session save/load. It is
headless and does not require a display server. `--recovery` exercises silent
loss in each TCP direction, accepted work and staging ownership. `--fixture`
adds exact PCM/MIDI checks, a 40 MiB audio session round trip, capacity tests and
100 warmed attach/stage/cancel/detach cycles. `--small-fixture` uses small media
and omits the recording-pool fill; it does **not** pass the full fixture gate. `--port-base` changes only
host forwarding when running independent experiments.
For a longer optional soak, use `integration.py --fixture --small-fixture --soak-cycles 1000`;
the bounded PR gate retains 100 measured cycles after ten warm-up cycles.

The first command fetches only the required AMD source directories. Further
offline builds omit `--fetch`. `ARM_CC` and `QEMU` select installed executables;
the version checks still apply. Build products and captured evidence live under
`work/zynq/`, which is ignored by Git. The test owns its QEMU process and terminates
it on success, failure or timeout. It binds host forwarding only to loopback.

For an interactive connection, explicitly provision a 32-byte binary token:

```sh
mkdir -p work/zynq
python3 -c 'import os; p=os.open("work/zynq/host.key", os.O_CREAT|os.O_EXCL|os.O_WRONLY, 0o600); os.write(p, os.urandom(32)); os.close(p)'
python3 platforms/zynq7000/run.py --credential-file work/zynq/host.key
```

The runner prints the native UI command and exports
`work/zynq/interactive/profile.json`. Its credential path names the token compiled
into that ELF. Data/management guest ports are 15400/15401. These endpoints use
the existing trusted-LAN policy and are intended for an explicitly trusted
network. There is no production token default. CI generates fresh test tokens;
its archived firmware is a test image, not a deployment credential.

## Platform configuration and provenance

The checked-in BSP configuration expresses the PS addresses, clock assumptions,
interrupt routing, a 1 GiB DDR window, RTOS configuration and lwIP pool settings
needed by the emulated reference PS. The driver configuration tables are explicit;
there is no opaque Vitis-generated archive. Regenerate/review these settings
against a board's exported hardware description before physical deployment.

AMD supplies startup, MMU/cache routines, exception vectors, FreeRTOS context
switching, GEM and UART drivers and the lwIP port. Their source and license
headers are fetched at the pinned commit, rather than copied into the repository.
AMD standalone/driver and FreeRTOS source carries MIT licensing; lwIP and its
adapter include BSD-style notices. Preserve all applicable notices when
redistributing firmware and its dependencies. QEMU is an external GPL emulator.

`patches/emacps-read-clear.patch` fixes duplicate reads of queue-zero interrupt
status in the pinned AMD driver: queue zero shares the clear-on-read global ISR.
QEMU delivered packets into RX descriptors, but the second read discarded their
completion bits. Reusing the first read passes actual TCP exchange. The patch
applies only to a build-local copy; the source checkout stays pristine and the
build manifest records the patch hash. This driver fix requires no GEM model
changes or network bypass. Deployment additionally supplies a watchdog model.

`patches/freertos-fpu-irq.patch` routes the C IRQ dispatcher through FreeRTOS's
assembly wrapper that preserves D0–D31 and FPSCR. Task FPU support alone does not
protect caller-saved registers against an interrupt. Two fixture tasks verify
distinct D0/D16 patterns across preemption while the timer ISR deliberately
clobbers those registers. The fixture also verifies allocator alignment up to
4096 bytes and 64-bit atomic carry across the 32-bit boundary.

`include/bspconfig.h` must leave `USE_AMP` undefined. AMD's startup code tests
`#if USE_AMP!=1` and enables the L2 cache, but `xil_cache.c` tests `#ifndef
USE_AMP` and compiles out all L2 maintenance if the macro is defined, even as 0.
GEM DMA then reads stale DDR on silicon (QEMU has no caches to expose this).

The lwIP allocator uses `MEM_ALIGNMENT=64`, matching AMD's generated options.
DMA payloads must not share cache lines with pbuf metadata: an unaligned cache
invalidation can flush dirty metadata lines back over newly received frame
bytes. The first Z-turn V2 hardware test exposed repeated receive loss with
8-byte alignment; the aligned native host passed authenticated control,
reconnect and shutdown. See the [hardware evidence](../../docs/hardware/zturn-v2-bringup.md).

The link pulls newlib's ARMv7-A memory routines (`memcpy`, `memset`, `memmove`
and their `__aeabi_*` entry points) from libc before the Rust library, so they
override `compiler_builtins`' weak, generic versions for both C and Rust. The
Rust firmware is built with `-C target-feature=+neon`; rustc warns that the
32-bit ARM `neon` feature is unstable but applies it. LLVM vectorizes integer
work with it, not `f32` arithmetic, because ARMv7 NEON is not IEEE-compliant.

AMD's initialization convention requires `lwip_init()` to initialize both its
core and socket-mode TCP/IP thread. Calling `tcpip_init()` alone skips the core
initialization with AMD's default `LWIP_XINIT` setting. Startup also explicitly
enables the UART instead of assuming a prior physical bootloader did so.

The bootloader must place a **fresh, nonzero 64-bit boot nonce at CPU address 0x8000**
before entering the ELF. The direct-load runner supplies a fresh random value on each launch;
the deployment loader durably advances redundant SD counters before every entry.
Physical deployment must supply a hardware random value or a persistent boot
counter; recompiling once with a fixed seed does not satisfy restart identity.
The nonce is not secret. The firmware refuses a missing nonce and combines it
with a checked per-boot connection counter for lease identity. Monotonic time
comes from the 64-bit PS global timer, independent of sample-frame progression.

## Scheduling and socket ownership

A priority-6 task owns the prepared renderer. The 1 kHz timer accounts fractional
48 kHz/256-frame periods and notifies that task; late notifications are bounded
and counted as xruns. The priority-3 control task owns the session service and
backend. Its engine clock yields to FreeRTOS while synchronous preparation waits.
Rendering is guarded against Rust allocation **and deallocation** after startup.

Render outranks control, so overload would starve the session service and let
app heartbeats expire. Render is therefore capped at 80% of each 100 ms window,
measured with the global timer; quanta beyond the cap are dropped and counted as
xruns. Fixture builds keep every quantum. The render task runs with FPSCR in
RunFast mode (flush-to-zero, default NaN). Each render step reports its busy
time as the DSP load, and the global timer (CPU/2, about 3 ns on a 667 MHz
ZedBoard) is the runtime profiling clock.

A priority-4 broker owns lwIP BSD sockets. Its calls may wait for lwIP's internal
TCP/IP API semaphore even with nonblocking BSD descriptors. The Rust `ByteIo`
adapter therefore only accesses fixed SPSC byte rings; it never calls lwIP or
waits. There are 24 connection slots, each with separate 32 KiB RX/TX rings;
copies are capped at 4096 bytes. Closing drains queued bytes and allows the peer
to send FIN for at most two guest seconds. Preferring a passive close keeps
ordinary UI/management reconnects out of the guest TIME_WAIT pool; forced closes
retain normal TCP lifetime rules. Handles include a generation. Accept/auth and live data and
management sessions each retain their finite limits (4 pending per listener,
4 live per endpoint; stream records 8/1/8). lwIP has 64 TCP PCBs/netconns to
include closing and TIME_WAIT peers. Heartbeats expire on guest monotonic time.

The render endpoint stays alive when a UI disappears. Shutdown first closes it,
observes the renderer disconnect, acknowledges all 32 MIDI reset packets in the
dummy sink, deletes the render
task and performs stopped-resource cleanup. Only then does the service confirm
shutdown. Test-only cleanup failure injection verifies a refusal and safe retry.
Fixture UART commands and input/output inspection are compiled out of production;
the build audits exported symbols and image markers to enforce that separation.

## Memory and diagnostics

The direct-load launcher uses `-M xilinx-zynq-a9 -accel tcg -smp 1 -m 1G` and loads
the ELF directly. The deployment launcher enters U-Boot with the same CPU/RAM
contract and a persistent emulated SD card. QMP verifies actual CPU/RAM configuration. The linker reserves
the low 1 MiB, keeps platform code/C heaps/RTOS/network/DMA/stacks below 128 MiB,
reserves a separate 768 MiB Rust arena, and leaves the remaining 128 MiB
unassigned. The linker rejects overflow. Neither allocator may consume the spare
region. The 128 MiB recording pool is inside the Rust arena. C/newlib and FreeRTOS
each have separate 16 MiB heaps; lwIP's heap is 4 MiB. Control and render tasks
each have 256 KiB stacks. AMD's aligned GEM BD array occupies complete 1 MiB
sections with strongly ordered attributes; packet buffers use AMD cache
maintenance. Link-map bounds include those arrays and the extra DMA reservation.

The board admits at most 64 MiB aggregate decoded media, 1 MiB transfer metadata,
512 objects and 65,536 MIDI events. These are board policy, not a wire-format
change; native defaults stay at their previous values. Ingress declarations,
incremental object/event assembly, backend replacement and capture admission use
the same limits. Recording consumes the prepared page pool; filling it does not
grant a larger persistence budget. Stop recordings before capture/export.
Session uploads retain their existing revision fence: concurrent session edits
can cause a revision refusal. Captured images remain immutable during download.

Measured startup Rust allocation is 344,480,136 bytes (328.52 MiB). The largest
qualified fixture, including the 40 MiB audio-plus-MIDI round trip and capacity
checks, peaked at 407,466,188 bytes (388.59 MiB). Budget admission includes retained canonical media,
raw transfer storage, decoded assembly and replacement overlap. The arena leaves
room for those simultaneous owners; OOM is a diagnostic halt, not an admission
mechanism. UART reports Rust used/peak, FreeRTOS free/min-free and task stack
watermarks. The final report includes all remaining FreeRTOS task stacks;
render-stack usage is saved before deleting that task. Tests enforce arena bounds,
a 1 MiB RTOS reserve and stack margins, then require zero Rust allocations,
application socket slots and netconns after teardown. BSP networking allocations
and finite TCP TIME_WAIT state have their own accounting.

Build manifests, ELF/map, exact launch command, serial output, QMP machine facts,
register diagnostics, Ethernet capture and test results are retained in `work/`.
Register capture occurs only while stopping a test; some status registers clear
on read. CI uploads compact evidence on success and includes packet/register
captures on failure. The build hashes the ELF and applied patches and records
the source revision; the independent firmware workspace has a locked resolution.

QEMU validates firmware behavior, not physical timing, cache coherency, DMA
correctness on silicon, codec wiring or production boot packaging. The
`USE_AMP` coherency failure above only appeared on a physical ZedBoard; see the
[JTAG guide](boards/jtag-development.md) for hardware results.

## Remote device reboot

The management endpoint (TCP 15401) accepts a destructive `Reboot` command in
application protocol 53 (introduced in 52). It uses the same credential-authenticated connection as
existing embedded management requests; there is no new authentication mechanism.
From a matching client, use:

```sh
nix develop --command target/debug/shoopdaloop \
  --reboot-host --host-profile PATH_TO_DEVICE_PROFILE
```

The acknowledgment means **request accepted**, not that reboot completed. The ARM
control task gives the management reply a bounded 250 ms opportunity to drain,
then disables interrupts and requests a full Zynq PS reset through SLCR. It clears
only the warm-boot status flags that could make the factory FSBL skip reloading
PL; other reboot status bits are preserved. It does not change the boot mode,
factory boot files, DDR initialization, clock configuration or QSPI contents.
The board follows its configured boot chain, not an in-place restart of the ELF.
For the factory SD development setup, UART automation must still intercept U-Boot
and load the desired firmware/bitstream with a fresh nonce.

Unlike **stop host**, this deliberately bypasses renderer/MIDI cleanup and can
interrupt a live UI. Unsaved session state is lost. It remains usable when the
engine has failed if the control task and management endpoint still respond;
a wedged ARM/network path, a stopped host whose listeners have closed, or loss of
power still requires another recovery method. No automatic retry is performed
after a lost acknowledgment; observe the boot before trying again.

Only the embedded composition enables this command; desktop/browser services
return `Unsupported`. Use matching firmware/client/profile versions (currently
53). The physical reboot evidence below used protocol 52, not the new backend-
restart implementation.
The register sequence and reply pacing have offline tests. On 2026-11-25,
[physical MYIR tests](../../docs/hardware/zturn-v2-bringup.md#physical-management-reboot-test)
verified acknowledged Ethernet reboot and fresh factory U-Boot output from both
the native and protected FPGA hosts, without a physical reset. This is basic
functional evidence, not qualification of recovery from a wedged ARM/network path.

## Restart the FPGA backend without rebooting the device

Current source requires a recovery-capable mailbox **ABI-14** bitstream and
protocol-53 firmware/client (ABI 14 adds fault-safe submission and diagnostics).
With a matching pair,
choose **Host → Restart backend…**, including from a disconnected UI, or run:

```sh
nix develop --command target/debug/shoopdaloop \
  --restart-backend --host-profile PATH_TO_DEVICE_PROFILE
```

This discards the active session and unsaved recordings. The existing authenticated
management connection supervises cooperative owner-task stop, FPGA AXI drain,
processing/audio reset, fresh queues/service and verified frame progress. The ARM,
FreeRTOS scheduler, Ethernet, factory clocks, DDR configuration and thermal guard
remain running; no bitstream reload or boot/storage change occurs. The UI reconnects
with a fresh client rather than replaying old work. Native-renderer and desktop
hosts report `Unsupported`.

A lost reply is not automatically resubmitted. Operation status and runtime
identity remain queryable while the data endpoint is unavailable. An unsafe drain,
worker stop or initialization reports **full device reboot required** and retains
uncertain allocations; it does not silently issue a device reboot. Health sampling
continues in the control task, but the health data stream needs the new attachment.

A matching protected ABI-13/protocol-53 pair passed **three healthy, recording-active
restarts** on the Z-turn V2. Its subsequent 16-loop bulk-arming phase lost ARM/
management responsiveness, so that ABI-13 run did **not qualify failed-owner recovery**; a
safety device reboot could not be confirmed and physical RESET was requested.
The old ABI-12/protocol-52 pair is incompatible and is not automatically updated.
See [physical results](../../docs/hardware/zturn-v2/2026-11-25/backend-restart/README.md)
and [mailbox bug and recovery design](../../BUG_MAILBOX.md) for exact limits, the
additional MMIO bus-error race, and the separate pending bulk-control timing fix.

### Fault containment and ARM exception diagnostics

The ABI-14 source follow-up acknowledges valid register accesses normally even
when a command is rejected by fault/epoch/queue state. Explicit submission results
and post-acknowledgement epoch checks replace CPU-trapping semantic rejections.
The FPGA retains first-fault cause, accepted mailbox command context and timing
through datapath recovery; the owner logs it before ABORT. A matching new protected
bitstream is required. A subsequent matched protected ABI-14/watchdog pair passed
three healthy restarts and one faulted-owner restart without an ARM reboot. Its
first-fault record identified control-window timeout at cycle 16665. This qualifies
containment/recovery for that run, not successful bulk arming or the old ARM hang.
See [physical follow-up](../../docs/hardware/zturn-v2/2026-11-25/fault-containment/README.md)
for exact artifacts, limits and the later test-harness factory-return failure.

Firmware also installs fatal exception handlers with a cache-flushed, CRC-protected
RAM journal and bounded PS-UART output. These halt safely rather than resuming an
uncertain instruction. See [ARM exception journal](crash.md) for layout, reset
retention caveats and read-only U-Boot inspection. Direct-loaded Z-turn V2 hosts
can explicitly opt into [the hardware watchdog](watchdog.md) using `--watchdog`.
Controlled CPU freeze and undefined-instruction runs returned through factory
U-Boot; one undefined journal survived and matched UART. The staged batch timing
fix remains pending. Compilation/mock tests alone are not physical qualification.

Do not directly boot factory Linux while a custom PL image remains loaded: its
device tree probes factory-only peripherals. Retain a watchdog backstop through a
full device reset/restoration of the factory chain before entering factory Linux.

## In-app host health

Every backend advertises `host.health`; open **Profiling… → Host → Host health**.
Health automatically samples at 1 Hz on client attachment, independently of audio
and task profiling. See [host health](../../docs/runtime_profiling.md#host-health)
for schema, availability and freshness semantics.

Physical Zynq boards report the shared PS/PL die temperature and XADC sensor
maximum, plus overtemperature alarm state. QEMU does not model the sensor and
reports no temperature sensors. Failed physical reads remain explicitly
unavailable. The maximum is the hardware's retained value, not necessarily a
maximum since this firmware boot.

The control-task sampler uses the PS-XADC serial FIFO at `0xf8007100`, with bounded
polls and a READ/NOP pipeline. It enables/configures only the PS serial interface;
it never writes XADC internal registers, resets the ADC, changes interrupt masks,
changes the factory clocks or weakens PL thermal shutdown. The factory's asserted
PS-interface reset is released only when both FIFOs are empty; this does not reset
the converter and never asserts a reset. Gated PCAP clocks or nonempty FIFOs while
the interface is held in reset yield unavailable readings. The conversion follows UG480's
`raw16 * 503.975 / 65536 - 273.15` °C with integer millidegree rounding. There is
one die sensor, not separate ARM and PL temperatures.

`nix develop --command python3 platforms/zynq7000/tests/temperature.py` exercises
conversion, negative temperatures, FIFO alignment/stale responses, alarm states,
bounded failure and the absence of ADC-register/reset writes with a simulated
register bus. A [15-second Z-turn V2 native run](../../docs/hardware/zturn-v2-bringup.md#native-temperature-reading-without-heatsink)
measured 52.539–53.154 °C, with a stored sensor maximum of 53.523 °C and confirmed
shutdown. That original run manually released the PS-interface reset in factory
U-Boot. The later native management-reboot test also verified automatic empty-FIFO
interface release on hardware, starting from MCTL `0x10` and FIFO status `0x500`.
This is not sustained-load or heatsink-free thermal qualification. Monitoring is
not a local PS thermal-stop handler or whole-board power cutoff.

## In-app task profiling

Open the application's Profiling window and select **Host**, then enable task
statistics. The service samples a bounded 32-task table at 1 Hz independently of
audio profiling. State, priorities, scheduler-accounted CPU and stack high-water
headroom are displayed; unavailable first/reset CPU intervals remain blank.
Collection time is reported because kernel snapshots/stack scans suspend scheduling.
64-bit runtime accounting remains enabled in the kernel even when UI sampling is
off. Bounded task lifecycle hooks assign non-reused boot-scoped identities and do
not collect a scheduler trace. The host report works without an audio profiling
clock and is not represented as graph nodes.

`tests/profiling.py` exercises task identity reuse and map exhaustion natively.
The production QEMU probe checks actual kernel task samples, interval CPU values,
independent toggles and restart fencing. The `--ui` integration gate paints the
Host panel with actual firmware reports. Existing memory, stack, rendering,
shutdown and authentication checks remain active. QEMU is software evidence;
physical latency/jitter and scheduler overhead require board qualification.
