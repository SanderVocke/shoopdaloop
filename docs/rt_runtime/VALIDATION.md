# Runtime implementation evidence

## Stage 0: planning baseline

- Planning commit: `76d1f3d9` (parent proposal `704fbb72`). Implementation branch: `feat/generic-rt-runtime`.
- Existing untracked `.pi/` is harness state and is not staged or modified.
- Read repository index, mandates, coding style, build/test guidance, and CI-debug guidance.
- `docs/rt_runtime/CONTRACTS.md` defines the API/ownership/timing table, A1–A8 test mapping, tiny-capacity cases, and benchmark workloads.
- Workspace membership is already `src/rust/*`. The test policy requires the shared dev-only `shoop_test` attribute. Wasm package discovery uses `[package.metadata.shoop-wasm-test]`, not package-name prefixes.
- Native CI already covers Linux x86_64, Windows x86_64, and macOS arm64 in debug/release. Wasm package discovery is automatic, but dependency auditing and browser-change filtering require explicit additions during stage 7.
- The existing Wasm asset server sends COOP/COEP headers. The independent AudioWorklet fixture needs an explicitly non-isolated origin; ordinary browser unit tests do not prove this.

### Baseline results

Commands executed through `nix develop --command` on NixOS x86_64, before Rust implementation changes:

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass. |
| `python3 scripts/check_shoop_test_usage.py` | Pass. |
| `RUSTFLAGS="-D warnings" cargo build --workspace` | Pass. |
| `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci` | 1800 passed, 4 skipped (existing suite). |

The current flake supplies Rust 1.97.1, nextest 0.9.143, Node 22.23.2, and wasm-pack 0.15.0. The baseline nextest result above uses 0.9.143, not the repository-documented 0.9.116; obtain the prescribed version for final verification. Both native and `wasm32-unknown-unknown` targets are installed. The browser runner pins Node 22.22.2 and Chrome/ChromeDriver 147; matching Node/Chromium/driver packages exist in the local Nix store but are not on the development shell PATH. Use those tools explicitly inside the selected shell for subsequent Wasm tests; do not weaken the runner's checks. No baseline browser execution is claimed here.

Raw baseline command logs were retained locally as `/tmp/rt-runtime-baseline-build.log` and `/tmp/rt-runtime-baseline-tests.log`. Future milestone entries must record committed commands/results rather than relying on these temporary paths as permanent evidence.

## Stage 1: core contracts and instrumentation

Implemented validated configuration/budgets, checked IDs and counters, borrowed audio/event buffers, processor preparation/process interfaces, and an empty-graph render endpoint. No graph or control/resource capability is claimed yet.

Verification (through the development shell):

- `cargo test -p rt_runtime`: 12 passed.
- `cargo test -p rt_runtime --release`: 12 passed, including direct allocator/reallocator/deallocator canaries, real collection lifetime detection, and a deliberately allocating callback rejected by the assertion harness.
- `RUSTFLAGS="-D warnings" cargo build -p rt_runtime --target wasm32-unknown-unknown`: pass.
- `RUSTFLAGS="-D warnings" cargo build --workspace`: pass.
- `cargo fmt --all` and `python3 scripts/check_shoop_test_usage.py`: pass.
- `python3 scripts/check_rt_runtime_boundary.py`: pass for Linux, Windows, macOS, and Wasm production/build trees (including all features).
- `python3 -m unittest scripts/tests/test_rt_runtime_boundary.py`: 2 passed; forbidden dependency canaries cover domain, UI, driver, and browser libraries.
- `python3 scripts/check_wasm_test_dependencies.py --package rt_runtime`: pass.
- `python3 scripts/run_wasm_tests.py --runtime node --package rt_runtime`: 10 passed.
- `python3 scripts/run_wasm_tests.py --runtime chrome --package rt_runtime`: 10 passed.

Wasm commands used the cached Node 22.22.2 and matching Chromium/ChromeDriver 147.0.7727.137 inside `nix develop`; reports are in `target/wasm-tests/dev/reports/{node,chrome}`. Native-only unwind tests account for the two-test difference. These are browser unit tests, not the stage-6 AudioWorklet fixture.

Render audit: `process` only checks fixed shape/counts, clears at most configured output channels × frames, checks the frame counter, and updates plain state. Event append validates capacity/order before bounded slice copying; rejected appends leave both arenas unchanged. No reachable locks, waits, heap owners, formatting/logging, I/O, or unsafe code exist in the production crate. The test allocator's unsafe forwarding is isolated to test support with safety comments. Third-party processor behavior remains a documented obligation and will be guarded in fixtures.

## Stage 2: bounded ownership transport

Implemented generic SPSC control/render endpoints using `rtrb`, separate normal/recovery admission lanes, sequence IDs, best-effort cancellation tickets, owning terminal completions with retirement bundles, bounded coherent Copy telemetry, and per-cycle dequeue/publication limits. The transport is independent of the render endpoint until graph/runtime operations are defined. Explicit host stop/join followed by non-RT endpoint destruction is the shutdown contract; there is no internal thread or blocking drain.

Verification through `nix develop`:

- `RUSTFLAGS="-D warnings" cargo test -p rt_runtime`: 24 passed (1 unit, 12 core-contract, 11 lifecycle).
- `RUSTFLAGS="-D warnings" cargo test -p rt_runtime --release`: same 24 passed, including RT heap guards and owning destructor probes.
- `RUSTFLAGS="-D warnings" cargo build --workspace`: pass; Wasm production build also passed during this stage.
- `cargo fmt --all`, shared-test usage, four-target production/build boundary, and Wasm dependency isolation checks: pass.
- Node and Chromium package runners: 20 passed each with the same pinned tools as stage 1. Four native-only tests cover unwind handling, two-thread stress, and Loom.
- Deterministic admission model: 5,000 cycles at each normal capacity 1–4, stalled completions, cancelled/normal requests, FIFO and credit conservation checks.
- Two-thread stress: 10,000 ordered operations, coherent telemetry payload/complement pairs, and per-callback heap assertions. Shutdown exercised in both endpoint orders with queued/current/completed owners.
- Loom explores the standard one-way cancellation flag observation race. It does not model `rtrb` internals; publication and queue ownership use the existing queue implementation rather than a new atomic protocol.

Audit: admitted ownership count bounds the return queue (normal + recovery credits). Credits are released only on control-side receipt, not when commands leave the ingress queues. Unexpected return saturation retains the packet in a fixed slot rather than freeing or retrying indefinitely. Cancellation drops nothing on render; cancellation flags stay in the returning packet even if their tickets are gone. Recovery may reorder against normal operations, intentionally and explicitly; FIFO holds within each lane. RT `complete` misuse returns ownership in its error. All production code remains safe Rust; no mutex, wait, log, format, or I/O call is reachable from render transport methods.

## Stage 3, milestone 1: compiler and descriptor validator (stage incomplete)

Added typed audio/MIDI endpoints and processor signatures, deterministic DAG compilation, immutable relocatable plan envelopes, and a preallocated incremental receiver validator. Port capacities include host and registered node ports. The receiver checks node generations, ordering, duplicates, input ranges, typed endpoints and dependencies without accepting compiler output on trust. Descriptor envelope creation only checks storage bounds; it is explicitly not validation or permission to execute.

Verification through `nix develop`:

- Warning-denying native debug/release tests: 31 passed each (1 unit + 12 core + 7 graph + 11 lifecycle).
- Node and Chromium package runners: 27 passed each with the same pinned tools as earlier stages.
- Warning-denying workspace build, formatting, shared-test usage, and four-target dependency boundary: pass.
- Graph tests cover deterministic declaration-order independence, host passthrough and empty DAGs, typed fan-in/out descriptions, duplicate/cyclic/stale/missing identities, capacity/port errors, malformed descriptor spans, and 256 seeded eight-node DAGs.
- All receiver validation advances run under the heap guard at budgets including one step per callback. Step counts do not depend on the selected batch budget and stay within the linear declared graph-size bound. Production code still forbids unsafe code.

No sample-rendering/reference-interpreter comparison, graph activation, or concurrent rewiring result is claimed for this milestone. Those remain required before stage 3 can be completed.
