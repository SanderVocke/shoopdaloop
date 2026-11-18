# Zynq-7000 board profiles

Select a board with `--board qemu` (the default), `--board zedboard`, or
`--board zturn-v2` on
`build.py`, `run.py`, `integration.py`, `deployment/build_boot.py`,
`deployment/bundle.py`, `deployment/card.py` and `deployment/emulator.py`.
Existing reference suite commands retain their default board and output paths.
The full deployment/watchdog and USB suites qualify the QEMU reference profile.
Other profiles can exercise their firmware on the emulated processing system;
QEMU does not model their physical wiring or DDR initialization.

| Contract | `qemu` | `zedboard` | `zturn-v2` |
| --- | --- | --- | --- |
| CPU family | Zynq-7000, one Cortex-A9 | Zynq-7000, one Cortex-A9 | Zynq-7000, one Cortex-A9 |
| PS DDR | 1024 MiB | 512 MiB | 1024 MiB |
| Low reservation | 0–1 MiB | 0–1 MiB | 0–1 MiB |
| Platform code/heaps/stacks/DMA | 1–128 MiB | 1–64 MiB | 1–128 MiB |
| Rust arena | 128–896 MiB | 64–384 MiB | 128–768 MiB |
| PL arena | — | — | 768–896 MiB |
| Two update buffers | 896–960 MiB | 384–448 MiB | 896–960 MiB |
| Loader/stack reservation | 960–1024 MiB | 448–512 MiB | 960–1024 MiB |
| Recording pages | 128 MiB | 32 MiB | 128 MiB |
| Decoded session media | 64 MiB | 16 MiB | 32 MiB |
| Native loop slots | 64 | 32 | 64 |
| Boot initialization | Pinned upstream ZC702 | Pinned upstream ZedBoard | Pinned upstream Z-turn development baseline |
| Audio | Dummy stereo, 48 kHz | Dummy stereo, 48 kHz | Dummy stereo, 48 kHz |
| MIDI | Dummy or optional USB0 | Dummy | Dummy |

The JSON profile is the authority for board-specific settings. `boards.py`
validates it and generates `shoop_board.h`, the linker script and a resolved
`board.json` in the build directory. CPU/timer clocks, GEM clock divisors and
interrupt routing, UART and SD controller settings feed the shared AMD BSP.
The bootloader uses the selected device tree and verifies the SHA-256 of its
PS initialization against the pinned U-Boot revision.

The shared Zynq-7000 implementation owns interrupts, caches, timers and peripheral
drivers. Board reset hooks own wiring-specific operations such as the ZC702 USB
PHY reset. Firmware policy selects recording/media budgets and supported I/O
implementations. A real codec or FPGA audio path needs a separate driver;
selecting a board does not enable audio hardware. Zynq UltraScale+ MPSoC requires
a separate platform implementation, rather than another Cortex-A9 profile.

Memory admission reserves recording pages, three simultaneous media owners and
an explicit runtime/headroom budget. It rejects overlapping firmware/update/
loader windows or budgets beyond DDR. Runtime allocation measurements remain
necessary: the static reserve is a policy bound, not a measured worst-case proof.
The generated linker bounds and bootloader ELF checks enforce the same windows;
TFTP and Fastboot use the profile's staging address. The boot record, signed FIT
and firmware/loader manifests identify the board. Packaging and provisioning
reject mixed board artifacts. Changing a board's memory or ABI contract requires
a new deployment profile identifier; deployed loaders keep their original policy.

## MYIR Z-turn V2 development profile

The profile targets `xc7z020clg400-2`. The MYIR V2 schematic confirms two
256M×16 DDR3 devices, GEM0 RGMII on MIO 16–27, MDIO on MIO 52–53, UART1 on
MIO 48–49, and PHY address 7. Ethernet reset follows board power-good. The
schematic labels the part `YT8531SC` and the product page lists `YT8531SH`. The
firmware checks the Motorcomm YT8531 MDIO ID before configuring the RGMII delays
and enabling negotiation. Physical bring-up must confirm the populated part,
link at the advertised speeds, and the board's exact DDR training parameters.
[MYIR schematic](https://www.myirtech.com/down-load1.asp?id=1027) ·
[MYIR specifications](https://www.myirtech.com/list.asp?id=708)

The schematic used for the development profile has SHA-256
`7fe273655dee3b5a2cade185efc584f68f3e6b86e9b658df405ae679615198ae`.
The upstream U-Boot Z-turn PS initialization is pinned for reproducible builds.
The V2-specific PS initialization and physical SD boot are qualification items.
The DTS overlay selects address 7 in U-Boot. Direct-load QEMU uses the built-in
Marvell PHY model at the same address. [Pinned Z-turn PS baseline](https://github.com/u-boot/u-boot/blob/5508406582f6f7120dce0c4b3059819a41788db2/board/xilinx/zynq/zynq-zturn/ps7_init_gpl.c)

## Choose JTAG development or SD deployment

For a development loop that loads a bare-metal ELF through JTAG, start with
[JTAG development without Vivado](jtag-development.md). It covers workstation
tools, OpenOCD, PS/DDR initialization requirements, the development credential,
boot nonce, UART, Ethernet and hardware verification checkpoints. The physical
procedure is an initial recipe; its PS initialization helper is still an explicit
gap to resolve during bring-up.

JTAG uses `host.elf`, built without `--deployment`, and does not need SD.
The commands below follow the separate SD/network deployment path using
`host-deployment.elf`. That variant expects the loader's handoff and provisioned
SD state. FreeRTOS itself does not impose an SD requirement. Neither workflow
requires installing Vivado or Vitis; hardware initialization is still required.

## Build and test before hardware

Use the pinned toolchain and dependencies described in the platform and deployment
guides. From the repository root:

```sh
python3 platforms/zynq7000/tests/boards.py
python3 platforms/zynq7000/build.py --fetch --board zedboard --app host --deployment
python3 platforms/zynq7000/deployment/build_boot.py --fetch --board zedboard
python3 platforms/zynq7000/deployment/build_qemu.py --fetch
python3 platforms/zynq7000/tests/deployment.py --board zedboard \
  --directory work/zynq/zedboard-deployment-test
python3 platforms/zynq7000/integration.py --board zedboard
python3 platforms/zynq7000/integration.py --board zedboard --fixture --small-fixture
```

Firmware outputs are in `work/zynq/boards/zedboard/build/`; bootloader outputs
are in `work/zynq/boards/zedboard/u-boot-build/`. Source downloads are cached
under `work/zynq/`; non-reference bootloader builds use isolated source copies.
Firmware Cargo outputs are isolated by board; builds for different variants of
one board should run serially in one checkout.
CI cross-builds both profiles and runs their native-client firmware tests in
separate jobs, including signed SD boot, an A/B network update and a persistent
restart. The deployment test requires a new output directory and keeps disposable
keys, card images and packet captures under its private directory. Its public
`result.json` records the outcome. The reduced fixture includes exact PCM/MIDI, the board's media
admission limit and lifecycle soak; it omits the reference 40 MiB/full-pool gates.

The deployment emulator's recorded `qemu-board-phy.patch` adds a `phy-address`
machine property (default 7). The launcher selects the profile's PHY address:
7 for the reference or 0 for ZedBoard. This exercises the board's original
device tree without changing its PHY wiring to match the emulator. BootROM,
physical DDR training and electrical behavior still require hardware testing.

After building, sign and provision with the same board selection:

```sh
python3 platforms/zynq7000/deployment/bundle.py --board zedboard \
  --elf work/zynq/boards/zedboard/build/host-deployment.elf --build-id direct-dev \
  --key-dir /path/to/signing-keys --output work/zynq/zedboard.itb \
  --loader-output work/zynq/zedboard-loader
python3 platforms/zynq7000/deployment/card.py --board zedboard \
  --boot-files work/zynq/zedboard-loader --bundle work/zynq/zedboard.itb \
  --build-id direct-dev --credential-file /path/to/device.key \
  --mac 02:00:00:00:00:02 --ip 192.168.1.50 --netmask 255.255.255.0 \
  --gateway 192.168.1.1 --server 192.168.1.10 --output work/zynq/zedboard.img
```

Use a fresh provisioned device credential and your own trusted-LAN addresses.
Keep the firmware's build manifest beside its ELF when packaging. Card images
are regular files; the tools do not write physical block devices. A physical SD
card may be larger than the image; loader I/O stays bounded to the defined layout.

## Adding a board

1. Add a JSON profile with a unique name and deployment identifier. Set DDR and
   firmware budgets, PS clocks, peripheral selections and clock divisors from
   that board's hardware export.
2. Pin the matching U-Boot device tree and PS initialization source/hash. Review
   its DDR, MIO, peripheral reset and PHY settings against the board revision.
3. Add wiring-specific reset hooks for optional peripherals and explicitly enable
   only implemented I/O drivers. Unsupported combinations must fail at build time.
4. Add the profile to the CI board matrix. Run contract tests, cross-build firmware
   and SPL/U-Boot, then run firmware
   integration at the profile's RAM size. Preserve the reference regression gates.
5. Qualify physical hardware last and record evidence before claiming support.

## Final stage: physical ZedBoard qualification (pending)

No physical ZedBoard test has been performed. After software checks pass:

1. Verify the board revision, PS initialization, supply/jumpers and SD boot straps.
   Program a spare card with the provisioned image using a separately chosen tool.
2. Capture UART1 at 115200 baud. Verify BootROM → SPL → U-Boot → FreeRTOS,
   512 MiB DDR, correct clocks and the generated memory bounds.
3. Connect Ethernet on the configured trusted LAN. Test native UI attach, session
   transfer within 16 MiB, dummy rendering, disconnect/reconnect and clean shutdown.
   Record allocation peaks, stack margins and DMA/cache behavior on silicon.
4. Test signed A/B updates, reboot identity, confirmation, watchdog rollback and
   interrupted SD writes/power loss. Keep recovery media and serial logs.
5. Qualify USB PHY/reset/VBUS wiring before enabling USB MIDI. Codec/FPGA audio
   development and audio timing qualification follow as separate driver work.
