# Stage 7 native graph actor transport

## Source and scope

Source is the containing commit, based on
`b31d2995ad688c07e81308a14e17e967afb9cde1`. Structured browser reports retain that
base and `source_dirty: true`; testing preceded the containing commit. No production
runtime/loop Rust, manifests, lockfiles, dependencies or browser JS changed.

Native graph descriptors/commands/owning returns now execute through a real bounded
two-thread host, not merely through direct calls or Wasm bindings. The renderer
constructs its engine and graph banks locally. Two prepared `sync_channel(1)` queues
carry copied protocol records and fixed render/sample observations. Graph owners
never cross the thread boundary. Actor waits/sends/joins occur outside guarded
callbacks; final variable-sized diagnostic copies happen only after successful
permanent stop. Native coordination tags are not a new portable wire layout.

The same script runs directly and through the actor: Record0/Play4/Stop12,20 graph
payload records, Stage, a real audible one-sample Ready checkpoint, Activate4,
Release at129, then permanent cleanup. Malformed reserved-byte payload and
foreign-session Stage attempts return4/9 and retry their original sequences without
losing owners/credits. All129 stereo samples,134 MIDI packets,2440 semantic bytes,
three accepted graph requests/four results/six primitive results and payload bytes
match. Published graph epoch2, requested/effective/delivered/terminal activation4,
retired bank generation2/capacities3/12/2, five actual zero ownership counts and
256/16/16/0 resource recovery are required.

Both default CLI scripts finish at130 engine frames/three callbacks (prime1,
resume128, silent release1). The new native case also compares128 singles and
`[0,3,0,5,120,0]`; zero calls are inert. Callback counts may differ across partitions,
but frame130, complete media/semantic/byte results and ownership remain exact.
Existing direct-reference partition coverage remains active.

The runner retains `native-graph.json` and `native-graph-two-thread.json`, requires
full equality with only consistently correlated opaque graph-scope normalization,
and records `native_graph_two_thread` capacity1/frames130/callbacks3. Sessions,
sequences, epochs, generations/capacities, clocks, padding and all musical content
remain exact. A new Python mutation case rejects incomplete/corrupted data,
inconsistent Ready identity, ownership-count changes and altered clocks without
mutating the input reports. Its committed/supplemented native input is synthetic
parser data, not a fresh execution.

## Verification

| Gate | Result |
| --- | --- |
| Standalone native fixture debug/release | 43 passed per profile, zero ignored, zero doctests |
| New exact native actor filter | One test executes/passes per profile |
| README `--graph-two-thread-json` commands | Executed/passed per profile |
| Node and Chromium bindings | Fourteen groups per runtime/profile |
| Actual successful contexts | Primitive offline/live and graph offline/live, both profiles |
| Complete direct/threaded native graph reports | Required and equal per profile except opaque scope |
| Primitive direct/wire/two-thread replay | All six mode/profile combinations/four partitions |
| Python report/decoder/failure-gate tests | Eleven pass, including the final ownership-count mutation |
| Workspace nextest | 2141 passed/four existing skips,33.273s summary |
| Warning-denied workspace build and Rust docs | Pass |
| Formatting/test-usage/boundaries/tracing | Pass; tracing inventory unchanged |

Code-complete native/full workspace and actual host gates passed. The final Python
ownership mutation and prose clarifications were checked afterwards; no Rust,
runner or browser behavior changed after those full gates. All captured commands
passed; no workload/assertion/guard/stack/watchdog was weakened. Warm harness times
are not performance/deadline/CI-margin measurements.

Fresh normal worklets retain fixed1,835,008 debug /1,769,472 release bytes, zero
heap/overflow and complete256/16/16/0 recovery. Graph observations:

| Profile/context | Engine frames/callbacks | Browser segment starts |
| --- | ---: | --- |
| Debug offline | 257 / 3 | 128,256 |
| Debug live | 1025 / 9 | 0,1024 |
| Release offline | 257 / 3 | 128,256 |
| Release live | 1025 / 9 | 1024,1536 |

Release enqueue/terminal is129 offline/513 live. Live origins/gaps remain
schedule-dependent; the acknowledged engine pause is not continuous-live-edit or
MessagePort deadline evidence. Full downstream samples and offline output are
checked under the preceding milestone's explicit clock mapping.

Fresh Wasm hashes are **identical** to the preceding actual graph-worklet milestone:

- Debug `2c54aebfe964d2d640d867a6bf1820d53bb6b9798b6767c43ee3a1e66e454a81`
- Release `92735bce306b8532b0d2f3ad73908996ffb90b2b0fda11821f0195336f059387`

No fresh intentional-worklet-failure campaign was run for this native-only change.
The four kinds' actual per-profile passes remain historical
[graph-worklet evidence](graph-worklet-stage7.md), with unchanged browser JS and
exact matching Wasm bytes; they are not relabeled as new-head runs. Final source-
matched delivery gates remain mandatory. No Miri/benchmark/remote CI/review or
production-application smoke pass is inferred.

The native actor test
`browser::native_graph::tests::bounded_two_thread_graph_transport_matches_complete_native_reference`
brings graph-focused standalone Miri debt to19 cases. All remain unregistered/
uninterpreted; keep the applicable consolidated Stage7 selection, unchanged900s
watchdog/guards/workloads and required final freshness checks. No routine per-commit
interpreter run was launched.

## Plan status

Stage7 now correctly records **3/8** rows: the fixture-documentation row was already
satisfied by the setup/runnable commands, wire/journal formats, failure/shutdown
contracts and unsupported-production limits maintained with `b31d2995`, then extended
here. This corrects bookkeeping against implemented evidence; it does not change
acceptance or close other rows. Native/export ownership coverage, program/resource
payloads, immutable exports, broader musical/failure/stall scenarios, consolidated
Miri, CI artifacts and Stages8–10 remain incomplete. Observed MIDI/semantic journals
are not immutable capture exports. Actor queue bounds are not native RSS or a
production deadline claim.

## Commands and raw artifacts

Cargo/Python commands ran under `nix develop --command ...`, with
`RUSTFLAGS="-D warnings"`, `RUSTDOCFLAGS="-D warnings"` and prescribed nextest0.9.116.
Tools remain rustc/cargo1.97.1, wasm-pack0.15.0, host-only wasm-bindgen0.2.127,
Node22.22.2 and Chromium/ChromeDriver147.0.7727.137.

```sh
cargo fmt --all
cargo fmt --manifest-path tests/loop_engine_host/Cargo.toml
cargo fmt --all -- --check
cargo fmt --manifest-path tests/loop_engine_host/Cargo.toml -- --check
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml --release
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml \
  browser::native_graph::tests::bounded_two_thread_graph_transport_matches_complete_native_reference -- --exact
# Repeat the exact filter with --release before its name.
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --graph-two-thread-json
# Repeat with --release before --bin.
python3 scripts/run_loop_engine_hosts.py --profile debug
python3 scripts/run_loop_engine_hosts.py --profile release
python3 -m unittest discover -s scripts/tests -p test_loop_engine_hosts.py
cargo build --locked --workspace
cargo doc --locked -p rt_runtime -p shoop_loop_engine --no-deps
cargo doc --locked --manifest-path tests/loop_engine_host/Cargo.toml --no-deps
python3 scripts/check_shoop_test_usage.py
python3 scripts/check_rt_runtime_boundary.py
python3 scripts/check_loop_engine_boundary.py
python3 scripts/check_tracing_coverage.py --require-closed
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --locked --workspace \
  --features shoop_engine/app_backend --profile ci
```

[`graph-thread-stage7-logs.tar.gz`](graph-thread-stage7-logs.tar.gz) retains focused,
initial/final native/Python/workspace/docs logs, native JSON and complete fresh final
normal native/Node/Chromium/worklet reports. Stale prior failure directories/logs are
excluded. SHA-256:
`7ee08cde757fc26960b4072022ee7aaa60a26e1a9e044bca3a429a6134ccd5de`.
Local originals: `target/loop-layer-graph-thread-stage7/` and
`target/loop-engine-host/reports/{debug,release}/`.
