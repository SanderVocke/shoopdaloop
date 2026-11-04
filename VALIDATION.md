# Verification and evidence

[Workstreams, acceptance and remaining checks](PLAN.md) · [Architecture](ARCHITECTURE.md) · [Historical records](docs/history/README.md)

## Native session recovery checkpoint

The current session-service milestone is tracked by
[PLAN_NATIVE_SESSION_RECOVERY.md](PLAN_NATIVE_SESSION_RECOVERY.md), its
[A1–A11 acceptance index](docs/native_session_recovery/acceptance.md), and the
[feature parity handoff](docs/native_session_recovery/feature-parity.md).
The complete code/test candidate `929ed71b4800e208620262f1bafd12acd970735d` is pushed in
[PR #899](https://github.com/SanderVocke/shoopdaloop/pull/899).

Local gates pass: warning-denying workspace build, 2404 native tests, 2026 Node
tests across 31 packages, 142 Python tests, browser core/burst/media/MIDI/settings
workflows, packaged Linux companion workflows, policies and 35 selected Miri
executions run after ordinary integration gates. The acceptance index records
source provenance, workloads, timing/resource measurements and hardware limits.
The 16 parity gaps remain OPEN with retained implementation/UI/tests and concrete
completion criteria. The full CI matrix is green at
[41306455452](https://github.com/SanderVocke/shoopdaloop/actions/runs/41306455452):
all eight native/web debug/release jobs, coverage and bare-metal checks pass. Its
Node run confirms 2026 tests across 31 packages. All CodeQL analyses, Codecov and
documentation checks also pass. Windows ACL module lookup, the packaged web path
and native driver fixture timing/counter assumptions are corrected.

The completion follow-up changes documentation/evidence and the native recovery
stress fixture's recording budget. A Windows run outlasted its reduced 16 MiB
capacity; the fixture now uses the normal 256 MiB default and retains all 32 cycles
and continuity/resource assertions. The [controlled capacity probe](docs/native_session_recovery/evidence/recording-capacity.md)
isolates the exhaustion without client loss. Production sources, manifests, assets
and workflows are unchanged. [Current-head checks](https://github.com/SanderVocke/shoopdaloop/pull/899/checks)
are the delivery gate for that head. No independent code reviewer or verdict was
available; code-review approval is not claimed. The acceptance index retains the
exact source pins, selected Miri reports and physical-hardware limitations.

## Evidence boundary

The historical workstream record below is the maintained verification/status index
for `PLAN.md`. Its documentation migration consolidated existing results; **no build, test, browser, Miri,
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
| [Composable loop boundary](PLAN.md#composable-loop-boundary) | Delivered; local stage-by-stage verification, a CI replay and the acceptance check recorded [below](#composable-loop-refactor-evidence) | Remote CI and review (no PR yet); acceptance of the open portable-loop checks |
| [New engine backend](PLAN_RT_BACKEND.md) | Stage 10 local validation at `0d3925bb` and the merge `8cadc053`: the CI Linux and web command set, the shared contracts, the core workflow natively (dummy, CPAL) and in Firefox and Chrome for Testing, and the Miri inventory; B1–B12 checked [below](#new-engine-backend-evidence) | macOS and Windows runs, real MIDI hardware, a manual Chrome check, remote CI and review |

<a id="composable-loop-refactor-evidence"></a>

## Composable loop refactor evidence

Executed for the [composable loop boundary](PLAN.md#composable-loop-boundary) work, on Linux (AMD Ryzen 9 5950X,
Ubuntu 20.04 host) in `nix develop`, with `RUSTFLAGS="-D warnings"`. The baseline is
source `2d13b8da`. Stages 1–6 and the first Stage 8 run used the Nix shell's Node
22.23.2 and nixpkgs Chromium/chromedriver 151 instead of the pinned Node 22.22.2 and
Chrome 147; the [CI replay](#ci-replay-at-61beffad) repeats Stage 8 with CI's own
pins and limits. Miri runs outside the Nix shell with the pinned
`nightly-2026-05-04`; inside it, host proc-macro builds of `ctor`'s
`linktime-proc-macro` fail.

| Stage | Source | Verified |
| --- | --- | --- |
| 1 — rename | `2031ee25`, `ec2d05a8` | Workspace build, fmt; debug/release `rt_runtime` 193, `shoop_loops` 155, DSP 15, service 20; host crate 60; native, Node (189/155/15/18) and browser hosts with failure canaries; boundary, tracing, test-usage and script unit tests — all identical to the baseline |
| 2/3 — runtime | `b96f5165`, `b7ec7886`, `fad7f7a0` | `rt_runtime` 197 (four new: generic cancellation, three admission paths) in debug/release and 193 on Node; all 18 runtime examples; native and browser hosts; Miri (seed 0) on `typed_schedule`, `typed_execution`, `planner_admission`, `planner_execution`, `graph_activation`, `initial_graph`, `contracts`, `lifecycle` and the delivery model |
| 4/5 — loop parts, hosts | `4929d185` | `shoop_loops` 159 (four new graph guards) in debug/release and on Node; all loop examples in both profiles and the draft-book example; host crate 60; native and browser loop hosts with failure canaries |
| 6 — demonstrator | `87fc3c85` | DSP 15 and service 20 in debug/release, 15/18 on Node; `one_track` 42 frames / 487 packets / 378 bytes and `latency_probe` 18 cases in both profiles; Wasm build; dependency and boundary checks |
| 8 — end to end, first run | `fcbd5ea6` | CI Linux command set: fmt (workspace and both hosts); workspace build; `rt_runtime` 197, `shoop_loops` 159, DSP 15, service 20 in debug/release; host crate 60 per profile; every runtime, loop and service example in both profiles plus the draft-book example; boundary, tracing (217 modules, closed), test-usage, script and collector unit tests; workspace nextest with `shoop_engine/app_backend` and `SHOOP_ALLOW_MISSING_BACKENDS=1`: 2191 passed, 4 skipped; `shoopdaloop` Wasm check, worklet build and browser dependency isolation; Wasm test policy; runtime benchmark; native runtime/loop hosts in both profiles; browser runtime host and loop hosts (debug and release) with failure canaries; Node and Chrome Wasm suites for all 21 packages; the complete CI Miri inventory, 16 batches with 152 commands and 308 interpreted test executions, confirmed by `run_rt_runtime_miri.py --verify-batches` |

The `rt_runtime` docs checker (`check_rt_runtime_docs.py`) failed on the baseline
because its manifest still marked finished chapters as pending and omitted twelve
examples. With the manifest completed, `check_rt_runtime_docs.py --run-examples`
passes and runs all 18 examples. It is not run in CI.

The first Stage 8 run relaxed the Miri runner's 900 s per-command watchdog to
3,600 s, and one loop-host browser run crashed its tab under a load near 200 and
passed on rerun. The CI replay below supersedes it.

Five links in unchanged milestone reports under `docs/loop_layer/evidence/` point at
pre-rename paths (`shoop_loop_engine`, `tests/loop_engine_host`). Evidence stays
byte-for-byte as recorded; the renamed files are `src/rust/shoop_loops/` and
`tests/loops_host/`.

### Baseline

Source `2d13b8da`, before the refactor, under the old names. All runs passed.

| Suite | Debug | Release |
| --- | --- | --- |
| `rt_runtime` | 193 tests / 36 binaries | 193 / 36 |
| `shoop_loop_engine` | 155 / 20 | 155 / 20 |
| `shoop_example_dsp` | 15 / 3 | 15 / 3 |
| `shoop_engine_service` | 20 / 6 | 20 / 6 |
| `tests/loop_engine_host` crate | 60 / 3 | — |

| Host / Wasm run | Result |
| --- | --- |
| `run_loop_engine_hosts.py --native-only --profile debug` | passed |
| `run_rt_runtime_hosts.py --native-only --profile debug` | passed |
| `run_loop_engine_hosts.py --profile debug --verify-failures` | passed (offline and live contexts, failure canaries) |
| `run_rt_runtime_hosts.py --verify-failures` | passed (failure canaries) |
| `run_wasm_tests.py --profile dev --runtime node` | `rt_runtime` 189, `shoop_loop_engine` 155, `shoop_example_dsp` 15, `shoop_engine_service` 18 |

The measurement baseline is in [Measurements](#measurements).

### CI replay at `61beffad`

Every Linux and web step of `build_and_test.yml`, in CI's order, from clean
checkouts, plus the Rust coverage and Miri jobs. This includes the scheduled-only
optimized Node and Chrome Wasm suites and the dispatch-only Perfetto failure canaries
(native, Node and Chrome), which all pass; the optimized suites cover all 21
packages.
Tools are CI's pins: Node 22.22.2 (official build), Chrome for Testing and
ChromeDriver 147.0.7727.117, nextest 0.9.116, wasm-pack 0.15.0, trunk 0.21.14,
cargo-llvm-cov 0.8.7, and Carla 2.5.10 built from the pinned source with cx-Freeze
7.2.7 and PyQt5 5.15.11; the Firefox step uses the then-latest Firefox 159.0,
geckodriver 0.37.1 and Selenium. Rust is the dev shell's 1.97.1: CI installs the
current stable (1.99.1), but rustup's compiler cannot load proc macros linked against
the Nix glibc on this host.

| Job | Result |
| --- | --- |
| Linux debug | All steps pass: application build; fake-Carla rejection; Carla probe; packaging, archive verification and extracted-archive probe (bundled Carla runtime and UI from a `Shoop Δ` path); complete workspace nextest with `SHOOP_REQUIRE_CARLA_TESTS=1`, 2191 passed and the 4 intentional skips; runtime, loop library and native-host block (tests, host crate 60, examples, host runners, boundary checks); feature and dependency isolation; Carla external UI smoke; formatting, tracing (closed) and test attributes; Perfetto trace validation |
| Linux release | All steps pass: application build, packaging and archive probes; application-path nextest; runtime, loop library and native-host block; guarded runtime and loop benchmarks; Carla benchmark smokes; Carla external UI smoke |
| Web debug | All steps pass: trunk build, packaging and artifact verification; Wasm package check and browser dependency isolation; Wasm test policy; Node suite for all 21 packages (`rt_runtime` 193, `shoop_loops` 159, DSP 15, service 18, `shoop_engine` 870); Chrome suite for all 21 packages; runtime and loop AudioWorklet hosts with failure canaries; hosted Chrome application smoke; worker and AudioWorklet Perfetto smokes with their trace queries |
| Web release | All steps pass: optimized trunk build, packaging with the single-file HTML and artifact verification; Wasm package check and browser dependency isolation; loop AudioWorklet host with failure canaries; self-contained Chrome smoke; Firefox Web Audio workflow |
| Rust coverage | Instrumented workspace nextest with `shoop_engine/app_backend`: 2191 passed and the 4 intentional skips; lcov report written |
| Miri | 146 of 152 commands (296 of 308 executions) pass within the unchanged 900 s watchdog; six commands exceed it, equally on the baseline (below) |

Host adaptations, all environmental; no test, check, limit or repository file changed:

- Chrome, ChromeDriver and Node run without the Nix shell's library path, and Chrome
  without the inherited X display (SSH forwarding), the host session's D-Bus (its
  calls stall for 25 s from inside the sandbox) and its PulseAudio server. GitHub
  runners have none of these. `LANG=C.UTF-8`, as on GitHub runners.
- The Carla UI is frozen with the Nix Python 3.11, copied out of the store because
  ZIP rejects the store's 1970 timestamps. pip's PyQt5 wheels get store copies of
  their system libraries, and the two frozen launchers use the Nix glibc loader that
  matches the libraries they bundle (the host has glibc 2.31).
- Perfetto's checksum-verified `trace_processor_shell` needs glibc 2.34 and runs
  through the Nix glibc loader; for `validate_perfetto_traces.py` a wrapper places it
  in the script's private home before each query.
- The Firefox step runs outside the Nix shell with a private PulseAudio null sink.

Before those adaptations were complete, the hosted Chrome smoke and the Perfetto
smokes failed for the environmental reasons above, and `shoop_engine` on Chrome hit
ChromeDriver's 30 s renderer timeout three times while loading its 166 MB debug test
module on a busy machine. Both web jobs were then run again, every step once and in
order, with the results shown; `shoop_engine` passed its 870 tests there and in one
separate rerun. Master's nightly runs fail this step intermittently too: `bcf928e1`
passed it on push and failed it in its nightly run. The application crates are
identical to the baseline.

The six slow commands, run side by side without the watchdog. Each pair shares one
physical core (baseline on one SMT thread, refactor on its sibling), so both take
2–2.5 times as long as on a core of their own:

| Command | Baseline | Refactor |
| --- | --- | --- |
| `rt_runtime` `adversarial_capture`, seeded capture model | 2180 s | 2167 s |
| `rt_runtime` `midi_state` (7 tests) | 2636 s | 2629 s |
| `shoop_loops` `adversarial_timeline`, seeded programs model | 2841 s | 2844 s |
| `shoop_loops` `adversarial_timeline`, seeded sync model | 5134 s | 5128 s |
| Loop host, dense wire with capacity-one actors | 2441 s | 2420 s |
| Loop host, acknowledged restart | 2547 s | 2529 s |

The sync model already exceeded the watchdog at `8633d0f0`
([Miri evidence](#miri-evidence)). The commands queued after the timeout in shards
9–11 ran with the unchanged runner, which skipped only the timed-out command.

Not run: the macOS and Windows jobs, and the Codecov upload and remote CI, which
publish.

After this replay, CI stopped running Miri and Chrome for lack of capacity (user
decision, 2026-10-29). Both remain for manual runs; see the
[CI scope decision](PLAN.md#remaining-stage-checks).

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

<a id="composable-loop-acceptance-check"></a>

### Acceptance check

The [acceptance criteria](PLAN.md#acceptance-criteria), checked on `fcbd5ea6`
(code-complete) and again, with CI's pins, on `61beffad`.

| Criterion | Result | Evidence |
| --- | --- | --- |
| A1 — parts only | Met | No `shoop_loop_engine` crate; `shoop_loops` has no type owning or wrapping `GraphRuntime`, `Control` or `ResourceStore` (only its private tests create stores); `Engine`, `Controller`, `Stopped`, `Node<P>` removed |
| A2 — application-owned assembly | Met | The service owns `ResourceStore` and `GraphRuntime<ServiceNode, LoopPlanner, dsp::Operation>`; `LoopPlanner::prepare` borrows the store; the teaching examples do the same |
| A3 — invariants kept | Met | `graph_guards.rs` (four tests, each failing with the hooks disabled) plus the [mapped existing tests](#loop-invariants-and-their-tests) |
| A4 — independent processor operations | Met | `Processor<L, Q>`; `shoop_example_dsp` defines `Operation`/`Receipt`/`Refusal` and depends only on `rt_runtime` |
| A5 — DSP through the scheduler | Met | No transport, mailbox or callback hooks; tests cover the exact sample across partitions, receipts, late/invalid/stale refusals and cancellation before delivery |
| A6 — demonstrator unchanged in behaviour | Met | One runtime call per callback; `one_track` 42/487/378 in every partition run and `latency_probe` 18 cases in both profiles; stall, disconnect, fault and two-thread tests pass |
| A7 — vocabulary | Met | `NodePair*`, `LoopNodes`, `PlaybackNode`/`CaptureNode`; "role" remains only in the verbatim M1-A5 criterion and unrelated application code |
| A8 — nothing lost | Met | Counts as the [baseline](#baseline) plus new tests in every suite (native, Node, Chrome, hosts with canaries). Replaced tests: three DSP bridge tests became runtime cancellation tests; one `pending_operations` refusal case (planner given a processor operation) became a nonfinite processor operation, since the types now forbid the former |
| A9 — dev-only assembly | Met | `shoop_loops_test_support` is only a dev-dependency in the workspace, and `tests/loops_host` compiles its source by path; `cargo tree` finds no normal or build edge to it in either |
| A10 — boundaries and safety | Met | Boundary, tracing (closed) and test-usage checks; `forbid(unsafe_code)` in all five crates; new render paths run under heap guards |
| A11 — documentation | Met | Architecture, plan, validation, crate READMEs, guides, runtime book and draft book updated; links resolve apart from unchanged historical evidence reports. A later audit of the crate docs against the code found a dozen passages the refactor had left stale (old controller and engine names, cancellation and admission details, same-frame DSP order); they are fixed. Final sweep at `62feff2b`: no current document or script uses a removed name or loop role vocabulary; rustdoc and doctests pass with warnings denied; the runtime book check passes and runs all 18 examples; every loop and service example and the draft-book example run; all 16 marked loop-doc excerpts match their examples |
| A12 — measurements recorded | Met | Paired service and loop benchmark results under [Measurements](#measurements); the preset workload is 13–17% slower, with the cause recorded |

<a id="new-engine-backend-evidence"></a>

## New engine backend evidence

Stage 10 of the [new engine backend plan](PLAN_RT_BACKEND.md#stage-10--end-to-end-validation),
run on Linux (AMD Ryzen 9 5950X, Ubuntu 20.04 host) on 2026-10-30 at `0d3925bb` plus
the Stage 10 working-tree changes: the CI wiring below, an opt-in CPAL device test,
plan status and the [example fixes](#after-the-merge-of-8cadc053). The branch then
merged `8cadc053`, which changes only documentation, comments in `rt_runtime`
examples and the separate `shoop_loops` book example package; the checks it can
affect were repeated on it. Builds and tests ran in `nix develop` with
`RUSTFLAGS="-D warnings"` at `nice 19` and idle I/O under a memory watchdog; peak
session memory was 8.1 GiB. Tools: the dev shell's Rust 1.97.1, wasm-pack 0.15.0 and
trunk 0.21.14, with CI's nextest 0.9.116 and Node 22.22.2. Miri ran outside the Nix
shell with the pinned `nightly-2026-05-04`. The baseline is the plan's
[Baseline](PLAN_RT_BACKEND.md#baseline) at `137cc837`.

### CI wiring

`build_and_test.yml` now covers the new crates where comparable crates are covered:

- the application-path nextest of the Linux release, macOS and Windows jobs runs
  `shoop_rt_protocol`, `shoop_rt_engine`, `shoop_rt_backend`, `shoop_rt_host_native`
  and `shoop_rt_host_worklet`, debug and release (Linux debug runs the whole workspace);
- the Wasm dependency audit lists `shoop_rt_protocol`, `shoop_rt_engine` and
  `shoop_rt_backend` next to `shoop_rt_host_worklet`;
- browser dependency isolation also checks the new worklet's tree for device and
  plugin libraries;
- feature isolation checks `shoop_rt_host_native` without its CPAL feature and that
  this build has no `cpal`, `midir` or `jack`.

Already wired: the Node suite discovers the four Wasm-enabled new crates from their
manifests (`shoop_rt_host_native` is native only); `check_rt_engine_boundary.py` and
its tests run in the native and web jobs; the web jobs build `shoop_rt_host_worklet`
for wasm32, run `rt_worklet_contract.mjs` and verify the packaged worklet. Miri and
Chrome stay manual.

### Local CI run

The Linux and web jobs of `build_and_test.yml`, step by step in CI's order.

| Job | Steps and results |
| --- | --- |
| Linux debug | Application build; complete workspace nextest (`--features shoop_engine/app_backend --profile ci`, `SHOOP_ALLOW_MISSING_BACKENDS=1`): **2327 passed, 4 skipped** (baseline 2191, the same four intentional skips); `rt_runtime` and `shoop_loops` (361) and the loop host crate (60); the four examples; native runtime and loop host runners; runtime, loop and engine boundary checks for the Linux, Windows, macOS and wasm32 targets with their 9 script tests; feature and dependency isolation; formatting, tracing inventory (269 modules, closed), test attributes and 73 script tests; Perfetto trace validation |
| Linux release | Application build; application-path nextest including the new crates: **1720 passed**; `rt_runtime`, `shoop_loops`, loop host crate, examples, host runners and boundary checks in release; guarded runtime and loop benchmarks without misses |
| Web debug | Trunk build; packaging and artifact verification; Wasm check and both worklet builds; browser dependency isolation, the legacy worklet's contracts and `rt_worklet_contract.mjs`; Wasm test policy and the dependency audit of 24 packages; **Node suite: 25 packages, 1976 passed, 0 failed**, plus the 2 intentionally ignored canaries (`shoop_rt_protocol` 5, `shoop_rt_engine` 20, `shoop_rt_host_worklet` 26, `shoop_rt_backend` 14, `shoop_engine` 870); runtime and loop host formatting and boundary checks |
| Web release | Optimized trunk build; packaging with the single-file HTML and artifact verification; release Wasm check and worklet builds; browser dependency isolation and both worklet contracts; loop host formatting and boundary checks; Firefox Web Audio workflow passes |

Per-crate workspace counts against the baseline:

| Crate | Baseline | Now |
| --- | --- | --- |
| `shoop_rt_protocol`, `shoop_rt_engine`, `shoop_rt_host_native`, `shoop_rt_host_worklet`, `shoop_rt_backend` | — | 5, 20, 21, 26, 15 |
| `shoop_backend` | 123 | 128 |
| `shoop_app` | 126 | 136 |
| `shoop_egui` | 212 | 222 |
| `shoopdaloop` | 50 | 68 |
| `shoop_app_api` | 23 | 24 |
| `shoop_loops` | 159 | 164 |
| `shoop_engine`, `rt_runtime`, `shoop_worklet_client` and the rest | — | unchanged (`shoop_engine` 1054) |

Every baseline test still passes. The two legacy contract tests,
`fake_backend_satisfies_contracts` and `engine_dummy_backend_satisfies_contracts`,
moved from `shoop_backend`'s private tests into its shared `contracts` module under
the same names. The workspace run predates the opt-in CPAL test; the release run
includes it (`shoopdaloop` 69), where it skips without its variable.

`shoop_engine::midi_state_diff the_pitch_wheel_is_independent_from_channel_pressure`,
which crashed once with SIGSEGV in the Stage 7 workspace run, passed in both runs and
in 20 further runs of its binary. `shoop_engine` is unchanged since `cb8a0324`.

Not run: the Carla steps (building the pinned Carla 2.5.10 runtime, the fake-Carla
rejections and probes, native packaging with the Carla runtime and its archive probes,
the Carla UI and benchmark smokes, and `SHOOP_REQUIRE_CARLA_TESTS=1`), which need that
build and cover unchanged legacy paths; the Rust coverage job; the optimized Node suite
and the Perfetto failure canaries, which run only on schedule or dispatch; the macOS
and Windows jobs; uploads, Codecov and remote CI. CI has no clippy step.

### After the merge of `8cadc053`

Repeated on the merge, with the Stage 10 changes:

- the workspace nextest: **2328 passed, 4 skipped**, now including the opt-in CPAL
  test, which skips;
- `rt_runtime` and `shoop_loops` tests with their examples and CI's four examples, in
  debug and release; formatting, tracing, test attributes and script tests;
- the `rt_runtime` book check with all its examples
  (`check_rt_runtime_docs.py --run-examples`), which CI does not run;
- the `shoop_loops` book examples (`src/rust/shoop_loops/docs-new/examples`, a
  separate Cargo workspace that neither CI nor a script builds): all seven binaries
  build against the current loop library and run to completion, so the loop-library
  additions of this milestone do not break them.

The other results stand: apart from comments, the merge changes no compiled code.

Fixed defects the merge brought:

- Its comments in seven `rt_runtime` examples start at column 0, so CI's
  `cargo fmt --all -- --check` failed. `cargo fmt` re-indented those 18 comment lines
  and nothing else.
- Four of those comments fall inside excerpts that the `rt_runtime` book quotes, so
  its check reported four mismatches. The same comment lines were added to the
  excerpts in chapters 2, 5 and 9.

Not changed: in the draft `shoop_loops` book (`docs-new`), 12 of 41 marked excerpts
differ from their examples, as on the merged branch itself (9 already before its last
two commits), and `overdub_loop.rs` and `program_loop.rs` are not rustfmt-clean. No
check in CI or `scripts/` covers them.

### Shared contracts

| Backend | Runner | Result |
| --- | --- | --- |
| `FakeBackend`, with all capabilities and with none | `shoop_backend` `contracts::tests::fake_backend_satisfies_contracts`, `fake_backend_without_capabilities_satisfies_contracts` | Pass: debug, release, Node |
| `EngineBackend::new_dummy` | `engine_dummy_backend_satisfies_contracts` | Pass: debug, release, Node |
| `EngineBackend`, Web Audio port model | `engine_web_audio_backend_satisfies_contracts` | Pass: debug, release, Node |
| Legacy timing reference (`WebAudioTimingHarness`) | `legacy_web_audio_backend_satisfies_harness_contracts` | Pass: debug, release, Node |
| New engine backend: adapter, in-process carrier, engine, offline host | `shoop_rt_backend::contracts` `adapter_satisfies_backend_contracts`, `adapter_satisfies_harness_contracts` | Pass: debug, release, Node |

The harness contracts are the transition-timing, signal-chain and loop-playback
contracts built from Stage 0's legacy reference. The new backend passes them with its
declared observation lag (see [known differences](#new-engine-backend-known-differences)).

### Core workflow

**Dummy driver.** `shoopdaloop`
`tests::new_engine_backend_runs_the_core_workflow_in_the_application` passes in debug
and release. It starts the application on the new engine, creates a new session, adds
an audio and a MIDI track, connects them to host ports and Master, records and plays
the sync loop, records and plays a synchronized audio loop and an immediate MIDI
loop, changes gains, balance, mute and monitoring, waits for meters and loop
progress, reads the waveform and MIDI details, sees a refused driver switch, stops
and clears. Its timings with `--no-capture`, from the request to the confirming
snapshot, polled every 5 ms:

| Step | Release | Debug |
| --- | --- | --- |
| Application start | 825 ms | 1987 ms |
| New session | 198 ms | 197 ms |
| Two new tracks | 132 ms | 137 ms |
| Connections and routes | 66 ms | 75 ms |
| Sync loop recording / playback | 20 / 35 ms | 35 / 20 ms |
| Planned synchronized recording shown | 5 ms | 5 ms |
| Synchronized recording / playback (waits for a wrap) | 112 / 178 ms | 147 / 183 ms |
| Immediate MIDI recording / playback | 35 / 35 ms | 20 / 20 ms |
| Controls and meters | 20 ms | 20 ms |
| Waveform / MIDI detail | 71 / 66 ms | 76 / 71 ms |
| Refused driver switch shown | 5 ms | 5 ms |
| Immediate stop | 38 ms | 20 ms |
| Clear | 310 ms | 321 ms |

**Real CPAL device.** The opt-in test
`tests::new_engine_backend_runs_the_core_workflow_on_a_cpal_device`
(`SHOOP_TEST_CPAL_DEVICE=1`; it skips without it) passes in debug and release on
ALSA's default device, through the host's PulseAudio plugin
(`ALSA_PLUGIN_DIR=/usr/lib/x86_64-linux-gnu/alsa-lib`), into a private PulseAudio
server with a null sink: 44.1 kHz, 512 frames, two capture and two playback ports.
It creates a new session and a stereo track connected to the capture ports and routed
to Master, records and plays the sync loop and a synchronized loop, changes Master's
gain, reads both waveform channels at full length, plays for one second (44,544
frames, 0 xruns, load 0.9% in release and 11.5% in debug), stops immediately and
clears, without an error notification or a host fault. The sandbox has no ALSA
sequencer, so midir found no MIDI ports.

**Browser.** Against the release build:

| Browser | Run | Result |
| --- | --- | --- |
| Firefox 136, headless, geckodriver 0.37.1, private PulseAudio null sink | CI's `browser_firefox_smoke.py` (legacy) | Pass: 10 callbacks of 128 frames, no overflows |
| | `browser_firefox_smoke.py --backend new_engine` | Pass: the in-page self-test runs the core workflow (new session, an audio and a MIDI track, host connections and Master routes, synchronized and immediate recording and playback, MIDI, waveform, clear) at 44.1 kHz in 128-frame quanta, 0 xruns |
| Chrome for Testing 147.0.7727.117, headless | `NEW_ENGINE=1 node --experimental-websocket browser_smoke.mjs` | Pass, hosted, 0 xruns |
| | The same with `SELF_CONTAINED=1` (the single-file HTML) | Pass, 0 xruns |
| | Legacy single-file output-only smoke | Pass |

The Nix shell has no Chrome or Chromium, and the host's snap Chromium cannot start in
the sandbox. The Chrome for Testing binary is the one kept from the earlier
[CI replay](#ci-replay-at-61beffad) (CI's former pin); nothing was installed. All
browser runs use fake media devices; a manual check in an installed Chrome with real
devices and permission prompts remains open.

### Miri

`run_rt_runtime_miri.py` with Miri `0.1.0 (ad3a598ca4 2026-05-03)` and its unchanged
900 s per-command watchdog, at `0d3925bb` (inventory 158 commands, 327 executions,
SHA-256 `900afdac…`), in 16 batches (`--shard N/16`), four at a time with a target
directory each; 5,601 s for all 16.

| Batches | Result |
| --- | --- |
| 1–7, 15, 16 | Pass: 28, 20, 33, 21, 36, 22, 18, 11 and 11 executions |
| 8 | 9 of 10 commands pass; `shoop_rt_engine` `control` exceeds the watchdog |
| 9, 10, 11 | Stop at `rt_runtime` `midi_state` and the loop host's two `native_dense` cases; their other commands, rerun with the unchanged runner without only the timed-out command, pass (9, 14 and 11 executions) |
| 12, 13, 14 | 9 of 10 commands pass; the seeded `adversarial_capture` model and both seeded `adversarial_timeline` models exceed the watchdog |

So 151 of 158 commands and 308 of 327 executions pass, including every new engine
and protocol case except `control`. The six other commands are the ones that exceed
the watchdog equally on the baseline (see the [CI replay](#ci-replay-at-61beffad)).
`--verify-batches` therefore refuses the set as incomplete.

`control` held all seven tests of its binary in one command. Run alone without the
watchdog, each passes in 153–194 s (1,129 s in all), so together they cannot fit
900 s. The inventory now lists them as seven exact commands, as it already did for
`structure` (164 commands, 327 executions; the runner's unit tests follow the count).
The engine's part of that inventory (`--package shoop_rt_engine`, 11 commands in three
batches, at the merge `8cadc053`) then passes within the watchdog: 12 executions, the
longest command 422 s with the batches in parallel.

### Platforms

macOS and Windows could not run here. No standard library for those targets is
installed (the Nix toolchain has the host and wasm32 only), so the pinned nightly
type-checked them with `-Zbuild-std` (`cargo check`, warnings denied):

| Target | Type-checks | Not checkable here |
| --- | --- | --- |
| `aarch64-apple-darwin` | `shoop_rt_protocol`, `shoop_rt_engine`, `shoop_rt_host_worklet`; `shoop_rt_host_native` with CPAL and midir and without | `shoop_rt_backend`, `shoopdaloop` and all test targets: they build the legacy engine's or the test support's Perfetto C++ SDK, which needs the target's SDK |
| `x86_64-pc-windows-msvc` | `shoop_rt_protocol`, `shoop_rt_engine`, `shoop_rt_host_worklet`; `shoop_rt_host_native` without CPAL | `shoop_rt_host_native` with CPAL (CPAL's ASIO build needs Visual Studio), `shoop_rt_backend`, `shoopdaloop` and all test targets (Perfetto) |

The new crates have no platform-specific code; CPAL and midir are the device layer.
With the CI wiring above, a push builds the application and runs the new crates'
tests in CI's macOS and Windows jobs.

<a id="new-engine-backend-known-differences"></a>

### Known differences and limits

- **Observation lag.** The snapshot describes the state at the start of the last
  rendered callback, one callback behind legacy. Transition frames, lengths and
  content are equal; the timing harness declares the lag. A synchronized request
  within one callback of a wrap can be scheduled one cycle later than on legacy.
- **Host ports fixed at preparation.** Natively the driver's ports are fixed at
  startup; in the browser, output-only mode reserves two capture channels and Web
  MIDI ports that appear after audio starts need a reload.
- **Restart for driver, capacity and backend changes.** Runtime driver switching is
  refused; settings are saved for the next start.
- **One fixed pool segment.** Capacities are set at startup; growth is later work.
- **Native device bridging.** Separate CPAL input and output devices are not
  resampled to each other, so drift shows as xruns; MIDI output is sent when the pump
  sees it, not placed within the buffer.
- **Browser.** Offline and Worker modes are refused for the new engine; its worklet
  has no trace realm.

<a id="new-engine-backend-acceptance-check"></a>

### Acceptance check

The [acceptance criteria](PLAN_RT_BACKEND.md#acceptance-criteria), checked on
`0d3925bb` and the merge `8cadc053`, with the Stage 10 changes.

| Criterion | Result | Evidence |
| --- | --- | --- |
| B1 — opt-in selection | Met | `audio.backend` defaults to legacy and requires a restart (`shoop_egui` `audio_backend_settings_default_to_legacy_and_require_a_restart`, `audio_backend_choice_shows_new_engine_pools_only_when_selected`); `--audio-backend` (`cli_overrides_the_audio_backend`) and `?audio-backend=` (`web_query_overrides_the_audio_backend`, Node) override it. Both composition roots choose once at startup; a failing driver falls back to the dummy driver on the chosen backend (`a_failing_device_driver_falls_back_to_the_dummy_driver`, `new_engine_startup_fallbacks_are_shown_to_the_user`). `shoop_rt_backend` uses only the protocol client and refuses what it does not support; it never constructs a legacy backend |
| B2 — platforms | Partly verified | Linux: dummy and CPAL core workflow tests in debug and release; midir only through the native host's MIDI bridge tests (no ALSA sequencer here). Browser AudioWorklet: Firefox 136 and Chrome for Testing 147. macOS and Windows: [type checks](#platforms) of the engine-side crates only; not run |
| B3 — core workflow | Met on Linux and in the browser | [Core workflow](#core-workflow): the dummy and CPAL application tests, `shoop_app::rt_backend core_workflow_runs_on_the_new_engine_backend` (native and Node), the browser self-test in Firefox and Chrome |
| B4 — explicit capabilities | Met | Legacy backends report their full sets (`legacy_backends_report_the_capabilities_they_support`); the new backend reports only MIDI injection (asserted by the core workflow test; `only_midi_injection_is_reported_and_the_rest_is_refused_with_its_reason`). UI: `unsupported_global_controls_are_disabled`, `unsupported_latency_and_dry_wet_options_are_disabled`, `context_menu_disables_items_the_backend_cannot_perform`, `hover_variants_are_hidden_without_grab_and_dry_wet_modes`, `unsupported_bus_and_track_features_cannot_be_requested`, `driver_switching_is_hidden_when_the_backend_cannot_switch`. Refusals reach the user: `backend_refusals_and_failed_actions_reach_the_user`, `intents_needing_unsupported_capabilities_are_refused_visibly`, `unsupported_actions_are_refused_visibly_on_the_new_engine_backend`. Whole rejection: `composite_session_is_rejected_whole_without_composite_support`, `session_content_is_rejected_whole_without_content_loading`, `session_documents_list_every_unsupported_capability_they_need`, `script_operations_needing_unsupported_capabilities_are_refused_visibly` |
| B5 — one implementation | Met | One engine, protocol and adapter used by both composition roots: natively `RtBackend::connect` over the in-process carrier, in the browser `RtBackend::begin` over the MessagePort record carrier, with the same engine compiled into the worklet module. `check_rt_engine_boundary.py` passes for the engine, the protocol and both hosts on the Linux, Windows, macOS and wasm32 targets (native and web CI jobs); the hosts depend only on the engine, the protocol and device libraries |
| B6 — carrier-neutral protocol | Met | `shoop_rt_protocol` runs the same checks on the in-process and record carriers (`lifecycle_credits_resync_bulk_and_limits_behave_alike_on_both_carriers`, `a_version_mismatch_is_rejected_on_both_carriers`, `telemetry_keeps_the_latest_records_while_reliable_and_bulk_lanes_refuse`, codec round-trip and malformed-input tests), natively, on Node and under Miri; `LimitsChanged` and the growable-limits capability; resync (`resync_streams_the_confirmed_state`, `connecting_removes_engine_structure_the_application_does_not_know`); disconnect (`a_disconnected_engine_is_reported_failed_and_operations_are_refused`, `a_framing_error_ends_the_connection_and_a_new_hello_reconnects`, `a_render_fault_silences_output_and_ends_the_connection`); generations (`a_new_backend_uses_newer_generations_after_reconnecting`) |
| B7 — audio never stops for structural changes | Met | `edits_keep_other_paths_continuous_and_ramp_removed_paths`, `master_outputs_are_connections_that_cross_fade_and_resync`, `repeated_create_and_remove_hands_slots_over_without_leaking`; on the real-time dummy driver `the_driver_loop_does_not_touch_the_heap_through_structural_edits`. Driver, rate and buffer changes are refused while running and saved for the next start (`driver_settings_are_saved_for_the_next_start_when_the_backend_cannot_switch`, `new_engine_driver_settings_saved_without_switching_apply_on_restart`, the refused switch in the core workflow test), so a restart is the only operation that changes them |
| B8 — prepared, segmented pools | Met, with one segment | Pools are segments sized from four settings, mapped onto one segment (`capacity_settings_become_one_prepared_segment`); exceeding them is refused visibly (`pools_and_identities_are_enforced`, `exhausted_pools_are_refused_with_a_visible_message`, `new_engine_capacity_settings_bound_the_application_visibly`); inactive slots are not rendered (`only_occupied_slots_are_rendered`). No segment is added at runtime |
| B9 — timing equivalence | Met, with the declared observation lag | The harness contracts pass on the legacy reference and on the new stack ([Shared contracts](#shared-contracts)); `shoop_rt_engine::timing synchronized_transitions_match_the_legacy_reference` |
| B10 — callback contract | Met for heap use; locks and waits by construction | Heap guards around the control endpoint and render owner in every engine test, the native host's driver loops (`heap_guards_are_live`, `threaded_streams_render_while_a_client_works_without_heap_operations`, `the_driver_loop_does_not_touch_the_heap_through_structural_edits`) and the worklet's render and lane writes (`message_port` tests); Miri for the codec, the endpoint's command materialization and pool handover. No test detects locks or waits: the engine, protocol and worklet host have none, and the native host's only mutex and sleeps are off the render path (the dummy driver's controlled-mode requests, its period between callbacks, the MIDI pump). Xruns are reported (`xruns_are_reported_since_the_previous_poll`, `bridge_underruns_and_overruns_and_backend_underruns_are_counted_as_xruns`, `reports_driver_state_load_and_xruns_in_telemetry`) |
| B11 — legacy unchanged | Met | `shoop_engine`, `shoop_audio_worklet` and `shoop_audio_protocol` are unchanged since `cb8a0324`; `shoop_backend` and `shoop_worklet_client` changed only for capability reports, `FakeBackend`'s switchable capabilities and the shared contracts. Every baseline test passes ([counts](#local-ci-run)), and so do the legacy browser smokes |
| B12 — documentation | Met | READMEs for all five new crates; `ARCHITECTURE.md` (implementation map and "New engine backend"); `PLAN.md` (workstream, status, authority, parity gaps); this section; the user guide's "New engine backend" page (`docs/source/usage.new_engine_backend.rst`, `0d3925bb`) |

Open for the user: running on macOS and Windows (a push runs CI there), real MIDI
hardware, a manual Chrome check with real devices, and remote CI and review.

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
integration notes. The demonstrator was subsequently retired; its useful composition
and latency scenarios now live in the [shared test support](src/rust/shoop_loops_test_support/README.md).

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
Miri recovery (a manual run; CI does not run Miri or Chrome), relevant final
merged-tree reruns, checked Linux/Windows/macOS and web artifacts, unmerged PR,
final-head required green CI, available review through approval (or explicit
absence) and independent final audit remain open.
Old parent/PR run states must be rechecked before delivery. No migration commit or
historical green run can certify a later head merely because only docs changed.

Evidence must retain command, source, relevant input equivalence, flags/toolchain,
actual listed/executed/passed/skipped counts and reasons, complete content/owner
recovery, specific failure witnesses and artifact identity. Never publish credentials
or signed artifact URLs. A run ending successfully is not by itself sufficient.


<a id="native-engine-ipc-evidence"></a>

## Native engine IPC evidence

Execution follows [PLAN_ENGINE_IPC.md](PLAN_ENGINE_IPC.md). No IPC delivery or
cross-platform acceptance is claimed by the baseline below.

### Stage 0: baseline and shared-memory feasibility

Source: `f22dbd1c8ea72ae1bc5497b40cd7daf6066f3b39`; execution branch
`feat/engine-ipc`, approved plan committed as `b603cd11`. Environment: Debian 13
x86_64, rustc 1.99.1 (`58cf8f9c3`), cargo 1.99.1, Node 24.19.0, Chromium installed.
The environment initially lacked Rust and development headers; Rust and Wasm/rustfmt
components were installed locally, ALSA/JACK headers and protobuf 3.21.12 unpacked
under `/workspace/.native-deps`. Initial linker failures from missing development
symlink targets were corrected in that local prefix without repository changes.

Commands ran with `RUSTFLAGS="-D warnings"`, two Cargo/test workers, debug info and
incremental compilation disabled, `nice -n 19 ionice -c 3`, and a process-group RSS
watchdog enforcing 3276.8 MiB (20% of the 16 GiB cgroup limit).

- `cargo test --locked -p shoop_rt_protocol -p shoop_rt_host_native --no-default-features`:
  14 passed (protocol 5, dummy/native unit 9), peak build RSS 2742.1 MiB.
- `cargo test --locked -p shoop_rt_backend -p shoop_rt_host_worklet -p shoop_rt_host_native`:
  62 passed (adapter 15, native host 21, worklet 26), peak RSS 1540.6 MiB.
- `cargo build --locked -p shoop_rt_host_worklet --target wasm32-unknown-unknown`
  followed by `node src/rust/shoopdaloop/rt_worklet_contract.mjs target/wasm32-unknown-unknown/debug/shoop_rt_host_worklet.wasm`:
  built artifact has no imports and the simulated MessagePort contract passes;
  peak build RSS 565.7 MiB. This is not a fresh real-browser smoke.
- Standalone `iceoryx2 = 0.10.0` probe (safe API; std feature; fixed `[u8;1280]`
  samples; one publisher/subscriber; queue capacity two; safe overflow disabled;
  discard-on-backpressure): parent/child exchanged the first two records; the
  third send reported zero deliveries and did not overwrite either unread record.
  Both processes exited successfully; peak build RSS 762.7 MiB. Probe source:
  [shared_memory_probe.rs](docs/engine_ipc/shared_memory_probe.rs).

The selected dependency supplies mapping, process-shared queue publication and
sample lifetime/reclamation; the application copies encoded records using its
safe API. Pin 0.10.0 rather than following upstream main. Its MIT OR Apache-2.0
license and Rust 1.89 minimum fit this workspace's stable toolchain. Actual IPC
mock/device, crash cleanup and macOS/Windows runtime acceptance remain later gates.


### Stage 1: common transport extraction

The existing typed and record carriers, endpoint interface, lane queues and
credit/correlation client moved to `shoop_rt_transport`; the protocol keeps its
schema and codec. Existing carrier fixtures/assertions are available through the
transport's `contracts` feature. No message or execution semantics changed.

- `cargo test -p shoop_rt_transport -p shoop_rt_protocol -p shoop_rt_backend -p shoop_rt_host_native -p shoop_rt_host_worklet`:
  67 passed, preserving the prior 62 plus the five split schema/carrier tests;
  peak build RSS 1123.0 MiB.
- Warnings-denied native build of protocol/transport/engine/backend/both hosts:
  passed. Wasm worklet build and Node artifact/import contract: passed.
- `python3 scripts/check_rt_engine_boundary.py`: all five audited packages pass
  on Linux, Windows, macOS and Wasm dependency trees. This is a dependency audit,
  not native execution on those platforms. The audit now rejects engine/host/OS
  dependencies in protocol/transport and transport dependencies in protocol.
- Boundary policy unit tests: six passed; test-attribute policy and closed tracing
  inventory: passed. Miri registration retains the five original protocol cases
  as two codec cases and three carrier cases in their respective packages; Miri
  execution remains a final-stage gate.


### Stage 2: MessagePort transport extraction

`shoop_rt_transport_message_port` contains both record-port halves and optional
main-thread browser bindings. Common record framing and prepared byte buffers
moved into `shoop_rt_transport`. Worklet configuration/ABI, device MIDI lanes and
rendering remain in the host. The app's MIDI wrapper delegates record delivery
and scheduled batch posting to the extracted carrier.

- `cargo test -p shoop_rt_transport -p shoop_rt_transport_message_port -p shoop_rt_host_worklet`:
  30 passed, including a MessagePort adapter running the shared lifecycle,
  credit/backpressure and bulk contract.
- Warnings-denied worklet Wasm build and Node artifact/import contract: passed.
  The MessagePort crate's `browser` feature and full `shoopdaloop` application
  also pass warnings-denied `wasm32-unknown-unknown` checks.
- `NO_COLOR=true trunk build` with Trunk 0.21.14: passed (peak RSS 2278.3 MiB).
  Independent hook invocations preserve the worklet's feature/import isolation.
- `CHROME_BIN=/usr/bin/chromium NEW_ENGINE=1 node browser_smoke.mjs`, from the
  application directory: actual hosted Chromium core workflow passed at 900x600,
  including synchronized audio, immediate MIDI, waveform/detail reads, stop and
  clear, with zero reported xruns; peak session RSS 1974.8 MiB. Fake microphone
  input was used; this does not certify real audio/MIDI hardware.
- Updated boundary audit, test-attribute policy and closed tracing inventory pass.


### Stage 3: setup schema and endpoint ownership

Portable `host` request/reply types cover paginated discovery, scalar engine
settings, bounded segment/MIDI-port additions, driver settings, readiness,
warnings/errors, status and explicit stop. Their codec uses the same bounded
record format with distinct host-family tags; OS/runtime types stay out of schema.
`start_endpoint` returns native host, endpoint and outbox independently. Common
record adapters expose transport failure separately from idle and can relinquish
an endpoint without destroying it. The engine exposes connection retirement that
preserves admitted music.

- Selected protocol/transport/adapter/native/worklet regression suites: 67 passed.
- Guarded native lifetime test: passed; connection retirement performs no heap
  work, 640 controlled frames render with the endpoint retained, then explicit
  endpoint drop/host stop sends MIDI reset and recovers the engine.
- New host-schema tests cover every request/reply family, UTF-8/text/record bounds,
  truncated records and separation from engine messages. New transport tests
  cover receive-only EOF notification, one terminal delivery and endpoint recovery.
- Warnings-denied native build and worklet Wasm build/Node artifact contract:
  passed. New portable ownership/buffer/schema cases are registered additively
  for final Miri execution; the retained inventory is 168 commands/336 executions.


### Stage 4: socket transport

`shoop_rt_transport_socket` supplies authenticated loopback setup and a common
record port with fixed prepared lane queues, incremental framing, retained
partial writes, bounded I/O passes and explicit EOF/framing failures. It adds no
engine semantics or timing changes. The feature-gated transport contract harness
now has an independent-peer fixture/barrier so the existing assertions can run
across real processes rather than assuming immediate delivery.

- `cargo test -p shoop_rt_transport_socket`: six passed, including the complete
  shared lifecycle/credit/bulk contract across a subprocess, fragmented/coalesced
  records, idle versus EOF, invalid sizes, rejected authentication and saturation
  with whole-record retry/order verification. The child authenticates through a
  private stdin token and exits after socket closure. Fixture controls are test-only.
- The first saturation test exposed a fixture mistake (reusing its pending retry
  payload as the receive buffer); separate buffers corrected the test. The fixed
  test passes with the original carrier implementation.


### Stage 5: shared-memory transport

`shoop_rt_transport_shared_memory` pins iceoryx2 0.10.0's safe threadsafe IPC API.
Six fixed-payload queues cover both directions and all lanes. Private descriptors
validate version, record width and exact capacities before access. Library-owned
relocatable mappings, sample publication/reclamation and overflow semantics are
used directly; no second raw-memory queue algorithm was introduced. Reliable and
bulk lanes disable overflow; telemetry enables the library's safe oldest-drop.
A per-session resource prefix scopes dead-peer cleanup; optional private socket
liveness distinguishes a departed peer from empty queues.

- Warnings-denied check and `cargo test --locked -p shoop_rt_transport_shared_memory`:
  six passed, peak build RSS 635.0 MiB.
- Bounded observable models: 1000 FIFO capacity/refusal/retry/recycle rounds and
  latest-value replacement agree with expected contents. Corrupt descriptor
  version/record width/reserved fields, truncated layout and mismatched negotiated
  capacity are rejected. The resulting port satisfies `Send` without application
  unsafe code.
- The same shared lifecycle/credit/bulk contract passes across a real subprocess.
  Its separate socket carries only authentication/readiness, with no engine records.
- Concurrent subprocess producer: all 10,000 full-width records have coherent,
  ordered sequence/payload bytes. Killing the producer during publication gives
  terminal EOF, and explicit dead-node cleanup reports successful reclamation.
  These are native process tests, not an assertion that Miri models OS mappings.
- Publication ordering, alignment and counter wrap are delegated to the pinned
  dependency's queue primitives; project-level queue/record ownership remains in
  the common transport. macOS/Windows execution and device hosting remain later gates.


### Stage 6: process hosting and supervision

`shoop_rt_host_process` composes the existing native endpoint with either record
carrier, using child-owned discovery/setup and an independent stdin shutdown
signal. The parent authenticates its launched child, bounds startup/stop waits,
reaps rollback failures and resolves a companion beside the application. EOF
retires control while retaining the live endpoint/host; only explicit shutdown
uses endpoint-drop, MIDI reset and render cleanup.

- `cargo test -p shoop_rt_host_process`: 12 passed (Linux), peak RSS 1257.0 MiB.
  Both carriers run shared backend contracts, threaded CPAL/MIDI mocks with zero
  callback heap operations, deterministic audio/MIDI/transition equivalence,
  malformed traffic, receive-only engine death, rollback and independent stop.
- A real crashing-parent fixture leaves its recording engine in Running phase
  with advancing frames; fixtures externally terminate intentional survivors.
  Data-channel loss likewise leaves a controlled engine advancing until explicit
  stop confirms MIDI reset and cleanup. Device faults retain the existing native
  terminal-failure policy; a failed device may not acknowledge MIDI reset.
- Native in-process and IPC traces agree exactly on loop states, 16/32-frame
  recorded ramp content and MIDI event frames. The native host's existing
  one-buffer lead differs from the older zero-lead offline fixture; the reference
  therefore uses the same native host/configuration. No production lead, latency
  mitigation or scheduling policy changed. Barriers exist only in test builds.
- Warnings-denied process-host build, four-target production dependency audit,
  tracing inventory and project test-attribute policy pass. Platform execution
  and app/package integration remain later gates.


### Stage 7: native application selection and fatal failure

The saved restart-required `audio.engine_transport` and `--engine-transport`
override choose in-process, socket or shared-memory delivery for native new-engine
runs. Legacy remains the default; explicit legacy/browser IPC is rejected.
Child-owned discovery/configuration reuse the existing application settings mapper.
The persistent fatal screen blocks app commands without closing/dropping the live
runtime or supervisor. Explicit close uses independent engine shutdown.

- Both IPC choices pass the existing full cooperative application core workflow:
  structure-only New Session, routing, sync/immediate recording and playback,
  gains/meters, capability refusals, clearing and chunked audio/MIDI detail reads.
- The UI failure test passes over both carriers: fatal text is actually painted,
  no window-close command is issued, later dispatch is refused, engine frames
  continue advancing, and explicit app close confirms MIDI reset and cleanup.
- The test exposed a retained shared-memory bootstrap socket clone that hid EOF;
  startup now flushes replies and transfers unique liveness ownership. Independent
  control-pipe loss retires control as well. Device failure also closes control.
- Complete retained native nextest suite: **2374 passed, 4 skipped**, 120.876 s,
  peak RSS 1231.6 MiB. Optional hardware tests are not hardware evidence when their
  opt-in environment is absent. A subsequent process-host regression run after
  device-failure retirement: **12 passed**. Warnings-denied workspace build passes
  (peak 1383.9 MiB); dummy-only app/companion build and CPAL-free graph check pass.
- Nextest's single-worker libtest prefix initially obscured fixture stdout clock/
  stop acknowledgements. Terse fixture output fixes that test-only issue; all 13
  focused process/fatal-UI nextest tests then passed. Production wire records and
  timing are unchanged.
- Four-target production boundary audit now also rejects concrete carriers/hosts
  or the engine as direct adapter dependencies. Tracing/test-attribute policies,
  formatting and all **134 Python orchestration/packaging tests** pass. The Python
  suite also uncovered a package-import path bug in the retained host runner;
  it now supports package and standalone invocation. Miri inventory expectations
  include the added portable cases (336 executions total).


### Stage 8: packaging, crate documentation and CI wiring

Native packaging builds and requires `shoop_rt_host_process` beside the app,
checks its architecture/execute bits, and probes both carriers before archiving.
The extracted-archive smoke runs both carriers from a temporary path containing
spaces and Unicode. CI builds/tests/packages both binaries on Linux/macOS/Windows,
includes every new carrier/process crate in native matrices, and adds the two
portable transport packages to Node discovery. Native IPC dependencies remain
absent from the intended no-default-features browser graph and worklet.

- Created and verified a fresh Linux debug archive from the Stage 7 binaries;
  both staged and extracted socket/shared-memory probes passed capture/playback,
  chunked stereo audio/MIDI reads and explicit shutdown. The independent companion
  is required and resolved beside the app, outside the source checkout/cwd/PATH.
- Reused the unchanged, verified Carla runtime component from base-branch CI run
  `41182627311`, artifact `13035653789`; its checksums/runtime lock were validated
  before repackaging. No Carla binary or dependency pin changed. This is a fresh
  app/engine package smoke, not a fresh Carla build. Archive size about 186 MiB;
  package verifier peak RSS 489.5 MiB.
- GitHub connector artifact/log access works; direct shell downloads from GitHub's
  Azure artifact redirect were blocked by the execution proxy. Artifact/log
  verification therefore uses the connected GitHub capability.
- All 134 Python checks pass, including package metadata, native runner invocation,
  additive Miri inventory partitioning and dependency policies. The workflow YAML,
  documentation links/navigation, closed tracing inventory, test attributes and
  formatting pass. Six relevant portable/host Wasm test dependency audits pass.
- Architecture, user setup/lifecycle limitations and crate READMEs now describe
  the actual shared schema/codec, transport families and host boundaries. Native
  platform CI execution, final browser smoke and review remain Stage 9 gates.


Stage 9 delivery checkpoint: PR [#897](https://github.com/SanderVocke/shoopdaloop/pull/897)
targets the still-open integration branch. Existing workflows filtered PRs to
`master`, so their PR filters now also include `feat/loop-execution-layer` to run
required build/test, documentation and CodeQL checks on this PR. Push/deployment
filters remain unchanged.


A final disconnect audit found that unexpected bytes on the shared-memory
liveness socket could remain unread and hide a later EOF. The liveness channel
carries no post-start records, so any such data now causes terminal protocol
failure. A regression test covers clean EOF and trailing data followed by EOF,
including latched send/receive refusal. All seven shared-memory carrier tests and
the warnings-denied carrier build pass; this native-only correction does not
change the protocol, engine, portable transport, worklet or timing.


### Stage 9: platform and device checkpoint

PR #897's initial validation head `494c79215632f885ecb5b54b7ae05f83a441228c`
passed all required checks: build/test run
[41259254391](https://github.com/SanderVocke/shoopdaloop/actions/runs/41259254391),
Docs and CodeQL. All six native debug/release variants (Linux, macOS arm64 and
Windows x86_64), Rust coverage and both web variants succeeded. Both carriers'
subprocess suites and staged/extracted package probes ran on every native
platform; macOS debug's main native suite passed 1765 tests with zero skipped.
Native release variants also exercise the engine/host callback heap guards.
The independent liveness edge-case correction is being validated on the next head.

The local real CPAL integration workflow passed in all three modes (`in_process`,
`socket`, `shared_memory`), using JACK 1.9.22's paced virtual dummy device at
48 kHz/256 frames: 3 tests passed in 15.122 s, peak RSS 2161.3 MiB. Each exercised
structure-only New Session, audio routing, sync recording/playback, control changes,
waveform reads, stop/clear and graceful teardown. The measured one-second windows
advanced 48128 frames with zero app-reported xruns in the IPC runs; the JACK server
reported startup xruns separately. This uses real CPAL/JACK APIs and a virtual
device, not physical audio hardware or the CPAL mock. Physical audio/MIDI hardware
is unavailable; mock MIDI routing and reset coverage is reported separately.

Protocol and portable-transport Miri selections passed 4 and 10 executions.
The selected engine Miri run subsequently completed as recorded below, retaining
the workload, seed and 900 s per-command watchdog. A local Node run was interrupted after an
unintentional overlap with Miri; it is not claimed as a complete local pass.
The complete Node suite subsequently passed in CI. Remaining heavy local checks
run sequentially; the overlapping processes remained below the session RAM cap.
Final source/evidence mapping and the manual Chrome check were still open at this checkpoint.


The following-head CI run `41262115536` exposed two timing assumptions. macOS
caught a bootstrap listener reading the child authentication token with a 100 ms
cap despite its 10 s launch deadline; the listener now uses the remaining launch
deadline. A 250 ms delayed-token regression failed on the old implementation and
passes with the fix; all seven socket tests pass. Windows caught the cooperative
app test assuming a 150 ms wall-clock sleep guaranteed 4800 rendered frames.
That test now waits for the same minimum confirmed recording length before
requesting playback. Production render scheduling and transport latency policy
are unchanged. All 16 focused subprocess, app workflow and fatal-UI tests, plus
the warnings-denied app/process/socket build, pass after these corrections.

The managed workspace went offline during engine Miri verification. Its files,
compiled artifacts and logs survived recovery, but the interpreter process did
not. Eight completed engine commands (nine passing executions) are retained;
the interrupted render-fault case and two remaining structure cases are rerun as
the unchanged runner's disjoint 9/11, 10/11 and 11/11 engine shards, each retaining
its exact workload, seed and 900 s watchdog. This is not a claim of an uninterrupted
whole-engine batch or whole-workspace Miri coverage.


macOS then exposed a separate bootstrap mode issue in run `41263243058`:
accepted sockets can inherit a nonblocking listener's mode. A token read could
therefore return WouldBlock immediately and reset the launched peer, bypassing
its configured blocking deadline. Bootstrap now explicitly selects blocking mode
on both ends for bounded authentication; `Port::from_stream` restores nonblocking
mode for ordinary record traffic. The delayed-token regression and all seven
socket tests pass, as does the warnings-denied socket build. The initial 100 ms
cap was independently reproduced and fixed; neither correction adds reconnection
or alters render timing.

### Stage 9: selected Miri, native and browser completion

The selected Miri campaign passed **16 commands / 26 interpreted executions**:
protocol 4, portable transport 10 and engine 12. The eight verified engine
commands preserved across the workspace interruption account for nine executions;
the remaining three exact shards passed in 399.137, 414.439 and 491.040 seconds.
The seed remained zero and the per-command watchdog remained 900 seconds.
[The archived index, reports and logs](docs/engine_ipc/evidence/miri/index.json)
include log digests and Rust-source/Cargo-manifest fingerprints. Those dependencies
are unchanged since `de43061e947dcf56cc24548f696f32091b2ce28d`; later changes affect
native carriers, app integration, test fixtures and documentation. This is selected
coverage, not an uninterrupted engine batch, all 336 inventory executions or OS
shared-memory interpretation.

At implementation checkpoint `21dc6759e9d9cbc6abc57a6f6458a4cb7cde4abf`, the
warnings-denied workspace build passed (peak RSS 1369.8 MiB), and the retained
native suite passed **2376 tests, 4 skipped** in 119.659 seconds (peak 1896.5 MiB).
The new backend now refuses unsupported audio preview before probing a UI-side
default device. Its existing three-mode core workflow asserts that refusal.

The final browser app rebuilt successfully with Trunk 0.21.14 and Node 22.22.2
(peak RSS 2252.8 MiB). A workspace restart interrupted the first final build;
the orphan was stopped and the successful build reran with the RSS monitor.
Both new and legacy worklet artifact/import contracts passed. The actual
new-engine workflow in Chromium 154.0.8037.57 passed at 900x600 with zero reported
xruns (peak 1970.0 MiB): sync and immediate audio/MIDI capture/playback,
controls, waveform/MIDI reads, stop and clear. This uses fake microphone input.
It is the manual application smoke, not the Chromium-147-pinned full Wasm runner.
[Browser records and logs](docs/engine_ipc/evidence/browser.json) pin these results.

An additional legacy **debug** Chrome smoke failed its unchanged 240-second
deadline at `WaitForPianoPress`, with zero synth output while callbacks advanced.
The same script reproduced the same failure against the base branch's debug
bundle: PR head `f22dbd1c`, CI run `41182627311`, artifact `13035548996`
(`shoopdaloop-web-wasm32-debug-dev-fd044144.zip`, SHA256
`43b3f850bcfcd10bdb3039937c7741895cf938e2b510b65feff771811965cb6e`).
Both failures are retained; this additional inherited gap is not a passing smoke.
Required web-release CI's Firefox workflow passed on the implementation checkpoint.
No legacy synth code or browser test deadline was changed for this milestone.

CI run `41265161972` exposed two more observation assumptions in native fixtures.
The crash-parent fixture treated admission/idle as confirmation of recording;
it now waits for the confirmed recording state before crashing. The app workflow
required seeing a short pending sync transition; it now accepts either the pending
state or recording already in progress, then still checks recording, playback and
the exact multiple of the sync length. Deterministic subprocess contracts retain
the precise transition/frame assertions. Neither change alters production timing
or adds transport latency mitigation. The four affected tests passed ten stress
iterations each (**40 executions**, no retries), with four test workers and peak
RSS 3248.9 MiB under the 3276.8 MiB cap.

After those fixture corrections, implementation commit
`0f52c68d0a3ceff1fea5607225871dc93e3450b6` again passes the warnings-denied
workspace build and complete retained native suite: **2376 passed, 4 skipped**,
121.914 seconds, peak RSS 1864.3 MiB. [Local records](docs/engine_ipc/evidence/local.json)
retain compressed logs and source pins. Only native test code changed after the
browser checkpoint; production browser and interpreted Miri sources are unchanged.

### IPC acceptance evidence map

The implementation uses one schema/codec, portable connection implementation,
application mapper and engine endpoint. Native subprocess and extracted-package
tests exercise the concrete carriers; browser tests exercise MessagePort.

| Criterion | Evidence |
| --- | --- |
| I1 — selection and isolation | `transport_cli_overrides_saved_selection_and_rejects_legacy_ipc`, `browser_rejects_native_ipc_selection`, settings tests and the app's three core workflows; `Pending` launches the separately packaged companion and child-owned native host. No fallback from carrier failure. |
| I2 — equivalent core behaviour | `both_process_carriers_run_the_backend_contracts_and_stop_gracefully`, `both_carriers_preserve_the_shared_transition_signal_and_playback_contracts`, app workflows, native-reference sample/MIDI/transition trace, chunked reads and capability refusals. Test-only admission barriers preserve exact comparisons without changing production lead. |
| I3 — genuine carrier choice | Socket/shared-memory subprocess contract tests; socket fragmentation, saturation/order and authentication; shared-memory bounded FIFO/latest-value models, 10,000 coherent records, descriptor rejection, producer death and reclamation. Only shared-memory setup/liveness uses a socket. |
| I4 — engine-side setup | `device_discovery_and_rejections_are_served_by_the_child`, host-schema round trips/bounds, app settings mapping/fallback tests, `Pending::discover` and child `serve` setup execution. Unsupported preview returns before UI-side device discovery. |
| I5 — terminal app failure | `idle_is_distinct_from_eof_and_eof_is_delivered_once_without_a_send`, carrier EOF/malformed-input tests, `engine_death_reaches_a_polling_client_without_new_mutations` and `ipc_disconnect_shows_fatal_error_keeps_app_open_and_stops_engine_on_close`. The last test inspects painted fatal text, command refusal and absence of a close command. |
| I6 — autonomous engine and explicit shutdown | `retiring_a_connection_preserves_the_endpoint_and_running_host`, `data_disconnect_keeps_the_process_running_until_independent_shutdown`, `a_crashed_app_leaves_its_recording_engine_running_without_control`, malformed traffic and device-fault suites. Survivors advance after parent exit; independent explicit stop confirms MIDI reset/cleanup; fixtures externally terminate intentional crash survivors. |
| I7 — callback and dependency boundaries | Guarded native host/endpoint/render tests, threaded CPAL/MIDI mocks on both carriers, release CI heap guards and selected ownership Miri cases. Four-target production/build boundary audit passes, including direct adapter exclusions. Socket/mapping/process work stays outside endpoint/render calls. |
| I8 — platforms and delivery | Linux/macOS arm64/Windows x86_64 debug/release subprocess and staged/extracted package gates; local fresh Linux package probes; three-mode real CPAL/JACK virtual-device workflow and mock MIDI routing/reset. Physical audio/MIDI hardware is unavailable. Final PR-head CI/artifact/review closure is recorded below; inherited legacy debug Chrome failure remains explicit. |
| I9 — shared protocol and crate roles | Shared carrier fixtures run typed local, encoded records, MessagePort, socket and shared memory. `check_rt_engine_boundary.py`, Wasm dependency audits and import-free built-worklet contracts enforce separation. Protocol, portable connection, concrete transport and host roles are documented in `ARCHITECTURE.md` and crate READMEs. |

### Final CI and delivery closure

Implementation/evidence head `bcc863647ff45ef5af2db811a160584d11931bf7` is green:
build/test run [41266246856](https://github.com/SanderVocke/shoopdaloop/actions/runs/41266246856),
Docs `41266246854`, CodeQL `41266246888`, and Codecov project/patch. The merge
`b063c01a6a4ef70ab303bb00d6389001e941593a` has exactly the pushed head's tree
(`bbb4d259fe583ddf470eabc089e1042556250d77`); its parents include the integration
base and the pushed head. [CI records](docs/engine_ipc/evidence/ci.json) preserve
check conclusions, source pins and relevant native log excerpts.

All six native platform/profile jobs passed both carrier subprocess contracts,
CPAL/MIDI callback mocks, autonomous-parent-crash and fatal-app workflows, plus
two staged and two extracted package probes per job. Linux debug's retained
workspace passed **2376 tests, 4 skipped**; Linux release's main native suite
passed **1768**, macOS debug/release **1767 each**, and Windows debug/release
**1765 each**, with zero skipped in those five main native suites. Additional
filtered tracing canaries intentionally skip unrelated cases; these are not
missing core tests. Release jobs execute the heap guards. The scheduled/optional
CPAL-free nightly job is skipped on this PR; local CPAL-free build/audit evidence
is recorded separately above.

The full Node suite passed **1986 tests across 27 packages**, with two intentional
failure-capture canaries ignored in the ordinary suite and exercised separately
by its required canary step. The portable packages include protocol 4, transport
10, MessagePort 5, engine 20, backend 14, worklet 17 and app 21 passing tests.
Its source-matched runner summary, tool versions and asset manifest are retained
in [the compressed report](docs/engine_ipc/evidence/node/summary.json.gz).
Artifact `13068147219` has SHA256
`cb383e0939382ae10f9fbe5c3ed1f6f7e8363da614afaa4e7f9b9810073fe85e`; its asset manifest pins merge `b063c01a`.
The web release job also passed its existing Firefox 159 AudioWorklet smoke.

Downloaded representative macOS debug artifact `13067411273`
(`shoopdaloop-macos-arm64-debug-dev-b063c01a.tar.gz`, SHA256
`1911421b9f1553e0725ee4245ba8aaa2f83e4525584cb24686a668e18ecce761`)
passes the native package verifier for arm64. Both app and companion are present
under `Contents/MacOS` with mode 0755; binary digests are retained in the CI record.
This inspection ran on Linux; macOS CI supplies actual execution and extracted
space/Unicode-path probes. The fresh local Linux package and CPAL/JACK workflow
remain separate evidence, with their stated component/hardware limits.

PR [#897](https://github.com/SanderVocke/shoopdaloop/pull/897) remains open against
`feat/loop-execution-layer`; it is not merged. Review submissions and requested
reviewers are absent. The only bot comment is Codecov; its patch check passes
at 82.78% against the configured 80% floor, and project coverage passes at 90.25%.
No automated code-review response was available, so no approval is claimed.
Physical audio/MIDI hardware and the inherited legacy debug Chrome synth gap
remain explicit; the milestone does not close unrelated historical acceptance gaps.

The final closure commit changes documentation/evidence only. Its Rust source tree
matches implementation `0f52c68d`; final-head checks are rechecked before handoff
and remain visible on the PR. Source-pinned artifacts above retain their actual
checkpoint identities instead of being relabelled with that later documentation SHA.
