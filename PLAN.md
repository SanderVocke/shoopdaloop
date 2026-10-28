# Investigate Carla processing performance

## Goals and scope

Determine whether a reproducible performance regression exists between ShoopDaLoop 0.2.0, the Rust backend before/after direct native Carla hosting, and the current implementation. Separate Carla build/version effects, LV2/native hosting effects, and engine/bridge execution overhead. Produce evidence even if the reported regression cannot be reproduced.

Run headless Linux experiments autonomously in the Codex environment. macOS support, GUI changes, speculative optimizations, and reproducing the user's physical laptop/audio interface are outside scope. Do not fix performance issues until the measurements identify a cause.

## Immutable acceptance criteria

- A documented runner reproduces build preparation, workload validation, measurements, and analysis without manual UI interaction.
- Experiment 1 compares reference and packaged-build recipes at the same Carla revision through both native and LV2 interfaces. Different revisions are compared separately when the historical revision can be established.
- Experiment 2 compares direct native calls, the in-process bridge, the subprocess bridge, and an equivalent headless engine session using the same Carla/plugin binaries and workload.
- Experiment 3 attempts equivalent measurements at 0.2.0, immediately before/after the native switch, and the pinned current revision. Any unavailable or incompatible cell has an explicit reason; it is never represented as passing or as evidence of equivalence.
- Every successful measurement validates plugin loading, active processing, expected output, actual frame counts, and configured sample rate/buffer size. Failures, fallback output, and deadline misses are counted explicitly.
- Results include exact source/binary identities, build settings, raw measurements, repeated-run distributions, and a report classifying each hypothesis as supported, unsupported by these measurements, or inconclusive.
- No historical regression is claimed from a current-only experiment. No physical-device xrun claim is made from offline or VM timing.

## Design rules and constraints

- Pin the initial current baseline to `580f6191c9b2d00c6ad43fcbf30445e590235a49`; do not silently follow moving branches. Verify `9b169b5` and `0cec63c` and their relationship before treating them as the native-hosting boundary. Resolve the exact 0.2.0 release commit from release metadata/history; local tags are currently absent.
- Preserve the existing checkout. Create a dedicated investigation branch in an isolated checkout under `work/`, with separate historical checkouts and build directories. Keep downloaded runtimes, logs, and intermediate data under `work/`; export reviewable results to the session's designated `output/` directory. Commit scripts, compact fixtures, and the report, not binary runtimes or large raw captures.
- Use release builds and record compiler versions, flags, dependency locks, runtime paths/checksums, plugin format/preset, and CPU/container limits. Activate the prepared toolchain using `/workspace/.setup/activate.sh`; follow repository build guidance if the execution environment changes.
- Carla is currently pinned to 2.5.10 by `third_party/carla/runtime-lock.json`. Establish the older release's actual Carla provenance rather than assuming it. Distinguish upstream defaults from a distribution recipe or historical binary whose settings cannot be reconstructed.
- Hold plugin format, routing, channel count, input, and preset constant across comparisons. Begin with a stereo rack. Additional patchbay/channel-count cases follow only when a result warrants them.
- Use an empty rack, a deterministic single-threaded test plugin with adjustable bounded DSP work, and one obtainable open-source effect. Use identical plugin binaries across hosts. Voyage Voyage is optional if it runs unattended; its absence must not block the core experiments.
- Keep measurement hooks outside normal production operation or explicitly benchmark-only. Preallocate sample storage; no logging, allocation, or file I/O in timed processing. Quantify timer/instrumentation overhead and retain low-instrumentation control runs.
- Measure monotonic wall time and processing-thread CPU time on their owning threads. Thread CPU time is not total plugin CPU time for a multithreaded plugin; record that limitation and use the single-threaded fixture for attribution.
- Embedded Carla DSP load is a decaying peak of elapsed processing time divided by configured period, not a mean or remaining host budget. JACK-backed Carla's meter has different scope. Use measured durations as primary evidence and audit the meter separately.
- Run timing experiments serially, with builds stopped and controlled load processes cleaned up afterward. Record affinity, scheduling policy, CPU quota/throttling, and other available contention indicators. Do not assume exclusive physical CPUs or real-time scheduling privileges.
- Benchmark deadline misses are software timing observations, not hardware xruns. Unsupported scheduling controls or missing historical dependencies become explicit limitations; do not substitute unequal workloads unnoticed.

## Stage 1 — Freeze inputs and prepare isolated builds

- [x] Create the investigation branch/checkouts and record the untouched source checkout's status. Keep this plan updated in the investigation branch.
- [x] Read applicable repository instructions in each historical checkout before changing it. Inspect the current benchmark examples, `carla_native.rs`, `carla_processor.rs`, `carla_subprocess.rs`, `engine.rs`, runtime lock, and runtime packaging/build workflow.
- [x] Resolve all comparison commits and historical Carla provenance. Record available dependencies, VM limits, toolchain, and runtime acquisition/build commands. Use the cloud environment networking guidance before downloads or container builds.
- [x] Prepare reference and packaged-recipe Carla builds from the same source revision, including native and LV2 artifacts. Use the same compiler/dependencies where possible; list intentional differences. Add historical Carla revisions only as a separate axis.
- [x] Verification: probe every runtime, record actual loaded library/plugin identities, and verify the build/interface matrix is feasible. Report missing provenance or unavailable cells before interpreting timing results.

## Stage 2 — Implement and validate the measurement harness

Depends on Stage 1. Extend existing examples/shared benchmark support rather than duplicate the host implementation.

- [x] Extend `carla_inprocess_benchmark.rs` and `carla_bridge_benchmark.rs` with workload loading, runtime selection, frame-size selection, measurement export, and validation. Clearly distinguish direct native execution from the existing bridge benchmark's `direct` label, which still uses `spawn_processor_bridge`.
- [x] Create the deterministic test plugin and fixed seeded/non-silent input. Ensure its DSP work affects output so compilers cannot eliminate it. Define expected output/tolerances and reset state consistently for every run.
- [x] Capture correlated request/worker/process timestamps and processing-thread CPU time. Separate end-to-end latency, worker wake-up delay, actual Carla processing, and remaining bridge overhead. Collect actual call counts/frame sizes per engine period and all fallback/deadline counters.
- [x] Inspect the exact Carla timing scope/formula for tested revisions. Add benchmark-only inner timing when needed to distinguish Carla's own scope from the outer host call; record instrumentation patches.
- [x] Support continuous throughput and periodically paced runs. Pace against absolute monotonic deadlines, record late starts, and specify behavior after an overrun; avoid cumulative `sleep(period - processing)` drift.
- [x] Establish the sampling protocol before comparison: 48 kHz, blocks 1024/256/128, at least ten alternating paired runs per comparison, warm-up, and at least 2,000 measured blocks per run. Use a calibration pilot to freeze workload strengths, warm-up duration, and any required larger sample count before collecting comparison data.
- [x] Define primary metrics as per-run mean processing-thread CPU time and p99 wall time; also report mean wall time, p50/p95/max, lateness and miss counts. Compare paired run ratios and confidence intervals across independent runs, with baseline-versus-baseline variation as the noise control. Do not treat individual blocks as independent experimental replicates.
- [x] Verification: test known output, deliberate missing-plugin/bypass/fallback detection, bounded instrumentation storage, clock attribution, and missed-deadline accounting. Confirm instrumentation overhead is negligible relative to the target difference, or publish separate diagnostic and low-overhead results.

## Stage 3 — Experiment 1: Carla build, version, and hosting interface

Depends on Stage 2; run before engine comparisons.

- [ ] Run the two-build/two-interface matrix with all core workloads, starting at 1024 frames and expanding to 256/128. Alternate order and include repeated identical-baseline runs.
- [ ] Compare configured buffer size against actual processing frames and reproduce Carla's meter behavior from diagnostic samples. Treat meter spikes separately from mean cost.
- [ ] If older Carla provenance is available, repeat the relevant fixed-host comparison across revisions, keeping plugin/compiler settings constant where possible.
- [ ] Verification: check output equivalence and build identities for every cell; summarize paired CPU/wall-time differences and uncertainty. A build effect requires a same-revision comparison; a version effect requires a fixed recipe comparison or an explicit confounding qualification.

## Stage 4 — Experiment 2: bridge and engine execution

Depends on Stage 3's validated harness/runtime selection; continue even if Stage 3 finds no difference.

- [ ] Hold Carla/plugin binaries fixed and compare direct native, in-process bridge, subprocess bridge, and full headless engine processing. Exercise all core workloads, including no-FX engine and empty-rack controls.
- [ ] Run continuous and paced cases at the three frame sizes. Verify that the full engine actually invokes the intended chain and accounts for every sub-block.
- [ ] Repeat selected paced comparisons with a bounded, reproducible CPU-contention workload. Record worker/callback policies and test affinity changes only where supported. Stop the load after each run.
- [ ] Verification: correlate end-to-end spikes with inner Carla wall/CPU time and worker wake-up delays. Distinguish increased execution cost from scheduling/blocking; unchanged inner timing with worse completion latency does not explain an increased embedded Carla meter.

## Stage 5 — Experiment 3: historical regression comparison

Depends on Stages 3–4 establishing reliable measurements.

- [ ] Build 0.2.0, pre-switch, post-switch, and pinned current versions in isolation. Add minimal headless adapters using each version's real processing path; do not rewrite historical processing algorithms.
- [ ] Reconstruct equivalent no-FX and loaded-FX sessions with matching active monitoring, routing, and loop work. Hold Carla/plugin binaries constant where compatible; separately report as-shipped comparisons with differing runtimes.
- [ ] Repeat the paired protocol and identify the earliest supported slow/fast boundary. If a historical adapter or dependency cannot be made equivalent, document the blocked comparison and complete unaffected cases.
- [ ] Bisect only a repeatable regression interval. Freeze the workload/metric and decision threshold before bisecting; skip incompatible commits and reconfirm both neighboring revisions with repeated measurements.
- [ ] Verification: distinguish historical evidence from current-only mechanism tests. Trace any confirmed boundary to a concrete change and rerun a minimal isolated comparison before attributing causality.

## Stage 6 — End-to-end validation and report

Depends on completion or explicit disposition of every experiment cell.

- [ ] Run the documented driver from a fresh results directory and confirm it builds/selects pinned artifacts, validates workloads, executes the matrix, and regenerates tables/report without manual intervention. Failed or unavailable cells must remain visible and must not count as successful measurements.
- [ ] Publish exact commands, manifests/checksums, raw samples, per-run summaries, comparison uncertainty, and timing plots. Explain VM limitations and identify which results do or do not bear on the reported laptop symptoms.
- [ ] Write conclusions for each hypothesis and recommend only the next investigation justified by evidence. A documented failure to reproduce is an acceptable scientific outcome; missing comparisons remain limitations rather than negative findings.
- [ ] Run targeted harness correctness tests, `cargo fmt --all -- --check`, warning-denying workspace build, tracing inventory checks if instrumentation changes, and the repository-required complete Rust suite. Run the test-policy checker if Rust tests change. Required Carla-dependent tests must not silently skip missing runtimes.
- [ ] Commit the report and meaningful implementation milestones; push the investigation branch and create a PR describing the measured question, findings, limitations, and validation. Keep public timing assertions tied to the recorded local environment; CI verifies correctness rather than absolute performance thresholds.
- [ ] Monitor CI for the exact PR head and resolve relevant failures until green. Check for automated review; repeatedly address applicable feedback and rerun affected validation until approval, or document an unavailable reviewer/blocker. Do not merge as part of this investigation.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Execution log

- Started on branch `investigate/carla-performance` in `/workspace/work/carla-investigation/repo`; original checkout has only the requested untracked plan.
- Verified the native-switch commits are adjacent. Carla 2.5.10 source archive checksum matches the runtime lock. Initial download required the execution tool network capability; the configured proxy works with that capability.
- Environment: Rust 1.99.1, GCC available, VM with a four-CPU quota and five visible CPUs. No preinstalled Carla/LV2 runtime.

- Resolved v0.2.0 to `d08aafcb049f6c7a848c97a18b5c3a1456046696`. Its portable package contains the backend library but no Carla runtime; the user's historical Carla version therefore cannot be recovered from that package.
- Prepared a deterministic LADSPA recurrence workload, a native/LV2 ABI harness, a benchmark-only timing shim, and a current-engine adapter. Initial synthetic output validation passed.
- Carla needs liblo for rack LV2 exposure. Rebuilding both same-source recipes cleanly with locally extracted liblo; headless optional-feature differences from CI will be reported.

- Sampling design clarification: primary comparisons retain ten repetitions of 2,000 continuous blocks. Paced scheduling diagnostics use ten repetitions of 200 blocks (2,000 total per cell), because 2,000 paced blocks per repetition across all modes would spend roughly 38 minutes on the paced subset alone. They remain secondary evidence with explicitly weaker tail estimates. Acceptance criteria are unchanged.
- Native and LV2 ABI comparisons use a small C++ harness to avoid introducing a second LV2 dependency into the current application. Engine modes use a dedicated Rust example sharing the existing production host/bridge/session code; legacy examples remain unchanged. This keeps the CI invariant that the current application has no LV2 dependency.
- The Rust build reused the prepared checkout's Cargo target cache to avoid duplicating its compiled dependencies; all edited source and historical target directories remain isolated.

- Stage 1 complete: both native and LV2 libraries are byte-identical between the release build-target recipes after PyQt registration is enabled (checksums in manifest). This controlled target comparison does not reproduce every optional dependency of Ubuntu CI or an unknown distribution build.
- Stages 2/5: all four engine paths have passed smoke output validation; both historical Rust adapters build. The as-shipped 0.2.0 backend also processes the exact synthetic workload through a timing-interposed LV2 bundle with verified output and exact call counts. Its dummy driver paces internally, so old inner-processing measurements are comparable as CPU cost, while total callback latency is not directly comparable to the new engine runner. Use the authentic release backend rather than rebuilding the old Qt/C++ packaging stack; record its checksum/compiler confound.
- Full required Rust suite: 1,809 passed, four skipped after enabling local sockets. The initial sandbox-only run failed on denied sockets; it is not a product failure. A required real-Carla installed-runtime test also passed with explicit runtime overrides.
- Frozen primary decision rule: compare ten independent paired per-run means; report bootstrap 95% intervals. Flag CPU changes only when the interval excludes 1, the effect exceeds both 5% and three times the observed median absolute A/A ratio deviation, and at least 8/10 pairs agree in direction. Tail-latency findings require a 20% effect and are scheduling diagnostics; maximum values alone never establish a regression.

- Stage 2 complete: source-preserving benchmark adapters and preparation/measurement drivers are implemented; positive/negative workload checks reject missing plugins, bypass and missing runtimes. Existing examples were extended by adding a dedicated investigation example rather than altering their default behavior. Request-to-inner-start includes copies/dispatch, so report it as an upper bound on worker wake-up latency. Exact wake-up attribution would require additional engine instrumentation and is not inferred from these samples.
- Warm-up validation now allows at most ten paced retries after the continuous warm-up. This avoids classifying one transient warm-up deadline miss as a missing plugin; measured misses and invalid output remain unfiltered.
- 0.2.0 initialization must wait for its asynchronous driver attachment before creating FX. A rejected pilot initialized Carla with the session's pre-attachment default period; the adapter now waits and asserts 48 kHz/the selected block size. The LV2 timing shim independently records the configured period and rejects mismatches.
