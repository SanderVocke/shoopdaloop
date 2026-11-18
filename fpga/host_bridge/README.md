# Z-turn V2 FPGA bridge bring-up

The development bridge maps AXI-Lite through PS GP0 and connects the 64-bit
media master to PS HP0. It reserves DDR for the PL and exposes the register
map defined in `registers.json`. Run the generators after changing the ABI or
wrapper. The generated audio build uses a deterministic mono source at 48 kHz
and sends each quantum through the stereo audio bank controller.

## Vivado and diagnostic firmware

Use Vivado 2021.1 with the matching IP catalog. The workspace build uses
`SHOOP_VIVADO_SERIAL_SYNTH=1` to disable the parallel synthesis helper and
synthesize the block design together with its top-level wrapper.
Use the provisional PS settings below to reproduce routing and generate
reports in the workspace. Use the exact MYIR V2 PS configuration for JTAG
and verify the populated DDR and connector configuration before powering the
physical codec. [Compiler and catalog provenance](../zedboard/CATALOG.md)

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
  --server TCP:127.0.0.1:3121
```

Expect `SHOOP_FPGA_PROBE_READY` and `SHOOP_FPGA_PROBE_RETURNED` on UART. The
QEMU startup test exercises the bounded refusal path without a configured PL.
