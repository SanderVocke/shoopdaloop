# Stage 7 public graph command bindings

## Source and scope

Source is the commit containing this report, based on
`e68433cfa97982011a09c729fc362bd015dad776`. Final browser reports correctly retain
that base and `source_dirty: true`: testing preceded the containing commit.
No production runtime/loop Rust, manifest, lockfile or dependency changed.

`BrowserGraphClient` and opt-in `BrowserBridge.new_graphs` now execute actual runtime
Stage/Activate/Release through public Node/Chromium bindings. This is **binding-level
Wasm execution, not MessagePort or downstream Web Audio graph-change evidence**.
The actual offline/live worklets below remain primitive regressions. Native Rust
test functions are not described as executed in Wasm.

Stage7 remains2/8 checklist rows verified. Bounded two-thread graph payload transport,
MessagePort graph dispatch, actual graph worklets, program/resource staging, immutable
exports and broader scenarios remain open. Stages8–10 remain open. No new Miri,
benchmark, production hosted smoke, remote CI or reviewer approval is claimed.

## Interfaces and preparation

- `BrowserGraphClient::new(session)` compiles this fixture's host-output-bypass graph
  through the public controller API off-render. Its temporary preparation engine is
  never rendered and is explicitly cleaned/reclaimed, checking256/16/16/0 recovery.
  The retained template contains copied plain descriptor fields, not owners/pointers.
- `payload_count`/`prepare_payload(index, bankSlot, generation, epoch, sequence)`
  materialize one canonical96-byte record in prepared local storage. The caller
  supplies the independent monotonic payload sequence; all64-bit JS values are BigInt.
- `stage`, `activate`, `release`, `receive`, `outstanding`, `retained_graphs` wrap the
  same two-credit graph client used natively. Requests are96 bytes, responses128;
  local pointer getters address only their exporting instance.
- `BrowserBridge::new_graphs(session, fault)` opts into the existing prepared native
  graph bridge plus96/96/128-byte payload/request/result banks. Default primitive
  bridges allocate no graph banks and expose graph pointer0.
- `ingest_graph_payload` returns runtime error codes; `ingest_graph` uses protocol
  codes. `poll_graph_response` returns0 none/1 ready/high bit plus protocol error.
- Successful `stop` now refuses15 without consuming the engine while any graph is
  Ready/Taken, even if command counts are zero. Fault abandonment remains an explicit
  off-render disposal path and never fabricates client credit/owning returns.
- The native graph factory shares allocating preparation with its new fault-enabled
  fixture helper. All preparation-free descriptor/client/bridge operations remain
  heap-guarded; actual render captures independent allocation/reallocation/deallocation
  counts. There is no callback graph construction or vector growth.

The [fixture README](../../../tests/loop_engine_host/README.md#submit-graph-commands-through-public-wasm-bindings)
and [wire recipe](../../../tests/loop_engine_host/WIRE.md#public-graph-command-bindings)
provide ordered steps, shutdown rules and a runnable public-API Node example. That
example stages/releases using two explicit one-frame callbacks, checks stop refusal
and next bank generation2; it does not claim output/activation evidence.

## Actual binding scenarios and independent checks

Three new groups execute in each Node/Chromium/profile:

1. Constructor refusal, payload/command full64-bit values, two finite graph credits,
   invalid-bank/index refusal and malformed-response credit preservation.
2. Record0/Play4/Stop12 through the primitive client; Stage a bypass candidate, render
   one real sample for Ready, then activate at4. Check all13 stereo samples, all134
   MIDI packets, complete2440-byte semantic journal, all graph results and owning
   retirement. Admission is not activation. Stop refuses while a graph remains retained.
3.128 further actual runtime Stage/Release cycles, reusing the smaller retired initial
   graph without growing it. Every generation and Ready/release result is checked;
   final bank generation130, all command/graph counts zero, full stopped recovery.

The route-cut left samples are `1,2,3,4,1,2,3,4,1,2,3,4,0`; right is ten times left.
Capture still follows half-gain FX. The journal deliberately omits graph/routing
metadata and remains identical to the primitive semantic table; graph epoch2 is
independently required in activation results. MIDI remains the complete134-packet
native table, including mandatory restoration on all16 channels.

`bindingChecks.graph` retains all route-cut samples, MIDI/semantic bytes, four complete
128-byte graph results,128-cycle/generation130 totals, prepared/final memory and
recovery. Node and Chromium must match the complete report, not merely check names.
The Python runner compares output against native scalar/media/semantic expectations
and independently builds all four complete response byte tables. Opaque local graph
scope is correlated within the report, not treated as a cross-instance capability.

Two new Python cases reject all512 graph receipt truncations and512 single-byte
corruptions, plus missing/wrong music, MIDI, semantic data,128-round/generation totals,
fixed-memory evidence, recovery and incomplete receipt inventories. These synthetic
corruption checks are decoder/oracle tests, not additional musical host executions.

## Two new native binding-buffer tests

- `browser::tests::graph_binding_buffers_cover_route_cut_and_reliable_owning_release`:
  complete media/semantic/receipt checks after a common one-frame Ready acknowledgement,
  with remainder partitions `[12]`, `[3,8,1]` and zero/single-sample calls. Exact activation
  requested/effective/delivered/terminal4, epoch2; actual retired capacities3/12/2 and
  next bank generation2; stop refusal before release; zero final counts and256/16/16/0.
- `browser::tests::graph_binding_fault_disposal_abandons_credits_without_fabricating_returns`:
  both actual FX processor/heap fault modes with a queued graph, permanent render
  refusal, explicit stopped cleanup and unchanged abandoned client credit1. This is
  native fault disposal, not a new graph-worklet failure canary.

These add to the15 preceding graph-focused native cases. All17 remain unregistered/
uninterpreted Stage7 debt in [MIRI_JOBS](../MIRI_JOBS.md). Existing900s watchdog,
workloads/guards and one consolidated applicable stage selection remain required.

## Fresh gates

| Gate | Result |
| --- | --- |
| Fixture native debug/release | 41 passed per profile, zero skips/zero doctests |
| New route-cut exact-filter command | One test executes/passes per profile |
| Public wire-guide Node recipe | Executes/passes in debug and release assets |
| Shared Node/Chromium public bindings | Fourteen groups per runtime/profile |
| Native primitive direct/wire/two-thread | All six mode/profile combinations/four partitions |
| Actual primitive offline/live AudioWorklets | Pass in both profiles |
| Actual primitive processor/heap failure gates | Both intentional children exit1 with exact diagnostics/Rust witnesses per profile |
| Python helper tests | Seven pass, including complete graph byte-table corruption checks |
| Workspace nextest | 2141 passed/four existing skips, final33.387s summary |
| Warning-denied workspace build/rustdoc, formatting, test/boundary/tracing policies | Pass |

Final native/doc gates and fresh normal browser builds were rerun after the guide
updates. No assertions, musical workloads, guards, stack limits or watchdogs were
weakened. These are warm test observations, not benchmarks/cold-cache/deadline/CI
margin claims. No fresh package-Wasm/native-release production suites or generic
runtime worklet campaign is inferred beyond the listed gates.

## Final ordinary-memory/primitive worklet results

| Profile | Main graph bindings and actual primitive worklet prepared = final bytes | Offline callbacks/frames/start | Live callbacks/frames/start |
| --- | ---: | ---: | ---: |
| Debug | 1,835,008 | 2 / 256 / 128 | 4 / 512 / 512 |
| Release | 1,769,472 | 2 / 256 / 128 | 4 / 512 / 0 |

Live clock origins/counts are schedule-dependent, not assumptions about browser frame
zero or MessagePort deadlines. Full downstream256-sample/channel probes and all512
OfflineAudioContext samples/channel match, as do complete MIDI/receipts and semantic
bytes. Normal render heap counts are `[0,0,0]`, memory never grows after preparation,
and cleanup returns256 pages/16 versions/16 snapshots/zero media references.

Primitive canary witnesses per profile: processor origin1/code10/heap`[0,0,0]`;
heap origin2/code`0xffff0001`/heap`[4,20,4]`, each exit1. Their failed sessions do
not prove successful disposal. The canonical semantic stream remains SHA-256
`1c37c5d4f3607432c147852004a985b6ad2f6f3d1cf369287680ed9e9d5899dd`.

Final Wasm SHA-256:

- Debug `b5225b5c568b6d3b73766a12c399db89701500ba78cf26374562895faeee7566`
- Release `8e95e2fe87ad2d3d44f38a6a8459df2ed775612516b2544ae80059d93647ddff`

## Commands, correction and artifacts

All Cargo/Python commands ran through `nix develop --command ...` with
`RUSTFLAGS="-D warnings"`, warning-denied rustdoc and prescribed nextest0.9.116.
Tools remain rustc/cargo1.97.1, wasm-pack0.15.0, host-only wasm-bindgen0.2.127,
Node22.22.2 and Chromium/ChromeDriver147.0.7727.137. No manifests/lockfiles changed.

```sh
cargo fmt --all
cargo fmt --all -- --check
cargo fmt --manifest-path tests/loop_engine_host/Cargo.toml
cargo fmt --manifest-path tests/loop_engine_host/Cargo.toml -- --check
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml --release
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml \
  browser::tests::graph_binding_buffers_cover_route_cut_and_reliable_owning_release -- --exact
# Repeat the exact-filter command with --release before the filter.
python3 scripts/run_loop_engine_hosts.py --profile debug --verify-failures
python3 scripts/run_loop_engine_hosts.py --profile release --verify-failures
cargo build --locked --workspace
cargo doc --locked -p rt_runtime -p shoop_loop_engine --no-deps
cargo doc --locked --manifest-path tests/loop_engine_host/Cargo.toml --no-deps
python3 -m unittest discover -s scripts/tests -p test_loop_engine_hosts.py
python3 scripts/check_shoop_test_usage.py
python3 scripts/check_rt_runtime_boundary.py
python3 scripts/check_loop_engine_boundary.py
python3 scripts/check_tracing_coverage.py --require-closed
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --locked --workspace \
  --features shoop_engine/app_backend --profile ci
```

Retained initial failure: `Error: graph command check: command high bits`. The
new JS test incorrectly read the graph credit's32-bit field at24 as a u64 epoch.
Corrected it to the documented actual epoch offset40, retaining all four full-width
assertions. No codec/behavior changed to accommodate the test. `retry-hosts.log`
explicitly reuses the just-built debug artifact; final hosts rebuilt both profiles.
`initial-native.log` is the39-test pre-addition discovery; final inventory is41.

[`graph-bindings-stage7-logs.tar.gz`](graph-bindings-stage7-logs.tar.gz) retains
initial/focused/retry/final native/workspace/docs logs and complete final native,
Node/Chromium/browser/canary reports. SHA-256:
`7b98e52d599e6d8391d04d0455d23bf8886043a481dab52f997b5e2740a305a1`.
Local originals: `target/loop-layer-graph-bindings-stage7/` and
`target/loop-engine-host/reports/{debug,release}/`.
