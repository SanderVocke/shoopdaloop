# Observed test-suite execution times

Notes for designing a later test strategy, not new acceptance thresholds or
permission to omit required gates. Recorded 2026-10-26 during Stage 5 facade work.

## Conditions and provenance

- Local Linux/Nix development environment on an AMD Ryzen AI 7 350
  (8 cores / 16 logical CPUs). No CPU-frequency pinning or isolated benchmark host.
- Existing build/dependency caches; these are warm/incremental runs, **not cold
  builds**. Most commands include Cargo/build/harness startup as applicable.
- Rust warnings denied. Workspace nextest pinned to 0.9.116. Node/Chromium use
  the versions and paths recorded in the facade evidence.
- Broad gates run sequentially, while the selected Miri runner runs concurrently.
  Initial compilation/CPU contention can affect both. No statistically controlled
  repeated benchmark or CI-platform scaling study was performed.
- Exact command wall times were measured with Python `time.monotonic()` in
  `docs/loop_layer/evidence/facade-stage5.json`. Logs live under
  `target/loop-layer-facade-stage5/`.
- The preceding broad run and its source/logs are preserved under
  `target/loop-layer-facade-stage5-first/`, including `checks-result.json`.
  That run had one fewer runtime test; the two observations are not identical-tree
  repeated measurements.

## Warm command wall times

| Command / scope | Latest | Previous observation | Scope notes |
| --- | ---: | ---: | --- |
| `cargo test --locked -p rt_runtime -p shoop_loop_engine` | 8.44 s | 8.31 s | Latest: 178 runtime + 94 loop tests; includes Cargo/harness overhead |
| Same packages, `--release` | 12.83 s | 12.77 s | Same latest test counts; incremental release build included |
| `cargo build --locked --workspace` | 1.40 s | 2.58 s | Warm build; not representative of rebuilding the full workspace from scratch |
| Package rustdoc, `--no-deps` | 0.96 s | 0.89 s | Both runtime and loop packages; warnings denied |
| `scripts/run_wasm_tests.py --runtime node --package rt_runtime --package shoop_loop_engine` | 200.04 s (3m20s) | 199.32 s | Latest: 174 runtime + 94 loop tests; includes package Wasm build/harness work |
| Same Wasm packages, `--runtime chrome` | 215.99 s (3m36s) | 216.95 s | Real browser package tests, not the loop AudioWorklet acceptance fixture |
| Workspace nextest with `shoop_engine/app_backend`, CI profile | 42.59 s | 44.58 s | Latest: 2,072 passed, four existing skips; `SHOOP_ALLOW_MISSING_BACKENDS=1` |
| Existing generic runtime native host, debug | 1.77 s | 1.82 s | `scripts/run_rt_runtime_hosts.py --native-only --profile debug` |
| Existing generic runtime native host, release | 3.40 s | 3.12 s | Corresponding release replay; not a fresh browser session |
| Existing actual-worklet regression and failure canaries | 40.68 s | 40.52 s | `scripts/run_rt_runtime_hosts.py --verify-failures`; generic fixture, not new loop-host acceptance |
| Coordinated loop example, debug / release | 0.11 / 0.11 s | 0.13 / 0.12 s | Warm `cargo run`; full stereo/MIDI lifecycle assertions |
| Coordinated loop example, Wasm build | 0.67 s | 1.33 s | Compilation only, not execution |
| Runtime / loop dependency-boundary checks | 0.47 / 0.44 s | 0.47 / 0.48 s | Four target dependency trees each |
| Test-attribute / tracing-inventory policies | 0.04 / 0.05 s | 0.05 / 0.04 s | Separate Python commands |
| Miri-runner Python unit tests | 0.08 s | — | Three timeout/output/process-cleanup tests, **not Miri interpretation** |

The latest sequential **25-gate broad driver totals 532.27 s (8m52s)** by summing
its measured command durations. This excludes the concurrent Miri runner,
separate compile-fail proofs/provenance work, and subsequent documentation checks.
The preceding driver had 24 gates, before the runner-hygiene tests were added.

A focused native `shoop_loop_engine --test runtime` run reported **0.13 s for ten
integration tests** after about 1.3 s of incremental compilation. That harness
number excludes Cargo startup/build and should not be compared directly with
whole-command wall times above.

## Miri: much slower, and its harness clock is not wall time

- The full latest selected run completed **159 executions** (118 runtime + 41
  loop), including all ten new facade integration cases, in one invocation.
- Approximately **64 minutes elapsed** between launching the parallel broad/Miri
  tools and their completion, as observed in session progress. The Miri wrapper
  did not record an exact whole-run monotonic duration, so this is an approximate
  planning observation, not a precise benchmark.
- A separately timed dense facade test completed in **890.36 s wall time
  (14m50s)** after the MIDI pedal refinement, before reducing unused checkpoint
  storage in that playback-only fixture. Its command, source comparison and
  timing are in `target/loop-layer-facade-stage5/miri-dense-refinement.json`.
- The prior dense invocation reached the **900-second wall watchdog** and had no
  success result. Its partial output and complete pre-refinement sources remain
  in the first-run archive. Timeout cleanup originally left an interpreter child;
  the runner now kills/reaps its process group and retains captured output.
- All three dense profiles still execute 128 one-sample boundaries and compare
  every sample/packet/journal/ownership result. The later fixture change reduces
  unused MIDI index storage from eight versions × eight rows to two versions ×
  one row, sufficient for its two immutable imports (one/two packets, replay span
  128). It changes no musical workload, guard or assertion, on any platform.
  The final full run passed with the **same 900-second per-command watchdog**.

**Do not interpret Miri's printed `finished in …s` as elapsed host time.** For
example, the precisely timed 890.36-second command printed **1682.05 s** in its
Rust test summary. Miri supplies a simulated clock. The final dense case prints
1677.68 s but passed the real 900-second subprocess watchdog. Record outer
monotonic durations when collecting future Miri timings; do not sum these
simulated harness values into a supposed wall-time total.

These observations do not establish a stable optimization speedup, a safe CI
watchdog margin, or a callback deadline guarantee. Native callback timings and
Miri interpreter costs are different measurements.

## Implications to consider when designing the strategy

1. Focused native tests and cheap policies provide fast iteration feedback.
   Keep full independent sample/event/content assertions rather than replacing
   them with smaller smoke tests merely to improve turnaround.
2. The two package Wasm commands account for about seven minutes of this warm
   broad run. Run both for portability changes; do not count Node as browser
   evidence or package browser tests as actual AudioWorklet evidence.
3. Miri dominates turnaround and deserves separately planned execution/artifact
   collection. Preserve the complete selected set and explicit filters/counts;
   label partial runs honestly and retain timeout output.
4. Collect cold-cache, CI matrix, machine-load and per-command Miri wall times
   before setting scheduling expectations. The Windows/macOS, full application
   browser suite, future loop host and future loop benchmarks are **not measured
   by this note**.
5. These are scheduling inputs only. The plan's required final-head gates,
   semantic workloads, guards and failure canaries remain unchanged.
