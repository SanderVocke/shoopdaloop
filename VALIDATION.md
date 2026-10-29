# Verification and evidence

[Workstreams, acceptance and remaining checks](PLAN.md) · [Architecture](ARCHITECTURE.md) · [Historical records](docs/history/README.md)

## Evidence boundary

This is the single maintained verification/status index for the workstreams in
`PLAN.md`. It consolidates existing results; **no build, test, browser, Miri,
benchmark or remote-CI campaign was run by the documentation migration**. Recorded
passes prove their named source/configuration/inventory, not today's merged tree
by implication. Implementation completion, local execution, remote acceptance,
review approval and independent final audit remain distinct.

The migration starts from `fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a`, following
Milestone 1 source consolidation `30d09bbd`. Raw evidence and milestone reports
remain unchanged under `docs/{rt_runtime,loop_layer,integration}/evidence/`.
The 34 superseded narrative documents are preserved byte-for-byte in the
[offline snapshot](docs/history/pre-consolidation-fffc1faa.tar.gz); the
[history index](docs/history/README.md) provides readable archived views and their
complete immutable Git versions. Retired redirect pages are removed, not kept as
parallel ledgers. Old source hashes/counts/failures are not rewritten or relabelled.

Current debt belongs here; retained unfinished checkboxes belong only in
[PLAN.md](PLAN.md). The user retired the old executor and will continue elsewhere;
these gaps remain unsatisfied, not a reason to resume that native goal or claim
completion. Do not maintain a second editable Miri ledger or native task plan.

## Recorded verification by workstream

| Scope | Recorded result / source | What it does not establish |
| --- | --- | --- |
| Initial generic runtime | Stages 0–8 complete; source `fefe626b1ff866daaa16d55de9091668defc32e3`, CI run [40145050855](https://github.com/SanderVocke/shoopdaloop/actions/runs/40145050855) | Final loop/service branch acceptance or production application migration |
| Loop Stages 0–6 | Implemented and verified; Stage 6 collected 81 commands / 228 interpreter executions at frozen `faec78df0a990d28b4fefb2f75e43ea0ace7f1ea` | Later expanded tests/hosts, new source or remote final acceptance |
| Independent runtime/loop hosts | Native replay, public bindings, actual non-isolated independent-memory offline/live contexts and specific failure controls pass in recorded local campaigns | Remote OS builds, arbitrary MessagePort capacity, continuous live deadlines or production service deployment |
| Loop models, callback audit, benchmarks and tracing | Native/Wasm model, source-review/guard and measurement milestones verified | Current complete interpreter or checked remote final-head CI |
| Public examples / Stage 9 | 23 Cargo-discovered public examples in both profiles, 46 invocations plus strict rustdoc/doctests; 48 successful commands at `3151c2b5` | Doctests discovering zero are not executable example coverage; final branch delivery remains open |
| Final local native/repository-Wasm checkpoint | Source `24b458ad`, full suite/policies and all 19 then-opted-in packages pass | Not fresh results for the subsequently merged DSP/service packages |
| Final local hosts/measurements/application checkpoint | Source `85e36348`, host/failure controls, full default benchmarks and packaged Chrome/Firefox smokes pass | Does not erase the failed Miri campaign or certify later merged source/remote CI |
| Native Milestone 1 | Stages 0–6 verified at original pin; narrower merge/smoke/push delivered at `30d09bbd` | Original full integrated Stage 7/M1-A10 closure, fresh merged-head release/Node/browser/Miri/CI/review |
| Runtime book / existing loop documentation | Completed authoring/examples/editorial work recorded in archived documentation plans | Not a fresh engine audit, platform campaign or implementation acceptance |
| Native bus-FX fix | Completed regression/build/suite and PR #873 delivery recorded | No new live check of that PR during migration |
| Replacement loop book / protocol | Draft or proposed only | No implementation or acceptance claim |
| [Composable loop refactor](PLAN_REFACTOR.md) | Local stage-by-stage verification recorded [below](#composable-loop-refactor-evidence) | Remote CI and review (no PR was requested); acceptance of the open portable-loop checks |

<a id="composable-loop-refactor-evidence"></a>

## Composable loop refactor evidence

Executed under [PLAN_REFACTOR.md](PLAN_REFACTOR.md), on Linux (AMD Ryzen 9 5950X)
in `nix develop`, with `RUSTFLAGS="-D warnings"`. The baseline is source
`2d13b8da`. Local runs relax only two tool pins: the Nix shell provides Node 22.23.2
(pinned 22.22.2), and Chromium/chromedriver 151 come from nixpkgs (pinned 147).
Miri ran outside the Nix shell with the pinned `nightly-2026-05-04`; inside it,
host proc-macro builds of `ctor`'s `linktime-proc-macro` fail.

| Stage | Source | Verified |
| --- | --- | --- |
| 1 — rename | `2031ee25`, `ec2d05a8` | Workspace build, fmt; debug/release `rt_runtime` 193, `shoop_loops` 155, DSP 15, service 20; host crate 60; native, Node (189/155/15/18) and browser hosts with failure canaries; boundary, tracing, test-usage and script unit tests — all identical to the baseline |
| 2/3 — runtime | `b96f5165`, `b7ec7886`, `fad7f7a0` | `rt_runtime` 197 (four new: generic cancellation, three admission paths) in debug/release and 193 on Node; all 18 runtime examples; native and browser hosts; Miri (seed 0) on `typed_schedule`, `typed_execution`, `planner_admission`, `planner_execution`, `graph_activation`, `initial_graph`, `contracts`, `lifecycle` and the delivery model |
| 4/5 — loop parts, hosts | `4929d185` | `shoop_loops` 159 (four new graph guards) in debug/release and on Node; all loop examples in both profiles and the draft-book example; host crate 60; native and browser loop hosts with failure canaries |
| 6 — demonstrator | `87fc3c85` | DSP 15 and service 20 in debug/release, 15/18 on Node; `one_track` 42 frames / 487 packets / 378 bytes and `latency_probe` 18 cases in both profiles; Wasm build; dependency and boundary checks |
| 8 — end to end | `fcbd5ea6` | CI Linux command set: fmt (workspace and both hosts); workspace build; `rt_runtime` 197, `shoop_loops` 159, DSP 15, service 20 in debug/release; host crate 60 per profile; every runtime, loop and service example in both profiles plus the draft-book example; boundary, tracing (217 modules, closed), test-usage, script and collector unit tests; workspace nextest with `shoop_engine/app_backend` and `SHOOP_ALLOW_MISSING_BACKENDS=1`: 2191 passed, 4 skipped; `shoopdaloop` Wasm check, worklet build and browser dependency isolation; Wasm test policy; runtime benchmark; native runtime/loop hosts in both profiles; browser runtime host and loop hosts (debug and release) with failure canaries; Node and Chrome Wasm suites for all 21 packages; the complete CI Miri inventory, 16 batches with 152 commands and 308 interpreted test executions, confirmed by `run_rt_runtime_miri.py --verify-batches` |

The `rt_runtime` docs checker (`check_rt_runtime_docs.py`) reports the same stale
pending-chapter and example-inventory lists on the unchanged baseline as after the
refactor; it is not run in CI.

Two Stage 8 runs needed a local deviation or a repeat, recorded here:

- **Miri watchdog.** The runner limits each command to 900 s, including its build.
  On this machine the seeded capture and timeline models take 1092–1222 s under
  Miri, equally on the baseline and the refactor (capture 1096 → 1092 s, timeline
  1222 → 1215 s, run side by side without the watchdog). The final inventory ran
  outside the Nix shell with warmed builds and the per-command limit relaxed to
  3,600 s; nothing else changed.
- **Browser tab crash.** One loop-host browser run crashed its tab while the full
  Wasm suites and four Miri workers held the load near 200; the debug and release
  loop-host browser runs, with their failure canaries, passed on rerun.

Not run locally: the real Carla plugin suites, application packaging and the hosted
Chrome/Firefox application smokes, Perfetto trace validation, macOS and Windows jobs,
and remote CI (no pull request was requested).

Five links in unchanged milestone reports under `docs/loop_layer/evidence/` point at
pre-rename paths (`shoop_loop_engine`, `tests/loop_engine_host`). Evidence stays
byte-for-byte as recorded; the renamed files are `src/rust/shoop_loops/` and
`tests/loops_host/`.

### Measurements

Release builds on the same machine, pinned to CPU 8 (`ondemand` governor). Callback
median / p99 in ns from `measure_service` (`collect.py --cpu 8` defaults: three runs
of 2,048 samples after 128 warm-up callbacks), with the baseline (`2d13b8da`) and the
refactor (`87fc3c85`) measured alternately in one session:

| Workload | Baseline 1 | Refactor 1 | Baseline 2 | Refactor 2 |
| --- | --- | --- | --- | --- |
| current/idle | 2014 / 2074 | 1994 / 3486 | 2014 / 2084 | 1954 / 3637 |
| current/steady | 2174 / 3516 | 2104 / 4138 | 2175 / 3526 | 2124 / 3486 |
| service/idle | 3717 / 3887 | 3637 / 3827 | 3708 / 3807 | 3667 / 3777 |
| service/steady | 3256 / 7575 | 3336 / 7835 | 3246 / 7624 | 3326 / 7834 |
| service/poll | 3948 / 8536 | 3987 / 8716 | 3967 / 8556 | 3947 / 8616 |
| service/export | 5280 / 6171 | 5280 / 5761 | 5300 / 8576 | 5300 / 5831 |
| service/preset | 3296 / 7815 | 3847 / 8466 | 3417 / 7824 | 3867 / 8496 |

Idle, poll and export are unchanged and steady is about 2.5% slower. The preset
workload, which selects a preset on every 64-frame callback, is 13–17% slower: the
selection now passes through the runtime's command admission, scheduler and execution
records, and the service's ingress and execution forwarding, instead of the DSP's
private atomic mailbox. An earlier pair of separate sessions showed steady at +13%,
which this alternating run attributes to session variation. The current-engine
reference is unchanged throughout.

The loop benchmark (`benchmark_loops.py --iterations 1024 --trials 3 --cpu 8`, median of
per-trial p50 ns) is within run-to-run variation; for example loop-idle 360 → 341,
fx-capture 4779 → 4809, nested-regular 225296 → 225246, audio-4-257 13095 → 14377,
midi-4-4 859236 → 815742.

### Loop invariants and their tests

Each guarantee kept from the earlier critique has tests that fail if its guard is
removed. The generic-path guards are new; the rest run unchanged through the
dev-only test assembly.

| Invariant | Tests (`shoop_loops`) |
| --- | --- |
| Required consumers, bindings and input tracking | `graph_guards.rs` (initial graph, generic commit without a node, crossed bindings, processor install/remove; all four fail with the hooks disabled); `invalid_candidate_shapes_epochs_and_raw_program_tags_return_owners_without_mutation` |
| Paired program/routing publication | `running_program_and_routes_publish_at_one_zero_cut_with_full_independent_reference`; `paired_cancellation_released_generations_and_intervening_graphs_never_publish_the_program` |
| Node-pair reassignment | `candidate_capture_preflight_failure_preserves_old_configuration_and_retries_without_revision_gaps`; `input_projection_is_explicitly_preserved_or_reset_once_before_the_first_new_capture` |
| Capture preflight before affected samples | `capture_preflight_and_optional_export_refusals_do_not_publish_partial_takes`; `record_to_play_and_monitored_capture_failure_use_exact_common_boundaries` |
| Receipts, cancellation and reclamation | `recovery_and_cancellation_do_not_require_normal_domain_or_runtime_result_credits`; `permanent_quiescence_recovers_pending_speculative_and_committed_owners_without_advancing_time`; `stopped_extraction_recovers_active_and_partially_staged_faulted_capture_owners` |

## Loop evidence

### Current local checkpoints

| Evidence | Recorded results and source limits |
| --- | --- |
| [Native and complete repository-Wasm](docs/loop_layer/evidence/final-native-wasm-stage10.md) | At `24b458ad`: 21 native/policy commands; runtime 193 + loop 155 + fixture 60 per profile = 816 passes; workspace **2148 passed / four intentional skips**; 91 helpers. Complete then-discovered 19-package suites: **Node 1837 / Chromium 1838 passed**, exactly two shared opt-in ignored canaries/runtime. All 509 scoped execution inputs matched frozen `8633d0f0` before/after |
| [Hosts, benchmarks and application](docs/loop_layer/evidence/final-hosts-benchmarks-application-stage10.md) | At `85e36348`: eight host commands, **38 native-only subcommands**, **36 actual successful offline/live contexts**, **16 specific actual failure controls**, **80 binding groups** across profiles. Four application compiler and seven packaging/smoke commands pass, including hosted/direct-file Chrome and hosted Firefox. All 509 scoped execution inputs remain unchanged |
| [Examples and acceptance identity audit](docs/loop_layer/evidence/examples-stage9-logs.tar.gz) | 48 successful commands; both doctest harnesses discover zero tests. Benchmark example invocations are functional smokes, not full measurement campaigns. The archived validation narrative retains the 34 exact-case identity map, intentional legacy differences and evidence limits |
| [Broader models](docs/loop_layer/evidence/adversarial-models-stage8.md) | Program and sync models each retain 288 scalar/reference runs, typed delivery four seeds × 512 actions / 18 semantic witnesses, capture 48 takes / 1536 stereo frames. Native/Wasm pass; full current interpreter coverage remains separate |
| [Descriptor model](docs/loop_layer/evidence/descriptor-model-stage8.md) | Complete descriptor banks, checked generations/epochs, refusals and exact owners across four seeds × 512 actions; no new production behavior |
| [Callback audit](docs/loop_layer/evidence/callback-audit-stage8.md) | Source-scoped transaction/render-resource/planner-protocol review and active heap controls. Not certification of arbitrary downstream traits, JS/OS/device deadlines or new remote builds |
| [Collected Stage 7 recovery](docs/loop_layer/evidence/miri-consolidated-stage7.md) | Full frozen 146-command / 299-execution pass at `3bf1dbca`; cannot cover the five later cases merely by source similarity |

Generic-fixture Cargo test harnesses and library doctest harnesses discovering zero
prove compilation/harness operation only. Real fixture drivers and executable
examples provide their distinct functional evidence. Actual-worklet, package-Wasm,
public binding, native fixture and production application scopes are not substitutes.

The package-Wasm checkpoint reused the production worklet asset with actual source
`dfd610d90c3484dc7ec7ffaa86d1cf80f861c6d6`, input hash
`f84144c75f79007b0a3f8e0819436e16c0795ad22f5622e7adb148aab17cecd8`,
and Wasm SHA256 `f14cf70c8a17e49140fb2b2cfa4588107b116efcb9f18d4c54106234ec475bc6`.
It is not relabelled as a fresh binary from the checkpoint or merged head.

The prescribed final nextest run used verified **0.9.116**. Earlier PATH metadata
with a nonexistent cache `bin/` child actually selected **0.9.143**; retain those
raw results under the real version. Correct cache PATH is
`$HOME/.cache/rt-runtime-nextest-0.9.116`. Separate Cargo-test and Miri results retain
their own actual toolchains. Four workspace skips are explicit intentional failure
controls, not skipped portable-library coverage.

### Performance observations

The full default campaigns at `85e36348` retain all **63 raw CSVs** and **12 exact
native allocation/reallocation/deallocation controls**:

| Campaign | Measured callbacks | Observed deadline misses |
| --- | ---: | ---: |
| Generic: 3 × 7 × 20000 | 420000 | 0 |
| Loop: 3 × 14 × 4096 | 172032 | **201** |

Loop misses are 1 in `midi-4-4`, 21 in continuous `cascade-4-1`, and 179 in
`cascade-bursts`. All **49152 measured same-frame Play cascade starts** remain.
Correctness, raw recomputation, guards, finite work/storage and owner recovery pass;
timing is not an xrun-free acceptance gate. Normal caches, different affinity and
concurrent Miri prohibit paired speedup, cold-cache, device/browser or hard-deadline
claims. Earlier campaigns with 6144 and 43 misses remain in their original reports,
not replaced by these measurements.

## Miri evidence

**Current complete Stage 8/final interpreter acceptance is unsatisfied.** The
registered expanded inventory is **151 commands / 304 executions**: 138 runtime,
102 loop and 64 standalone host. Preserve full filters, seeds, order, workload,
assertions, guards and the **900s per-command watchdog**. Procedures are in the
[runner operational reference](docs/loop_layer/MIRI_BACKGROUND.md); cadence and
final-input rules are in [PLAN.md](PLAN.md#miri-execution-contract).

| Job | Frozen source / outcome | Interpretation |
| --- | --- | --- |
| `20261027T025325-e065d48941` | `faec78df`, **passed/collected 81 commands / 228 executions**, 7318.292420581012s | Stage 6 scope only |
| `20261028T145739-89dfd53bdb` | `3d10c7a0`, exact dense-host case **passed/collected**, 770.7792120630038s supervisor wall | All seven internal replays retained; not a whole-suite or deadline-margin claim |
| `20261028T153110-784d2380c0` | `3bf1dbca`, **passed/collected 146 commands / 299 executions**, 10478.51554903097s | Complete frozen Stage 7 inventory; not expanded 151/304 |
| `20261028T190146-abeee2e59e` | `8633d0f0754a2f4445c3d0febb1f1ae2d7e9572e`, **failed**, 13519.300086269039s | First 150 commands / **303 partial executions passed**; final sync model exceeded 900s. No final pass/report/collection; supported collection refuses |
| `20261028T225455-65cf2c5a28` | Same `8633d0f0`, exact final command/seed0 with declared measureme, **failed/zero pass**, 407.5124054910266s | Identity-checked SIGINT finalized a censored diagnostic; not acceptance or a completed replay |

Failing exact test:

```text
shoop_loops / adversarial_timeline
seeded_sync_components_deliver_each_occurrence_once_with_saturated_results_and_optional_journals
-- --exact
```

The [verified failure/diagnostic report](docs/loop_layer/evidence/miri-sync-model-failure-stage8.md)
checks all 890 immutable source entries, literal archive, copied supervisor, full
selection/environment, ordered raw summaries, process death and collection refusal.
Original reports are unchanged. The 1555724199-byte finalized profile is retained
locally with SHA256
`ece35f348124a4c477a93ddbe12c0ce529be1202b2aa166910df74c97914dff1`.
Pinned summarize analyzed an equal-hash copy outside immutable reports. Completed
function events include overlapping/censored `Boundary::settle` 285.561397094s /
1694 calls and `PreparedTopology::node` 193.840898050s / 155061 calls. These support
investigating a checked lookup fast path, not an implemented fix, additive complete
cost, native speedup or recovered interpretation. **303 is not 304 acceptance.**

Earlier failures also remain failed:

| Job | Recorded failure |
| --- | --- |
| `20261028T083225-374aa055bf` | Original consolidated dense-host timeout after nine completed executions |
| `20261028T094652-f2718d3398` | Focused dense-host retry exceeded unchanged 900s |
| `20261028T102117-ca19c81152` | Diagnostic SIGINT; censored, zero completed executions |
| `20261028T113106-86abdbc4e8` | Measured-cost recovery again exceeded unchanged 900s |
| `20261028T131907-e502c4cd23` | Residual diagnostic SIGINT; censored, zero completed executions |

Their complete logs/provenance, failed setup attempts and historical timing tables
remain in unchanged milestone reports and the archived old Miri/validation/timing
narratives. Native dispatch, helper tests, launch checkpoints and benchmarks are
not terminal interpreter evidence. Do not duplicate or mutate old job directories;
a concrete relevant implementation fix can justify scoped recovery under the plan.

## Integration evidence

Milestone 1's original pin is `d038c208f46c98ac8d7561ddcf09ff7c8e7a9d94`.
Stages 0–6 include independent DSP, service/native Node tests, full public media
scenario, D=3 physical capture probe, full-credit/disconnect/fault/two-thread
ownership tests and source-scoped measurements. The original draft PR #889 and
initial-pin results are historical, not a new review/CI result for consolidation.

The later requested merge/smoke/push was delivered at
`30d09bbdf4bbfd8f63b4eb58cc3e111b8e7a7a8c`, combining loop parent `c33c9ce5`
and milestone tip `7a499de2`. Focused Nix checks with warnings denied passed:

- Locked workspace build, formatting and **35 native debug tests**: 15 DSP / 20 service.
- All six public `one_track` partition/declaration runs: complete **42-frame / 487-packet / 378-byte** oracles.
- `latency_probe`: all **18 physical cases**.
- Both new libraries compile for Wasm; test-attribute, dependency isolation and closed **217-module** tracing inventory pass.

Log: `.cache/milestone-1-consolidation/smoke.log`. Compilation is not fresh
Node/browser execution. Full workspace tests, release/measurement collection,
Node/browser/worklet/Miri and final-head CI/review were **not rerun**, as requested.
The old dependency wait is historical; original full final acceptance is unclaimed.
Detailed contracts/probes and complete earlier stage history are in the archived
integration notes; usage remains in the [service README](src/rust/shoop_engine_service/README.md).

Historical release source `c2734153` retains 21 processes / 43008 callback rows and
complete raw-bundle validation under
[service-release-c2734153](docs/integration/evidence/service-release-c2734153/summary.json).
For the declared common workload, service median idle is **4.048 vs 2.185 µs
(+85.3%)**, steady **4.278 vs 2.385 µs (+79.4%)**. Keep work/MIDI differences,
tails, run variation, admission/refusals, copy and memory observations. No parity,
superiority or deadline claim follows. Raw capture exposes missing dry history,
wet tail and independent playback windows; latency arithmetic is not implementation.

## Initial runtime, documentation and bugfix history

- Initial runtime CI run [40145050855](https://github.com/SanderVocke/shoopdaloop/actions/runs/40145050855), source `fefe626b`, passed all ten workflow jobs; representative native/worklet artifacts were inspected. [ci-stage8.json](docs/rt_runtime/evidence/ci-stage8.json) preserves details. No automated code reviewer was available; green CI/coverage was not labelled reviewer approval. Later heads need their own checks.
- The runtime documentation completion records nineteen chapters, six reference pages and existing loop guides, with new teaching examples run in debug/release. Approved Part I/chapter 7 and completed authoring remain accepted. Old archive-immutability/tooling/Git-delivery instructions were superseded by explicit user directions; they are historical, not current gates to recreate.
- Bus-FX delivery records actual non-silent native routing/activation regression checks, 1800 native-featured passes / four environment skips and PR #873. It also retains a non-required web-release Firefox smoke race, rather than claiming universal green. The migration makes no fresh remote assertion.
- The plan/tooling review is architectural analysis only. No replacement execution extension was implemented, configured or granted focus by that review.

## Remaining verification boundary

[PLAN.md](PLAN.md#remaining-stage-checks) is the sole checklist. Current loop full
Miri recovery, relevant final merged-tree reruns, checked Linux/Windows/macOS and
web/Miri artifacts, unmerged PR, final-head required green CI, available review
through approval (or explicit absence) and independent final audit remain open.
Old parent/PR run states must be rechecked before delivery. No migration commit or
historical green run can certify a later head merely because only docs changed.

Evidence must retain command, source, relevant input equivalence, flags/toolchain,
actual listed/executed/passed/skipped counts and reasons, complete content/owner
recovery, specific failure witnesses and artifact identity. Never publish credentials
or signed artifact URLs. A run ending successfully is not by itself sufficient.
