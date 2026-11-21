# Shared RT benchmark

Run from the repository root with the normal Rust build environment and Python
with Matplotlib installed:

```sh
python3 -m pip install 'matplotlib>=3.9,<4'
python3 scripts/benchmark_rt.py --output work/rt-benchmark
python3 scripts/benchmark_rt.py --preset tiny --output work/rt-tiny
python3 scripts/benchmark_rt.py --preset quick --output work/rt-quick
python3 scripts/benchmark_rt.py --preset full --output work/rt-full
python3 scripts/benchmark_rt.py --media audio --backends cpu,fpga-reference --output work/rt-audio
python3 scripts/benchmark_rt.py --report-only --output work/rt-benchmark
python3 scripts/benchmark_rt.py --preset full --profile-rtl --output work/rt-profiled
```

`--media audio,midi,audio-midi` is the default matrix. `--preset` accepts only
`tiny`, `quick` and `full` (default). The old smoke workload is now `full`;
the former standard preset and both old CLI names are removed. Historical
archives still regenerate with `--report-only`.

| Preset | RTL per media profile | Native CPU statistics | Warm-cache target |
| --- | --- | --- | --- |
| `tiny` | One 52-callback stress sequence: cold recording across audio/MIDI blocks, full 2,560-message merge, quiet playback, simultaneous retake allocation, stop | Up to two complete passes, 30-second soft budget | About 5 minutes |
| `quick` | One 172-callback stress sequence: tiny's scenarios plus two planned retirements per loop, wraps, full-length retakes and half-record/half-play contention | Up to three complete passes, 45-second soft budget | About 20 minutes |
| `full` | Original two-iteration lifecycle (298 callbacks), mixed turnover (232), and capability-dependent grabs | One complete pass, matching the former smoke campaign | About 40 minutes |

Short presets retain separate native lifecycle/mixed/history-grab patterns for
statistics, in addition to the compact common stress sequence. The functional
reference runs each selected pattern once for cross-backend comparison. RTL runs
the stress sequence once per media profile, plus explicit unsupported grab slots.
`full` runs lifecycle/mixed/grab on every selected backend. `--patterns` overrides
these variant-specific selections; `--repeats` overrides repetition counts for all
backends, including RTL. These overrides can exceed the targets above.

CPU budgets include preparation, stimulus, endpoint service and validation.
`--cpu-budget-seconds` overrides the preset budget. The first pass always completes;
an optional pass starts only while the budget remains and finishes once started.
The limit is soft by at most one complete pass. Intentionally skipped optional
slots are recorded in the manifest and PDF, separately from unsupported cases or
failures. Required RTL scenarios are never truncated by a timer.

Tiny explicitly omits playback wraps, planned transitions and mixed loads in the
stress sequence; its JSON and PDF label those omissions. Quick requires all three.
All stress runs must reach synchronized retakes and, when MIDI is enabled, the
full 64-loop merge. Both compact sequences preserve the initial full-workload
stimulus through the first dense playback peak.

Three RTL workers are used by default, after serial CPU/reference timing finishes.
Targets assume the measured host's CPU capacity and warm build/model caches;
cold Actions toolchain, Rust and RTL compilation adds setup time. Fabric-cycle
measurements remain independent of host scheduling.

Unsupported cases remain explicit run slots. Missing prerequisites,
build/processing failures and incorrect results
produce distinct statuses and a nonzero campaign result.

For very short **RTL-only ideal-memory versus DRAMsim3 comparisons**:

```sh
python3 scripts/benchmark_rt_ddr.py --media audio --output work/rtl-audio-ddr
python3 scripts/benchmark_rt_ddr.py --media midi --output work/rtl-midi-ddr
python3 scripts/benchmark_rt_ddr.py --report-only --output work/rtl-midi-ddr
```

Each command builds once and runs two independent simulations concurrently, using
the same 38-callback `ddr` fixture and frozen binary. It retains 64 loops and the
normal Q32 / 32×32 geometry. Record all loops for 33 quanta to cross audio/MIDI
block boundaries, play once (2,560 output messages for MIDI), stop, retake all
loops and stop again. Quiet playback, wraps, planned transitions and mixed loads
are explicitly omitted. There are no native CPU/reference runs or grab slots.
`--jobs 1` runs the pair serially; `--binary` skips the Cargo build, and
`--dram-config` selects the simulated DDR configuration.

The one-page `report.pdf` pairs matching callbacks and shows cycles, microseconds
at 150 MHz, differences and ratios. `callbacks.csv` retains every paired callback;
`summary.json` retains separate model statistics. The `ideal/` and `dramsim3/`
directories keep raw runs, per-model reports and provenance. Any missing case,
output failure, unequal fixture, binary or functional work makes the comparison
fail. Report-only regeneration needs no simulator.

`--memory-model ideal` on the main runner uses zero-added-delay service on the
shared AXI controller. AXI handshakes/transfers still cost cycles; there is no
added refresh, bank contention or DRAM service delay. The existing `fixed` mode
still adds seven cycles and is not used as the ideal baseline. DRAMsim3 retains
the shared 150 MHz Z-turn V2 DDR3-1066 x32 configuration by default.

Fixture revision 5 uses **64 loops, Q=callback=32, and 48 kHz**:

| Media | Loop contents and routing |
| --- | --- |
| `audio` | Stereo audio through one stereo strip and stereo master. Distinct deterministic left/right saw signals; loop gain 1/128. |
| `midi` | One MIDI stream recorded independently into all 64 loops, with playback merged to one output. No audio channels, ports, capture or playback on the FPGA paths; upstream scratch initialization remains. |
| `audio-midi` | Every loop contains stereo audio plus MIDI. The compiled audio operations match the audio-only graph; MIDI uses the same SP/JP queue. |

The FPGA adapters use the upstream full graph compiler and 32-frame media chunks
with 32-chunk blocks (1,024 audio frames/block). The benchmark's former
`GraphDescription` stereo/compiler extensions and native `SteppedDummy` helper
have been removed. Native execution calls the production renderer synchronously
with prepared dummy buffers. No driver thread, UI, plugin or physical device is
started, and no deadline is attached.

All presets initially record for 1,056 frames, crossing an audio allocation block.
The full mixed pattern also records for 1,088 frames. Tiny's retake records two
quanta beginning on a 40-message input burst. Quick's retakes cross a second audio
block and combine 32 recording loops with 32 playing loops. `--rtl-jobs N`
controls independent RTL processes; simulator wall time is retained separately.

MIDI input repeats a 16-quantum cycle: 40 messages in the first quantum, 16 in
each of the next seven, then eight silent quanta. Messages are complete three-byte
controller packets with deterministic bytes, sample offsets including 0 and 31,
and tied timestamps. Playback can emit 2,560 messages in one quantum. All output
bytes, timestamps, counts and same-time ordering are checked by the common
runner against the recording history. Quiet playback, burst playback and
simultaneous retakes are covered. MIDI storage uses 256-record blocks, 512 blocks,
2,048 events/view, 256 views, 128 retirement slots, 64 sequence/cursor slots,
128 scratch-list descriptors and a shared 65,536-record scratch arena. The
initial recordings cross event blocks as well as audio blocks. No messages are
silently dropped or truncated at 256 events/quantum.

This is a stress workload. Upstream's [64-loop playback measurements](../../../docs/fpga/midi-data-jobs-benchmarks.md)
normally emit four or five messages per quantum, assume 100 MHz and use independent
ideal/fixed-delay memory ports. This fixture reaches 2,560 messages on one merged
output and measures recording, growth, wraps and retakes with shared DRAM at
150 MHz. The raw counts distinguish quiet callbacks from dense processing.

All measured controls use `shoop_rt_transport::carrier::Endpoint`. The runner
stages complete 32-command groups before releasing a callback and validates their
execution frame, responses and snapshots afterward. The Rust reference endpoint
feeds the upstream MIDI data-job processor; RTL uses upstream packed-state
control helpers. Stimulus never calls a backend's scenario testbench. The RTL
adapter reuses simulation drivers and AXI responders as infrastructure.

The native CPU immediate endpoint currently refuses MIDI-bearing loop transitions.
The first command responses are checked, and those cases are reported
**unsupported**, including combined audio/MIDI, rather
than timing a different authored-transport path. The adapter checks actual endpoint
responses on each run.
MIDI-only and combined results are available for the Rust FPGA reference and connected RTL. FPGA history/grabs
remain unsupported; MIDI history/grabs are also unsupported. Audio history grabs
still run on native CPU after filling and wrapping a one-second ring, using
32,768-frame windows and a separate 64 MiB shared-take pool. Normal lifecycle and
turnover runs have input history disabled on all backends. The ordinary audio
media budget remains 16 MiB.

Native f32 and Rust FPGA Q8.24 processing use host wall time. Functional-reference
time measures CPU execution of that model, not hardware speed. RTL is enabled
with `cargo build --release -p shoop_rt_benchmark --features rtl`. Prepare pinned
simulation prerequisites first:

```sh
bash scripts/build_verilator.sh
export PATH="$PWD/work/verilator/bin:$PATH"
python3 fpga/axi_memory/build_dramsim3.py
```

All three media profiles use the same connected `midi_audio_pipeline_axi` top and
elaborated capacities. Its audio and MIDI 64-bit AXI ports share **one DRAMsim3
controller at 150 MHz** and disjoint physical address ranges. New runs default
to upstream's provisional Z-turn V2 DDR3-1066 x32 profile; `--dram-config PATH`
(or `SHOOP_DRAMSIM3_CONFIG`) selects another profile, including the older Zynq7010
x16 configuration. The configuration and library hashes are retained. One fabric cycle
advances the shared controller once, even though both port responders sample it.
Input/output DMA, metadata, event records and intermediate merges therefore share
DRAM queue/bank/refresh timing. The simulation does not model a board's complete
interconnect or ARM CPU service. `--memory-model fixed` uses a diagnostic
seven-cycle/16-byte-request controller through the same sharing interface.

RTL timing covers commands, audio input upload, input-event validation, the
compiled SP pass, shared queue/JP work, required completion and cleanup. Prepared
MIDI input packets are placed in backing memory before timing; ARM input staging
is unmodeled. Indexed sample/state observation ports advance no clocks and avoid exporting
thousands of separate RAM signals on every simulation edge. A regression compares
them with static probes and verifies unchanged RTL state, cycle count and time. Cycles convert to
microseconds as **cycles/150**; DRAM ticks remain separate. The report validator
checks both port clocks and the measured fabric-to-DRAM tick ratio, including
separately reported final drain. Per-callback diagnostics retain SP/JP activity,
queue peaks, separate audio/MIDI AXI reads/writes and stalls, and MIDI event counts.
Stage/stall intervals overlap and must not be added.
AXI read/write counts are transactions. Response backpressure counts an available
response waiting for RTL readiness; it does not measure JP time waiting for DRAM.

The PDF stays within **five landscape pages** as the matrix and repetitions grow.
The first three cover FPGA media/pattern statistics and distributions, stress and
quiet-playback populations, and worst-callback diagnostics. One page covers host
results; one covers correctness, unsupported cases and provenance. All per-repeat,
event/steady/quiet statistics remain in `summary.json`; full callback/event
records remain in run JSON and CSV. Timelines are omitted from the PDF. Percentiles
require enough observations, and maxima describe this fixture rather than proven
worst-case bounds.

Fresh campaigns require revision 5 and a fresh output directory. Report-only mode
needs no backend/compiler and still accepts older fixture versions with their
original media and geometry labels. Revisions 2/3 and the pre-refresh audio report
are historical configurations; rerun the matrix for an upstream performance
comparison.

Every new campaign writes `runtime.json` and `runtime.md` with setup/build,
serial host cases, parallel RTL cases, report generation and individual child
start/end times. Parallel child durations overlap and must not be added to obtain
the suite's elapsed time. `--host-profile` adds per-process `.host.json` files
covering preparation, the callback loop, stimulus, command delivery, validation,
snapshot observation, memory-statistic finalization, teardown and JSON output.
These are inclusive host wall-clock counters; nested phases overlap.

On Linux, `--profile-rtl` implies `--host-profile` and additionally uses installed
GNU gprofng to sample each RTL process's CPU stacks every 20 ms. Experiment data
is saved in `.er` directories. Sampling adds host overhead; neither timers nor
sampling advance the model's clock. The timed fixture and FPGA cycle statistics
are unchanged. Inspect a completed experiment with
`gprofng display text -limit 40 -functions <case>.er`.

The published [audio/MIDI smoke report and raw bundle](../../../benchmarks/rt-audio-midi-smoke/README.md)
cover all three media profiles. Capture took about 54 minutes with three RTL
workers and a prebuilt binary/model cache on the recorded host; regenerating its
five-page report took 2.4 seconds.

The subsequent [profiled smoke replay](../../../benchmarks/rt-smoke-runtime-20261121/README.md)
took 39:51 with three RTL workers and warm caches: 39:37 in RTL tasks,
12.45 seconds in CPU/reference cases, and 1.34 seconds generating the report.
It includes a worker timeline, per-case preparation/processing/validation times
and exported CPU sampling tables. Signal access and its callees consumed 64.2%
of sampled RTL-process CPU. All 1,590 RTL callback cycle/traffic/stage records
match the preceding capture. Host conditions and sampling affect wall time;
the replay applied no simulator-speed optimization.

Manual workflow:

The **RT backend benchmarks** Actions workflow is manual-only. Select a branch
and `tiny`, `quick`, `full`, `rtl-audio-ddr` or `rtl-midi-ddr` under Actions → Run workflow. Its run summary links
directly to the PDF artifact; a separate data artifact retains JSON/CSV, runtime
breakdowns and logs for 30 days, including evidence from failed campaigns.
Each variant has a separate concurrency group, so launching all three does not
cancel any of them. The workflow keeps the registered `fpga_simulation.yml` path
so it can be dispatched on an unmerged PR; automatic component verification now
lives in `fpga_components.yml` and does not run these benchmarks.

```sh
gh workflow run fpga_simulation.yml --ref <branch> -f variant=tiny
gh workflow run fpga_simulation.yml --ref <branch> -f variant=rtl-audio-ddr
gh workflow run fpga_simulation.yml --ref <branch> -f variant=rtl-midi-ddr
```

Validation:

```sh
cargo test --locked -p shoop_rt_benchmark -p rt_fpga_compiler
cargo test --locked -p rt_fpga_reference --all-features
cargo test --locked --release -p shoop_rt_benchmark --features rtl
python3 -m unittest discover -s scripts/tests -p test_benchmark_rt.py
```

Rust smoke tests exercise the complete mandatory patterns and guard native and
functional-reference processing against heap allocation. They check exact stereo
samples, exact MIDI bytes/order/offsets, same-frame commands, finalized take
lengths, block growth, wraps, retakes and planned-slot retirement. Separate tests
check shared-port clocking and retrying invalid MIDI input without consuming
endpoint commands. Reporter tests enforce the page limit across all three media
profiles and retain every per-repeat export.
