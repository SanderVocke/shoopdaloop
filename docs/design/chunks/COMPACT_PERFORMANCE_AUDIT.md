# Compact performance completion audit

Objective: fully execute [`PLAN_COMPACT_PERFORMANCE.md`](../../../PLAN_COMPACT_PERFORMANCE.md).
Audited final implementation: `44241a05`; documentation head: `f6ffde4f` plus
this audit. Delivery PR: [#963](https://github.com/SanderVocke/shoopdaloop/pull/963).

## Prompt-to-artifact checklist

| Requirement | Concrete evidence | Status |
| --- | --- | --- |
| Pi 4 is the sole performance target | Every baseline/final `scheduling.json` records CPU 1, scheduler 1/FIFO 80 and RT runtime `-1`; machine/run files are in the published archives. No laptop timing is used in result tables. | Complete |
| Short iterative benchmarks | Stage runs used 128 Q=32 callbacks and one/two repetitions; acceptance used 256 Q=32 or 32 Q=256 callbacks, four repetitions. | Complete |
| Laptop builds do not saturate CPU | `scripts/build_pi4_compact.sh` re-execs under `nice -n 15 taskset --cpu-list 0,1`, caps Cargo/CMake/Ninja/Make/Rayon at two jobs and prints architecture/hashes. Builds were not run concurrently. | Complete |
| Reproducible dense workload | `dense_chunk_benchmark.rs` creates eight tracks, 64 grid loops, 32 active voices and validated stereo media; profile helper accepts matching pool/callback limits. | Complete |
| Stage 0 baseline | 16 Q=32 plus 16 Q=256 validated runs from `4a293b51`; archive `compact-baseline-4a293b51.tar.gz`, SHA-256 `b2c02d51bcbc2599bfbbd5dc4cf5d043e044dd6ee6aaba754f2a01bc8474af10`. | Complete |
| Stage 0 profiling | Separate 99 Hz zero-loss render-thread call stacks and PMU-stat runs for idle/play/record after a five-second launch settle. Baseline IPC 0.55–0.64; archive and method in `COMPACT_PERFORMANCE.md`. Standalone host Perfetto control is explicitly unavailable. | Complete |
| 1. Skip dormant loop work | `09391ebd` adds bounded activity queries and capture skipping; `09d69a47` clears outputs before skipping stopped/no-source playback and settled silent faders. Dense idle executes 21/216 operations. | Complete |
| 2. Reduce routing/clearing/dispatch | Skipped operations bypass input clearing, route traversal and processor/reference dispatch while unknown processors retain fallback. Exact graph/sample tests pass. | Complete |
| 3. Reduce callback overhead | `489e7ea3` omits the per-node MIDI buffer sweep only when preparation proves there are no MIDI buffers. MIDI executor and native CPAL/JACK contract tests pass. | Complete |
| 4. Reduce recording index work | `8b76497d` removes the redundant next-page lookup while retaining a debug invariant; all 96 runtime tests and capture/cancellation/wrap tests pass. | Complete |
| Additional bounded cleanup | `44241a05` removes duplicate compact configuration application after activity checking; exact sample and structure tests pass. Its separate timing effect is reported as noise-level. | Complete |
| Behavioral parity | `shoop_rt_engine` audio-variant sample/capture/partition tests, retained-grab tests, full runtime/loop/engine suites and dense media/state checks pass. The stale-output prototype failed and was corrected before retention. | Complete |
| RT bounds | Existing heap/destructor guards pass in runtime, loop, engine, dummy, CPAL and worklet tests. New callback work uses prepared fields/slices only; no unsafe code, allocation, lock, wait or I/O was added. | Complete |
| Diagnostics integrity | `nodes_skipped` is exported beside executed nodes; callback CSV still records each callback, drop and render-fault state. Final archives validate zero dropped records/fault rows. | Complete |
| Q=32 performance threshold | p50 reductions: idle 40.9%, playback 22.3%, record 19.4%, grab 21.1%; active overruns 545 → 130 (76.1%). Required minima were 10% and 20%. | Complete |
| Q=256 non-regression | p50 improves 18.1–36.2%; p99 improves 17.8–34.7%; two baseline record overruns become zero. Required maximum regression was 5%. | Complete |
| Final raw evidence | `compact-final-44241a05.tar.gz`, SHA-256 `0d74dcb493213ee2186b9fb81619f6b5f2c77e2783d36ccbfd85b6d96132dd4a`; 32/32 runs validate. | Complete |
| Final profiles | `compact-profiles-44241a05.tar.gz`, SHA-256 `ad679275f0f560032d22a57777532b81c8272a82743542dfcf5b141f8f2bb8dd`; final IPC 0.60–0.74. | Complete |
| Visual/report evidence | `pi4-benchmark-graphs-compact-optimized.zip`, SHA-256 `4fff276ad4b9da4f5138982e1d134b356a691870f7ab2347fb05475311e7f913`; artifact commit [`9c58b4d`](https://github.com/SanderVocke/shoopdaloop/commit/9c58b4d). | Complete |
| Formatting/build policy | Final `cargo fmt --all -- --check`, warning-denying AArch64 release host/launcher build, test-attribute policy and Python syntax checks pass. | Complete |
| Portable boundaries | Final `wasm32-unknown-unknown` builds for changed lower crates, engine dependency-boundary checker and `thumbv7em-none-eabihf` no-std core build pass. | Complete |
| Native broad gate | Full constrained nextest ran 3,192 tests: 3,191 pass; unrelated LAN startup hits its existing 15 s timeout only under the two-CPU cap. Dedicated changed-crate and native driver tests pass. | Complete with reported environment limitation |
| CI and review | PR #963 is open/unmerged. Hosted processor checks passed on the implementation head. Broader manual CI reported a coverage-only JACK revision conflict after the same test passed in nextest and a macOS release failure. No automated reviewer was available. User approved stopping repeated CI polling; failures are not labeled green. | Complete under approved delivery exception |
| Worktree hygiene | Only the pre-existing `dsp.png` through `dsp5.png` remain untracked; implementation files are committed and pushed. | Complete |

## Acceptance conclusion

All four requested optimization areas, staged verification, matched Pi performance
thresholds, portability checks, documentation, evidence publication and PR delivery
are complete. The remaining larger opportunity is recording planner settlement and
cross-loop capture reservation; it would require a more invasive active-set/batching
design and is intentionally outside this bounded follow-up. CI exceptions and the
single constrained local timeout are disclosed rather than treated as passing.
