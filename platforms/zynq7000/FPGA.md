# FreeRTOS FPGA host variant

`build.py --fpga` selects the shared FPGA adapter in place of the CPU renderer.
The network service and app protocol stay the same. The PL generates and sinks
audio; the ARM controls the core and reads observations. The CPU firmware remains
the default and retains its existing QEMU gates.

This path is under bring-up. The last host bitstream did not close timing at
100 MHz. A physical end-to-end app session and hardware transaction replay still
require the board. The benchmark's checked-in bitstream is a different flavor
and cannot run this firmware.

## Build and boot

Use the toolchain in `pins.json` and a 32-byte binary development credential:

```sh
python3 platforms/zynq7000/build.py --fetch --board zedboard --app host \
  --fpga --credential-file work/zynq/host.key
python3 fpga/zedboard/configure_host.py --output work/zedboard/host
vivado -mode batch -source fpga/zedboard/build.tcl -tclargs work/zedboard/host
```

The ELF and its audited manifest are in
`work/zynq/boards/zedboard/build-fpga/host-fpga.{elf,json}`. The firmware heap
ends at `0x17800000`; the following 8 MiB belong to the PL. The shared bridge
profile fits inside that reservation. Firmware checks the bridge ID, register
version, arena base and PL-generated audio setting before preparing execution.

For a register-only hardware check, run:

```sh
xsdb fpga/zedboard/host_check.tcl work/zedboard/host
```

That script programs the host bitstream, initializes PS/DDR, keeps both CPUs
halted and attempts one empty quantum. Its corrected PREPARE command still needs
to be rerun on hardware. For firmware boot, the required sequence is system
reset, halt both CPUs, program the host bitstream, run its generated `ps7_init`
and `ps7_post_config`, then download the ELF and supply a fresh nonzero 64-bit
nonce at `0x8000` before starting CPU0. Use the existing
[JTAG guide](boards/jtag-development.md) for ELF loading, nonce handling and
the network connection. A combined, hardware-qualified loader remains pending.

## Register capture

Build a separate diagnostic ELF, optionally specifying the maximum record count:

```sh
python3 platforms/zynq7000/build.py --board zedboard --app host --fpga \
  --fpga-trace 1048576 --credential-file work/zynq/host.key
```

This produces `host-fpga-trace.elf`. Capture its UART output from boot, exercise
a short scripted session, and request host shutdown through the app protocol.
After execution stops, firmware reads the counters and output digest before
resetting the core. It acknowledges shutdown before dumping the trace to UART.
Keep recording until `SHOOP_FPGA_TRACE_END` appears. A large dump can take minutes
at 115200 baud. Only the capture buffer remains allocated during the dump; the
execution owner is released before shutdown acknowledgement. Final memory
diagnostics run after the buffer is released.

Capture covers every register access from the initial identity probe through
cleanup. The buffer allocates once before execution and never grows during bus
accesses. Each record currently uses 12 bytes; the default buffer is 12 MiB.
`--fpga-trace` without a count uses 1,048,576 records; the accepted range is
1..4,194,304, additionally limited by the board's remaining Rust memory budget
after its recording, media replacement and runtime reserves. The current profile
admits at most 1,398,101 records (16 MiB). Capacity limits accesses, not session duration. Meter reads and
status polling can fill it quickly, so keep the scripted session short. Overflow
preserves the prefix and reports the dropped count; replay refuses it.

The text format includes a format version, register-map version, record count,
dropped count, indexed read/write records and a final count. The parser tolerates
ordinary UART messages but rejects missing, reordered or duplicate records,
unclosed captures, multiple captures and unsupported versions. Uninstrumented
firmware is audited to exclude the dump marker and recorder path.

## Exact offline replay

```sh
cargo run --locked -p rt_fpga_sim --features sim --bin fpga-bridge-replay -- \
  work/zynq/board-uart.log work/bridge-replay board
```

The default `board` profile uses the same capacities, PL-generated audio and DDR
arena address as the host bitstream. `external` and `direct` select the existing
simulation-address profiles. Replay applies writes in order and compares **every
read**, including responses, status polls, cycle counters and the output digest.
The first mismatch reports its access index, offset, expected value and actual
value. Invalid captures fail before constructing a model.

The log records register operations, without the fabric cycles between them.
Replay currently advances the functional DDR model only for its AXI-lite bus
transactions. CPU scheduling gaps and physical DDR latency can therefore make
hardware status polls and timing counters differ. Exact simulation-to-simulation
replay is tested; successful hardware-to-simulation replay is **not** established.
No polls or counter bits are masked or skipped. Timing alignment/modeling remains
work for the hardware replay acceptance criterion.

## Checks without a board

```sh
python3 platforms/zynq7000/fpga_check.py --fetch
cargo test --locked -p rt_fpga_adapter
cargo check --locked -p rt_fpga_adapter --no-default-features --target armv7a-none-eabihf
cargo test --locked -p rt_fpga_sim --features sim -- --test-threads=1
cargo test --locked -p rt_fpga_sim --features app -- --test-threads=1 \
  --skip application_store_holds_a_five_second_take_while_recording_another_and_reuses_it
python3 fpga/host_bridge/generate.py --check
```

`fpga_check.py` builds and audits both FPGA firmware variants, then boots each on
QEMU and requires a bounded missing-bitstream refusal. QEMU does not emulate the
PL. The bridge RTL test records a board-profile record/stop/play session, replays
all accesses against a fresh model, checks the final counters/digest and verifies
that a corrupted digest read fails. It saves
`work/marlin-tests/board-register-trace.log`, which CI also replays through the CLI.
The optional long recording-capacity test has a separate command in the
[simulator README](../../src/rust/rt_fpga_sim/README.md).
