# Plan: initial generic real-time runtime crate

Status: stages 0–5 complete; implementation branch `feat/generic-rt-runtime`. Evidence: `docs/rt_runtime/VALIDATION.md`; API contract/test matrix: `docs/rt_runtime/CONTRACTS.md`. No Shoop integration.

Design: `DESIGN.md`. Broader motivation: `PROPOSAL.md`. This plan delivers a new `rt_runtime` library under `src/rust/rt_runtime`, retained in the existing workspace. It is not a relocation of the existing engine.

## Goals and scope

- Establish a clean, application-independent boundary for audio/MIDI graph execution, control/render communication, and resource exchange.
- Improve real-time correctness by construction: explicit capacities, callback budgets, transactional changes, and safe ownership/reclamation on every outcome.
- Deliver a useful initial implementation with native and real AudioWorklet evidence, not interfaces or compile-only Wasm support.
- Keep all Shoop backend/application integration out of scope. Changes outside the crate are limited to workspace/lockfile metadata, generic test fixtures/tooling, CI, and documentation.

Included: deterministic DAG execution, stable processor state, bounded timed operations, MIDI 1.0 routing, revisioned audio/MIDI resources, append/capture snapshots, native/offline fixtures, and a non-shared-memory browser fixture.

Excluded: device adapters, Shoop domain concepts, production worklet integration, plugin hosting, feedback graphs, parallel rendering, automatic latency compensation, resampling, MIDI 2.0, unlimited recording, and shared-memory browser acceleration. Do not expand scope to rewrite existing backend mechanisms.

## Immutable acceptance criteria

These criteria may not be weakened or removed without explicit user approval.

- **A1 — Boundary:** `rt_runtime` is a workspace crate with no production/build dependency on `shoop_*`, UI libraries, device backends, or browser bindings. Public APIs contain no Shoop domain types. Repository test support is dev-only. Existing Shoop consumers remain unchanged.
- **A2 — Portable execution:** The crate builds and its applicable tests run on Linux, Windows, macOS, and `wasm32-unknown-unknown`. Offline and two-thread native fixtures plus an actual AudioWorklet fixture exercise the same logical control contracts. The browser fixture passes without shared memory, Wasm threads, or cross-origin isolation.
- **A3 — Real-time contract:** After preparation, the full Rust render entry path—including changes, transfers, publication, refusal, and retirement—performs no allocation, reallocation, deallocation, locks, waits, I/O, or unbounded retries. All runtime work has configured bounds; tests enforce operation/byte/event budgets. Allocator tests are active in both debug and release, with positive canaries proving detection. Supplied processors comply; third-party obligations and browser scheduling limits are documented.
- **A4 — Graph correctness:** Typed audio/MIDI DAGs support fan-in/out, multichannel routing, silent disconnected outputs, stable state across rewiring, and variable block lengths up to the prepared limit. Invalid/cyclic/oversized graphs are rejected. Fully validated graph transactions activate atomically at block boundaries; stale transactions cannot partially change the active graph. No extra signal block latency or implicit clipping is introduced.
- **A5 — Timing and MIDI:** Admitted, on-time parameter/MIDI operations apply at their exact sample frames, including block edges and equal timestamps. Timing/order is deterministic; late, invalid, oversized, and overflowed events have explicit outcomes. MIDI supports bounded short and SysEx payloads, deterministic merges, and tested bounded note-release/reset recovery. Subrange and event work cannot grow without bound.
- **A6 — Resource safety:** Audio and MIDI upload, revision commit, capture, snapshot, and release work while audio continues. Readers see immutable coherent revisions/prefixes, never partially replaced data. Callback copy work stays within declared budgets regardless of resource length. Slow readers, pool exhaustion, aborted transfers, and cancellation return explicit status without corrupting playback or leased content.
- **A7 — Ownership and overload:** Queue saturation, undrained completions/retirements, disconnects, stale IDs, cancellation races, and shutdown neither block rendering nor force callback destruction. Memory use remains within configured pools/in-flight limits. All owned objects are eventually reclaimed off RT after readers release and the host stops/drains; no leak is used as a fallback. Applied/rejected completions are not silently dropped.
- **A8 — Evidence:** Deterministic reference tests, generated/adversarial tests, allocator/lifetime checks, concurrency stress, real browser scenarios, and reproducible benchmarks are checked in and run in CI as appropriate. Final validation records commands, platforms, results, and limitations. Existing repository gates remain green.

## Design rules and constraints

- Follow `DESIGN.md`; prefer small explicit modules in one crate, not a framework of speculative abstractions.
- Treat the current engine as evidence, not an API specification. In particular, do not copy its allocation-permitted command execution or native-thread assumptions.
- Separate control compilation/preparation, bounded render staging, atomic activation, and reclamation. Never rebuild a graph or deserialize an arbitrary heap object in the callback.
- No arbitrary command closures. Reserve completion/retirement capacity before accepting operations. A stalled control side must not stall audio.
- Stable generational handles and transaction epochs are mandatory. Document wrap/exhaustion handling; never allow silent ABA reuse.
- Declare audio aliasing, event ordering, resource ownership, timestamp, overflow, and shutdown contracts before implementing them. Use compact errors on RT; format diagnostics elsewhere.
- Bound memory and work separately. Queue capacity alone is not a callback budget. Avoid polling/spinning for coherent snapshots; use bounded publication slots.
- Keep the first graph executor and resource pools simple and auditable. Optimizations require correctness tests and measurements, not assumptions.
- Any `unsafe` needs a local safety argument and aliasing/lifetime/concurrency tests. Use existing queue primitives where possible; no custom lock-free algorithm without a demonstrated need and validation.
- Runtime-enforced bounds cannot make arbitrary third-party DSP or JavaScript hard-real-time. Document obligations and test the supplied implementations without allocation exemptions.
- Use `#[shoop_wasm_test_support::shoop_test]` for repository Rust tests; use justified native-only annotations for thread tests. Keep guard scopes inside test-framework setup/teardown so tracing does not invalidate measurements.
- Follow `.agents/info/build.md`, `.agents/info/test.md`, and `.agents/rules/mandates.md`; read `.agents/rules/style.md` before code changes. Use the development shell where required.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Ordered implementation stages

Stages are sequential unless explicitly stated otherwise. Keep each milestone runnable; do not postpone overload/error tests until the final stage.

### 0. Establish branch, contracts, and validation matrix

- [x] Inspect the current worktree and preserve unrelated changes. Create an implementation branch from the committed proposal/design/plan state; do not reset the existing proposal branch.
- [x] Confirm workspace membership, test-attribute requirements, Wasm discovery metadata, browser runner headers, and native CI matrix. Record a baseline and any existing failures before implementation.
- [x] Write the API contract table: capacities, per-callback budgets, frame/sequence/epoch semantics, timestamp ordering, zero/invalid block behavior, saturation policies, shutdown, and processor obligations.
- [x] Map A1–A8 to named test groups. Specify small fixed-capacity configurations that force each exhaustion path; define example benchmark workloads and output metrics.
- [x] Verify: review the contracts against `DESIGN.md`; no implicit dependency on Shoop or mandatory Wasm shared memory. Commit the planning baseline before coding.

### 1. Crate skeleton, types, and real-time test instrumentation

Depends on stage 0.

- [x] Add `src/rust/rt_runtime/Cargo.toml`, library modules, crate documentation, and Wasm-test metadata. Use the existing workspace wildcard membership; update the lockfile only as needed.
- [x] Implement validated limits/configuration, frame positions, generational IDs, epochs/revisions, compact errors, bounded buffer/event views, and the prepare/process processor interface.
- [x] Add an allocator/deallocator guard harness that cannot compile to a release no-op; include allocation, reallocation, and deallocation canaries. Add destructor-location probes and a render-path audit for lock/wait/log/I/O calls; do not claim an allocator guard detects locks.
- [x] Add native and Wasm minimal render tests and a dependency-boundary check that excludes dev-only test support from A1 while checking production/build transitive dependencies.
- [x] Verify: warnings-denied native and Wasm builds, invalid configuration/overflow tests, buffer-shape tests, guard canaries in debug/release, and shared-test/dependency policy scripts. Commit.

### 2. Bounded control boundary and ownership lifecycle

Depends on stage 1; provides the foundation used by graph and resource transactions.

- [x] Implement typed operations, bounded SPSC endpoints, sequences, completion slots, and coherent fixed-size telemetry. No synchronous waits in the runtime.
- [x] Implement admission credits covering commands, terminal completions, and retirement. Encode explicit ownership on full queues, rejection, cancellation, and disconnection; never discard an owning `Result` on RT.
- [x] Implement per-callback command/publication budgets and reserved control capacity for release/recovery operations. Expose deterministic counters for work performed and refused/deferred work.
- [x] Define stop/park/drain lifecycle: stop callbacks before final destruction; allow the control endpoint to disappear while rendering safely continues with bounded retained storage.
- [x] Verify: FIFO/order, duplicate/stale identity handling, tiny capacities, stalled consumers, repeated refusal, endpoint drop in either order, outstanding cancellation, and all destructor probes. Run a two-thread stress test and a deterministic state-machine model; use concurrency model checking for any newly introduced atomic publication protocol. Commit.

### 3. Graph compilation and allocation-free execution

Depends on stages 1–2.

Completed: compiler/descriptor validation, guarded execution, stable reusable processor slots, and transactional activation. Evidence: `docs/rt_runtime/VALIDATION.md`.

- [x] Implement typed DAG construction, deterministic topological compilation, checked routing/scratch layouts, generational processor slots, and a relocatable plan descriptor.
- [x] Supply minimal source/gain/mix/MIDI-pass and stateful reference processors for tests/examples. Separate processor preparation from execution; do not add a domain processor catalog to the core.
- [x] Implement bounded plan staging/validation and epoch-checked activation. Keep the active plan unchanged until validation completes. Preserve retained processor state; retire displaced plans and removed nodes only when safe.
- [x] Implement planar audio routing, deterministic summation and MIDI merge, variable block processing, and silence for disconnected outputs. Reject cycles and aliasing/range errors rather than relying on compiler trust.
- [x] Verify: reference-interpreter comparisons for generated small DAGs; fan-in/out, multichannel, retained state, malformed descriptors, stale concurrent preparations, capacity limits, and invalid block shapes. Render continuously during staged rewiring and full retirement queues under RT guards. Commit.

### 4. Sample-accurate operations and MIDI overload behavior

Depends on stage 3.

Completed: conservative event/split/result reservations, exact-frame executor scheduling, bounded payload copying, separate admission/execution results, and internal/host MIDI reset recovery. Evidence: `docs/rt_runtime/VALIDATION.md`.

- [x] Implement bounded scheduled-operation storage and subrange processing for exact-frame parameters and MIDI, with documented equal-time and late-arrival policies.
- [x] Implement bounded MIDI 1.0 byte storage, short messages, SysEx limits, output writers, and stable fan-in ordering. Use checked timestamp arithmetic and explicitly reject unsupported/out-of-range data.
- [x] Implement visible event-overflow status and bounded note-release/reset recovery; reserve the required event capacity. Define behavior for future-schedule saturation and a full subrange budget without silently shifting already accepted on-time operations.
- [x] Verify: sample-zero/end-of-block/next-block events, equal timestamps from multiple sources, randomized block partitions, long frame counters, late events, MIDI byte/event floods, SysEx boundaries, and note-on/off recovery. Compare native/Wasm logical outputs; assert maximum subranges/events consumed per callback. Commit.

### 5. Revisioned resources, capture, and snapshots

Depends on stages 2–4.

Completed: fixed-page upload/capture/snapshot lifecycle, reliable owning resource requests, reserved processor I/O headroom, executor integration, and public-API concurrent playback/capture/transfer fixtures. Implementation contract: `docs/rt_runtime/RESOURCES.md`; native/Wasm evidence: `docs/rt_runtime/VALIDATION.md`.

- [x] Implement bounded resource/page/lease pools, checked metadata, upload IDs, chunk sequence/range validation, incremental validation, and cancellation. Reserve storage before accepting a transfer.
- [x] Implement expected-revision commits and immutable snapshot leases; activate only complete revisions. Return displaced storage through the lifecycle from stage 2, never final-reference-drop it on RT.
- [x] Implement minimal audio/MIDI append/capture writers and coherent prefix snapshots. Bound partial-tail copying and publication work; report captured frame/length and pending/stale/incomplete states explicitly.
- [x] Add a resource-backed playback/capture fixture to verify data replacement and fetching during processing. Resource lengths must exceed a single callback's transfer budget by a large factor.
- [x] Verify: exact sample/event round trips; snapshot immutability across later writes/replacement; length/revision races; reordered/duplicate/missing chunks; stale commits; slow readers; many pinned revisions; pool exhaustion; interrupted upload/export; and shutdown with leases. Assert maximum copied bytes and bounded memory, and recover all credits after teardown. Commit.

### 6. Host fixtures and genuine non-shared-memory AudioWorklet path

Depends on stages 3–5; fixture scaffolding may be prepared earlier without bypassing contract tests.

In progress: allocation-free prepared submission flags and reusable, incrementally materialized plan storage are implemented/tested as worklet prerequisites. Native/actual-worklet fixtures and wire credit validation remain pending; ordinary Chromium test results are not worklet evidence.

- [ ] Add deterministic offline and two-thread native examples using only the public crate API. Demonstrate graph edits, timed MIDI, resource replacement, capture, and snapshot reads.
- [ ] Add a minimal independent Wasm/JavaScript test fixture with a real AudioWorklet. If packaging requires a helper Cargo manifest, keep it test-only; no additional production abstraction crate or dependency on Shoop's worklet/backend.
- [ ] Implement a versioned bounded wire frame for the tested generic operations, sequence/epoch/revision fields, and chunked relocatable descriptors/resources. Reuse the same operation semantics as native; validate all decoded lengths and IDs.
- [ ] Compile on the control side; preinitialize worklet memory and processor slots before rendering. Limit ingress and incremental materialization by credits and work budgets. Recycle transferable buffers and acknowledge their ownership; reject oversized frames before decoding payloads.
- [ ] Run the fixture from an origin without COOP/COEP isolation headers and assert `crossOriginIsolated === false`; the current repository asset server supplies these headers, so add a fixture-specific serving mode rather than accidentally testing only the isolated case.
- [ ] Verify: actual rendering and acknowledged updates, native/browser command-journal equivalence for on-time inputs, delayed messages, transfer saturation/cancellation, stale epochs, and teardown. Capture rendered output through a test sink/OfflineAudioContext where supported and exercise live AudioContext rendering too; never substitute a mock for the worklet check. Assert no Wasm memory growth or Rust heap operations after preparation. Record browser/JS limits separately. Commit.

### 7. Adversarial validation, benchmarks, documentation, and CI

Depends on stages 1–6.

- [ ] Add reproducible seeded adversarial runs mixing graph commits, resource revisions, timed MIDI, snapshots, consumer pauses, disconnects, and restarts. Run long enough to exercise slot/page reuse and track peak outstanding ownership.
- [ ] Add fuzz/property coverage for plan/wire validation and resource chunk state machines. Run Miri on supported ownership/aliasing tests and any new unsafe buffer code; explicitly record tool exclusions and provide alternate tests rather than claiming unsupported coverage.
- [ ] Add release benchmarks for idle, representative and near-capacity DAGs, MIDI bursts, sustained capture, and graph/resource transfers. Report configuration, callback duration distribution/max, work counters, transfer throughput, memory high-water marks, and refusal counts.
- [ ] Keep performance acceptance based on enforced work/memory bounds and correctness. Record deadline misses on a specified local host; do not claim hard-real-time proof or speedups over the old engine from noisy CI timings. Investigate unexpected tail spikes before declaring completion.
- [ ] Add API examples, capacity sizing guidance, real-time processor obligations, ownership/teardown rules, supported host matrix, and explicit capability limitations. Update `DESIGN.md` to match the implementation.
- [ ] Wire native debug/release tests into the existing Linux/Windows/macOS matrix; discover core tests in Node/Chrome and explicitly run the AudioWorklet fixture in browser CI. Add dependency-boundary and allocator-canary gates; include artifacts for failures and benchmark reports without brittle timing thresholds.
- [ ] Verify: each A1–A8 test group is scheduled, not merely checked in; guard canaries and browser fixture failures actually fail their jobs. Commit.

### 8. Final end-to-end validation and delivery

Depends on all earlier stages. Do not mark complete for compile-only, skipped, or simulated browser coverage.

- [ ] Execute one public-API scenario in offline, two-thread native, and actual AudioWorklet hosts: start audio/MIDI routing; schedule exact-frame changes; stage and activate a new graph; capture; fetch a prefix; replace a large audio/MIDI resource; stall consumers to saturate queues; resume; verify outputs/completions; stop and reclaim everything.
- [ ] Compare deterministic sample/event results and effective revisions/frames across hosts for identically admitted journals. Use exact comparisons for copying/routing and documented tolerances for floating DSP; transport lateness must be reported, not hidden by comparisons.
- [ ] Re-run debug/release RT guards across successful, rejected, saturated, cancellation, and shutdown paths. Audit all callback-reachable ownership drops and budget loops. Confirm existing backend/application/protocol sources were not integrated or altered.
- [ ] Run targeted and full repository gates below in the prescribed environment. Record commands/results and resolve introduced regressions; report any unavailable local platform checks and obtain their CI results before completion.
- [ ] Commit final documentation/evidence. Push the implementation branch and create a PR describing scope, A1–A8 evidence, benchmarks, and deliberate limitations. Read `.agents/info/ci-debug.md` when inspecting CI failures; fix failures until required CI is green.
- [ ] Check for automated PR reviews. Evaluate and address feedback with tests as appropriate, document justified disagreements, and repeat until the automated reviewer approves if one is present. Do not treat CI success alone as review approval.
- [ ] Update this plan with completed checkboxes, validation evidence, remaining limitations, and PR link. No Shoop integration is required for completion.

## Validation commands

Run in the environment selected by `.agents/info/build.md`. Add the exact independent AudioWorklet fixture command here once stage 6 establishes its entry point; the ordinary Chrome unit-test runner alone is insufficient.

```sh
# Targeted iteration and release guards
cargo test -p rt_runtime
cargo test -p rt_runtime --release
RUSTFLAGS="-D warnings" cargo build -p rt_runtime
RUSTFLAGS="-D warnings" cargo build -p rt_runtime --target wasm32-unknown-unknown
python3 scripts/run_wasm_tests.py --runtime node --package rt_runtime
python3 scripts/run_wasm_tests.py --runtime chrome --package rt_runtime
python3 scripts/check_wasm_test_dependencies.py --package rt_runtime

# Required repository checks; format before Rust commits
cargo fmt --all
cargo fmt --all -- --check
python3 scripts/check_shoop_test_usage.py
RUSTFLAGS="-D warnings" cargo build --workspace
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci
python3 scripts/check_tracing_coverage.py --require-closed
```

Also execute the new boundary audit, fuzz/property/concurrency checks, release benchmarks, and actual worklet runner from their committed documented entry points. Run repository-prescribed application browser build/smoke gates where required; they are regression checks, not runtime integration work.
