# Shared RT benchmark: 32-frame chunks, 32-chunk blocks

[Open the PDF report](report.pdf) · [Download raw results](results.tar.gz)

Measured source: `2f3d34f4cca1999221360ed36e134355ba1f05ad`. Fixture revision 3 uses 64 stereo audio loops, Q=callback=32, 48 kHz, and 1,056-frame recordings. Both FPGA adapters use 32-frame chunks and 32-chunk blocks (1,024 frames/block), with a 16 MiB ordinary-media budget. RTL uses DRAMsim3 and a 150 MHz fabric clock.

All 21 runs are accounted for: 17 completed and four explicitly unsupported FPGA grab runs. The campaign retained 46,015 callback measurements. Every completed run passed exact stereo samples, command execution-frame checks, allocation growth, storage release and wrapping; mandatory patterns also verified simultaneous growth at the first 1,024-frame boundary and finalized take lengths. Lifecycle additionally checked take replacement and planned-slot retirement.

RTL measurements cover the complete callback and convert fabric cycles to microseconds as cycles/150. Simulator wall time is retained separately. Stage/stall activity overlaps. Values are observed maxima under this fixture; they are not proven worst-case bounds or board certification.

| RTL pattern | Callbacks | Median µs | Maximum cycles | Maximum µs | Over 666.67 µs |
| --- | ---: | ---: | ---: | ---: | ---: |
| lifecycle | 1096 | 296.40 | 133,777 | 891.85 | 7 |
| mixed | 636 | 299.75 | 90,251 | 601.67 | 0 |

CPU/reference results retain three independent repetitions; deterministic RTL replay runs once. Native CPU timings include scheduler variability from the recorded host. Rust FPGA reference time describes CPU execution of the functional model.

Regenerate the report from the published JSON without building a backend:

```sh
python3 -m pip install 'matplotlib>=3.9,<4'
mkdir -p work/rt-published-report
tar -xzf benchmarks/rt-standard-32x32/results.tar.gz -C work/rt-published-report
python3 scripts/benchmark_rt.py --report-only --output work/rt-published-report
```

Run a fresh standard campaign using the prerequisites in [the benchmark documentation](../../src/rust/shoop_rt_benchmark/README.md):

```sh
python3 scripts/benchmark_rt.py --output work/rt-benchmark
```

The archive includes the complete fixture and hash, per-callback/event measurements, backend/model/toolchain provenance, correctness coverage, final drain statistics, CSV, summary and manifest. The PDF and data bundle are a published example; ordinary generated campaigns stay in the caller-selected output directory.
