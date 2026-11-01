# Incremental validation

This is transport-extraction evidence only. It does not establish A1–A11 or a
completed native-session-recovery milestone. Session service, app protocol/client,
independent native roles and recovery UX remain pending in the plan.

Baseline: `cb6410f0`, with the transport extraction changes in the following commit.
Environment: managed Debian 13, x86_64, stable Rust 1.99.1 installed under
`/workspace/tools`; no Nix or preinstalled Rust. Network commands use the managed
proxy with the execution tool's network grant. No credentials were copied or logged.

Commands below ran on the extraction candidate, before its stage commit:

| Command | Result / scope |
| --- | --- |
| `cargo test -p shoop_transport -p shoop_rt_transport -p shoop_rt_protocol -p shoop_rt_transport_socket -p shoop_rt_transport_shared_memory --no-fail-fast` | Passed 28 tests: codec, carrier lifecycle/credits/bulk, socket authentication/deadlines and shared-memory delivery; moved buffer/framing tests included. |
| `cargo test --locked --no-default-features -p shoop_rt_engine -p shoop_rt_host_native -p shoop_rt_host_worklet -p shoop_rt_transport_message_port --no-fail-fast` | Passed 51 tests: engine timing/media/routing, prepared native/worklet heap guards and MessagePort contracts. CPAL feature disabled; no hardware claim. |
| `cargo test --locked -p shoop_rt_host_process --no-default-features` | Passed all 9 subprocess tests over both carriers, including crash/recording continuity and independent shutdown. These are baseline engine-host tests, not app-service recovery tests. |
| `RUSTFLAGS='-D warnings' cargo build -p shoop_transport -p shoop_transport_socket -p shoop_transport_shared_memory -p shoop_rt_protocol -p shoop_rt_transport -p shoop_rt_transport_socket -p shoop_rt_transport_shared_memory` | Passed. |
| `cargo build --target wasm32-unknown-unknown -p shoop_transport -p shoop_rt_protocol -p shoop_rt_transport` | Passed. |
| `cargo build --locked --target wasm32-unknown-unknown -p shoop_rt_host_worklet -p shoop_rt_transport_message_port` | Passed. |
| `RUSTFLAGS='-D warnings' cargo build --locked --no-default-features -p shoop_rt_host_process -p shoop_rt_host_worklet` | Passed, including the native/process/worklet consumers. |
| `python3 scripts/check_rt_engine_boundary.py` | Passed all 13 package boundaries on Linux, Windows, macOS and Wasm dependency trees. This is not cross-platform execution. |
| `python3 -m unittest scripts.tests.test_rt_engine_boundary scripts.tests.test_rt_runtime_miri scripts.tests.test_rt_runtime_miri_batches scripts.tests.test_rt_runtime_miri_jobs` | Passed 43 Python tests. Only inventory/runner tests; no Miri execution. |
| `cargo fmt --all -- --check` | Passed. |
| `python3 scripts/check_shoop_test_usage.py` | Passed. |
| `python3 scripts/check_tracing_coverage.py --require-closed` | Passed, 291 production modules accounted for. |

The Miri selection still contains 168 commands / 336 executions. Five shared byte
buffer/framing cases move from the engine transport's library target to the new
shared transport library; no workloads, seeds or assertions were weakened. Miri
execution is deferred to the final stage as requested.

Full retained-workspace native/Node/browser gates, hardware smokes, three-platform
packages, app attachment tests, performance measurements and final-head CI/review
are not covered by this checkpoint.

## Initial app protocol/client contracts

Candidate based on `b826242d`, before the following stage commit. This is portable
contract/client evidence only. No production UI or Lua path has been migrated to
the new crates, and no authoritative session service or independent app host exists
yet. Therefore no A1–A11 completion is claimed.

- `cargo test -p shoop_app_protocol -p shoop_app_client -p shoop_app_api --features shoop_app_api/serde`: 24 existing API tests plus 13 new protocol/client tests pass. Cases cover typed/encoded projections, malformed/truncated/version/oversize records, MIDI domain invariants, hostile lengths, partial snapshot rejection, lost reply reconciliation, bounded cooperative local queues, Ready ordering and correlated shutdown refusal/retry.
- `RUSTFLAGS='-D warnings' cargo build --locked -p shoop_app_protocol -p shoop_app_client`: passed; the same packages also build for `wasm32-unknown-unknown` with warnings denied.
- `RUSTFLAGS='-D warnings' cargo build --locked --target wasm32-unknown-unknown -p shoop_app -p shoop_rt_backend`: passed for the existing application/adapter consumers.
- Four-target dependency audit includes both new crates and forbids engine/runtime/device/UI/concrete-carrier dependencies.
- Python boundary/Miri-runner tests pass (44 tests). The Miri inventory adds four exact portable codec/client cases; original workloads/seeds remain intact. New total: 172 commands / 340 interpreted executions. Miri itself has not run.
- Formatting, test-attribute and closed tracing inventories are checked before the stage commit. Browser/Node execution and production service integration are not established by Wasm compilation.

The preceding transport commit's full workflow was dispatched as run
[41281596959](https://github.com/SanderVocke/shoopdaloop/actions/runs/41281596959).
The Rust coverage job failed with exit 100; its diagnostic log/artifact downloads
returned Forbidden in this environment. That failure is unresolved, not a pass.
The newer protocol/client candidate needs its own final-head CI evidence.
