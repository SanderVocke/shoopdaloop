# 64-loop callback and full-pipeline RTL measurements

`cpu-*.json` contains the three CPU sample batches and startup measurements.
`cpu-*.csv.gz` contains every timed callback, including its loop position and MIDI
output count. `rtl-*.json` contains clock counts for startup and 16 steady playback
quanta. `summary.json` pools the CPU batches and summarizes both kinds of result.
`rtl-ideal-*.json` repeats the RTL workload with zero added DDR service delay on
both memory ports. AXI protocol cycles remain included.
Percentiles select sorted sample `floor(n * percentile)`, capped at `n - 1`.

The [report](../../midi-reference-layout-benchmarks.md) explains the workload,
measurement boundaries and limitations. These measurements do not replace board
DDR measurements or FPGA timing closure. CI and review feedback were not used.

## Reproduction

Enter the repository's development environment with Rust and Verilator 5.032.
Build the two examples in release mode:

```sh
RUSTFLAGS='-D warnings' cargo build --locked --release -p rt_fpga_reference --example midi_callback_64
RUSTFLAGS='-D warnings' cargo build --locked --release -p rt_fpga_testbench --features sim --example midi_quantum_64
```

Run the CPU commands **sequentially**, on one available CPU. These recordings use
CPU 4, with RTL simulation on other CPUs. Setup and validation are outside the
CPU timer. Defaults select 64 loops, 30-second clips, a 150-callback warmup and
three consecutive batches of 9,000 callbacks:

```sh
taskset -c 4 target/release/examples/midi_callback_64 --backend fpga --csv cpu-fpga-mixed.csv > cpu-fpga-mixed.json
taskset -c 4 target/release/examples/midi_callback_64 --backend fpga --midi-only --csv cpu-fpga-midi.csv > cpu-fpga-midi.json
taskset -c 4 target/release/examples/midi_callback_64 --backend production --csv cpu-production-mixed.csv > cpu-production-mixed.json
taskset -c 4 target/release/examples/midi_callback_64 --backend production --midi-only --csv cpu-production-midi.csv > cpu-production-midi.json
```

Run the full RTL benchmarks. Their clock counts are deterministic under the
recorded fixed-memory model; CPU execution speed affects simulation wall time:

```sh
target/release/examples/midi_quantum_64 --output rtl-mixed.json
target/release/examples/midi_quantum_64 --midi-only --output rtl-midi.json
target/release/examples/midi_quantum_64 --ddr-delay 0 --output rtl-ideal-mixed.json
target/release/examples/midi_quantum_64 --ddr-delay 0 --midi-only --output rtl-ideal-midi.json
```

Defaults record startup, skip 16 more quanta and record 16 steady quanta. The
retained seven-clock recordings were captured from the validated per-quantum stdout of
initially longer runs, intentionally stopped after quantum 32. Their JSON marks
this capture method. Each log line is printed after the Rust/output checks pass.
The current example defaults reproduce the same window directly and complete
normally. The full original stdout is retained in `validation/rtl-*-capture.log`.
`extract_window.py midi|mixed LOG OUTPUT_JSON` reproduces the retained JSON from
those logs without running a simulation.

The ideal-DDR runs use the current example directly and finish normally. CPU
measurements use host RAM and have no simulated DDR latency setting; they are
shared by both RTL comparisons.

Keep Verilator available even when using the prebuilt example: the model builder
checks its version. Loading the large session can take considerably longer than
the measured playback. This setup time is outside the reported clock counts.

Compress CSVs with `gzip -n`, place the outputs here and run `summarize.py`.
Summary generation uses Python's standard library; rendering the figure also
requires Matplotlib. `environment.json` records tool versions and host details;
`sources.sha256` identifies the measured implementation and benchmark sources.
Those hashes refer to the revisions recorded in `environment.json`. Later
testbench changes add optional profiling; the follow-up breakdown verifies that
its observed quantum counts match this original recording exactly.

`validation/` retains local build/test logs. The full workspace and browser/device
suites were not rerun: this change is confined to the FPGA reference and testbench,
and its affected Rust, RTL and AXI paths were tested directly.
