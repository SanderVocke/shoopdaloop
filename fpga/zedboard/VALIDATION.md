# Public Vivado container validation

Validation date: 2026-11-12. Workload: the default 64-loop deployment, Q=32,
64-bit AXI3, four read buffers, four write buffers, eight metadata cache lines,
four reference cache lines, 100 MHz requested fabric clock.

These records predate hardware testing. See [hardware results](HARDWARE.md) for
the first board run and the defects it found in this deployment.

## Matching catalog and full board retry

Found the complete 2021.1 catalog in the public image
`pwang7/vivado_ubuntu:standard-2021.1`. The entire source layer was downloaded
and SHA256-verified before extracting the catalog and resolving its original
hardlinks. [Catalog instructions](CATALOG.md) include pinned image/layer digests;
[catalog provenance](validation/2021.1/catalog-provenance.json) records the
required-IP descriptor hashes.

The compiler remains the unmodified vendor payload described below, with the
catalog mounted read-only at its canonical `data/ip` location. The banner is
**Vivado 2021.1, SW Build 3247384, IP Build 3246043**. The board was regenerated
from RTL with no previous IP outputs or checkpoints.

- Native AXI3 HP0 block-design validation and all IP output generation pass.
  The DDR address segment covers 512 MiB, and board-specific `ps7_init.tcl`
  is exported.
- PS7, reset, ILA, VIO, benchmark and top-level synthesis complete with zero
  errors and zero critical warnings.
- Automatic JTAG debug-hub generation/synthesis succeeds, also with zero
  errors and zero critical warnings.
- Full board logic optimization and placement complete successfully, with
  zero errors and zero critical warnings.
- The complete board is fully routed: **51,330 / 51,330 routable nets, zero
  routing errors**.
- Bitstream generation succeeds. Its DRC has **zero errors, zero critical
  warnings and ten warnings**: six DSP pipeline suggestions, three LUT-equation
  term warnings inside the generated JTAG debug hub and one no-routable-loads
  warning. The earlier invalid-site DRC errors did not reproduce.
- `bench.bit`, `bench.ltx`, `bench.xsa` and `ps7_init.tcl` are exported. The XSA
  archive passes its ZIP integrity check and contains exactly the exported
  bitstream and PS initialization script.
- The 100 MHz timing report has WNS **-0.425 ns**, TNS **-6.270 ns** across
  60 failing setup endpoints; hold WHS is **+0.014 ns** with no hold violations.
  This is not a timing-qualified 100 MHz deployment. Timing closure remains
  outside this integration work.

The total deployment uses **30,857 LUTs, 27,104 flip-flops, 68 BRAM36 equivalents
and four DSPs**. See [resource accounting](RESOURCES.md) for subsystem and debug
overhead. The build now exports total and hierarchical utilization reports.

Retained board evidence: [DRC](validation/2021.1/board_drc.rpt),
[route status](validation/2021.1/board_route_status.rpt),
[timing summary](validation/2021.1/board_timing_summary.rpt) and
[utilization](validation/2021.1/board_utilization.rpt). Artifact sizes and SHA256
hashes are in the catalog provenance. Local deployment outputs are in
`work/zedboard/board2021-catalog`; build logs are in `work/vivado-2021`.
No hardware was connected, so real DDR access, ILA/VIO operation and processing
times remain unmeasured.

The first implementation attempt failed during automatic debug-hub generation.
A retry also exposed a `malloc_usable_size` crash in `libudev`, called through
Vivado registration/Webtalk at project launch. Preloading the runtime's original
`/lib/x86_64-linux-gnu/libudev.so.1` allowed project launch and automatic debug-hub
synthesis to complete. This retry retains the normal `/sys` view and normal
device licensing; no vendor files were modified.

## Fresh Vivado 2021.1 build

Removed the Vivado 2024.1 installation, its Docker runtimes and all previous FPGA
build intermediates. Regenerated the workload and board wrapper at RTL revision
`9386dffd14408df52b87179e834b1bdc383bf07c`, then built from source with
**Vivado 2021.1 for Linux x86_64, SW Build 3247384**. No old IP outputs or
checkpoints were reused.

The tool source is `siliconbootcamp/xilinx-vivado:stable`, pinned to
`sha256:d24586405894b07f6af006d5cb15ba01c7e8585de97b28b6121fcf6282db7a5f`.
All image-layer hashes were verified. The complete vendor layer,
`sha256:187ac31317b800d0b7782a3aff7fdedd200a27303a22121d7c3a6a623b36bf26`,
was extracted without trimming or patching its files and mounted read-only.
Later image overlays, including a replacement `base.tcl`, were excluded.
The runtime uses the image's original base OS and dependency layers, without
the `/sys` masking workaround used during the 2024.1 experiments.

Results:

- The tiny counter passes physical DRC and generates a bitstream. Its one DRC
  warning is the absent PS7 block in this diagnostic design.
- The complete configured `zedboard_bench` RTL, including the autonomous runner,
  statistics and processing pipeline, synthesizes with zero errors and zero
  critical warnings in out-of-context mode on `xc7z020clg484-1`.
- The benchmark is **fully routed: 39,744 / 39,744 routable nets, zero routing
  errors**. Post-route DRC has **zero errors, zero critical warnings and eight
  warnings**. These comprise six DSP pipeline suggestions, an output with no
  routable loads and the intentionally omitted PS7. The invalid-site errors
  from the 2024.1 runs did not reproduce in this build.
- The initial full board build was blocked earlier: this image has no Vivado IP catalog.
  PS7 5.5, proc_sys_reset 5.0, xlconstant 1.1, VIO 3.0 and ILA 6.2 are all absent.
  The other image layers contain no replacement catalog. The build now checks
  these dependencies explicitly and accepts a matching external repository
  through `SHOOP_IP_REPO`.

The routed RTL diagnostic excludes PS7, ILA and VIO. It does not establish full
board connectivity, DDR operation or board timing closure. Its out-of-context
clock has no physical clock-source assignment. That diagnostic run produced no
deployable benchmark bitstream; `canary.bit` is only the small toolchain check.
The subsequent catalog retry above produced the full deployment bitstream.

Retained evidence: [provenance and outcomes](validation/2021.1/provenance.json),
[source hashes](validation/2021.1/source-manifest.json),
[measured diagnostic script](validation/2021.1/core.tcl),
[DRC](validation/2021.1/core_drc.rpt) and
[route status](validation/2021.1/core_route_status.rpt).
The script is a snapshot using the container's absolute paths and the prepared
manifest; it reads RTL and regenerates synthesis/placement/routing results.
Local checkpoints are in `work/zedboard/core2021`; tool and board logs are in
`work/vivado-2021`.

## Retry after timing MR #975

Merged `proto/fpga-rust-reference` at `a29e1999`, including timing improvements
`f7cca570` (#975) and native AXI3 (#974), into the temporary deployment branch.
The tested merge revision is `1b32bef7dc5e4e328dced1bb9586ad887172bebf`.
The same default 64-loop workload and board configuration were regenerated.

- Generated AXI, audio and benchmark interface checks pass.
- Full configured board-wrapper lint passes with warnings enabled.
- Four-loop harness control tests pass, including watchdog/drain/restart,
  snapshots and stop during boot.
- The full 64-loop schedule completes 80 simulated quanta. All measured counter
  boundaries agree with observed run/done edges. After 72 warmup quanta, the
  eight measured quanta total 185,712 cycles, with a range of 22,974..23,454
  cycles. These are native instant-memory simulation results, not hardware or
  DDR latency measurements; audio-output correctness is not checked.
- Block-design validation, IP generation and full synthesis pass again.
  Benchmark synthesis has zero errors, zero critical warnings and 145 warnings;
  top-level synthesis has zero errors, zero critical warnings and one warning.
  The three cache RAM inference critical warnings from the initial run are gone.
  The merged synthesis checkpoint and board initialization were exported as
  `bench_synth.dcp` and `ps7_init.tcl`.
- Placement completes and the implementation writes a routed checkpoint.
  Bitstream generation fails physical DRC again: the routed report contains
  667 `PDIL-1` invalid-site-configuration errors and insufficient routing.
  `Route 35-557` pin-assignment failures also remain. No `bench.bit`, `bench.ltx`
  or `bench.xsa` was produced. The tool still reports timing violations for the
  full board deployment at 100 MHz; no timing tuning was performed here.

The retry also tested the small counter with the normal `/sys` view, both in
the image's original Ubuntu Focal runtime and a fresh Ubuntu 22.04 runtime.
Both abort in `libudev` device enumeration called through Vivado's registration/
Webtalk code after routing. Ubuntu 22.04 was based on
`ubuntu:22.04@sha256:74b12b11ffa550fba1929ad4d8d16533c34f6612728469671afba1b03b66881c`,
with normal distribution libraries and the same read-only Vivado installation.
The supported `webtalk.disableTransmit` setting did not prevent the Focal crash.
The exact source of the runtime/physical-build failures remains unresolved.

Run outputs are under `work/zedboard/vivado-timing-merge`; simulation results
are under `work/zedboard/runner-timing-merge` and
`work/zedboard/runner64-timing-merge`. The merge and retest required no deployment
RTL changes.

## Initial result before timing MR #975

Outcome: block-design/IP generation, full synthesis and placement completed.
The implementation wrote a routed checkpoint, but physical DRC errors blocked
bitstream generation. `bench_synth.dcp` and `ps7_init.tcl` were exported;
`bench.bit`, `bench.ltx` and `bench.xsa` were not produced.

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

## Initial build checks

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
They referred to existing core RTL and were unresolved in that run; #975
subsequently replaced the RAM template, and the retry no longer reports them.
The initial build did not qualify that RAM inference. No core rewrite or
resource/timing optimization was performed as part of this integration work.
The linked synthesized checkpoint retains a block RAM in each of these three
cache banks.

Placement completed successfully. The implementation reports timing violations
at the requested 100 MHz; this integration exercise leaves those violations to
the separate timing work. A generated bitstream must not be treated as a
timing-qualified 100 MHz deployment.

The bitstream attempt reached `write_bitstream` and stopped at physical DRC:
the routed report contains 1,119 `PDIL-1` invalid-site-configuration errors,
and the route-status report contains unresolved routing conflicts. The tool
also reported `Route 35-557` pin-assignment failures. The routed checkpoint is
a diagnostic artifact, not a successfully routed deployment.

A separate 32-bit counter/XOR design, using only a clock input and LED output
on the same XC7Z020, reproduced invalid-site-programming failures at bitgen
(`RTSTAT-11` on the clock input, clock buffer and output sites), plus incorrect
LUT-pin-equation warnings. It uses no benchmark, AXI, cache or debug RTL.
The physical build failure therefore also occurs outside this deployment.
Its cause has not been isolated between the public tool installation, the
trimmed runtime and the cloud host. Recheck implementation/bitgen with a known
working Vivado installation before relying on any exported hardware image.

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
