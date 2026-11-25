# Z-turn V2 FPGA bridge bring-up

The development bridge maps AXI-Lite through PS GP0 and connects the 64-bit
media master to PS HP0. It reserves DDR for the PL and exposes the register
map defined in `registers.json`. Run the generators after changing the ABI or
wrapper. The generated audio build uses a deterministic mono source at 48 kHz
and sends each quantum through the stereo audio bank controller.

## Automatic PL temperature protection

Every Z-turn V2 build includes an autonomous XADC monitor and enables
`BITSTREAM.CONFIG.OVERTEMPPOWERDOWN`. The nominal trip point is **75°C junction
temperature**, with a nominal lower threshold of 65°C (`INIT_53=B0D3`,
`INIT_57=ABC4`). The default monitoring sequence and OT alarm remain enabled;
fabric DRP writes and reset are tied inactive. The monitor is retained even
though its ordinary fabric outputs are unused. XADC can fall back to its
internal oscillator if the fabric clock stops.

This is emergency **PL-only** protection, not ARM throttling or a board power
switch. Sensor accuracy and response delay still require thermal margin. It
only takes effect after the protected bitstream is configured; it does not
retrofit protection into the factory image. Fit appropriate cooling before
hardware tests and do not use repeated thermal trips as normal regulation.

The build checks the retained XADC parameters and shutdown property before
bitstream generation and again on the routed design, writing `thermal.rpt`.
Missing/disabled protection aborts the build. The portable regression test
requires a Tcl interpreter (`TCLSH` can select its path):

```sh
python3 fpga/host_bridge/test_thermal.py
```

[AMD XADC thermal management](https://docs.amd.com/r/en-US/ug480_7Series_XADC/Automatic-Alarms) ·
[Zynq critical overtemperature behavior](https://docs.amd.com/r/en-US/ug585-zynq-7000-SoC-TRM/Critical-Over-temperature-Alarm)

## Vivado and diagnostic firmware

Use Vivado 2021.1 with the matching IP catalog. The workspace build uses
`SHOOP_VIVADO_SERIAL_SYNTH=1` to disable the parallel synthesis helper and
synthesize the block design together with its top-level wrapper.
Use the provisional PS settings below to reproduce routing and generate
reports in the workspace. Use the exact MYIR V2 PS configuration for JTAG
and verify the populated DDR and connector configuration before powering the
physical codec. The development preset uses DDR3-1066 with seven cycles of
CAS read latency and six of write latency, a 50 ns four-activate window, and
a 37.5 ns minimum active time. The Micron datasheets specify these settings
for the x16 geometry and compatible DDR3-1066 speed bin. The V2 board delays
and training still need qualification. [Micron part datasheet](https://s3.amazonaws.com/snapeda/datasheet/MT41K256M16TW-107%3AP_Micron_Technology.pdf) · [Micron 1.5 V timing](https://www.micro-semiconductor.com/datasheet/56-MT41J256M16RE-15E-IT-D.pdf) · [Compiler and catalog provenance](../zedboard/CATALOG.md)

```sh
SHOOP_VIVADO_SERIAL_SYNTH=1 vivado -mode batch \
  -source fpga/host_bridge/zturn_v2.tcl \
  -tclargs work/zturn-v2/route fpga/host_bridge/zturn_v2_route.ps bitstream
python3 platforms/zynq7000/build.py --board zturn-v2 --app host --fpga-probe
python3 platforms/zynq7000/fpga_pair.py pair work/zturn-v2/route \
  work/zynq/boards/zturn-v2/build/host-fpga-probe.elf work/zturn-v2/diagnostic-pair.json
python3 platforms/zynq7000/fpga_pair.py verify work/zturn-v2/diagnostic-pair.json
```

The paired manifest records the board and mailbox contract and hashes the ELF,
bitstream, XSA and PS files. The signed SD deployment has its own manifest.
Mailbox version 14 retains composite ending handoffs for Record and Stop
and conditional restarts for live edits. A switch can include the edited
loop slot in bits 12:1 of payload word 4 and a 32-bit condition split across
bits 31:16 of words 5 and 6, low half
first, with words numbered from zero. A nonzero limit arms a check for the
next audio quantum. Values with high half `0xffff` request a restart check.
The low half gives the outgoing source slot, `4096` for an absent source, or
`0xffff` for an unchanged source. The response gives the armed frame and
the current revision. Read the revision after that quantum to learn whether the switch
took effect. Use a version 14 bitstream with the matching host or firmware.
The diagnostic ELF checks the fabric aperture and requests an empty snapshot.
Pass `looping` after `bitstream` to try the resource bounds used by the
production image packer. This geometry uses 32 samples per media chunk and
reserves the upper portion of the PL arena for metadata.

With a hardware-qualified pair, start the hardware server and capture UART1 at
115200 baud. Use the JTAG command with the address of the server attached to
that board. The script resets the board, initializes the PS, configures the
fabric, checks DDR, then boots the probe on CPU0 with fresh boot identity.

```sh
python3 platforms/zynq7000/fpga_pair.py jtag work/zturn-v2/diagnostic-pair.json \
  --server TCP:127.0.0.1:3121 --uart /dev/ttyUSB0 \
  --uart-log work/zturn-v2/diagnostic-uart.log
```

Expect `SHOOP_FPGA_PROBE_READY` and `SHOOP_FPGA_PROBE_RETURNED` on UART. The
QEMU startup test exercises the bounded refusal path without a configured PL.

## Staged bulk control (mailbox ABI 15)

Current source adds a bounded compare/masked-write batch for future Transition
groups. ARM staging does not hold audio; whole-batch validation and commit happen
locally in one unchanged bounded control window. The SP still owns musical sync
and transition execution; no transition processor is reintroduced. See the
[wire contract, scope and regression commands](control_batch.md).

A protected matching ABI-15 pair passed routing and one supervised 16/32-loop
future-recording-arm run plus two recording-active backend restarts. See the
[physical evidence and limits](../../docs/hardware/zturn-v2/2026-11-25/bulk-arming/README.md).
This is not physical-output, soak or standalone-SD qualification. The ABI-14 pair
remains preserved; its recovery results are a separate earlier checkpoint.

## Fault-safe MMIO and retained diagnostics (introduced in mailbox ABI 14)

Aligned, full-strobe writes to defined mailbox command registers receive AXI
`OKAY` even when the operation is rejected due to a fault, full queue, stale
identity, missing session or an outdated acknowledgement. Rejected operations do
not mutate the active command/response/event. Unknown/read-only addresses and
malformed accesses still produce `SLVERR`; ordinary lifecycle races must not.

After `REQUEST_PUSH`, the exclusive register owner reads `SUBMISSION_RESULT`
(`0x120`): accepted 0, full 1, epoch mismatch 2, fault 3, invalid tag 4, unavailable
session 5. It rechecks epoch and translates the receipt into a normal driver
error. Response/event acknowledgements and ABORT recheck epoch after writing.
The receipt is acceptance, not execution completion; a later fault still fails
an accepted command through normal status polling. There must be only one
mailbox writer; this is not a multi-producer transaction protocol.

`FAULT_EPOCH` through `FAULT_FRAME_HIGH` (`0x124`–`0x14c`) expose one first-fault
record for the latest runtime that asserted FAULT, including an annotated
intentional recovery ABORT. The dispatcher latches a cause mask,
last accepted mailbox tag/op, execution/control cycles, phase/error detail,
graph revision and frame before software ABORT. Cause bits are memory, audio,
event overflow, control timeout, ABORT, watchdog, pipeline error and recovery
annotation (bits 0–7). Detail bits 4:0 contain dispatcher phase, 9:8 pipeline
error stage, and 23:16 pipeline error code. Paced audio uses tag zero and the
RUN_QUANTUM operation. This is mailbox context, not an application batch ID.

The always-on mailbox retains the record through epoch-zero/datapath reset.
Additional causes and ABORT in the same epoch cannot overwrite it; the first
fault of a later epoch replaces it. Whole-device reset/reconfiguration clears
it. Readers bracket the record with `FAULT_EPOCH` and reject a changed identity.
The owner logs `SHOOP_FPGA_FIRST_FAULT` before issuing its failure ABORT. These
registers are not yet exposed through a new management diagnostic API or UI.

A protected ABI-14 pair subsequently passed three healthy backend restarts and
one after a retained control-timeout fault, without rebooting ARM. See the
[physical evidence and limits](../../docs/hardware/zturn-v2/2026-11-25/fault-containment/README.md).
The later ABI-15 timing-fix run is linked above; the earlier ABI-14 results alone
do not qualify that timing fix.

## Runtime recovery (introduced in ABI 13, retained in ABI 15)

`RECOVERY_ID` at `0x114` advertises `0x52535431`; `RECOVERY_STATUS` at `0x118`
reports idle (0), drain (1), reset (2), ready (3), or failed (4). Writing the live
mailbox epoch with full strobes to `RECOVERY_REQUEST` at `0x11c` starts recovery
only from idle/ready. The ARM supervisor must first stop the owner; raw reset
register writes are not a supported user-facing recovery procedure.

The controller is outside the processing reset and tracks accepted AW-to-B and
AR-to-final-R transactions. It aborts new execution, suppresses maintenance, and
requires consecutive datapath-idle/zero-outstanding observations before a 128-cycle
processing reset. Drain has a 100,000,000-cycle bound. The audio adapter receives
the same runtime reset and fresh epoch; GP0, the recovery aperture and thermal
monitor remain running. Epoch zero flushes mailbox staging/replies/events during
reset. Counter errors, timeout or unexpected reset-time traffic fail closed;
only a ready state with exactly one epoch advance is a successful fence.

No control-window deadline is enlarged. Recovery remains distinct from the
ABI-15 staged-batch timing fix above. Regression tests are in `rt_fpga_rtl`'s
`recovery` and `recovery_pipeline` test targets. The ABI-15 physical run is bounded
and does not exhaust recovery/failure scenarios. The historical ABI-13 pair
passed three healthy restarts but lost ARM responsiveness during bulk arming;
ABI-12 images do not provide the recovery controller. See [mailbox investigation and proposal](../../BUG_MAILBOX.md).

## Production FPGA firmware

Build `--fpga` with the looping geometry to run the authenticated FreeRTOS
service with the fabric renderer. A direct-load image takes the same
32-byte development credential as the native host. The QEMU test creates its
own credential and exercises network control and shutdown against scripted
registers. Marlin exercises the same owner through the actual AXI mailbox.

```sh
python3 platforms/zynq7000/tests/fpga_host.py
cargo test --locked -p shoop_rt_host_marlin --features sim --test mailbox_endpoint
```

Use `SHOOP_ZYNQ_FPGA=1 cargo run --locked -p shoop_host --no-default-features
--example zynq_probe -- PROFILE.json` to run the authenticated FPGA smoke
against the TCP endpoints in the profile. This test shuts down the host.
Set `SHOOP_ZYNQ_FPGA_BENCH_SECONDS=1800` to include a 64-loop recording and
playback run. It changes the graph while playback continues, sends repeated
authenticated management requests over Ethernet and reports completion time,
xruns, control latency and FreeRTOS task CPU use.

## Provisional Pmod I²S2 adapter

Append `looping i2s` after `bitstream` when building with Vivado. The
schematic marked Ver 1 ties J11 bank 13 to 3.3 V. Use the Pmod in ADC slave
mode with JP1 at SLV. Supply 3.3 V and a common ground. One external
12.288 MHz oscillator drives the FPGA audio reference and both Pmod MCLK
inputs. The FPGA generates 3.072 MHz SCLK and 48 kHz LRCLK. The current
graph consumes the left ADC channel and drives both DAC channels.

| Signal | MYIR J11 | FPGA ball | Pmod J1 |
| --- | --- | --- | --- |
| 12.288 MHz reference | 7 | U7 | 1 and 7 |
| SCLK | 11 | T9 | 3 and 9 |
| LRCLK | 13 | U10 | 2 and 8 |
| DAC data | 17 | Y7 | 4 |
| ADC data | 19 | Y6 | 10 |
| Common ground | 2 | — | 5 and 11 |

Confirm the populated board, power and connector orientation at the bench.
The physical adapter uses a dedicated clock-capable input and the audio-bank
crossing. The batch build applies the bundled-data delay and skew constraints
and exports timing and detailed CDC reports. [MYIR schematic](https://www.myirtech.com/down-load1.asp?id=1027) · [Digilent reference manual](https://media.digikey.com/pdf/Data%20Sheets/Digilent%20PDFs/Pmod_I2S2_RM_Web.pdf)

Run the checkpoint check after building the I²S image.

```sh
vivado -mode batch -source fpga/host_bridge/zturn_v2_i2s_check.tcl \
  -tclargs work/zturn-v2/route
```

It writes `skew-signoff/arrival-bounds.rpt`, checks the original datapath-only
max-delay slack, and measures the spread of capture-relative arrivals across
both timing corners, including clock insertion and uncertainty. The script
works on an in-memory copy of the routed checkpoint. Set
`SHOOP_VIVADO_FULL_SKEW=1` during the build to request Vivado's exhaustive
report after bitstream and XSA export. Vivado 2021.1 can stall when generating
that report for wide buses between independent clocks.

## Signed SD image

Build the loader and deployment firmware, then pair the ELF and the looping
bitstream. The FIT signature binds both images to the board profile. Keep
device credentials and the deployment signing key in private files. The
loader verifies both hashes before programming the fabric and starting ARM.
Use a dedicated route and release directory for each candidate. Copy the ELF
and its manifest to the release directory before pairing, so a subsequent
firmware build can reuse the board build directory.

```sh
python3 platforms/zynq7000/deployment/build_boot.py --board zturn-v2 --fpga-build work/zturn-v2/route
python3 platforms/zynq7000/build.py --board zturn-v2 --app host --fpga --deployment --build-id zturn-fpga
mkdir -p work/zturn-v2/sd
cp work/zynq/boards/zturn-v2/build/host-deployment-fpga.elf work/zturn-v2/sd/
cp work/zynq/boards/zturn-v2/build/host-deployment-fpga.json work/zturn-v2/sd/
python3 platforms/zynq7000/fpga_pair.py pair work/zturn-v2/route \
  work/zturn-v2/sd/host-deployment-fpga.elf work/zturn-v2/sd/pair.json
python3 platforms/zynq7000/deployment/bundle.py --board zturn-v2 \
  --elf work/zturn-v2/sd/host-deployment-fpga.elf --build-id zturn-fpga \
  --key-dir "$SHOOP_SIGNING_KEYS" --fpga-pair work/zturn-v2/sd/pair.json \
  --output work/zturn-v2/sd/shoop.itb --loader-output work/zturn-v2/sd/loader
python3 platforms/zynq7000/deployment/card.py --board zturn-v2 \
  --build-id zturn-fpga --boot-files work/zturn-v2/sd/loader \
  --bundle work/zturn-v2/sd/shoop.itb --credential-file "$SHOOP_DEVICE_CREDENTIAL" \
  --output work/zturn-v2/sd/card.img
python3 platforms/zynq7000/tests/fpga_sd.py work/zturn-v2/sd
```

Supply the deployment key and certificate, credential and network settings
described in the deployment guide. The QEMU gate checks authentication of
the signed pair through U-Boot and refusal of a changed fabric payload.
Physical PCAP programming requires the board. [Deployment guide](../../platforms/zynq7000/deployment/README.md)

Keep the factory loader directory, including `factory.json`, with the factory
image. To package a later FPGA release, pair its routed artifacts and firmware
as above and pass `--factory-loader work/zturn-v2/sd/loader` to `bundle.py`.
Write the new FIT to the release directory. The signer and installed loader
check the same versioned PS contract. It covers the board profile and the
XSA's generated `ps7_init_gpl.c` and `ps7_init_gpl.h`. A new PL build can have
a different XSA and bitstream while using the same PS initialization. A PS
initialization change needs a separately provisioned loader. Factory loaders
built before this contract need to be regenerated once for this update flow.

## Native I²S comparison

Use `bitstream native i2s` in the Vivado command and a fresh route directory.
Build the host with `--native-i2s --deployment` in place of `--fpga --deployment`.
For the signed SD commands, use `host-deployment-native-i2s.elf`, its manifest,
the native route and a separate release directory. The pairing contract binds
the native ELF to the audio-only register map. The default 66-loop pool leaves
room for the sync loop and one live edit during the 64-loop workload.

```sh
SHOOP_ZYNQ_NATIVE_I2S=1 SHOOP_ZYNQ_NATIVE_I2S_BENCH_SECONDS=1800 \
  cargo run --locked -p shoop_host --no-default-features --example zynq_probe -- PROFILE.json
```

Leave out `SHOOP_ZYNQ_NATIVE_I2S_BENCH_SECONDS` for the short record/play smoke.
Capture `SHOOP_NATIVE_I2S_RESULT` on UART1. `peak_callback_ticks` and `callback_hz`
measure the ARM callback with PCM conversion. `peak_bank_cycles` and `fabric_hz`
measure the time from GP0 bank availability to release, including IRQ dispatch
and PCM transfers. The client reports native deadline misses and stops the timed
comparison at the first observed miss.
