# ZedBoard development over JTAG without Vivado

This is an initial hardware bring-up recipe for iterative verification, not a
physically qualified setup. CI validates builds and emulated execution only.
Assumptions: stock ZedBoard Rev C/D, 512 MiB DDR, onboard Digilent USB-JTAG,
PS UART1 at 115200 8N1, one Cortex-A9 running Shoop, and an isolated Ethernet
connection. Record your board revision and any deviations before testing.
No Vivado, Vitis, FPGA bitstream, SD card or Linux on the board is required for
this direct-load path. FreeRTOS is linked into the ELF like other bare-metal code.

## Choose the workflow first

| | JTAG development | SD/network deployment |
| --- | --- | --- |
| Firmware | `host.elf` | `host-deployment.elf` |
| Build options | `--app host --credential-file ...` | `--app host --deployment` |
| PS/DDR initialization | Debugger initialization before loading DDR | SPL using pinned board PS initialization |
| Credential | Compiled into the development ELF | Provisioned device state on SD |
| Boot identity | Debugger writes fresh nonce at `0x8000` each launch | Loader advances persistent SD counter |
| SD card | Unnecessary | Required by this project's boot/update design |
| Updates/recovery | Reload with JTAG | Signed bundles, A/B slots, watchdog rollback |
| Physical validation | Pending | Pending |

FreeRTOS does not require SD. The deployment build requires its loader handoff,
configuration and persistent state; it is not a substitute for the direct-load
ELF. The development ELF contains your credential: keep it private.
CI's `zynq-board-zedboard` artifact includes board ELFs and manifests, but
direct-load test ELFs use disposable CI credentials. Build your own image for
an interactive connection. Preserve each ELF's adjacent JSON manifest.

## Install a Linux workstation environment

Use Debian 13 for the firmware compiler versions used by CI. On other
distributions, use a Debian 13 build container or install the exact pinned
toolchains; do not assume the distribution's default Arm GCC passes the build's
version check. OpenOCD and UART can run on the host while compilation runs in a
container; mounting the checkout is sufficient, without forwarding USB.

On Debian 13:

```sh
sudo apt-get update
sudo apt-get install --yes --no-install-recommends \
  ca-certificates curl git patch python3 xz-utils build-essential pkg-config \
  protobuf-compiler libasound2-dev libjack-jackd2-dev \
  gcc-arm-none-eabi=15:14.2.rel1-1 \
  binutils-arm-none-eabi=2.44-3+23+b1 \
  libnewlib-arm-none-eabi=4.5.0.20241231-1 \
  libnewlib-dev=4.5.0.20241231-1 \
  openocd gdb-multiarch picocom iproute2 netcat-openbsd
```

Install rustup using its [official installation instructions](https://rust-lang.org/tools/install/),
then install the repository's pinned Rust toolchain:

```sh
rustup toolchain install 1.99.1 --profile minimal
rustup target add --toolchain 1.99.1 armv7a-none-eabihf
arm-none-eabi-gcc -dumpfullversion
openocd --version
gdb-multiarch --version
```

GCC must report `14.2.1`. Treat [pins.json](../pins.json) and the
[CI workflow](../../../.github/workflows/zynq_freertos.yml) as the version
authority if these commands become stale. OpenOCD 0.12.0 is the starting
assumption, not a version qualified by this repository. GDB is optional if your
existing loader can load ELF segments and enter its ELF entry point.

QEMU is only needed for emulated tests. U-Boot, signing tools, DTC, Bison and
Flex are needed for the SD workflow, not normal JTAG application builds.
The native UI has additional desktop dependencies; see the main build guide.

## Connect the board

1. With power off, select JTAG boot using the boot-mode jumper table for your
   board revision. Use the board manual rather than copying jumper positions
   from another Zynq board.
2. Connect the onboard USB-JTAG port, the separate USB-UART port, and power.
   Connect Ethernet directly to a spare workstation interface for initial tests.
3. Identify the UART using `ls /dev/serial/by-id/`. Open it before starting:
   `picocom --baud 115200 --flow n /dev/serial/by-id/YOUR_UART`.
4. Install the distribution's OpenOCD USB udev rules. Reconnect USB afterwards.
   Serial access may require membership in `dialout` and a new login.
   A USB permission failure is distinct from an incorrect JTAG adapter driver.

The OpenOCD 0.12.0 board file uses `interface/ftdi/digilent_jtag_smt2.cfg`
and `target/zynq_7000.cfg`. It supports the assumed onboard adapter; an
external probe needs its own interface configuration. Do not load both the
board file's interface and a second interface file.

From the repository root, in a terminal that stays open:

```sh
openocd -f board/digilent_zedboard.cfg \
  -c 'bindto 127.0.0.1' -c 'adapter speed 1000' \
  -c 'init; targets zynq.cpu1; halt; targets zynq.cpu0; halt'
```

Newer OpenOCD trees may name this board file
`board/digilent/zedboard.cfg`; use the file shipped with your installation.
Expected checkpoint: JTAG scan succeeds, both Cortex-A9 targets appear, and
CPU0 is selected and halted. Leave CPU1 halted. Stop here on adapter/scan errors.

## Initialize PS and DDR before loading the application

**This step needs board verification before the recipe is complete.**
OpenOCD's stock ZedBoard board/target files configure debug access; they do not
supply the board's PLL, MIO or DDR initialization. Loading an ELF into DDR before
that initialization is not sufficient. A processor reset alone does not perform it.

If your existing bare-metal workflow already performs known-good PS initialization,
reuse that sequence, provided it leaves CPU0 ready for a fresh bare-metal entry
and CPU1 stopped. For a completely Vivado-free setup, the open-source starting
point is the board initialization already pinned by this repository:

- U-Boot commit `5508406582f6f7120dce0c4b3059819a41788db2`.
- `board/xilinx/zynq/zynq-zed/ps7_init_gpl.c`, SHA-256
  `f0b9799eedb0ec8a3174ea442a15f0e643da3b4349094fdb396519d9c00c1cb3`.
- The matching U-Boot PS initialization interpreter and headers.

That source is available without installing AMD tools. To use it over JTAG,
an OCM-resident initialization helper or an equivalent OpenOCD register sequence
must be prepared and checked against the board revision. **This repository does
not yet ship that helper or a qualified OpenOCD PS initialization script.**
Do not pass the C file to OpenOCD `source`, or assume an AMD debugger's
`ps7_init.tcl` works unchanged: debugger-specific Tcl commands need adaptation.

The next hardware iteration should settle the initialization method and add its
exact build/load/run commands here. Until then, this is an explicit prerequisite,
not a claim that a clean workstation can complete cold boot using this guide alone.
No SD card is needed to solve it.

After successful initialization, with CPU0 halted and before loading any image,
a small scratch check at a DDR address used by the application can catch basic
access failures. In OpenOCD's telnet console (`telnet localhost 4444`, if installed):

```tcl
targets zynq.cpu0
mww 0x00100000 0x12345678
mdw 0x00100000 1
mww 0x00100000 0
```

Expect `0x12345678` on readback. This overwrites scratch memory and must only
run before loading the ELF; it is not a full DDR test. Do not issue another reset
after initialization unless you repeat the required initialization.

## Build your direct-load ELF

Run from the repository root:

```sh
mkdir -p work/zynq
python3 - <<'PY'
import os
from pathlib import Path
path = Path("work/zynq/zedboard.key")
fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
with os.fdopen(fd, "wb") as stream:
    stream.write(os.urandom(32))
PY
python3 platforms/zynq7000/build.py --fetch --board zedboard --app host \
  --credential-file work/zynq/zedboard.key
arm-none-eabi-readelf -h -l work/zynq/boards/zedboard/build/host.elf
```

Generate the credential only once; the exclusive create deliberately refuses
to overwrite an existing key. Subsequent builds reuse it and can omit `--fetch`
when the pinned AMD sources are already downloaded. The compiler, BSP, FreeRTOS
and Rust library are linked into the ELF. No AMD SDK installation is involved.
Optional input-history policy is selected with `--engine-config`; see the
[platform README](../README.md).

Expected checkpoint: `host.elf`, `host.map` and `host.json` under
`work/zynq/boards/zedboard/build/`, using the ZedBoard profile and 512 MiB RAM.

## Load, supply boot identity, and start

After PS/DDR initialization, start a second terminal from the repository root:

```sh
gdb-multiarch work/zynq/boards/zedboard/build/host.elf
```

In GDB:

```gdb
target extended-remote localhost:3333
monitor targets zynq.cpu0
monitor halt
load
```

Generate two fresh nonce words on the workstation for **every launch**, including
reloads of the same ELF:

```sh
python3 - <<'PY'
import secrets
nonce = secrets.randbits(64) or 1
print(f"set {{unsigned int}}0x8000 = 0x{nonce & 0xffffffff:08x}")
print(f"set {{unsigned int}}0x8004 = 0x{nonce >> 32:08x}")
PY
```

Paste the two printed commands into GDB. The low word is at `0x8000`, the high
word at `0x8004`; the firmware reads them as a little-endian 64-bit value.
The nonce is public and must be fresh/nonzero; it is not the credential.

Run `info files` in GDB, copy its reported ELF entry point, then:

```gdb
x/2wx 0x8000
set $pc = ENTRY_ADDRESS_FROM_INFO_FILES
continue
```

Replace the placeholder with the actual numeric entry address. Enter at the ELF
entry, not directly at `main`: the startup code initializes runtime state.
Do not use `run` or reset the target between writing the nonce and starting.

Expected UART checkpoint:
`SHOOP_BOOT FreeRTOS AMD-lwIP board=shoop-zedboard-512m-v1 ram_mib=512`,
followed by a `SHOOP_BUILD` line. Capture the complete log. A banner alone does
not prove scheduler, networking or memory stability. A missing/zero nonce prevents
normal service startup. For each new attempt, return to a known bare-metal state;
a power cycle plus repeated PS initialization is the initial reproducible baseline.
Reloading a previously running cached/MMU-enabled image without restoring that
state has not been qualified.

## Connect over physical Ethernet

The non-deployment branch in `host.c` currently hardcodes:

| Setting | Value |
| --- | --- |
| Device IP / mask | `10.0.2.15/24` |
| Gateway | `10.0.2.2` |
| MAC | `02:00:00:00:00:01` |
| TCP data / management | `15400` / `15401` |

These are also the QEMU defaults. There is no DHCP or command-line firmware
network override in this path. For the initial physical test, use an isolated
direct cable and a spare workstation interface, substituting its actual name:

```sh
ip -brief address
sudo ip link set dev YOUR_ETHERNET_INTERFACE up
sudo ip address add 10.0.2.2/24 dev YOUR_ETHERNET_INTERFACE
nc -vz -w 3 10.0.2.15 15400
nc -vz -w 3 10.0.2.15 15401
```

Choose an interface without a competing network manager configuration or existing
route for `10.0.2.0/24`. Remove the temporary address afterwards with
`sudo ip address del 10.0.2.2/24 dev YOUR_ETHERNET_INTERFACE`.
No routing/NAT is required for a direct local connection.
Only run one device with the default MAC/IP on that link.

Configure the native client's remote connection with data
`10.0.2.15:15400`, management `10.0.2.15:15401`, and the workstation copy of
`work/zynq/zedboard.key`. QEMU loopback forwarding ports do not apply here.
An open TCP port proves reachability, not authentication; complete a client
attach and session operation next. Audio/MIDI are dummy endpoints, so successful
rendering will not produce sound at the physical codec.

## Record each hardware checkpoint

Keep results with the board revision, host OS, OpenOCD/probe versions, firmware
commit and manifest, initialization source/hash, GDB commands and UART log:

1. USB/JTAG scan and halt of both cores.
2. Cold-boot PS initialization and DDR scratch readback.
3. ELF load and fresh nonce readback.
4. Boot banner, scheduler/service startup, and both TCP listeners.
5. Authenticated native-client attach, a small session transfer, reconnect and
   clean shutdown.
6. Power cycle and repeat with a fresh nonce; then investigate allocation peaks,
   stack margins and DMA/cache behavior.

If a checkpoint fails, preserve its output before changing later steps. Adapter
errors, uninitialized DDR, UART clock/baud issues, Ethernet PHY initialization,
subnet routing and credential mismatch have different causes. Physical timing,
DDR training and cache/DMA correctness are not established by QEMU.

## References for the initial assumptions

- [OpenOCD 0.12.0 ZedBoard configuration](https://github.com/openocd-org/openocd/blob/v0.12.0/tcl/board/digilent_zedboard.cfg)
  and [Zynq target configuration](https://github.com/openocd-org/openocd/blob/v0.12.0/tcl/target/zynq_7000.cfg):
  adapter selection, CPU target names and debug configuration.
- [Digilent ZedBoard hardware user guide](https://files.digilent.com/resources/programmable-logic/zedboard/ZedBoard_HW_UG_v2_2.pdf):
  board revision, boot straps, onboard JTAG, UART and Ethernet connections.
- [AMD UG821: Zynq PS configuration](https://docs.amd.com/r/en-US/ug821-zynq-7000-swdev/Zynq-PS-Configuration):
  why clocks, MIO and DDR initialization precede a DDR-resident application.
- [Pinned open-source ZedBoard PS initialization](https://github.com/u-boot/u-boot/blob/5508406582f6f7120dce0c4b3059819a41788db2/board/xilinx/zynq/zynq-zed/ps7_init_gpl.c):
  starting point for an initialization path without Vivado.
