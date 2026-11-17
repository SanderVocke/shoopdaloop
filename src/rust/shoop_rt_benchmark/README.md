# Shared RT benchmark

Run from the repository root with the normal Rust build environment and Python
with Matplotlib installed:

```sh
python3 -m pip install 'matplotlib>=3.9,<4'
python3 scripts/benchmark_rt.py --output work/rt-benchmark
python3 scripts/benchmark_rt.py --preset smoke --backends cpu,fpga-reference --output work/rt-smoke
python3 scripts/benchmark_rt.py --report-only --output work/rt-benchmark
```

The standard fixture has 64 stereo loops on one stereo strip, mixed to a stereo
master, Q=32, 32-frame callbacks, and a 48 kHz descriptive sample rate. It uses a
16 MiB ordinary-media allowance, one second of input history where supported, no
smoothing or ramps, no MIDI, and no plugins or physical drivers. The two channels
use different deterministic bounded saw signals. Signal time starts at fixture
callback zero, independently of setup's processing-frame origin.

Both presets retain this layout and their recording duration of 256 frames,
which crosses the current media allocation boundaries. Smoke runs two lifecycle
iterations and one independent repetition; standard runs eight iterations and
three independent CPU timing repetitions. Deterministic RTL replay runs once by
default; `--repeats` overrides either backend's repetition count. Mixed turnover
uses 256, 288, and 352-frame recordings, alternating halves. Every run retains its complete versioned fixture
and hash, callback timing, event execution frames, and coverage observations.
Fixture version 2 shortens the original 1,024-frame recording to cross the same
128-frame allocation boundaries with greater transition-event density and less
simulation time. Grab storms use a separate 32,768-frame clock after filling the
history for at least twice its retention duration; adjacent full-cycle windows
exercise ring wrap while staying within one second of retention.
The native adapter reserves a separate 64 MiB shared-take pool so all 64 long
grabs and their replacement can be admitted together; the ordinary-media budget
remains 16 MiB. Prepared capacities are included in backend metadata.

All measured controls use the common transport `Endpoint`: complete 32-command
groups are staged before releasing a dummy callback, and responses and snapshots
are validated afterwards. No deadline is attached. Native processing uses a
synchronously stepped production dummy driver; the FPGA reference is called only
by that same scheduling contract. Setup, response decoding, sample validation,
and artifact serialization happen outside measured processing. Output validation
accounts for each driver's declared processing latency; native host buffering
currently contributes one quantum.

The CPU and Rust FPGA reference use the same fixture and sample checks. The
reference's Q8.24 CPU time describes its functional model, not FPGA hardware
speed. The graph compiler's `compile_with_channels` frontend prepares up to 64
mono or stereo loops with independent channel media. The legacy application
prepared-graph profile remains restricted to 16 mono loops.

RTL is opt-in (`cargo build --release -p shoop_rt_benchmark --features rtl`). It
uses the shared logical compiler and reusable Marlin simulation infrastructure,
with a queue-backed command/snapshot frontend and actual RTL processing. Prepare
its pinned prerequisites first:

```sh
bash scripts/build_verilator.sh
export PATH="$PWD/work/verilator/bin:$PATH"
python3 fpga/axi_memory/build_dramsim3.py
python3 scripts/benchmark_rt.py --preset smoke --output work/rt-rtl-smoke
```

The primary memory profile is DRAMsim3 at a 150 MHz fabric clock; `--memory-model
fixed` selects a diagnostic alternative. The modeled DRAM ratio and report
conversion both use 150 MHz. Fabric cycles convert to microseconds as cycles/150.
DRAM ticks are reported separately. RTL's window includes command configuration,
input upload, SP/JP/media processing, required completion, and queue drain.
Passive output/state probes advance no clocks; ARM CPU service time is unmodeled.
SP/JP activity, job-queue peaks, AXI traffic and stall counters are observed
without changing fabric cycles. Stage and stall intervals overlap.
Deferred final memory drain is retained in finalization diagnostics.

Reports distinguish missing prerequisites, build failures, processing failures,
incorrect results, and unsupported operations. Grabs/history are unsupported on
the FPGA paths and are explicitly reported without substitution. Incomplete
mandatory comparisons return nonzero after producing the available report.
Fresh campaigns require a fresh output directory. Report-only mode uses no
backend or compiler and regenerates the PDF and CSV from the retained manifest
and run JSON files.

The PDF compares per-repeat processing distributions, annotates callback
timelines, aligns views around transition events, and includes coverage and
provenance. Scheduled-transition execution, storage growth and storage release
are annotated when observed, in addition to submitted commands. Steady summaries
separate recording/playback populations. `summary.json` retains these statistics, observed maxima,
worst callback identities, and exceedances of the hypothetical 666.67 µs budget.
Extreme percentiles are suppressed when the observation count is too small.
These measurements are observed maxima under the declared fixture and memory
model, not a proven worst-case execution bound or physical-board certification.

Validation:

```sh
cargo test --locked -p shoop_rt_benchmark -p rt_fpga_compiler
python3 -m unittest discover -s scripts/tests -p test_benchmark_rt.py
```

The shared Rust smoke test checks both complete mandatory patterns, exact stereo
samples, same-frame command execution, completed-take replacement, planned-slot
retirement, allocation growth and wrapping. The full campaign also verifies exact
stereo samples from the long history grabs. A heap
guard surrounds every native and functional-reference processing call.
