# Profiling measurements

Build and run the owning benchmarks in release mode with competing heavy work
stopped. Report the workload, configuration, timings, deadline misses, memory and
failures. Compare identical workloads on the chosen baseline and candidate;
these offline measurements do not qualify physical audio/MIDI hardware.

```sh
cargo build --release -p shoop_rt_engine --example profiling_benchmark --example profiling_clock_benchmark
cargo build --release -p shoop_egui --example profiling_table_benchmark
mkdir -p artifacts/runtime-profiling
python3 scripts/benchmark_profiling.py --baseline /absolute/path/to/baseline/profiling_benchmark
target/release/examples/profiling_clock_benchmark > artifacts/runtime-profiling/native-clock.json
node scripts/benchmark_profiling_clock.mjs > artifacts/runtime-profiling/wasm-import-clock.json
target/release/examples/profiling_table_benchmark > artifacts/runtime-profiling/ui-frames.jsonl
cargo run --release -p shoop_rt_engine --example profiling_benchmark -- 16 32 256 on partitions
cargo run --release -p shoop_egui --example profiling_graph_benchmark
cargo run --release -p shoop_egui --example profiling_graph_benchmark -- --dense
cargo run --release -p rt_profiling --example capture_benchmark
```

The comparison runner writes raw callback samples, summaries and a table under
`artifacts/runtime-profiling/` by default. Use `--output` for another local artifact
directory. The baseline binary must support the same benchmark arguments and
output format; an arbitrary older application binary is not interchangeable.
Retain all repeated samples, including noisy runs and deadline misses, with the PR
or CI artifact. Historical results remain in Git history.

The screenshot images in this directory illustrate the interface using synthetic
data; they are not timing evidence. See the [profiling guide](../runtime_profiling.md)
for operating the profiler.
