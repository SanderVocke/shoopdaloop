# Temporary ZedBoard processing benchmark

This deployment runs the real transition/state/job/media pipeline against the
ZedBoard's PS DDR through HP0. The FPGA loads its own workload and generates its
own input. Both Cortex-A9 CPUs remain halted after debugger initialization; no
ELF, operating system, application integration or audio hardware is required.

The goal is processing-time measurement. There is no expected-output comparison.
An unverified rolling output digest keeps the arithmetic results observable.
Faults, workload completion and watchdog expiry remain visible so that a stalled
or failed run cannot be mistaken for a fast successful run. Resource optimization
and timing closure are outside this workflow; the selected fabric clock is an
input, rather than a frequency-search result.
See [routed resource usage](RESOURCES.md) for totals and the subsystem breakdown
of the fully routed 64-loop deployment, including debug instrumentation.

## AXI connection

The common master exposes native AXI3: four-bit AWLEN/ARLEN, WID=0 and one
ID-zero transaction outstanding. Bursts remain aligned, INCR, at most 16 beats
and within a 4 KiB page. The board wrapper supplies six-bit PS IDs, unlocked
accesses, AxCACHE=3, AxPROT=0 and AxQOS=0. A nonzero returned ID propagates as an
error. HP0 is 64 bits wide. There is no AXI protocol converter or interconnect
between the master and HP0, so measurements do not include converter latency.

Sample data and metadata share this same port. The default arena begins at
0x01000000. CPUs are halted and no other software owns this arena. The generated
PS initialization must match the block design, board preset and silicon revision;
the software-only PS initialization file does not configure this deployment.

## Prepare and build

Run from the repository root. Python requires the existing
`fpga/axi_memory/requirements.txt` and `scripts/rt_backend_testbench/requirements.txt`.
Rust builds only the workload compiler. Vivado needs the Digilent ZedBoard board
files and support for the actual XC7Z020 silicon. RTL validation uses Vivado
2021.1, SW Build 3247384; hardware qualification is pending.
For a board repository outside the Vivado installation, set `SHOOP_BOARD_REPO`
to its `new/board_files` directory. Both Avnet `zed` and Digilent `zedboard`
board identifiers are accepted. Build scratch files stay in the output directory,
so the RTL source tree can be mounted read-only in a container.
The installation must include PS7, proc_sys_reset, xlconstant, VIO and ILA IPs.
For an external matching IP repository, set `SHOOP_IP_REPO` to its directory;
the build checks the required definitions before creating the block design.
See [the verified public 2021.1 catalog source](CATALOG.md) if the tool image
omits these IPs.

```sh
python fpga/zedboard/prepare.py --output work/zedboard/workload
python fpga/zedboard/configure.py --workload work/zedboard/workload \
  --output work/zedboard/build --fabric-mhz 100
vivado -mode batch -source fpga/zedboard/build.tcl \
  -tclargs work/zedboard/build
```

`prepare.py` invokes the production Rust graph compiler without executing the
fixed/float result or accuracy paths. The default workload has 64 mono loops,
one dummy input, 64 gain nodes and eight mixed outputs. It records 64 quanta,
starts playback, retakes the even loops for two quanta, then continues playback
indefinitely. Q=32; the initial recordings exceed the sample caches. The source
and compiled image, ROMs, capacities and warmup recommendation are saved together.
The FPGA generates each input word as `(sample_address+1)*37 XOR quantum`;
the scenario's input arrays supply the compiler's shape, rather than expected data.
Use `--playback-only` to omit retakes, or change `--loops`, `--tracks`, `--q` and
`--record-quanta`. Rebuild after changing the ROMs or compiled capacities.

`configure.py` binds the compiled capacities into a board module and saves a
manifest. Cache capacities are adjustable with `--read-buffers`, `--write-buffers`,
`--metadata-lines` and `--reference-lines`. These values must also match simulation
when comparing results. The memory arena is checked against 512 MiB DDR.

The Vivado script builds the bitstream and exports `bench.bit`, `bench.ltx`,
`bench.xsa` and its generated `ps7_init.tcl` into the selected directory. It adds
VIO and a two-probe ILA. It also saves `utilization.rpt` and
`utilization_hierarchical.rpt` for total and subsystem resource accounting.
An optional second Tcl argument selects an explicit installed ZedBoard BOARD_PART.
Set `SHOOP_BUILD_STAGE=generate` to stop after block-design validation and IP
generation, or `SHOOP_BUILD_STAGE=synth` to stop after synthesis and save
`bench_synth.dcp`. The default stage is `bitstream`.

## Load and measure

Set the board to JTAG boot. Start a local hardware server, then load the FPGA and
initialize PS clocks, DDR and HP0:

```sh
hw_server -s TCP:127.0.0.1:3121
xsdb fpga/zedboard/load.tcl work/zedboard/build
```

Run `hw_server` in its own terminal. `load.tcl` performs a system reset, halts both
CPUs, programs the PL and applies the generated PS initialization. The harness
stays idle until a VIO start command; it never accesses uninitialized DDR.

```sh
vivado -mode batch -source fpga/zedboard/control.tcl \
  -tclargs work/zedboard/build measure 1000 72 0 10000000
```

The four numbers are total quanta, warmup quanta, gap clocks and per-quantum
watchdog clocks. Total quanta includes warmup; 0 means unlimited. Watchdog 0
disables the watchdog. `measure` waits up to 60 host seconds for STOPPED and saves
`results.json`. For an unlimited run use `run`, followed by `snapshot` or `stop`:

```sh
vivado -mode batch -source fpga/zedboard/control.tcl \
  -tclargs work/zedboard/build run 0 72 0 10000000
vivado -mode batch -source fpga/zedboard/control.tcl \
  -tclargs work/zedboard/build snapshot
vivado -mode batch -source fpga/zedboard/control.tcl \
  -tclargs work/zedboard/build stop
```

With no numeric overrides the controls use 1000 quanta, the prepared workload's
warmup recommendation, no gap, and a 10-million-clock watchdog. Start captures
the controls and reloads a fresh session. Commands issued while a run is active
do not restart it. Stop finishes an in-progress quantum before isolating and
draining external AXI. A fault or watchdog expiry stops admission and drains the
accepted transaction. A nonresponding slave leaves the harness in DRAINING;
start cannot bypass that condition. Global reset is reserved for cold/system
initialization. Dirty cache contents are discarded on a subsequent fresh start;
this benchmark does not preserve sessions across restart.

VIO outputs 0..6 are start, stop, snapshot, total quanta, warmup, gap and watchdog.
The first three are rising-edge commands. Input 0 is live status; inputs 1..16
are an atomic statistics snapshot. The script pulses snapshot before readout.
Stopped snapshots update automatically after the transport drains.

Live status bits:

| Bits | Meaning |
| --- | --- |
| 2:0 | Phase: IDLE=0, RESETTING=1, LOADING=2, RUNNING=3, DRAINING=4, STOPPED=5 |
| 3 | AXI transport quiescent |
| 4 | Failed run, sticky until next accepted start |
| 5 | Boot ROM loading complete |
| 6, 7 | Quantum in flight, processing core busy |
| 15:8, 17:16 | Retained error code and processing stage |
| 18, 19, 20 | Boot, control and memory faults |

Harness error codes F1/F2/F3/F4 identify memory/control/boot/watchdog faults;
otherwise the processing core's code and stage are retained.

## What the counters measure

Statistics words, in ascending order:

| Word | Value |
| --- | --- |
| 0 | Status at snapshot |
| 1, 2 | All completed quanta; measured quanta after warmup |
| 3 | Sum of measured run-acceptance-to-done cycles |
| 4, 5, 6 | Last, minimum and maximum measured quantum cycles |
| 7 | Wall cycles after warmup, including controls and configured gaps |
| 8, 9 | Accepted AXI read/write bursts |
| 10, 11 | Transferred read/write bytes, including narrow beats |
| 12 | Clocks with a stalled AXI address channel |
| 13 | Clocks with read/write response backpressure |
| 14, 15 | Output sample writes; unverified rolling output digest |

Traffic and output counters cover the entire execution, including warmup and
the final transport drain. Boot loading is excluded. AXI byte counts describe
this interface, not physical DDR command bytes or bus utilization. Word 13 counts
response backpressure, not time spent waiting for the DDR controller to respond.
Write byte counts use AWSIZE, rather than counting asserted WSTRB bits.
Individual quantum times exclude the command-ROM work before run acceptance;
word 7 retains that scheduling overhead. A gap of zero schedules quanta as soon
as the core and per-quantum command fence are ready. All clocks keep running.

ILA probe 0 is `{quantum[63:0], run_cycles[31:0], status[31:0]}`. Probe 1 is
`{AWADDR, ARADDR, AWLEN, ARLEN, AWSIZE, ARSIZE, BRESP, RRESP, AWVALID, AWREADY,
WVALID, WREADY, WLAST, BVALID, BREADY, ARVALID, ARREADY, RVALID, RREADY, RLAST,
run_pulse, done_pulse}`. Trigger on faults, run/done or AXI handshakes.

## Verify the deployed harness in simulation

These tests check execution and counter boundaries, not expected audio results:

```sh
python fpga/zedboard/generate.py --check
python fpga/zedboard/prepare.py --loops 4 --tracks 2 --record-quanta 2 \
  --output work/zedboard/smoke-workload
python fpga/zedboard/simulate.py --workload work/zedboard/smoke-workload \
  --control-tests --output work/zedboard/runner-native
python fpga/zedboard/simulate.py --workload work/zedboard/smoke-workload \
  --memory-model fixed --memory-cycles 25 --output work/zedboard/runner-fixed
```

The same runner accepts `--memory-model dramsim3` with the existing native
library and a board-appropriate `--dram-config`. Supply `--fabric-hz` matching
the deployment, and enough `--quanta` to exceed the prepared warmup. Simulation
compares the hardware counters with observed run/done edges and starts from
nonzero memory. Control checks withhold an accepted response, verify watchdog
drain, release it, restart, snapshot and stop. All results are written as JSON.

Native AXI3 endpoint protocol tests and generated-wrapper checks also remain
applicable. See [public-container validation](VALIDATION.md) for the tool version,
board preset and build results. Real PS/HP initialization and ILA/VIO operation
still require an attached ZedBoard. Record board/silicon revision and the
generated `deployment.json` alongside hardware measurements.
