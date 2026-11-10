# Rust FPGA reference

An audio-only functional reference for the decomposition in `FPGA_ARCHITECTURE.md`.
Readability and explicit subsystem boundaries take precedence over performance.
This is not a cycle simulator, RTL generator, or hard real-time guarantee.

## Follow one quantum

1. `compiler::compile` visits the graph, assigns temporary buffers and state
   addresses, and produces a sequential program. It rejects cycles, missing
   inputs, sink-as-source connections, and duplicate physical output writers.
2. `state::StateProcessor` executes that program. It loads/stores metadata,
   advances playback positions and ramp state, and emits concrete jobs. It has
   no graph or sample-store access.
3. `jobs::JobProcessor` drains the FIFO in order. Only this side resolves media
   views, accesses sample payloads, and performs sample arithmetic.

The host-facing `Backend` joins these phases without giving either processor
access to the graph. Its bindings allow controls to update state addresses.
Instructions and jobs derive `Debug` so their complete contents can be inspected.

```sh
cargo run -p rt_fpga_reference --example record_play
cargo test -p rt_fpga_reference
```

`record_play` prints the compiled instructions and emitted jobs, records a block,
then verifies exact playback through a separately compiled program.

## Deliberate first-model choices

- Single issue, four typed metadata registers; no VLIW yet.
- Input, looping playback, gain/ramp, binary mix, capture, and output nodes.
- A finite job FIFO holds an entire quantum before execution. Insufficient
  capacity is refused before state changes. There is no concurrent execution or
  cycle-level backpressure model yet.
- Fresh temporary buffers per producing operation, valid for one quantum. Fan-out
  shares a buffer. Reads before production and multiple writes are errors.
- A `Memory` interface separates job execution from external-memory representation.
  `MediaStore` models mono chunks and external chunk-ID tables; extra channels use
  separate paths. Chunk size and processing quantum are independent.
- Recordings append privately, then seal for immutable playback. IDs never recycle;
  snapshots, editing, reclamation, and finite external-memory admission are deferred.
- Samples and gain factors use provisional signed Q8.24 (`i32`). Multiplication
  uses `i64`, truncates toward zero, and saturates to `i32`; binary mixes saturate
  per job. Ramp interpolation uses a wide intermediate and its original time
  domain. This arithmetic is a readable reference, not a proposed hardware layout.
- A standalone ramp starts at its current value on the first affected sample and
  reaches the target at offset `duration`. A zero duration changes immediately.
- Compilation and standalone capture may allocate. No standalone RT claim is made.
- Execution errors can leave partially completed work; stop and reconstruct the
  backend rather than retrying an affected quantum.
- No MIDI, effects, smoothing, history/grab, or advanced loop editing.

## Normal app / host selection

The native host has a fourth experimental audio renderer:

```sh
SHOOP_AUDIO_RENDER=fpga-reference SHOOP_AUDIO_BENCH_AUDIO_ONLY=1 \
  target/debug/shoop_host --config host.json --export-profile profile.json
# Attach the normal app to the exported profile using --ui-only.
```

The native adapter disables device MIDI, input history, restart smoothing, and
hosted processors for this mode. It supports basic audio strips, input monitoring,
recording, playback, gains/balance/mute, routing, and buses. Unsupported processing
nodes are refused rather than run through an existing audio renderer. MIDI tracks,
processed tracks/buses, preview, nonzero smoothing, special playback windows, and
advanced recording modes are outside the subset.

The app bridge lives in `rt_runtime::fpga` and the engine's `compile_fpga` methods.
It translates the active prepared graph and settled app metadata into a program
for each quantum, then invokes the same reference state and job interpreters.
The graph is visited only during compilation. The compiler uses prepared storage;
rendering remains under the engine's existing heap guards.

The existing app control endpoint and loop planner still settle musical
transitions, media ownership, and grouped capture publication. This adapter does
not claim to replace that entire control system with state-machine instructions.
The standalone compiler additionally illustrates persistent playback/ramp state
owned directly by the state processor. Moving more app state into that program
is a subsequent experiment.

`ExternalMemory` adapts existing retained media indexes and capture intervals to
view/range jobs. App storage and I/O retain their existing `f32` representation;
conversion occurs at the fixed-point job boundary. The standalone `MediaStore`
uses actual fixed-point chunks. Neither model requires full media indexes in
on-chip state memory. Native ramp endpoints come from existing control semantics,
which differ from the standalone ramp's start-at-offset-zero convention.

Existing callback adaptation, bulk media export and telemetry are host services.
Their code and overhead are included in an app-connected run, not models of FPGA
sample-engine execution. Chunk-copy counters do not measure total DDR traffic.

## Comparison

`shoop_rt_engine`'s `audio_variants` test compares all four renderers using the
same audio scenario, including capture, simultaneous playback/monitoring,
controls, wrap, deletion/recreation, and callback partitions 1/13/128 at Q=1/32.
Fixed-point output/capture uses numerical tolerance. Ramp endpoints are compared;
stacked native descriptor ramps may differ between endpoints.

```sh
cargo test -p shoop_rt_engine --test audio_variants
cargo build -p shoop_host --no-default-features --features testing \
  --bin shoop_host --example chunk_benchmark
python3 scripts/benchmark_audio_chunks.py \
  --host target/debug/shoop_host \
  --launcher target/debug/examples/chunk_benchmark \
  --output work/fpga-reference-comparison \
  --modes current chunks chunks-descriptors fpga-reference \
  --scenarios idle record play record-play ramps churn
```

The comparison script keeps its historical three-renderer default. Explicitly
select the fourth mode and compatible scenarios; history/grab scenarios are
unsupported. Timing numbers characterize this readable software implementation,
not predicted FPGA timing.
