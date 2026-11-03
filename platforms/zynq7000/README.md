# Zynq-7000 firmware

Target: one Cortex-A9, 1 GiB PS DDR, FreeRTOS 10.6.1 and AMD lwIP 2.2.0.
The firmware runs the portable Shoop session service, backend and RT engine with
stereo 48 kHz dummy audio, 256-frame quanta and one dummy MIDI input/output.
Clients use the existing authenticated TCP data/management protocol. Session files,
codecs and scripts remain on the client machine. The
[SD boot and network deployment guide](deployment/README.md) covers generic
signed firmware, device credentials provisioned on SD, A/B updates and recovery.
The direct-load development mode below compiles its explicit test credential.

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
correctness on silicon, codec wiring or production boot packaging.
