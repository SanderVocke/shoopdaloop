# Perfetto tracing migration plan

## Status

Implementation in progress. Stage 0 is complete; core upstream, facade, native/browser runtime, and per-test capture milestones are implemented and under final validation.

## Goals

1. Replace Tracy and `tracy-extensions` throughout the active ShoopDaLoop build, runtime, test, CI, tooling, and documentation paths with [`perfetto-everywhere`](https://github.com/SanderVocke/perfetto-everywhere).
2. Keep `perfetto-everywhere` and its target-specific types private behind `shoop_tracing`, which provides Shoop-oriented spans, structured logs, instant events, integer/floating counter plots, capture lifecycle, and test-capture APIs.
3. Produce standard `.pftrace` files natively and in the hosted browser application. A browser capture must combine Window, dedicated engine Worker, and AudioWorklet data when those realms are active.
4. Give every eligible Rust testcase its own capture lifecycle in native nextest, Node Wasm, and Chromium Wasm suites, retaining traces according to an `off | failure | always` policy. Keep equivalent scenario-level traces for packaged browser smokes.
5. Preserve the disabled-path and realtime-audio safety properties, then characterize Perfetto versus the retained Tracy baseline in the native audio domain and report the result. The comparison is informational, not a release gate.

## Scope and boundaries

- The required browser target is the Chromium configuration supported by `perfetto-everywhere`; Node 22 is also required for the Wasm test harness. Existing Firefox application smokes must keep working, but tracing may be feature-detected as unavailable there until `perfetto-everywhere` claims Firefox support.
- Cross-origin isolation and `SharedArrayBuffer` requirements must be explicit for multirealm browser/audio tracing. Unsupported deployments, including direct-file cases that cannot satisfy those requirements, must remain functional and show a truthful tracing-unavailable reason rather than silently omitting realms.
- AudioWorklet sample-clock spans describe logical audio-frame intervals. They must not be presented as callback CPU durations. Native callback spans remain suitable for CPU-duration analysis.
- Trace files are diagnostic and potentially contain user data. There is no automatic network upload outside the local test artifact sink used by the test runner.
- Historical documents may retain Tracy references when they describe past evidence. Active instructions, dependencies, CI, tools, UI text, and agent guidance must not.
- Large traces, Perfetto UI assets, and Trace Processor binaries are generated/downloaded artifacts, not committed source files.

## Immutable acceptance criteria

- [x] `Cargo.lock` and every active manifest contain no `tracy-client`, `tracy-client-sys`, `tracing-tracy`, `tracy-nextest-capture`, or Tracy patch; CI no longer downloads/builds Tracy or publishes `.tracy` artifacts.
- [x] No production or test crate other than `shoop_tracing` depends directly on a `perfetto-everywhere*` crate. Application-facing JavaScript also imports only a Shoop-owned tracing adapter/artifact, not Perfetto implementation details.
- [x] `shoop_tracing` exposes implementation-neutral APIs for lexical spans, structured logs, instant events, i64/f64 plots, fields, coarse/detail gates, and capture/test lifecycle. Existing `tracing`/`log` callsites are either routed through that facade or migrated to facade macros without losing severity, target, message, typed fields, late span fields, or repeated enter/exit behavior.
- [x] Disabled tracing does not allocate and does not call a tracing backend. Audio callback recording is bounded, nonblocking, non-growing, and free of formatting, file I/O, protobuf work, and locks. The import-free `shoop_audio_worklet.wasm` contract remains intact.
- [x] Native application capture supports start, save, discard, shutdown-save, and another capture in the same process. Saved files are atomically published as non-empty numbered `.pftrace` files with no leftover `.partial` file.
- [x] Hosted Chromium can start and stop tracing from the existing developer UI, save through an explicit browser download, discard, and start again. One saved trace contains synchronized Window, active dedicated Worker, and active AudioWorklet tracks plus observable loss/clock health.
- [x] Native nextest, Node Wasm, and Chromium Wasm give each eligible testcase a unique capture identity and implement `off`, `failure`, and `always`. Passing traces are discarded in `failure` mode; intentional failing canaries in all three harnesses publish one valid, queryable `.pftrace`, including a span, structured event/log, and plot. Panic-abort limitations must be solved by a harness-owned supervisor rather than silently losing the failed Wasm testcase.
- [x] CI always uploads finalized failure captures and never uploads partial files. Trace names and reports identify target/runtime, package or binary, testcase, attempt, and a collision-resistant attempt digest.
- [x] Trace Processor validation proves representative native, Window, Worker, AudioWorklet, Node-test, and Chromium-test traces are structurally valid and contain expected spans, logs/events, typed fields, counters, realm descriptors, clock snapshots, and health diagnostics.
- [x] Existing tracing coverage remains closed, native/Windows/macOS/web builds and supported test suites pass, and current browser packaging and Firefox smokes do not regress.
- [x] A final committed migration report records versions/commits, validation evidence, known limitations, trace locations, and the native audio-domain Tracy/Perfetto comparison. Any apparent major regression is reported with evidence but does not by itself block completion.

## Design rules and constraints

### Facade ownership

- `shoop_tracing` owns all backend selection, static metadata, subscriber-layer construction, capture state, browser producer/collector adapters, health reporting, and test policy parsing.
- Preserve `tracing` semantics for allocation-permitted code through facade re-exports/macros and the `perfetto-everywhere` compatibility layer. Realtime engine code uses only direct, statically named facade calls.
- Do not leak `PlatformBackend`, `CaptureSession`, `AudioRingBackend`, metadata IDs, record layouts, or collector types through public Shoop APIs.
- Prefer typed fields and counters over Tracy-style formatted message blobs. Preserve log severity, target, message, and callsite fields as separate Perfetto data.

### Realtime and browser realms

- Keep coarse callback/session instrumentation separate from detailed per-node/stage instrumentation.
- Use a compile-time/generated static metadata catalog for all realtime names, categories, and fields; no first-use registration or dynamic string work may occur in a callback.
- Retain the raw worklet module's zero-import ABI. Add an upstream pure-Rust/preallocated linear-memory producer if needed, and bridge its fixed records into the collector ring with bounded, allocation-free callback work.
- Pass exact frame position/quantum information into the worklet tracing producer. Preserve discontinuity, overflow, high-water, and clock-calibration diagnostics.
- Audit all current integer plots. Keep depths, counts, reason codes, and cumulative totals as i64; convert ratios, loads, durations expressed fractionally, and similar measurements to f64 where that is the clearer Perfetto representation.

### Capture and tests

- Use bounded buffers and make drops visible; application continuity takes precedence over capture completeness.
- Quiesce producers before finalization. Save via a `.partial` path followed by an atomic rename where a filesystem exists; browser save returns application-owned bytes and a user-initiated Blob download.
- The Wasm test supervisor must live outside the panicking Rust stack, receive testcase begin/records/metadata/calibration incrementally, and finalize a dangling failed testcase after the harness reports a trap. Do not rely on `Drop` or `catch_unwind` under the default `wasm32-unknown-unknown` panic-abort target.
- Keep failure-only capture as the CI default and provide `always` for canaries/investigation. Abort, timeout, process kill, and OOM limitations must be explicit and must not produce misleading finalized traces.
- Pin `perfetto-everywhere` and Trace Processor to reviewed revisions/releases and verify downloaded tool checksums.

## Current evidence and known integration gaps

- `shoop_tracing` currently wraps 43 direct realtime span callsites, 13 realtime plots, and callback frame marks, while `shoop_common` installs separate `tracing-tracy` span/event layers and owns the embedded Tracy lifecycle.
- Native nextest currently uses `tracy-nextest-capture` with failure-only per-attempt save/discard. `#[shoop_test]` expands directly to `wasm_bindgen_test` on Wasm and has no browser/Node capture lifecycle.
- `perfetto-everywhere` main at inspected commit `48ed779` provides native sequential capture, ordinary Window/Worker producers, an AudioWorklet SAB producer, a collector, and a `tracing-subscriber` layer, but is an unpublished `0.1.0` release candidate.
- Upstream currently pins wasm-bindgen 0.2.121 while Shoop pins 0.2.127, its supplied browser runtime has example-specific worker asset names, it has no Node/per-test supervisor, and its AudioWorklet backend assumes wasm-bindgen imports. Those gaps must be closed upstream or through generic upstream APIs before Shoop integration; they must not become permanent application-local forks.
- Shoop's production worklet is an import-free raw Wasm module, and current CI asserts that invariant. Browser collection therefore needs an upstream raw/preallocated producer path rather than directly instantiating the current wasm-bindgen AudioWorklet type.

## Staged implementation

Stages are ordered; a later stage may begin only after its required upstream/local interfaces are verified.

### Stage 0 — Freeze contracts and collect the Tracy baseline

- [x] Record current Tracy and `perfetto-everywhere` revisions, toolchain versions, capture policies, callsite inventory, and representative Trace Processor SQL expectations.
- [x] Add a deterministic release-mode native dummy-engine workload with fixed sample rate, quantum, graph, warm-up, and iteration count. Measure untraced, Tracy coarse, and Tracy detailed modes using external wall/CPU data, callback-budget misses, and the engine's existing profiling counters.
- [x] Run repeated baseline trials on one documented machine with idle/load notes; retain compact JSON/Markdown results and commands, not `.tracy` files.
- [x] Define the static metadata catalog and integer-versus-float plot mapping for every current direct realtime callsite.
- [x] Verify the baseline benchmark is repeatable enough to distinguish an obvious deadline-miss or order-of-magnitude regression; do not turn the result into a hard threshold.

**Verification:** baseline workload tests pass, its Tracy trace is queryable, and the checked-in baseline report contains enough environment/command detail to rerun it after migration.

### Stage 1 — Make `perfetto-everywhere` integration-ready upstream

- [x] Create focused branches/commits in `SanderVocke/perfetto-everywhere` for the missing generic capabilities: compatible wasm-bindgen/tool versions, configurable/packageable browser worker assets, reusable realm lifecycle, and Node-compatible collection/export.
- [x] Add an import-free, preallocated raw-Wasm producer API suitable for Shoop's AudioWorklet/Worker module. It must accept exact logical frame timestamps, expose bounded drain state without allocations, and retain the existing overflow/health contract.
- [x] Use the upstream complete-group drain/metadata hooks with a Shoop-owned external Wasm test supervisor that finalizes normal tests and retains a pre-published bootstrap after a harness-observed trap.
- [x] Add upstream and Shoop unit/browser/Node tests for sequential sessions, realm attach/detach, metadata collision handling, forced overflow, panic/trap finalization protocol, equal-timestamp ordering, and raw producer no-allocation/no-import behavior.
- [x] Run upstream native, Chromium multirealm/audio, tracing-compatibility, collector SQL, Node, MSRV, quality, and security acceptance; all seven PR checks passed.
- [x] Merge the upstream work as PR 1 and pin Shoop to immutable merge commit `f621af951b80f702c6b710e420c8a1abf5e333c7`; no floating branch is used.

**Verification:** upstream CI is green; standalone native, Node, Window/Worker, and AudioWorklet examples emit Trace Processor-valid `.pftrace`; a raw producer fixture has zero Wasm imports and bounded storage.

### Stage 2 — Rebuild `shoop_tracing` as the only tracing boundary

- [x] Replace Tracy features/dependencies with target-specific private `perfetto-everywhere` dependencies and a compile-time no-op mode.
- [x] Implement the facade macros/types for spans, detail spans, typed fields, events, logs, i64/f64 plots, frame/quantum events, gates, and pre-registration/prewarming.
- [x] Integrate the `tracing-subscriber` compatibility layer behind facade construction so ordinary `tracing` spans/events and `log` records retain typed semantics on native, Window, Worker, and tests. Avoid installing multiple global subscribers/loggers.
- [x] Move capture lifecycle, output naming/sanitization, atomic publication, status/health, and save/discard behavior out of `shoop_common` into `shoop_tracing`.
- [x] Implement browser producer/collector and Blob-download adapters behind Shoop names, including restart and truthful feature-detection errors.
- [x] Add facade contract tests for enabled/disabled/detail gates, all value types, late fields, nested/re-entered spans, logs/events, counter type selection, sequential capture, overflow, and error paths.
- [x] Replace Tracy-specific allocation tests with backend-neutral tests proving the disabled path allocates nothing and raw realtime recording stays within its bounded exception/producer contract.

**Verification:** `shoop_tracing` tests pass natively and in Node/Chromium; only this crate's dependency tree contains `perfetto-everywhere*`; a facade-only smoke trace passes SQL assertions.

### Stage 3 — Migrate instrumentation and realtime producers

- [x] Route all production `tracing`/`log` integration through `shoop_tracing`; remove direct Tracy clients, source locations, plot names, message formatting, and frame-mark conventions.
- [x] Migrate the engine's coarse/detail spans and 13 plots to the new typed facade without changing stable event names unless a documented Perfetto schema improvement requires it.
- [x] Integrate the raw producer into `shoop_audio_worklet` and the dedicated engine Worker path, including exact frame/quantum updates, static metadata export, bounded draining, and producer shutdown.
- [x] Preserve worklet render-time no-allocation tests, engine allocation guards, callback continuity, and disabled tracing behavior.
- [x] Update `docs/tracing_coverage.csv` rationales/classifications for Window, Worker, AudioWorklet, test harness, and collector boundaries.

**Verification:** closed tracing inventory passes; worklet no-allocation and no-import checks pass; native and Wasm trace fixtures show expected engine hierarchy/events/counters and explicit drop diagnostics under forced overflow.

### Stage 4 — Integrate native and browser application capture UX

- [x] Replace `NativeTracing`'s embedded Tracy bootstrap/lifecycle with the `shoop_tracing` controller. Keep CLI `--tracing` and `--tracing-engine-detail` behavior unless a neutral rename is intentionally made with compatibility aliases.
- [x] Preserve runtime Start, Save, Discard, automatic shutdown-save, and repeated capture. Update active status to show truthful Perfetto capacity/health rather than Tracy event-storage bytes.
- [x] Enable the developer tracing controls in browser builds. Start Window collection immediately and attach/detach the active engine Worker or AudioWorklet producer as backend state changes during a capture.
- [x] On browser Save, quiesce all realms, drain and finalize the collector, then present an explicit `.pftrace` download action. Discard must release all buffers without download; both paths must permit restart.
- [x] Package the Shoop tracing runtime and metadata with hosted/self-contained artifacts where platform policy permits. Collection is deliberately finalized in Window to avoid a second generated Wasm artifact; this evidence-backed revision and its UI-pause tradeoff are documented in the report. Document/serve COOP and COEP headers and show a precise unavailable reason otherwise.
- [x] Cover start/stop races, late realm attachment, realm failure, page teardown, collector failure, save/discard, and restart in application tests and hosted Worker/AudioWorklet smokes.

**Verification:** native CLI/UI and hosted Chromium UI each complete save/discard/save cycles; Trace Processor shows synchronized application and active engine realms; existing application/browser audio workflows remain responsive with tracing disabled.

### Stage 5 — Add per-testcase Perfetto capture to every Rust harness

- [x] Replace `tracy_capture_test` and `no_tracy` with backend-neutral `shoop_test` expansion and an explicit `no_trace = "reason"` escape hatch for lifecycle/allocation tests.
- [x] Implement native nextest `off | failure | always` sessions in `shoop_tracing`, preserving panic/`Result::Err`, retry identity, collision-safe paths, and no writer operation for discarded attempts.
- [x] Extend `scripts/run_wasm_tests.py` with a localhost capture supervisor/sink for both Node and Chromium. Give each macro-generated testcase an external begin/end identity and stream enough data that the supervisor can finalize a trapped final testcase.
- [x] Store traces beside reports under runtime/package/testcase paths, add trace references to JSON/JUnit, and delete passing traces in failure mode only after the parsed testcase result is authoritative.
- [x] Add intentional last-test failure canaries for native, Node, and Chromium plus an `always` canary containing multiple passing tests. Validate unique files, expected retention, no partials, and span/log/event/plot SQL.
- [x] Give packaged hosted Worker and AudioWorklet browser smoke scenarios their own capture output when supported; preserve a clear unavailable reason for an environment that cannot satisfy tracing prerequisites.

**Verification:** policy matrix tests pass in all three Rust harnesses; each failure canary yields exactly one valid identified trace; successful failure-only suites leave no trace; `always` produces one per eligible testcase.

### Stage 6 — Replace CI, packaging, dependency policy, and documentation

- [x] Remove Tracy workspace dependencies/patches, prebuilt-library setup, canary/query steps, `.tracy` upload, and the obsolete Tracy build workflow. Update lockfile and platform caches.
- [x] Add pinned Trace Processor acquisition/checksum handling, Perfetto canaries, and `if: always()` `.pftrace` uploads for native and Wasm report roots with 14-day retention.
- [x] Update Wasm/worklet dependency audits to allow only the reviewed `shoop_tracing`/raw producer path and continue rejecting native or wasm-bindgen imports from the raw worklet.
- [x] Ensure native Linux/Windows/macOS and web debug/release packaging include exactly the required tracing runtime assets and no Tracy binaries/libraries. (Linux/web verified locally and the Linux, Windows, macOS, and web CI packaging matrix passed.)
- [x] Replace active Tracy README/RST/help/UI text and `.agents/skills/tracy` guidance with Perfetto capture/query/privacy/clock semantics. Update test modifier and CI artifact documentation.
- [x] Run a repository-wide case-insensitive Tracy audit. Leave only explicitly labeled historical baseline/migration-report references.

**Verification:** clean platform CI reaches build/package/test/upload without Tracy setup; dependency trees and packaged-file manifests contain no Tracy; documentation commands produce/query `.pftrace` files.

### Stage 7 — Characterize audio-domain impact and write the result report

- [x] Run the Stage 0 workload in untraced, Perfetto coarse, and Perfetto detailed modes on the same documented machine/configuration and repetition scheme.
- [x] Compare against Tracy using callback budget misses/xruns, callback and cycle distribution summaries, throughput/wall/CPU ratios, trace drops/overwrites, and qualitative realtime behavior. Do not claim precision beyond the test setup.
- [x] Investigate and explain any obvious regression, including whether it comes from facade gating, timestamping, record copies, draining, collector work, or detailed event volume. Optimize only when evidence supports a bounded change.
- [x] Create `docs/perfetto_migration_report.md` with dependency revisions, schema/event changes, application/test/CI validation, trace examples and SQL, browser limitations, performance comparison, and remaining follow-ups.
- [x] State plainly whether a major audio-domain regression was observed. If one was observed, report its evidence and impact without making completion contingent on removing it.

**Verification:** the report links or names reproducible commands/artifacts and distinguishes measured observations from interpretation.

### Stage 8 — Final end-to-end validation

- [x] Run `cargo fmt --all -- --check`.
- [x] Run `RUSTFLAGS="-D warnings" cargo build --workspace` and target-specific native feature/dependency checks.
- [x] Run `python3 scripts/check_shoop_test_usage.py` and `python3 scripts/check_tracing_coverage.py --require-closed`.
- [x] Run `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci` with failure capture, then targeted `always` and failure canaries.
- [x] Build `shoopdaloop` and the import-free `shoop_audio_worklet` for `wasm32-unknown-unknown`; rerun dependency and import audits.
- [x] Run complete Node and policy-selected Chromium Wasm suites with capture canaries, plus hosted/self-contained Chrome and retained Firefox packaged smokes.
- [x] Validate representative native application, hosted browser multirealm, native-test, Node-test, and Chromium-test traces with pinned Trace Processor SQL.
- [x] Confirm save/discard/restart, failure artifact upload, atomic/no-partial publication, package contents, and repository-wide Tracy removal on the final commit.
- [x] Run/inspect the full GitHub Actions matrix and record run URLs/artifact names in the migration report.

**Verification:** all immutable acceptance criteria are checked, CI is green apart from explicitly documented unrelated infrastructure failures, and the final report is complete.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone, including upstream `perfetto-everywhere` milestones separately from Shoop integration milestones.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised only for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
