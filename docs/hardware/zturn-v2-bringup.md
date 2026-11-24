# MYIR Z-turn V2: first physical bring-up

## Result — 2026-11-24

**The native FreeRTOS Shoop host passed its authenticated hardware smoke test.**
This is not yet a hardware pass for the FPGA looper backend, physical audio, or
the repository's signed SD deployment chain.

The board started with its supplied SD card and factory PS initialization. No
JTAG cable was available. Factory U-Boot loaded a direct-load `host.elf` from an
additional file on the SD boot partition. The original `BOOT.BIN` and `image.ub`
were retained and their hashes checked after adding the test files. Linux's own
journal and other normal filesystem writes were not prevented.

[Machine-readable result and public transcripts](zturn-v2/2026-11-24/result.json).
The firmware manifest records base commit
`9efe47d7453a29d30de2c746c7b1bc86f6d94452` **plus local PHY and DMA-alignment
fixes**, not that unmodified commit alone. Its source-input and ELF hashes are
in the result. The firmware build ID is `zturn-aligned-native`.

## Observed hardware and factory environment

| Item | Observation |
| --- | --- |
| Bootloader | Factory U-Boot 2018.01, built 2022-11-19 |
| Factory OS | PetaLinux 2018.3; Linux 4.14.0-xilinx-v2018.3 |
| Silicon | U-Boot reports v3.1; Linux reports SoC ID 0x7, revision 0x3 |
| RAM | 1 GiB; factory Linux starts both Cortex-A9 CPUs |
| Clocks | U-Boot reports ARM 666 MHz, DDR 533 MHz; runtime counter 333,333,333 Hz |
| UART | J6 USB_UART, CP2103, UART1 at 0xe0001000; 115200 8N1, no flow control |
| Ethernet | GEM0; YT8531S ID 0x4f51e91a; native driver works at MDIO address 7 |
| Link | Direct cable to laptop; 1000 Mbit/s full duplex |
| Power during native tests | Supplied 5 V DC adapter, with USB_UART also connected |
| Native endpoints | 10.0.2.15:15400 data, :15401 management; laptop 10.0.2.2/24 |

The factory Linux MDIO scan exposed the same PHY ID at addresses **0 and 7** and
selected 0. This did not require changing the repository's address-7 board
profile: native negotiation and networking at 7 passed. The reason for the
second responding address was not established.

Factory PHY register observations at both addresses were `0xa000=0x0000`,
`0xa001=0x8160`, and `0xa003=0x00f1`. The native test retained the existing
1.95 ns RX/gigabit-TX delay configuration; it did not change PHY voltage or
operating-mode straps. No package/speed-grade or populated DDR marking was
independently photographed or qualified.

## Backup and recovery baseline

Before booting or adding files, the entire supplied SD card was read while both
partitions were unmounted. Its identity was SD32G, serial `0x5ec0cb55`, CID
`0353445344333247855ec0cb55019b00`; size **31,914,983,424 bytes**.

Decoded whole-device SHA-256:

```text
2a49e0828a9db3b69effd3aa0f7777b86d28704a1eb5fd4b997921820fa361f5
```

Verification checked zstd integrity, exact decoded length, and agreement with
the checksum captured during the initial device read. This was not a second
independent read of the physical card. A compressed-image checksum was checked
again after copying the backup to the laptop's other physical SSD.

Local backup directories:

- `/home/sander/backups/myir-zturn-v2/stock-20261124-144126/`
- `/mnt/data/backups/myir-zturn-v2/stock-20261124-144126/`

Each contains `stock.img.zst`, `SHA256SUMS`, a `VERIFIED` marker and identity/
size/checksum records. These backups and factory binaries are not checked in.

The factory ext4 journal needed recovery and FAT reported a previous unclean
unmount. The initial image preserves that state. No `fsck`, destructive storage
test, QSPI write, bootloader replacement or environment save was performed.

## Fixes found on silicon

### Explicit YT8531S support

The original driver accepted only YT8531 ID `0x4f51e91b`. This board reports
YT8531S ID **`0x4f51e91a`**, which is a distinct supported device, not an ID bit to
ignore indiscriminately.

The driver now accepts those two exact IDs. For YT8531S it first verifies the
strapped copper-to-RGMII mode, then selects the copper register bank before
configuration and link polling. Fiber/SGMII/other modes are refused without
changing the operating-mode or voltage fields. Existing masked delay writes
remain unchanged.

Tests cover both variants at 10/100/1000 Mbit/s, register preservation, copper
bank selection, refusal of other modes/IDs and MDIO read/write failures. Only
**1000 Mbit/s** has been exercised on this physical board.

References: [Linux Motorcomm driver](https://github.com/torvalds/linux/blob/v6.18/drivers/net/phy/motorcomm.c)
and [pinned U-Boot Motorcomm driver](https://github.com/u-boot/u-boot/blob/5508406582f6f7120dce0c4b3059819a41788db2/drivers/net/phy/motorcomm.c).

### Cache-line-safe lwIP allocation

The first native echo image booted and negotiated gigabit, but suffered repeated
receive-side packet loss. Small exchanges sometimes completed only after a
retransmission; a bounded exchange then stalled. These attempts are failures,
not successful network qualification.

`MEM_ALIGNMENT=8` allowed pbuf metadata and DMA payloads to share a Cortex-A9
cache line. The linked RX pool used 1616-byte elements: a 16-byte pbuf header
plus 1600 bytes of payload, alternating payloads between aligned and half-line
positions. The AMD receive path updates pbuf metadata before invalidating the
received DMA payload. Its unaligned invalidation flushes edge cache lines,
which can write cached bytes back over freshly received data.

`MEM_ALIGNMENT` is now **64**, matching the AMD generated configuration. This
separates metadata from payload cache lines and gives the RX pool a 1664-byte
stride. The native host then passed 20/20 pings and the authenticated protocol
smoke, with the same PHY delays. A configuration regression test enforces
cache-line-safe allocation and pool sizing.

The full multi-megabyte echo workload has **not** been rerun with the corrected
image. The successful result supports the alignment diagnosis but is not a
long-run throughput or packet-loss qualification. QEMU does not model the cache
behavior responsible for this failure.

AMD reference: [generated lwIP options](https://github.com/Xilinx/embeddedsw/blob/97f2baf7f69e9d5f447650110af0d8841e501277/ThirdParty/sw_services/lwip220/data/lwip220.tcl).

## Reproducing the direct-load SD procedure

First verify board markings, power and boot straps against the
[MYIR V2 quick-start guide](https://www.myirtech.com/download/Zynq7000/zturnV2QuickStartGuide.pdf).
Its SD setting is JP1 open, JP2 closed, with power off before changing jumpers
or inserting/removing the card. Capture UART before boot/reset. USB powered the
console and the user observed the board already running; that does not replace
using the documented DC supply for testing.

Build with `--board zturn-v2 --app host --credential-file PRIVATE_KEY` and without
`--deployment`. Use a fresh private 32-byte development credential and preserve
the build manifest beside the ELF. Provision the matching workstation profile
for the fixed native IP/ports above. Keep the credential, credential-bearing ELF,
generated credential source and raw authenticated packet captures private.

Add the ELF as a separate FAT file; this run used `shoop-native-aligned.elf`.
After a **clean reboot from Linux**, stop the factory U-Boot countdown. Its
preboot DHCP retries on an isolated cable can be interrupted with Ctrl-C. Check
`bdinfo` and `help bootelf`, then:

```text
fatload mmc 0:1 0x38000000 shoop-native-aligned.elf
crc32 0x38000000 ${filesize}
```

Compare the reported length and CRC to the workstation file before launching.
For the recorded image these were 7,456,096 bytes and CRC32 `8276eb2b`.

**Do not use the factory kernel staging address `0x10000000`.** These ELFs
zero-fill their 128–768 MiB Rust arena, which includes that address and can
overwrite their own staged ELF. `0x38000000` is 896 MiB, outside every ELF load
region and below the observed factory U-Boot stack/relocation near the top of
RAM. Recheck these ranges for each different image/loader.

Generate new nonce words on the workstation immediately before **every** launch:

```python
import secrets
nonce = secrets.randbits(64) or 1
print(f"mw.l 0x8000 0x{nonce & 0xffffffff:08x} 1")
print(f"mw.l 0x8004 0x{nonce >> 32:08x} 1")
```

Paste those two fresh commands, verify `md.l 0x8000 2`, then:

```text
bootelf -p 0x38000000
```

Expected UART: `SHOOP_BOOT`, matching `SHOOP_BUILD`, link at address 7, then
`SHOOP_HOST_READY`. Factory U-Boot printed cache-range alignment warnings while
loading the ELF; startup and the smoke still succeeded.

From the repository's development shell, run the native probe with the matching
profile:

```sh
cargo run --locked -p shoop_host --no-default-features --example zynq_probe -- PRIVATE_PROFILE.json
```

This test **shuts down the service**. It confirmed attachment, management, task
profiling, control, continued rendering while disconnected, fresh attachment and
shutdown. UART then reported all 32 dummy MIDI resets, complete cleanup, zero
remaining tracked Rust allocation and `SHOOP_HOST_RETURNED`. It does not imply
that control returns to the U-Boot command prompt; a hardware reset may still be
needed for another run. Do not reset a mounted factory Linux filesystem merely
to reproduce this shortcut: shut Linux down cleanly first.

The host used eight tracks/64 loop slots and dummy 48 kHz audio, not physical
I2S. Observed peak tracked Rust allocation was **551,320,304 bytes**. The first
client snapshot reported zero xruns; this is not a timed stress-test result.

## Remaining gates at the native checkpoint

- Rerun large exact-byte echo, lifecycle/load and long-duration tests on the fixed
  firmware; check fresh boot identity and rejection of previous-boot authority.
- Confirm physical board/DDR markings and obtain the exact factory V2 PS export;
  basic boot and allocation are not comprehensive DDR qualification. Read the
  BootROM mode register on the next U-Boot stop to independently verify the
  selected boot source; the recorded run proves SD file loading through factory
  U-Boot, not an independent register check of the boot straps.
- Build a current, matched generated-audio FPGA bitstream and firmware. No current
  bitstream or local Vivado installation was available at this checkpoint.
- Test FPGA register/DDR discovery and generated-audio record/play before adding
  a codec. The factory demo PL being configured does not test Shoop's FPGA logic.
- Qualify the repository-generated SPL/U-Boot and signed SD deployment separately;
  this run deliberately reused the factory boot chain.
- Verify codec wiring, I/O voltages, external clock and physical I2S, then the
  64-loop timed FPGA/native comparison.

The local QEMU is 10.1.5 rather than pinned 10.0.13; no fresh pinned QEMU suite
pass is claimed here. Seven board/configuration tests and both corrected Arm
cross-builds passed. The Arm vendor toolchain used GCC 14.2.1 and Rust 1.99.1;
its bundled newlib/binutils differ from the pinned Debian CI packages.

## Protected FPGA bitstream: offline build checkpoint

A fresh generated-audio, looping-geometry image was built offline with Vivado
2021.1, SW build 3247384. The exact compiler executable and matching IP catalog
were verified against the recorded provenance. The source snapshot and reports
are retained in [the offline build evidence](zturn-v2/2026-11-24/fpga-thermal/result.json).

The bitstream enables **automatic PL overtemperature shutdown**. A retained XADC
uses its default monitoring sequence, with nominal **75°C trip / 65°C lower
thresholds**. The build refuses disabled or missing protection before generating
the bitstream and checks it again on the routed design. Independent inspection
of the generated configuration packets confirmed CTL0 bit 12 enabled; the XSA
contains the identical bitstream. Seven thermal-policy regression tests pass,
and both probe and authenticated-host artifact pairings verify.

At 100 MHz the routed result has setup WNS **+0.494 ns**, hold slack **+0.043 ns**,
38,568 fully routed nets and zero routing errors. DRC reports zero errors/critical
warnings and 16 ordinary DSP-pipelining warnings. This is implementation evidence,
not a hardware functional, power or thermal qualification.

Stable local artifacts:

```text
work/zturn-v2/bringup/artifacts/thermal-generated/zturn_v2.bit
work/zturn-v2/bringup/artifacts/thermal-generated/zturn_v2.xsa
work/zturn-v2/bringup/artifacts/thermal-generated/probe-pair.json
work/zturn-v2/bringup/artifacts/thermal-generated/SHA256SUMS
```

The bitstream SHA-256 is
`0ee4640b2739ad17587195c6a2dfb77613c41fbe6e22ca63650097f2b5228ba3`.
The credential-bearing host and its pairing remain under the private workspace.
The complete FPGA marking, including package, speed and temperature grade, is
still to be checked against the target `xc7z020clg400-2`.

At this offline checkpoint, board work was held pending a heatsink. The user
subsequently withdrew that requirement and authorized the native test and then
the first FPGA run described below. At this offline checkpoint the candidate had
not been applied to the board. Its protection cannot power off the ARM cores or
board regulators. Remove both DC and USB_UART power
before mechanical work: USB keeps this board running. Do not execute provisional
generated PS/DDR initialization; retain the factory setup until an exact V2 export
is qualified.

## Native temperature reading without heatsink

On 2026-11-24 the user explicitly authorized resuming without a heatsink. USB UART
was restored at 115200 8N1 and the direct Ethernet link negotiated 1000 Mbit/s,
full duplex. Factory Linux's IIO XADC initially reported raw 2626, approximately
50 °C. The laptop used `10.0.2.2/24` and the board `10.0.2.15/24`.

A separate `shoop-health-native.elf` was copied through factory SSH to FAT and
hash-verified. `BOOT.BIN` and `image.ub` retained their factory hashes. Linux was
rebooted cleanly and factory U-Boot's countdown interrupted. No Shoop FPGA image,
generated PS initialization or QSPI image was programmed.

Live U-Boot readings confirmed BOOT_MODE `0x00000005` (SD), IO_PLL_CTRL
`0x0001e008`, FPGA0_CLK_CTRL `0x00200500` (nominal 100 MHz) and PCAP_CLK_CTRL
`0x00000501`. The PS-XADC serial interface was held in reset: MCTL `0x10`,
CFG `0x1114`, status `0x500` (both FIFOs empty). Before launching the tested ELF,
`mw.l 0xf8007118 0 1` released that communication-interface reset. This does not
reset the converter or change its sequencer/thermal thresholds. See UG585's
[PS-XADC interface reset](https://docs.amd.com/r/en-US/ug585-zynq-7000-SoC-TRM/PS-XADC-Interface-Reset).
The source now performs this release automatically only with empty FIFOs; that
follow-up initialization is covered by the C fixture, not this physical run.

Tested build ID: `zturn-health-native`; 7,471,656 bytes, CRC32 `5514a505`, SHA-256
`449ec992013baf17a32ee360a68bc66e9134bb85901e9db9be43efec923b45f1`.
It was staged at `0x38000000`, outside all load regions, and launched with a fresh
boot nonce through `bootelf -p`. Native FreeRTOS/lwIP reached `SHOOP_HOST_READY`
and gigabit networking at PHY address 7.

The authenticated `zynq_health` probe observed the common `host.health` source at
approximately 1 Hz for 15 seconds, with no audio/task profiling or stress workload:

- **15 samples**, die temperature **52.539–53.154 °C**.
- Hardware-stored maximum reached **53.523 °C**; it is not a peak-since-boot claim.
- All sampled overtemperature states were false.
- Processed frames advanced from 5,120 to 679,936 over approximately 14.05 seconds,
  consistent with the nominal 48 kHz dummy renderer.
- The probe used a conservative **65 °C stop threshold**, stopped on absent/stale
  readings, and requested shutdown after the short run.
- Shutdown was confirmed; UART reported all 32 dummy MIDI reset packets, complete
  cleanup, zero remaining tracked Rust allocation and `SHOOP_HOST_RETURNED`.
  Peak tracked Rust allocation was **551,220,496 bytes**.

[Evidence and checksums](zturn-v2/2026-11-24/native-health/result.json) contain the
samples, native UART transcript, register observations and credential-free build
manifest. The credential-bearing ELF remains private. The firmware was
intentionally stopped after the test; the board remains powered, not serving a
session. Another launch requires a reset.

This verifies native startup and live sensor delivery through the diagnostics
client, not a GUI paint test, sustained load, physical I²S, FPGA functionality,
thermal-trip behavior or safe heatsink-free operation. Telemetry does not stop
the ARM locally or cut board power. The protected PL bitstream was still offline
at this native-test checkpoint.

## First physical FPGA host and connected desktop UI

On 2026-11-25 the user requested the same connected UI setup using the physical
FPGA backend. The native session had already shut down with zero remaining tracked
Rust allocation. After one physical reset, factory Linux booted, the direct-link
address was restored, and separate `shoop-thermal-generated.bit` and
`shoop-health-fpga.elf` files were staged over SSH and SHA-256 verified. Both
factory `BOOT.BIN` and `image.ub` hashes were unchanged before and after staging.

Linux rebooted cleanly into factory U-Boot. The original PS/DDR initialization
and clocks were retained; no generated PS initialization, QSPI write or persistent
boot-environment change was performed. Live PS IDCODE was `0x23727093` (7020),
BOOT_MODE `0x5`, IO_PLL_CTRL `0x0001e008`, FCLK0 control `0x00200500` and PCAP
clock control `0x00000501`.

Factory `fpga loadb` programmed the protected bitstream through PCAP from a DDR
staging buffer at `0x02000000`. The SD bitstream was 4,045,674 bytes, CRC32
`4168b660`, with the SHA-256 recorded above. Loading returned success and PCAP
configuration DONE was asserted. Read-only PS-XADC DRP transactions confirmed
live protection registers `0x41=0000`, `0x42=0800`, `0x53=b0d3`, `0x57=abc4`.
Only the PS serial interface was configured/released from reset; the probe did
not write internal ADC configuration or thresholds. The live FPGA register bank
reported ID `0x53484f50`, ABI 12, clock contract 100 MHz and a 128 MiB arena at
`0x30000000`.

The authenticated firmware was `zturn-health-fpga`, 5,962,984 bytes, CRC32
`de088650`, SHA-256
`d5fbdf77a974fe40c45b19fc89d50d88be72d671b9f8540f04c5e10886c737f9`.
It was staged at `0x38000000`, given a fresh verified nonce, and launched with
`bootelf -p`. UART reported **`SHOOP_FPGA_OWNER_READY`** followed by
**`SHOOP_HOST_READY`**, with gigabit PHY link at address 7. This was physical RTL
execution, not the software RTL simulator or ARM-native audio renderer. The host
currently shares the `BackendType::RtlSim` setting/“RTL simulation” UI label with
the simulator; that label is not evidence of the physical transport in use.

A 15-second authenticated health/progress preflight, with a 65 °C cutoff and
stale/missing-sensor abort, passed:

- **16 samples**, current die temperature **54.261–54.631 °C**.
- Hardware-retained maximum **55.369 °C**, no sampled overtemperature alarm.
- Processed frames **1,664 → 721,440** across 15.000 seconds of samples, consistent
  with generated audio at nominal 48 kHz.
- **Zero reported xruns** throughout the check.

On success the probe used its explicit `--handoff` option to disconnect without
stopping the host; failures retain shutdown behavior. The laptop desktop UI then
launched with the existing private profile and its established TCP data connection
to `10.0.2.15:15400` was verified. The UI log showed no runtime error. At this
handoff the FPGA host was deliberately left running; the normal UI does **not**
retain the probe's 65 °C automatic cutoff. Close with **Close UI and stop host…**
to request clean shutdown.

[Evidence and checksums](zturn-v2/2026-11-25/fpga-host/result.json) retain the
credential-free startup, FPGA load, live register contract, temperature/progress
samples, firmware manifest and factory hashes. Credentials, nonce, ELF and local
bitstream remain private/ignored. Factory `bootelf` emitted its existing cache
alignment warnings before successful startup. This is initial startup, idle
rendering and remote-client handoff evidence, not a physical I²S/audio-output,
recording-loop, sustained-load, thermal-trip or heatsink-free qualification. The
complete chip package/speed/temperature marking is still unverified.

After the UI handoff, the user reported a crash. They explicitly requested no
investigation and a repeat of the same setup. No cause was established and no
stability qualification is implied by the successful initial health check.

### FPGA session capacity at this checkpoint

The bitstream reserves 128 MiB of DDR, of which 124 MiB is audio sample storage
shared by all channels/loops. The rest accommodates storage metadata, program
images and headroom. Raw four-byte samples at 48 kHz give an ideal capacity of
approximately 339 seconds of stereo audio across all recordings; block allocation
and retained/shared recordings affect usable capacity.

Separately, this firmware configures host-local session admission/transfer limits
of **32 MiB media**, **1 MiB serialized metadata**, **512 transferred objects**
(including metadata) and **65,536 MIDI events**. The 1 MiB serialized-metadata
ceiling is not the FPGA's internal recording-metadata capacity. The 32 MiB media
ceiling is smaller than the physical recording arena and can constrain session
load/save/capture paths independently. These are configured ceilings, not proof
that all those workflows have been hardware-qualified.

The FPGA host settings select eight tracks including sync, 128 loop slots shared
across tracks, disabled processors and a 32-frame quantum at 48 kHz. MIDI execution
is disabled in this image despite the common host-side event ceiling. These limits
come from `shoop_host_freertos`' FPGA host setup, `shoop_rt_host_reference`'s program
limits, the board profile and the bitstream's looping geometry.
