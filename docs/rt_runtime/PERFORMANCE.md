# Reproducible bounded-runtime measurements

These are historical initial-runtime measurements, not a new planner/group/loop campaign or final-head performance certification. `bounded_runtime` uses legacy quota-based capture/resource access; its counters/timings are not required-RenderIo or coordinated-group throughput. Later 1,024-callback documentation smokes are correctness executions, not replacements for the recorded full campaign. No loop benchmark or independent loop-host/newer deadline evidence is supplied in this snapshot. See the [capacity/work reference](../../src/rust/rt_runtime/docs/reference/configuration-and-errors.md) for current accounting.

For the subsequent native loop campaign, use the separate
[loop measurement guide](../../src/rust/shoop_loop_engine/docs/performance.md)
and [source-scoped results](../loop_layer/evidence/benchmark-stage8.md). It does
not replace or relabel the historical runtime measurements below.

Run from the workspace root:

```sh
RUSTFLAGS="-D warnings" python3 scripts/benchmark_rt_runtime.py
# Short CI measurement, with the same correctness/guard checks:
RUSTFLAGS="-D warnings" python3 scripts/benchmark_rt_runtime.py --iterations 4096 --trials 1
```

The runner builds `src/rust/rt_runtime/examples/bounded_runtime.rs` in debug/release, proves six deliberate allocation/reallocation/deallocation violations fail through the real guard, then measures release callbacks. Each case warms up for 512 callbacks and defaults to 20,000 measured callbacks in each of three separate processes. Linux runs pin to the first permitted CPU (`--cpu` overrides). JSON summaries, individual callback CSVs, and canary/build logs go to `target/rt-runtime-bench`. The report records compiler, release profile, flags, source SHA-256, hardware, governor, affinity, and limitations.

This is a **native core microbenchmark**, not a device benchmark, MessagePort throughput measurement, or comparison with the Shoop engine. `Instant` measures elapsed time around the guarded Rust render path, including guard overhead and preemption; it does not measure hardware CPU cycles. Control submission, result consumption/destruction, sample/event verification, and file I/O run outside the timed callback. Measurements use normal OS scheduling, not a real-time priority.

## Workloads and checks

All cases render 128 frames at 48 kHz. The nominal callback interval is 2,666,666 ns. The simple processor applies `input * gain + 0.03125` to two channels; this intentionally is not a simulation of expensive plugins.

| Case | Work |
| --- | --- |
| `idle` | Empty graph and silent outputs |
| `dag-16` | 16 stereo nodes, 34 edges, gain 0.75 |
| `dense-dag-64` | 64 nodes and 256 edges, exactly the configured maxima; both previous channels feed both next inputs, gain 0.375 |
| `midi-64-through-8` | Stereo chain plus 64 three-byte MIDI packets through eight nodes every callback |
| `stereo-capture` | Continuous 1,024-byte stereo captures; rotate recording every 32 callbacks while recycling the previous prefix |
| `graph-transfers` | Repeated validated rewires of 16 retained nodes, alternating host channels; old plans recycled on the control side |
| `resource-roundtrip` | Continuous eight-node audio, 64 KiB uploads, commits, snapshots, and exports using 2,048-byte chunks and one outstanding command |

The copy budget is 2,048 bytes and validation/maintenance budget 128 steps per callback. Capture reserves 1,024 bytes for processor I/O; resource transfers consume the full copy budget. Resource arenas contain 16 × 256 bytes normally, 256 × 256 bytes for capture, and 64 × 2,048 bytes for resource transfers. Metadata, graph scratch, queues, measurement storage, and host buffers are additional allocations. These are not total-memory estimates.

Each callback is heap-guarded and its samples are compared with a scalar reference. MIDI packets and offsets are checked exactly. Exported upload and capture bytes are verified. Callback counters must stay within budgets; render errors and MIDI loss fail the program. Teardown stops captures, cancels partial uploads, releases leases, removes processors/resources, drains reclamation, and checks all pages/version/lease slots and admission credits return. Any failure makes the runner fail regardless of timing.

CSV rows retain callback number, elapsed nanoseconds, copy bytes, validation steps, nodes, edges, and graph epoch. Reports include five slowest callbacks per case, work peaks, refusals, activations, and throughput using **both** summed callback duration and simulated audio duration. Core copy counters cover budgeted transfers/resource I/O, not every audio-routing memory access. Used-page peaks are sampled at callback boundaries. Linux `VmHWM` is cumulative whole-process RSS, including measurement storage; unsupported systems report `null`. OS context-switch/fault deltas cover the measured loop including its control-side verification, not individual callbacks.

## Recorded Linux results

Evidence: [`evidence/benchmark-stage7.json`](evidence/benchmark-stage7.json). Host: AMD Ryzen AI 7 350, Linux 7.0.3 x86-64, 16 logical CPUs, CPU 0 affinity, performance governor, Rust 1.97.1, warnings denied. Three trials × seven cases × 20,000 = **420,000 measured callbacks**. All passed correctness/ownership/heap/budget checks; **zero observed deadline misses**. This is an observation, not a deadline guarantee.

| Case | Median range (µs) | p99 range (µs) | Largest observed callback (µs) |
| --- | ---: | ---: | ---: |
| Idle | 0.060 | 0.070–0.071 | 22.993 |
| 16-node DAG | 1.112 | 1.142–1.473 | 19.497 |
| 64-node dense DAG | 6.061–6.091 | 6.462–9.708 | 23.996 |
| MIDI burst | 11.712–12.052 | 13.254–14.627 | 35.857 |
| Stereo capture | 1.183–1.193 | 2.695–2.705 | 19.116 |
| Graph transfers | 1.413–1.423 | 1.833–2.034 | 21.480 |
| Resource round trip | 0.722–0.731 | 1.092–1.112 | 13.686 |

Each trial copied 20,480,000 capture bytes, activated 10,000 graph rewires, and transferred 37,445,632 upload/export bytes. The latter is about 0.670 MiB per simulated audio second; the much larger cached CPU-loop throughput is separately labeled in the JSON. No workload had an admission/pool refusal in this configuration; saturation and refusal behavior are tested separately, not inferred from these timings. Maximum observed copy/validation work was exactly 2,048/128. Capture/resource page peaks were 128/64 respectively. Whole-process RSS peaked at roughly 4.4 MiB in the recorded runs; it is not a portable bound on the library alone.

### Tail investigation

A preliminary three-trial run, preserved in `evidence/benchmark-stage7-preliminary.json`, included one 90,159 ns dense-DAG callback (callback 2,069 of trial 1) above the investigation heuristic `max(50 µs, 8 × p99)`. Its work was identical to surrounding callbacks: 64 nodes, 256 edges, no transfers or validation, epoch 1. No data-dependent growth or ownership operation explained it. Capture's regular higher percentile is expected: rotation can reclaim 128 pages under the declared maintenance budget.

After adding OS-counter reporting outside the measured callback, the three recorded trials above had no heuristic tail candidates. Every case had involuntary context switches; no major faults occurred, and only the first two cases recorded minor faults (two and one). These observations are consistent with scheduling/system noise, but do not attribute any particular outlier conclusively. The preliminary maximum was still under 0.1 ms versus the 2.667 ms nominal interval. No algorithmic speedup is claimed from the repeat. Raw CSVs make future tail investigations possible instead of hiding maxima behind averages.

CI gates correctness, bounded work/memory, and canary detection. It uploads timings and records misses; it deliberately has **no wall-clock performance threshold** on shared runners.

## Actual browser failure detection

`python3 scripts/run_rt_runtime_hosts.py --verify-failures` first runs the complete real offline/live worklet scenario and native equivalence checks. It then launches two fresh browser sessions that must fail: a deliberate processor exception and a forbidden Rust allocation/deallocation inside the guarded worklet call. The latter must produce a Wasm runtime trap, not a missing binding or ordinary JavaScript error. The child runner must exit nonzero with the expected diagnostic; an unrelated failure, timeout, or undetected violation fails verification.

Both expected failures were observed with exit code 1 on Chromium/ChromeDriver 147.0.7727.137. Normal rendering still used ordinary independent memories, no cross-origin isolation, and unchanged 1,507,328-byte Wasm memory. See `evidence/hosts-stage7-failure-gates.json`. These intentionally failing instances are disposed by ending their browser sessions; they are not evidence for graceful shutdown. The successful scenario independently checks full pool/credit recovery and non-callback disposal.
