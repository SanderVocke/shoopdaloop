# Stage 10 fresh hosts, full benchmarks and packaged application

## Verified scope

At **`85e36348de9bff67e8b10cfa725216a3ca285730`**, eight host commands, two full
default benchmark campaigns and eleven application compiler/packaging/smoke
commands pass. **All509 scoped implementation/test/dependency/configuration/runner
inputs match frozen `8633d0f0` before/after** each phase and the final verification.
No production/API/dependency/workload change is made in this milestone. Every
Cargo/Python/Trunk/Node payload runs inside the repository Nix environment with
Rust/rustdoc warnings denied and no missing-backend allowance for real hosts.

This completes additional local gate execution, **not Stage10 or final acceptance**.
The current full interpreter attempt failed on its final sync-model900s watchdog;
see [verified failure/diagnosis](miri-sync-model-failure-stage8.md). Checked remote
artifacts, a pushed unmerged PR, final-head green required CI/review and independent
completion audit remain. Stage7 stays7/8, Stage8 stays6/8; Stage9 is verified.

## Native and real non-isolated host fixtures

Both profiles run the actual default-build commands, not `--no-build` substitutes:

```sh
python3 scripts/run_rt_runtime_hosts.py --native-only --profile PROFILE
python3 scripts/run_rt_runtime_hosts.py --profile PROFILE --verify-failures
python3 scripts/run_loop_engine_hosts.py --native-only --profile PROFILE
python3 scripts/run_loop_engine_hosts.py --profile PROFILE --verify-failures
```

| Checked scope | Per profile | Both profiles |
| --- | ---: | ---: |
| Generic native-only direct/two-thread golden replays |2|4|
| Loop native-only commands |17|34|
| Generic actual offline/live successes |2|4|
| Loop actual offline/live successes, eight scenario families |16|32|
| Generic actual specific failure controls |2|4|
| Loop actual specific Rust-origin failure controls |6|12|
| Loop Node+Chromium binding groups |20+20|80|

Native-only execution remains distinct from new browser execution. The generic
fixture's profile selects the native binary; its actual browser path deliberately
uses release Wasm in both runs. The loop fixture builds profile-matched Wasm.
Normal Cargo/wasm-pack caches are allowed: these are fresh build invocations and
executions, not claims of cold compilation or new bytes when source is unchanged.

The eight actual loop families are primitive, graph, immutable export, resource/
program staging, dense cascade/composite, transferred-return stall, prepared-media/
countdown/capture-exhaustion, and resolver-planner. Existing independent verifiers
check all downstream audio, MIDI bytes, semantic boundary journals, exact receipts,
immutable initial projections/content, generations, finite work/storage and full
logical owner/resource recovery. Direct/capacity-one native comparisons and fixed
ordinary independent worklet memories remain mandatory. No callback partition,
seed, dense replay, assertion or guard is removed.

Generic fresh actual offline results are2601callbacks/332928frames/34MIDI,
hashes480331717/3308430490/487351092; direct and two-thread replays agree exactly.
Both new live runs happen to produce13444callbacks/1720832frames and hashes
738985413/4022855181/487351092. These live totals are observed, not a deterministic
clock contract or an equality requirement with offline. Both memories remain
1703936bytes, `cross_origin_isolated:false`, independent instances true. Five held
transfers permit2048continued frames; delayed lateness1984 and duplicate/oversized
refusals are retained. All4096pages/16versions/16snapshots and transport owners
recover before successful stop.

Loop actual binaries match their recorded fresh report identities: debug
`8c697b29f9b4c6b0c422e434201f6e966b72600645310f0978eda97674000ef1`, release
`57ea62b0d11a19b19f6bdcb753050e4f59d0877f91b794a1284f3cc5db540426`.

The six loop failures/profile must exit1 with exact diagnostic/code/origin/phase,
positive fixed memory, callback1 and the following Rust counters. Fault abandonment
is not reported as successful cleanup:

| Failure | Code | Processor/planner origin | Plans/finishes | Alloc/realloc/dealloc |
| --- | ---: | --- | --- | --- |
| Processor |10|1/0|—|0/0/0|
| Rust heap |4294901761|2/0|—|4/20/4|
| Graph processor |10|1/0|—|0/0/0|
| Graph Rust heap |4294901761|2/0|—|1/5/1|
| Planner |10|0/3|1/0|0/0/0|
| Planner Rust heap |4294901761|0/4|128/128|1/5/1|

All old report directories were moved intact before each fresh gate; separate
copies preserve each result and raw failure evidence. The final proof rechecks
report hashes and exact witnesses. One proof helper initially confused eight
canonical loop payloads with seventeen executed commands; it refused. The corrected
proof requires all seventeen nonempty command logs/profile, separately from the
eight canonical payloads. The original failed proof log remains; no test changed.

## Full default release measurements

```sh
python3 scripts/benchmark_rt_runtime.py
python3 scripts/benchmark_loop_engine.py --report-dir FRESH_DIRECTORY
```

The generic campaign has7cases×3trials×20000=**420000 measured callbacks**,512
checked warmups/case/trial, six debug/release heap controls, and **zero observed
misses**. The loop campaign has14cases×3trials×4096=**172032 measured callbacks**,
128checked warmups/case/trial, six heap controls and **201 observed misses**.
The separate burst case executes **49152 measured same-frame Play cascades**;
the continuous one-frame-wrap workload is retained. All63 CSVs have exact ordered
callback counts. The proof recomputes percentiles/max/misses from every raw row;
loop full samples/packets/state/export checks, prepared work/credits, pending-zero
and final owner recovery gate success. All twelve native failure controls exit101
with exact witnesses1/0/0,0/7/0,0/0/1. Driver/source/binary hashes match after runs.

Loop elapsed callback timings, nanoseconds, across the three trials:

| Case | p50 range | Worst p99 | Maximum | Misses |
| --- | ---: | ---: | ---: | ---: |
| no-planner-idle |70–70|81|3817|0|
| loop-idle |231–240|261|2164|0|
| audio-1-257 |2564–2575|3457|15388|0|
| audio-4-257 |9258–10149|11692|21410|0|
| audio-16-257 |37079–42871|89929|1261762|0|
| audio-4-4 |67527–68128|138911|281239|0|
| midi-1-4 |139051–139422|303851|564922|0|
| midi-4-4 |538973–541358|1060424|3036159|1|
| cascade-4-1 |2158699–2163267|2499209|4768687|21|
| cascade-bursts |2154291–2164930|3278444|4859947|179|
| nested-regular |139522–140484|273515|1961228|0|
| seek-replace |3066–3086|4147|25829|0|
| fx-capture |3376–3437|8476|25738|0|
| upload-export |2966–3046|3867|11652|0|

Native Linux7.0.3/glibc2.42, Ryzen AI7 350/Radeon860M,16logical CPUs,
Rust1.97.1/Cargo1.97.1, default release profile. The generic driver selects CPU0,
reported `powersave`; loop is unpinned with no governor selection. The isolated
Stage8 interpreter was concurrent. Warm/uncontrolled results, differing affinity,
process-lifetime RSS and callback-boundary pool samples do not establish paired
speedup, legacy-engine comparison, cold-cache behavior, unsampled peaks, CI margin,
device/browser timing or hard-real-time guarantees. All201misses remain in raw
and summaries and do not determine correctness success. The historical generic
NoPlanner benchmark retains its own bounded-runtime workload; it is not a new
RenderIo throughput claim. Existing300s generic/600s loop watchdogs are unchanged.

## Existing application regression builds and packaged smokes

Four strict compiler gates cover `shoopdaloop --no-default-features` checks and
`shoop_audio_worklet` builds for `wasm32-unknown-unknown`, each debug/release.
Then seven commands run the README procedures: Trunk debug/release, hosted Chrome
in both profiles, release single-file generation/direct-file Chrome, and hosted
Firefox under Xvfb. All assert packaged loading, real128-frame callback progress,
output-only permission state and process teardown. Firefox reports Running,
48callbacks/6144frames/quantum128/zero command overflows/zero owned microphone tracks.

Tools: Trunk0.21.14, pinned Node22.22.2, Chromium/ChromeDriver147.0.7727.137,
Firefox150.0.1, Nix geckodriver0.36.0, Selenium/Python versions in the archive.
No timeout or permission assertion was relaxed. Previous dist was preserved;
debug/release outputs are moved into separate local evidence directories after
smokes. The archive contains complete build/smoke outputs and per-file asset
size/SHA256 manifests, not the large generated binaries. Observed artifacts:

| Asset | Debug bytes | Release bytes |
| --- | ---: | ---: |
| UI Wasm |81444124|15706639|
| Dedicated worklet Wasm |119154623|8973021|
| Single HTML |not generated|34011186|

Release HTML SHA256 is
`0617f1b3a4c361a23f3b403089d3f6c6de2e45d9b5842deee24846816c338cd6`.
These are existing-application compatibility smokes, not loop-library production
migration, physical microphone/Web MIDI/device coverage or new callback-safety
certification of the legacy application.

## Artifact and remaining barrier

`final-hosts-benchmarks-application-stage10-logs.tar.gz`:
**5886511bytes/333files**, SHA256
`58d51a85eba61181f912596c2392d331bd1ea21202ab05adc38c17d8946c6cec`.
Every member is byte-verified. Complete fresh reports/native journals/CSV rows,
exact failure logs, driver/verifier sources, source proof, tool metadata and prior
running-only Miri checkpoints are included. Those checkpoints remain historical;
the later verified timeout is not overwritten by them. Required current interpreter
recovery and final remote/delivery acceptance remain open.
