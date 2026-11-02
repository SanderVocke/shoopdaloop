# Zynq-7000 firmware

Target: one Cortex-A9, 1 GiB PS DDR, FreeRTOS 10.6.1 and AMD lwIP 2.2.0.
The current platform milestone is the real GEM/TCP echo test (Gate 1).
Shoop integration is tracked in `PLAN_ZYNQ_FREERTOS_HOST.md`.

## Reproducible inputs

`pins.json` records the tested tool versions and immutable AMD source commit.
The CI workflow pins its Debian image by digest and its Arm compiler/newlib/QEMU
packages by version. Install Python 3, Git, patch, those packages and the pinned
Rust toolchain with `armv7a-none-eabihf` for subsequent host integration. The build
rejects a different compiler/source revision; the runner rejects a different QEMU.
No Vitis installation, FPGA synthesis, TAP device or KVM is required.

From the repository root:

```sh
python3 platforms/zynq7000/build.py --fetch
python3 platforms/zynq7000/qemu.py
```

The first command fetches only the required AMD source directories. Further
offline builds omit `--fetch`. `ARM_CC` and `QEMU` select installed executables;
the version checks still apply. Build products and captured evidence live under
`work/zynq/`, which is ignored by Git. The test owns its QEMU process and terminates
it on success, failure or timeout. It binds host forwarding only to loopback.

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
build manifest records the patch hash. No QEMU changes or network bypass is used.

AMD's initialization convention requires `lwip_init()` to initialize both its
core and socket-mode TCP/IP thread. Calling `tcpip_init()` alone skips the core
initialization with AMD's default `LWIP_XINIT` setting. Startup also explicitly
enables the UART instead of assuming a prior physical bootloader did so.

## Memory and diagnostics

The launcher always uses `-M xilinx-zynq-a9 -accel tcg -smp 1 -m 1G` and loads
the ELF directly. QMP verifies actual CPU/RAM configuration. The linker reserves
the low 1 MiB, keeps platform code/C heaps/RTOS/network/DMA/stacks below 128 MiB,
reserves a separate 512 MiB Rust arena, and leaves the remaining 384 MiB
unassigned. The linker rejects overflow. Neither allocator may consume the spare
region. The recording pool will be a subdivision of the Rust arena.

Build manifests, ELF/map, exact launch command, serial output, QMP machine facts,
register diagnostics, Ethernet capture and test results are retained in `work/`.
Register capture occurs only while stopping a test; some status registers clear
on read. CI uploads this evidence on both success and failure.

QEMU validates firmware behavior, not physical timing, cache coherency, DMA
correctness on silicon, codec wiring or production boot packaging.
