# Public Vivado container validation

Validation date: 2026-11-12. Workload: the default 64-loop deployment, Q=32,
64-bit AXI3, four read buffers, four write buffers, eight metadata cache lines,
four reference cache lines, 100 MHz requested fabric clock.

## Tool and board inputs

- Public image: `gusanagy/xilinx-vivado:2024.1-x11`, pinned to
  `sha256:a9d2fdab99a4b1757e2d1f23babe68b02d4a00a0a39969d73a9b51f085fdfe0f`.
- Vivado 2024.1, SW build 5076996, IP build 5075265.
- Part: `xc7z020clg484-1`.
- Digilent [vivado-boards](https://github.com/Digilent/vivado-boards) commit
  `d8d43ed9f3f7e367ee3de82782ed601d88305677`.
- Board identifier: `digilentinc.com:zedboard:part0:1.1`. The preset's directory
  is named `1.3`; its declared board version is `1.1`.
- IPs: PS7 5.5, proc_sys_reset 5.0, xlconstant 1.1, VIO 3.0 and ILA 6.2.

The cloud workspace's 63 GB disk and Docker's `vfs` storage driver could not
accommodate ordinary expansion of the complete image and all its intermediate
layers. The public OCI layers were downloaded with their SHA256 digests verified,
then unpacked without unrelated device families, precompiled simulation models,
Vitis and Model Composer. Original hardlink targets needed by Vivado were
preserved. The publisher's Ubuntu runtime and unmodified Vivado files were used
together, with the installation mounted read-only. This was a locally trimmed
runtime derived from the pinned public image.

The image also required an `en_US.UTF-8` locale, generated with its own
`localedef` and Ubuntu Focal locale definitions. Source the Vivado-only
`/home/vivadouser/Vivado/2024.1/.settings64-Vivado.sh`; the top-level settings
script additionally expects the other installed products.

The first synthesis launch crashed in the image's `libudev` during Vivado host
registration/telemetry device enumeration. An empty read-only `/sys` mount
(`docker run --tmpfs /sys:ro,mode=0555 ...`) allowed launch to proceed. Synthesis
then obtained the standard XC7Z020 license normally. The runtime had no network,
no attached devices and no user license directory mounted.

## Build checks

The deployment was prepared with `prepare.py` and `configure.py`. With the
board repository mounted separately, the build uses:

```sh
export SHOOP_BOARD_REPO=/boards/new/board_files
export SHOOP_BUILD_STAGE=synth
vivado -mode batch -source fpga/zedboard/build.tcl \
  -tclargs work/zedboard/build
```

The source directory can be mounted read-only; the configured output directory
must be writable and retain the same absolute path as during configuration.

Completed checks:

- Full 64-loop board-wrapper Verilator lint with warnings enabled.
- Vivado RTL module resolution, block-design validation and IP generation.
- Native AXI3 inference with 64-bit data and six-bit IDs, directly connected to
  PS HP0; generated DDR address segment covers `0x00000000..0x1fffffff`.
- Generated ILA connections preserve both 128-bit and 96-bit probes.
- Generated board-specific `ps7_init.tcl` exported successfully.
- Out-of-context synthesis of the complete 64-loop benchmark and all PS/debug
  IPs, followed by top-level synthesis and an exported `bench_synth.dcp`.

Top-level synthesis completed with zero errors and zero critical warnings.
The benchmark's out-of-context synthesis completed with zero errors, three
critical warnings and 148 warnings. The three critical warnings are
`Synth 8-5796`: conflicting RAM writes in metadata-cache banks 0, 1 and 2.
They refer to existing core RTL and remain unresolved; this build does not
qualify that RAM inference for hardware measurements. No core rewrite or
resource/timing optimization was performed as part of this integration work.

Tool checks found and fixed missing project include files, the module-reference
requirement for a Verilog entry module, incomplete address metadata from manual
pin connections, and the ILA's default monitor mode overriding probe widths.

The Digilent board preset emits four PS DDR DQS-to-clock skew critical warnings
(`-0.073`, `-0.034`, `-0.030`, `-0.082` ns). These preset values were retained;
DDR initialization and operation need verification on the physical board.
PS validation also warns that the disabled GP master has no `/ps/Data` address
space. HP0's address segment is present.

No hardware was attached. These checks do not establish timing closure,
resource fit, DDR behavior or measured hardware processing times.
