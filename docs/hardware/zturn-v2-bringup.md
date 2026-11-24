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

## Follow-up: protected FPGA bitstream ready; hardware work on hold

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

**Stop here until the user has fitted a heatsink and resumes the work.** No board
commands, SD writes, programming or tests were issued after this hold. The new
protection is in the candidate only; it has not been applied to the board and
cannot power off the ARM cores or board regulators. Remove both DC and USB_UART
power before fitting the heatsink: USB previously kept this board running.
Do not execute the provisional generated PS/DDR initialization on hardware;
retain the factory initialization until an exact V2 export is qualified.
