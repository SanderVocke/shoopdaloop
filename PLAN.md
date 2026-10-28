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

- [x] Add a dedicated pinned-source adapter sharing the host/bridge/session APIs used by `carla_inprocess_benchmark.rs` and `carla_bridge_benchmark.rs`, with workload loading, runtime selection, frame-size selection, measurement export, and validation. Clearly distinguish direct native execution from the existing bridge benchmark's `direct` label, which still uses `spawn_processor_bridge`.
- [x] Create the deterministic test plugin and fixed seeded/non-silent input. Ensure its DSP work affects output so compilers cannot eliminate it. Define expected output/tolerances and reset state consistently for every run.
- [x] Capture correlated request/worker/process timestamps and processing-thread CPU time. Separate end-to-end latency, worker wake-up delay, actual Carla processing, and remaining bridge overhead. Collect actual call counts/frame sizes per engine period and all fallback/deadline counters.
- [x] Inspect the exact Carla timing scope/formula for tested revisions. Add benchmark-only inner timing when needed to distinguish Carla's own scope from the outer host call; record instrumentation patches.
- [x] Support continuous throughput and periodically paced runs. Pace against absolute monotonic deadlines, record late starts, and specify behavior after an overrun; avoid cumulative `sleep(period - processing)` drift.
- [x] Establish the sampling protocol before comparison: 48 kHz, blocks 1024/256/128, at least ten alternating paired runs per comparison, warm-up, and at least 2,000 measured blocks per run. Use a calibration pilot to freeze workload strengths, warm-up duration, and any required larger sample count before collecting comparison data.
- [x] Define primary metrics as per-run mean processing-thread CPU time and p99 wall time; also report mean wall time, p50/p95/max, lateness and miss counts. Compare paired run ratios and confidence intervals across independent runs, with baseline-versus-baseline variation as the noise control. Do not treat individual blocks as independent experimental replicates.
- [x] Verification: test known output, deliberate missing-plugin/bypass/fallback detection, bounded instrumentation storage, clock attribution, and missed-deadline accounting. Confirm instrumentation overhead is negligible relative to the target difference, or publish separate diagnostic and low-overhead results.

## Stage 3 — Experiment 1: Carla build, version, and hosting interface

Depends on Stage 2; run before engine comparisons.

- [x] Run the two-build/two-interface matrix with all core workloads, starting at 1024 frames and expanding to 256/128. Alternate order and include repeated identical-baseline runs.
- [x] Compare configured buffer size against actual processing frames and reproduce Carla's meter behavior from diagnostic samples. Treat meter spikes separately from mean cost.
- [x] Dispose of conditional version comparison: historical Carla provenance is unavailable (0.2.0 package has no runtime); report this axis as inconclusive, without substituting an arbitrary older revision.
- [x] Verification: check output equivalence and build identities for every cell; summarize paired CPU/wall-time differences and uncertainty. A build effect requires a same-revision comparison; a version effect requires a fixed recipe comparison or an explicit confounding qualification.

## Stage 4 — Experiment 2: bridge and engine execution

Depends on Stage 3's validated harness/runtime selection; continue even if Stage 3 finds no difference.

- [x] Hold Carla/plugin binaries fixed and compare direct native, in-process bridge, subprocess bridge, and full headless engine processing. Exercise all core workloads, including no-FX engine and empty-rack controls.
- [x] Run continuous and paced cases at the three frame sizes. Verify that the full engine actually invokes the intended chain and accounts for every sub-block.
- [x] Repeat selected paced comparisons with a bounded, reproducible CPU-contention workload. Record worker/callback policies and test affinity changes only where supported. Stop the load after each run.
- [x] Verification: correlate end-to-end spikes with inner Carla wall/CPU time and worker wake-up delays. Distinguish increased execution cost from scheduling/blocking; unchanged inner timing with worse completion latency does not explain an increased embedded Carla meter.

## Stage 5 — Experiment 3: historical regression comparison

Depends on Stages 3–4 establishing reliable measurements.

- [x] Run the authentic 0.2.0 release backend and build pre-switch, post-switch, and pinned current versions in isolation. Add minimal headless adapters using each version's real processing path; do not rewrite historical processing algorithms.
- [x] Reconstruct equivalent no-FX and loaded-FX sessions with matching active monitoring, routing, and loop work. Hold Carla/plugin binaries constant where compatible; separately report as-shipped comparisons with differing runtimes.
- [x] Repeat the paired protocol and identify the earliest supported slow/fast boundary. If a historical adapter or dependency cannot be made equivalent, document the blocked comparison and complete unaffected cases.
- [x] Bisect only a repeatable regression interval. Freeze the workload/metric and decision threshold before bisecting; skip incompatible commits and reconfirm both neighboring revisions with repeated measurements.
- [x] Verification: distinguish historical evidence from current-only mechanism tests. Trace any confirmed boundary to a concrete change and rerun a minimal isolated comparison before attributing causality.

## Stage 6 — End-to-end validation and report

Depends on completion or explicit disposition of every experiment cell.

- [x] Run the documented driver from a fresh results directory and confirm it builds/selects pinned artifacts, validates workloads, executes the matrix, and regenerates tables/report without manual intervention. Failed or unavailable cells must remain visible and must not count as successful measurements.
- [x] Publish exact commands, manifests/checksums, raw samples, per-run summaries, comparison uncertainty, and timing plots. Explain VM limitations and identify which results do or do not bear on the reported laptop symptoms.
- [x] Write conclusions for each hypothesis and recommend only the next investigation justified by evidence. A documented failure to reproduce is an acceptable scientific outcome; missing comparisons remain limitations rather than negative findings.
- [x] Run targeted harness correctness tests, `cargo fmt --all -- --check`, warning-denying workspace build, tracing inventory checks if instrumentation changes, and the repository-required complete Rust suite. Run the test-policy checker if Rust tests change. Required Carla-dependent tests must not silently skip missing runtimes.
- [x] Commit the report and meaningful implementation milestones; push the investigation branch and create a PR describing the measured question, findings, limitations, and validation. Keep public timing assertions tied to the recorded local environment; CI verifies correctness rather than absolute performance thresholds.
- [x] Monitor CI for the exact PR head and resolve relevant failures until green. Check for automated review; repeatedly address applicable feedback and rerun affected validation until approval, or document an unavailable reviewer/blocker. Do not merge as part of this investigation.

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

- Primary evidence collection is running serially with compilation stopped. Historical comparison includes a 2,000-block, 128-frame paced pre-switch native-host control because the old dummy driver always paces; comparing only old paced samples against new continuous samples would confound CPU idle/cold-cache behavior.
- Result export uses `/workspace/work/carla-investigation/output`: the chat frontend's designated `/codex/.../output` path is not mounted writable in the managed execution environment (read-only `/codex` filesystem). The reproducible outputs and raw samples remain in the managed workspace.

- Core matrix complete: 870 runs, zero failed harness invocations; 358 invalid-output blocks accompanied by 361 bridge deadline misses. These remain in raw records and latency summaries; CPU comparisons exclude affected runs. At 1024-frame synthetic load, median per-run mean completion is ~771/802/839/829 microseconds for native/bridge/subprocess/full engine, while corresponding inner CPU time is ~770/770/786/766 microseconds. These are preliminary current-only measurements, not historical-regression conclusions.

- Instrumentation/A-A controls complete: 120 additional runs. Historical collection is now running with the same frozen binaries and waveform validation. The 0.2.0 adapter additionally exports aggregate native-backend profiling; those aggregates are not substituted for unavailable per-block full-callback CPU measurements.

- Analysis rule clarification: CPU comparisons exclude any run with deadline misses as well as invalid output; inner-CPU comparisons additionally require one matched real process call per measured block. A constant-input workload cannot independently identify repeated/stale output, so transport counters and exact call matching are part of acceptance rather than waveform validation alone. Latency/failure summaries retain these runs.

- Added a bounded x86 floating-point-state diagnostic after source inspection found explicit FTZ/DAZ setup in Carla standalone JACK/bridge engines but not its native engine. Identical binaries took ~2,263 microseconds per 128-frame subnormal-heavy block with FTZ/DAZ off versus ~44.5 microseconds on. This demonstrates a mechanism, not the user's cause; plugins may implement their own suppression.
- The optional Voyage Voyage 1.0.2 Linux VST3 download loads successfully, but processing segfaults through every tested minimal-runtime host. A captured stack ends in `juce::ClientRemappedBuffer<float>` inside the plugin via Carla's native VST3 path. This is a rejected pilot, not performance evidence. Build-time JUCE availability selects a different VST3 implementation, so a feature-complete runtime is being built before attempting actual-plugin timing.
- Core binaries and their manifest were frozen under `frozen-core/` before extending workload validation. The new optional-plugin runs use a separately identified runtime and shared binary input; no production processing implementation is changed.

- Actual processing-thread captures confirm FTZ/DAZ enabled in the as-shipped 0.2.0 dummy backend and disabled in both historical Rust revisions and the pinned current engine. The feature-complete Voyage runs succeed. A ten-pair silent-tail control with that plugin gives an off/on CPU ratio ~1.070 (95% bootstrap interval ~1.014–1.131); this is a measured contribution under the tested state, not proof of the full reported regression or its large spikes.
- Added a matched-cadence Voyage control: old dummy-driver pacing rounds to `1 / (48000 / 1024)` using integer division, approximately 21.739 ms. Current native/subprocess and pre-switch native controls use that period and 200 paced warm-up blocks, so cross-version CPU comparisons do not mix throughput and paced execution.
- The benchmark adapter for pinned current source is moving into `experiments/carla/adapters/current.rs` and an isolated checkout, like the historical adapters. This preserves the immutable baseline while avoiding any production Cargo/example changes as upstream advances.

- Final evidence collection and analysis complete: 2,173 records (2,125 successful invocations, 48 failures), including 30 matched-cadence Voyage controls. Historical failures are 46 small-buffer subprocess warm-up/transport failures and two Voyage warm-up failures. Successful records retain all 14,936 invalid-output blocks and 14,939 reported misses; CPU attribution excludes affected runs. No failed cell is recast as passing.
- Stage 3 disposition: same-source minimal target recipes are byte-identical; the unknown former Carla version prevents a version-effect comparison. Fuller optional aliases are A/A controls, not independent full-build compiler comparisons. Same-full-binary VST3 implementation selection succeeds by default and crashes when forced to the native path; compatibility is established, performance of the crashing path is unavailable.
- Stage 4 complete: four execution paths, three periods, no-FX/empty controls and pinned-CPU load controls collected. Loaded p99 completion increases for bridge/subprocess/engine without corresponding mean inner CPU increase. Request-to-inner-start only bounds wake-up delay; it does not isolate scheduler wake-up from copies/dispatch.
- Stage 5 disposition: authentic old backend and matching active-monitoring/routing sessions run. Old no-FX output passes but only aggregate profiling is available; full-callback per-block CPU is explicitly unavailable. No recorded-loop workload is claimed. As-shipped historical Carla comparisons are unavailable because its package omitted Carla. Adjacent native-switch confirmation shows caller CPU ratio 1.124 [1.089, 1.160], inner Carla CPU 1.019 [0.999, 1.042]. Endpoints are adjacent, so there are no intermediate commits to bisect. Exact caller/waiting code cause remains unresolved; no speculative fix made.
- Traced old FTZ/DAZ state to the shipped zita library's `set_fast_math` initializer with fresh-process loader controls and disassembly. It changes the loading thread; subsequent thread inheritance explains the measured dummy contexts, without proving physical callback state. Same-binary Voyage silent-tail off/on ratio 1.070 [1.014, 1.131]. Matched-cadence Voyage does not reproduce a large steady-state slowdown; unknown user preset/physical scheduling remain limitations.
- Stage 6 local validation complete: final preparation succeeds; fresh-results all-suite smoke replay emits 229 records and explicitly retains five historical subprocess failures. Full evidence is exported to compact committed summaries/manifests/report and workspace `output/raw-results.tar.gz` (checksum in results/raw-archive.json). Full Rust suite: 1,809 passed/four skipped, 59.032 s; explicit installed-Carla test: one passed. Formatting, warning-denying workspace/release-adapter builds, positive/negative workload checks and tracing inventory pass. No production source or Rust tests differ from the pinned baseline. Publication/CI/review remain pending.

- Published PR #892: https://github.com/SanderVocke/shoopdaloop/pull/892. Merged upstream master bcf928e (macOS packaging only) into the investigation branch; all experiments remain pinned to their original source baseline. CI/review monitoring is active.
- Automated CodeQL review flagged float multiplication before accumulation into double in the optional output validator. Promote the sample before squaring; this validation is outside timed processing. Rebuilt validator and revalidated actual Voyage through native/LV2. Added supplementary binary identities and explicit build-protocol snapshot. No performance dataset was recollected or replaced for this validation-only correction.

- Publication follow-up: CodeQL's validator finding is fixed and its review thread is resolved; updated C++/Python/JavaScript analyses and docs pass. Exact-head c70f1333 Rust coverage and Codecov checks pass; platform jobs remain in progress. A final audit checks all 71 successful historical captures have exactly warm-up + measured calls, tightens the future guard to equality, and confirms it with an old-backend smoke check. Archive now includes old no-FX aggregate profiling and the build-protocol snapshot. README clarifies that commands run from this investigation branch while adapters use pinned scratch checkouts.

- Final stage complete: c70f133386d95cd861e9bc9e537020d62084c6b3 has green CI across Linux/Windows/macOS debug/release, browser debug/release, Rust coverage, Codecov, documentation and all four CodeQL languages. The scheduled-only nightly job is correctly skipped on a PR. CodeQL's sole review thread is resolved and its aggregate check succeeds. No separate automated reviewer provides an APPROVED review; no unresolved automated feedback remains. PR #892 remains open and unmerged. All experiment stages are completed or explicitly disposed of above; unavailable comparisons are limitations, not successful measurements.


## Follow-up — historical Carla version and thread FP policy

User requested a date-based historical-version proxy comparison and a clearer recommendation for subnormal handling. Reopen the version axis without claiming to recover the reporter's installation.

- [x] Establish 0.2.0 publication date, contemporaneous official/Homebrew Carla versions, and Catalina compatibility; record primary sources and uncertainty about actual installation prevalence.
- [ ] Build a justified older stable Carla with the same compiler/dependencies/features as 2.5.10, validate Voyage through native/LV2, and compare repeated fixed-host/plugin/preset/input/period measurements. Keep FP state controlled and report failures explicitly.
- [x] Clarify FTZ/DAZ are thread-local; recommend placement, inheritance limits, scope/restoration and platform handling without implementing an unrequested production policy.
- [ ] Regenerate/export report and data, validate affected harnesses, commit/push to PR #892 and check exact-head CI/review again.

- Follow-up evidence: ShoopDaLoop v0.2.0 was published 2025-11-09; Carla 2.5.10 was published 2025-07-31. Historical Homebrew formula at commit 5fbbc037e215a838737db86eec39fcd27d7ba684 (2025-10-28) also selects 2.5.10. Official universal macOS build advertises 10.12+, including Catalina. Thus a release-date proxy points to the same 2.5.10 already tested, not an established older version. Compare the preceding stable 2.5.9 (released 2024-09-22) as an explicitly speculative older-installation sensitivity control. Its source archive checksum matches historical Homebrew metadata: 226fb5d646b7541b82035080190e7440df1f92372fb798b4ad49289570e5ad81. No prevalence claim is made.
- Follow-up protocol frozen before timing: identical fuller SDK/compiler/release plugin recipe, same Voyage 1.0.2 VST3/factory state/input, 48 kHz/1024, native and LV2 fixed hosts, explicit FTZ/DAZ off and on as separate strata, ten alternating-order repetitions of 200 warm-up + 2,000 measured continuous blocks. FP changes/reads occur outside timed calls; actual before/after bits, rate/frame counts, output and failures are validated. Existing CPU screening threshold ~5.12% and 20% tail screen are retained. These are Linux source-build comparisons, not macOS Catalina binary timings.

- Pre-follow-up final head 9e8e858b3b7ffe526511f3237d7c5b67df6d65d7 also completed all PR CI checks successfully. The follow-up extends the experiment rather than replacing the completed evidence set.
- Carla 2.5.9 full-feature source build succeeds without source patches. New explicit-FP ABI harness passes 32 positive workload/version/interface/policy checks and eight deliberately missing/bypassed-plugin rejections. The DSP-meter implementation file is byte-identical between tested 2.5.9 and 2.5.10 sources.
